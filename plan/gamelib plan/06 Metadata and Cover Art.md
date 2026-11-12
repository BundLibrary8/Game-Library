---
tags: [gamelib, design]
status: draft-needs-review
section: 3
---
# 06 Metadata and Cover Art (Section 3, draft)

Back to [[00 Home]] · Prev: [[05 Launching and Playtime]] · Next: [[07 UI Screens]]

> [!warning] Draft
> Not agreed yet. Check the API details against the official docs before coding.

## Where it comes from
| What | Source | Key needed? |
|---|---|---|
| **Cover art** (portrait, 600×900) for all games | **SteamGridDB** API | Free API key (make an account, then Preferences → API) |
| **Description, genres, release year** for Steam games | **Steam Store** `appdetails` endpoint | No |
| Info for non-Steam games | Typed in by hand at first. **IGDB** could be added later, but it needs a Twitch developer app | Later |

## How it works (`metadata.rs`)
1. After a scan, every game **without a cover** gets a `FetchCover(game_id)` job.
2. The worker looks the game up on SteamGridDB: by **Steam app id** for Steam games (exact match), otherwise by **searching the title**.
3. It downloads the first portrait grid image to `%APPDATA%\gamelib\covers\<id>.jpg` and saves `cover_path`.
4. For Steam games, it also fetches description, genres and year from the Steam Store.
5. Requests go **one at a time with a short pause**, to be polite to the APIs and avoid rate limits.
6. The UI shows a placeholder card (the title on a coloured tile) until the cover arrives.

## Manual fixes
- The details panel has **"Change cover…"**, to pick a local image, and **"Search again"**, to type a better title if the search found the wrong game.
- Once a cover has been set by hand, a rescan never overwrites it.

## The API key
- Entered in **Settings** and stored in the `settings` table.
- **Never put it in the source code or commit it to Git.** That matters if the repo is public for the portfolio.
- With no key, everything still works except cover downloads.
