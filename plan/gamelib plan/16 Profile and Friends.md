---
tags: [gamelib, design, feature]
status: draft-needs-review
created: 2026-11-12
---
# 16 Profile and Friends

Back to [[00 Home]] · Related: [[17 Shop]] · [[03 Data Model]] · [[09 Milestones]]

> [!warning] Draft, and it changes an earlier assumption
> [[01 Requirements]] said "no accounts, no friends". A profile is still **local** (it lives on your PC), but a **friends list needs data from the internet**. The options are below, with a recommendation.

> [!info] Plan review, 13 Nov 2026
> Built ✅ as described (local profile, earned banners, read-only Steam friends cached in `friends_cache`). The live Steam Web API call hasn't been tried yet ([[09 Milestones]]).
> **Long term:** Option 2 below ("GameLib-to-GameLib friends") is no longer "never": it becomes **optional** GameLib accounts in Phase 4, designed in [[23 Social Platform Architecture]] with the backend options in [[24 Backend and Cloud Infrastructure]]. The local profile keeps working without an account. A cheap first step with no server is a **shareable profile card** (Phase 3). Typing a SteamID proves nothing, so public profiles will verify it with "Sign in through Steam".

## Mockups
![[gamelib-06-profile.png]]
![[gamelib-07-edit-profile.png]]

Opened from the **avatar in the top-right corner** of every screen.

## Profile page
- **Banner** across the top, with a "Change banner" button.
- **Avatar**, **display name**, up to **3 badges** next to the name, **status** ("Playing Ember Road", taken from the playtime tracker), and a short **bio**.
- **Stat tiles**: total playtime, games, most played, games finished. These reuse the queries from the Stats page.
- **Showcase**: up to 4 favourite games you choose.
- **Friends** panel, with each friend's online status and what they're playing.

## Editing the profile
The "Edit profile" window contains:
- **Picture**: choose an image file. It's copied to `%APPDATA%\gamelib\profile\` and cropped to a circle when shown.
- **Display name**: the first time is free; later changes use a **Name change** from the [[17 Shop]].
- **Bio**: up to 160 characters.
- **Banner**, from three places:
  1. **Upload** your own image
  2. **A game's art**: any cover or hero image already downloaded
  3. **Earned banners**: unlocked automatically by milestones (100 h played, 10 games finished, …). Locked ones show a 🔒 and the goal.
  4. Plus any banners bought in the [[17 Shop]]
- **Badges**: choose which 3 owned badges to show (earned or bought).
- **Showcase**: choose 4 games.
- **Steam account**: link your SteamID so the friends list works (see below).

## Friends list: the options
| Option | How it works | Effort | Verdict |
|---|---|---|---|
| **1. Steam friends** (read-only) | Use the **Steam Web API** to fetch your Steam friends, their avatars, online status and current game. Needs a free Steam Web API key + your SteamID, and your Steam friends list must not be private. | Medium: just HTTP calls from the worker thread | ✅ **Recommended.** Real friends and live status, with no server to run |
| 2. GameLib-to-GameLib friends | Your own **online server** with accounts, logins, friend requests and syncing (e.g. a Rust `axum` server + a database, hosted online). | **Very large**: a second project, with security, hosting and costs | ❌ Not now. Possibly a future project after GameLib is finished |
| 3. Local "friends" notes | A list you type in yourself: name, Discord tag, games you play together. Nothing live. | Small | Possible fallback if you don't use Steam |

### How Option 1 works
- Endpoints: `ISteamUser/GetFriendList` (your friends' SteamIDs) and `ISteamUser/GetPlayerSummaries` (name, avatar, online state, and which game they're in).
- Runs as a **worker job** (see [[02 Architecture]]) on startup and when you click ⟳ Refresh. No automatic refresh more often than every few minutes.
- Results are cached in a `friends_cache` table so the panel shows something straight away, even offline ("updated 2 h ago").
- The Steam Web API key goes in **Settings**, stored locally and **never committed to Git**, the same rule as the SteamGridDB key.
- Friends see your **Steam** name and status, not your GameLib profile. The GameLib profile is for you, and for screenshots in the coursework write-up.

> [!info] Check the docs
> Check the Steam Web API documentation (and its terms of use) before building this. Field names and rules can change.

## Data (adds to [[03 Data Model]])
```sql
profile(id INTEGER PRIMARY KEY CHECK (id = 1),   -- always exactly one row
        display_name TEXT NOT NULL, bio TEXT, avatar_path TEXT,
        banner TEXT,             -- 'upload:<path>' | 'game:<game_id>' | 'item:<shop item id>'
        steam_id TEXT, created_at INTEGER NOT NULL)
profile_showcase(slot INTEGER PRIMARY KEY CHECK (slot BETWEEN 1 AND 4), game_id INTEGER NOT NULL)
profile_badges(slot INTEGER PRIMARY KEY CHECK (slot BETWEEN 1 AND 3), item_id TEXT NOT NULL)
friends_cache(steam_id TEXT PRIMARY KEY, name TEXT, avatar_url TEXT,
              state TEXT, playing TEXT, updated_at INTEGER)
```

## Tests
- Earned-banner rules: give the function playtime and finished-game totals, and check which banners are unlocked.
- Parsing a saved Steam API JSON response from `tests/fixtures/` into `Friend` structs. Tests never call the real API.
- Profile save/load with an in-memory database, including "only one profile row".
