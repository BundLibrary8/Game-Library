---
tags: [gamelib, design, social]
status: proposed
created: 2026-11-13
phase: 4-5
---
# 23 Social Platform Architecture

Back to [[00 Home]] · Today's profile: [[16 Profile and Friends]] · Backend: [[24 Backend and Cloud Infrastructure]] · Roadmap: [[28 Expanded Development Roadmap#Phase 4 – GameLib Identity (first backend)]]

> [!abstract] Status: 🔭 future (Phase 4 identity, Phase 5 communities)
> Today GameLib has a **local** profile ✅ and a **read-only Steam friends list** ✅. Everything below about accounts, followers, feeds, reviews and communities needs an online backend and is **not built**.

## Principles
1. **Optional.** No account is ever needed for the library, launching, playtime, mods or the local profile.
2. **GameLib identity ≠ external accounts.** A GameLib account has its own ID. Steam, Epic, GOG, Xbox… are **linked accounts** with their own rows; unlinking one never deletes your GameLib account, and GameLib never uses Steam login *as* its login.
3. **Private by default.** Nothing leaves the PC until the user publishes it, field by field.
4. **The PC is the source of truth for your own data.** The server holds a *published copy* of what you chose to share, plus things that only make sense online (follows, reviews, comments).
5. **No feature ships without its moderation tools.** Anything other people can see needs report, block and remove first.

## Identity
### What exists ✅
`profile` (one row): display name, bio, avatar, banner, Steam ID; `profile_showcase` (4 games), `profile_badges` (3), earned banners, Stardust. Local only.

### Step 1 (no server): shareable profile card 🔜 cheap win
Export the profile as an image (and a small JSON) for sharing anywhere. Gives the "show off my gaming identity" value with zero infrastructure, and is great for the coursework write-up. (Phase 3, see [[28 Expanded Development Roadmap]].)

### Step 2 (server): GameLib accounts
| Piece | Proposal |
|---|---|
| Sign-up / sign-in | Email + password **or** "Sign in with Google/Discord" through the backend's auth service. The desktop app uses the **system browser** with OAuth 2.0 + PKCE and a loopback redirect (the standard for native apps, RFC 8252); GameLib never sees a third-party password. |
| Tokens on the PC | Stored in **Windows Credential Manager** (e.g. the `keyring` crate), not in `library.db`. Same move is recommended for today's API keys ([[12 Decisions Log#Still open]]). |
| Handle | Unique `@handle` + display name. Name changes keep using the existing Stardust rule ✅ for the display name; handle changes rate-limited. |
| Linking Steam | Prove ownership with **"Sign in through Steam" (OpenID 2.0)**, which returns the SteamID. Today the user types a SteamID ✅ which proves nothing; fine locally, not fine for a public profile. |
| Linking others | Only where an official, user-authorised flow exists (🔬 per provider, see [[27 Future Integrations]]). Otherwise "self-declared" with a visible label. |
| Delete account | One click: server data deleted; local data untouched. Data export (JSON) available. Needed for UK GDPR anyway. |

### Public profile (published copy)
- Fields publishable individually: avatar, banner, bio, badges, showcase, headline stats (total hours, games, finished), favourite games, recent activity.
- **Visibility per section:** Public / Followers / Friends / Only me. Default: Only me.
- Profile media (avatar/banner uploads) go to object storage, resized server-side, with limits ([[24 Backend and Cloud Infrastructure#Media storage]]).

## Social graph
| Relationship | How | Why both |
|---|---|---|
| **Follow** (one-way) | Instant, like a feed subscription | Low friction, works for creators and curators |
| **Friend** (two-way) | Request + accept | Needed for "Friends only" privacy and multiplayer discovery |
| **Block** | One-way, silent | Required before any public feature |

External friends (Steam ✅ today) stay a separate list ("Steam friends"), with an optional "Find my Steam friends on GameLib" that matches **only** users who linked and verified their Steam account and allowed being found.

## Activity
- **Generated locally**, from things the desktop app already knows: session ended (game + duration) ✅, game finished ✅, badge/banner unlocked ✅, mod profile shared, review posted.
- **Published only if** the user enabled that activity type. Options: Off / Followers / Public, per type. "Currently playing" is its own switch.
- Server stores activity items with a TTL (e.g. 90 days) to keep the database small on a free tier.
- Feed = activity from people you follow, newest first; no ranking algorithm initially.

## Unified stats and achievements
- Stats are computed **locally** (✅ `stats.rs`) and the published profile receives the summary numbers, not raw sessions.
- Achievements: imported per provider where an API exists (Steam: `ISteamUserStats/GetPlayerAchievements`, needs the profile's game details to be public 🔬). Shown per game; GameLib "milestones" (✅ earned banners) are GameLib's own cross-store achievements.
- Published stats are **self-reported** (the PC can be edited). Label them "tracked by GameLib", never present them as verified. No leaderboards that would reward cheating.

## Communities
> Gaming communities built around the **canonical game** ([[20 Game Identity and Installation Model]]), not chat servers.

| Feature | Phase | Notes |
|---|---|---|
| **Game page (community tab)** | 5 | Keyed by canonical game (IGDB ID as the shared key across users 🔬, since local game ids differ per PC) |
| **Reviews & recommendations** | 5 | 👍/👎 + text; show reviewer's GameLib-tracked hours ("12 h tracked") as context; one review per user per game |
| **Shared lists** ("Best co-op games") | 5 | Ordered list of games + notes. Also the basis for discovery ([[25 Game Discovery]]) |
| **Shared mod collections** | 5 | Publish the profile JSON from [[22 Modding Architecture#Collections]]; others import it |
| **Guides** | 5b | Markdown with images; versioned; report button |
| **Screenshots** | 5b | Upload with size caps; **clips/video link out** to YouTube/Medal etc. (video hosting is expensive) |
| **Discussions** | 5c / maybe never | Threaded comments on reviews/guides first. Full forums only if there's demand; consider linking to existing communities (subreddits, Discord, Steam forums) instead |
| **Groups & LFG** (looking for group) | 6 | Group pages; "looking to play X tonight" posts with expiry; friends-of-friends discovery |

How it connects:
- **Library → Community:** every game page in your library has a community tab.
- **Mods → Community:** "Popular collections for this game" next to your mod profiles.
- **Community → Discovery:** lists and reviews feed recommendations.

## Moderation, safety and law (must-haves before Phase 5 goes public)
- Report button on every user-generated item; block users; hide content; admin tools to remove content and suspend accounts; audit log.
- Rate limits and new-account limits on posting; automatic size/type checks on uploads.
- Text filters for obvious abuse; image moderation for uploads (🔬 free options vary; manual review queue at small scale).
- **Legal (UK):** a public service with user-to-user content falls under the **Online Safety Act 2023** (risk assessments, illegal-content duties, extra duties if children may use it) and **UK GDPR** (privacy notice, lawful basis, data export/delete). 🔬 Read Ofcom's guidance for small services before launch. Age requirement for accounts (e.g. 13+), and an **adult must own the service** (same reasoning as [[17 Shop#Why no real money (for now)]]).
- Terms of service and privacy policy written before the first outside user.

## Security
- Server is the only authority for follows, reviews, visibility; the client is never trusted for permissions.
- Row-level security (if Postgres/Supabase) or equivalent checks in every API handler; tests for "user A can't read B's private data".
- No storage of third-party passwords; linked-account tokens kept server-side only if needed, encrypted, minimal scopes.
- Store provider API keys server-side if a feature needs them (e.g. a shared IGDB proxy), never in the shipped exe.

## Out of scope (identity & community phases)
- Real-time chat, voice, DMs (maybe ever: Discord exists).
- Monetisation of any kind, ads, data selling.
- Verified/competitive leaderboards.
- Livestreaming, video hosting.
