---
tags: [gamelib, design, feature, stardust]
status: proposed
created: 2026-11-13
phase: 3
---
# 26 Progression and Stardust Economy

Back to [[00 Home]] · Today's shop: [[17 Shop]] · Profile: [[16 Profile and Friends]] · Roadmap: [[28 Expanded Development Roadmap#Phase 3 – Personal Discovery & Progression (local-first)]]

> [!success] What exists ✅ (checked in the code, 13 Nov 2026)
> - `stardust_ledger` + `owned_items`; balance = sum of the ledger (`shop.rs`).
> - Rewards: ✦10/hour per session (whole minutes, rounded down, paid inside `finish_session`), ✦50 once per finished game with ≥ 1 h played, ✦5 for the first launch each UTC day.
> - Built-in catalogue (`CATALOGUE` const): 3 library backgrounds, banners, badges, name change; Founder badge free.
> - Earned banners/badges from playtime/finished milestones (`profile::EARNED_ITEMS`).
> - Purchases in one transaction with tests for "not enough" and "already owned".

> [!abstract] Below: 🔭 proposed expansion. **Cosmetic only, always.** No real money, no loot boxes or random rewards, no trading.

## Weak spots in today's rewards (found in the code review)
| Exploit | Today | Matters locally? | Fix (proposed) |
|---|---|---|---|
| Leave a game running idle overnight | Every tracked minute pays ✦ | Only the user sees it | **Idle detection** for rewards (not for playtime): Windows `GetLastInputInfo`; after 15 min without keyboard/mouse/controller input, stop *reward* minutes until input returns 🔬 controller input may need XInput polling |
| Unlimited hours per day | No cap | Same | **Daily reward cap**: e.g. max ✦40/day from playtime (4 h); unlimited playtime is still *tracked* |
| Delete a game, re-add it, finish it again | `stardust_ledger.game_id` has no foreign key; a re-added game gets a **new id**, so the "once per game" check passes again | Minor | Key the "finished" reward on the game's **external ID** (after [[20 Game Identity and Installation Model]]) instead of the local id |
| Edit `library.db` directly | Possible | Accepted ✅ (decided in [[17 Shop]]: local, no value) | Stays accepted while local. Online, see below |
| Launch tiny "games" (any exe) to farm daily reward | ✦5/day max | No | Fine as is |

## Proposed progression model
### XP and levels (new, derived, no new currency to balance)
- **XP** is earned alongside Stardust but **never spent**. **Level** = f(total XP), e.g. level *n* needs `100 × n^1.5` XP. Shown on the profile.
- XP sources reward **variety and milestones**, not grinding:

| Action | XP | Stardust |
|---|---|---|
| Tracked hour (after idle filter, within daily cap) | 10 | ✦10 (as today) |
| First session of a game you've never played | 25 | ✦5 |
| Finish a game (≥ 1 h played) | 100 | ✦50 (as today) |
| Playtime milestones (10/50/100/500 h total) | 200–1000 | earned banner (as today) |
| Play 5 different games in a week | 50 | ✦20 |
| Write a review / share a mod collection (Phase 5, after moderation) | 20 (max 1/day) | — |

- Stored as ledger rows too (`xp_ledger`, same shape as `stardust_ledger`) so "why do I have this?" is always answerable, which is good coursework evidence.

### Customisation unlocks
| Unlock | How | Notes |
|---|---|---|
| Library backgrounds ✅ | Shop | Constellation, Aurora, Synthwave Grid built |
| Profile banners & badges ✅ | Shop / earned | |
| **Profile layouts** | Level unlocks (e.g. level 5: two-column showcase; level 10: "favourite mod collection" panel) | Pure UI arrangement |
| **Showcase types** | Shop | Achievement showcase (needs achievements, Phase 1 M14), "games finished this year", screenshot wall (Phase 5) |
| **Themes** | Create locally: a theme = small JSON of colour tokens (today's `ui/theme.rs` tokens ✅) + optional background id | Export/import as a file (Phase 3); share on the community (Phase 5) |
| **Community cosmetics** | Phase 5+: user-made themes/banners submitted, reviewed, published | Needs moderation + licensing rules for uploaded art |
| Daylight List / Couch Mode looks | Could become unlockable "layouts" | Still an open decision ([[12 Decisions Log#Still open]]) |

## Local vs online rewards
- **Local (Phases 0–3):** everything stays as now: ledger in SQLite, trust the PC. Editing the DB only affects the user.
- **Online (Phase 4+):** the **server keeps its own ledger**. The desktop *reports* events (session ended: game, duration); the server **re-applies the rules and caps** (daily cap, max session length, max 24 h/day, rate of events) and decides the balance shown on the public profile. Client-reported playtime is still unverifiable, so:
  - nothing purchasable has value outside GameLib (no trading, no marketplace, no real money) → little reason to cheat;
  - no public leaderboards ranked by Stardust or hours;
  - "suspicious" accounts (e.g. 24 h/day for weeks) just stop earning; no bans needed.
- Local balance and online balance may differ; the online one is what others see. 🔬 decide how to seed the online ledger from existing local history (suggestion: import capped at a fixed amount, e.g. ✦2,000, with a "Founder" badge).

## Out of scope
- Real-money purchases, premium currency, subscriptions, season passes.
- Random/loot-box rewards, gambling mechanics, streak penalties that pressure daily play.
- Trading or gifting between users (gifting maybe later, only for cosmetics, after moderation exists).

## Tests to add when built
- Idle filter: given input timestamps and a session, reward minutes exclude idle stretches.
- Daily cap across several sessions in one day, and across midnight UTC.
- XP level curve boundaries.
- Re-added game cannot re-earn the finished reward (after external IDs exist).
