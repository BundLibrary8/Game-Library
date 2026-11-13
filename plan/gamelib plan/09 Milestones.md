---
tags: [gamelib, plan]
status: mostly-done
updated: 2026-11-13
---
# 09 Milestones

Back to [[00 Home]]

Each milestone ends with an app that **runs and does something useful**.

> [!info] How to read the ticks (updated 13 Nov 2026)
> A ticked box means **the code for it exists and its automated tests pass**. The **"Done when"** lines are different: they are checks to do by hand on my own PC with my real games, so they stay unticked until I have tried them myself.
> So far on Windows: the app builds, opens, a scan finds installed games, and games start.

> [!info] Plan review, 13 Nov 2026
> - Re-checked against the code: every ticked box below has code behind it, and **148 tests pass** on Windows (138 unit + 10 in other test targets). No tick was found to be wrong.
> - Also in the code but not listed before: the **Epic folder fallback** (`sources/epic_folders.rs`, uncommitted), see [[04 Game Sources#Epic fallback: finding games by their folders (built, uncommitted)]].
> - **Not done that the plan suggested:** GitHub Actions CI ([[08 Errors and Testing]]), list view and Settings-screen ideas from Daylight List ([[15 Design Variants]]), info for non-Steam games.
> - **What comes next:** milestones continue as **M9–M31** in [[28 Expanded Development Roadmap]]. M9 = acceptance checks + v0.1 release (all the unticked "Done when" lines below).

## M0: Setup and Rust basics
Learn: [[11 Rust Learning Path#Before M0/M1]] · Tools: [[10 Setup Checklist]]
- [x] Rust and Git installed; `cargo run` shows an egui window titled "GameLib"
- [x] Repo on GitHub with a README and `.gitignore` (`/target`)

## M1: Core library (manual games)
- [x] `model.rs`, plus the database with a `games` table and schema versioning (it grew into a `db/` folder)
- [x] Add game manually (file picker → title)
- [x] Grid of games (placeholder tiles), details page, Play button launches `.exe`
- [x] Data survives closing and reopening the app
- [x] Tests: database insert/load/update
- [ ] **Done when:** I can add 3 games by hand, close the app, reopen it, and launch them.

## M2: Auto-detection
Learn: traits, threads and channels
- [x] `GameSource` trait + `worker.rs` (background scanning, status bar progress)
- [x] Steam, Epic and GOG sources
- [x] Folder scan + import review window
- [x] Rescan without duplicates; missing games marked "not installed"
- [x] Settings screen: enable sources, scan folders
- [ ] **Done when:** one click of Scan finds all my installed games and nothing appears twice. *(A scan finds games on my PC. Still to check: that it finds all of them, and GOG in particular.)*

## M3: Playtime tracking
- [x] `play_sessions` table; pure state logic + tests
- [x] Tracker thread using `sysinfo`; checkpoints every minute
- [x] Details page shows playtime + last played; sort by them
- [ ] **Done when:** I play a Steam game for 10 minutes and the app records about 10 minutes.

## M4: Cover art and info
- [x] Settings field for the SteamGridDB key
- [x] Cover download queue; covers show in the grid
- [x] Steam Store info (description, genres, year)
- [x] "Change cover…" / "Search again"
- [ ] **Done when:** almost every game has the right cover without me doing anything. *(Needs my SteamGridDB key; the live API has never been called.)*

## M5: Organising and stats
- [x] Search box + sort options
- [x] Favourites, tags, collections (sidebar)
- [x] Hide/remove games
- [x] Stats page with charts
- [ ] **Done when:** I can find any game in a couple of seconds and the stats page looks good in a screenshot.

## M6: Profile and friends
See [[16 Profile and Friends]]
- [x] `profile` table + profile page (banner, avatar, name, bio, stat tiles, showcase)
- [x] Edit profile window (picture, name, bio, banner picker, badges, showcase)
- [x] Earned banners from playtime / finished-game milestones (+ tests)
- [x] Steam Web API key + SteamID in Settings; friends fetched on a worker thread and cached
- [ ] **Done when:** my profile shows my real stats and my Steam friends with what they're playing. *(Needs my Steam Web API key; the live API has never been called.)*

## M7: Stardust and Shop
See [[17 Shop]]
- [x] `stardust_ledger` + rewards for playtime, finishing games, daily launch (+ tests)
- [x] Built-in catalogue + Shop screen + preview
- [x] Buying in one transaction (+ tests for "not enough Stardust" and "already owned")
- [x] Equip library backgrounds (incl. **Constellation**), banners, badges; name change item
- [ ] **Done when:** playing earns Stardust, and I can buy and use the Constellation background.

## M8: Polish and write-up
- [x] App icon, window size remembered, nice empty states ("No games yet")
- [x] `cargo build --release` → a single `gamelib.exe` (about 20 MB, no console window, needs nothing beside it)
- [x] README with screenshots, features and architecture diagram
- [ ] Coursework write-up from [[13 Dev Log Template]] entries. *(Not started: there are no dev log entries yet. This one is mine to write.)*

## Found and fixed while polishing (13 Nov 2026)
Worth a line each in the write-up, as examples of testing on the real platform finding things the Linux tests didn't:
- **The Stardust star ✦ showed as an empty box on Windows.** Neither the app's font nor egui's built-in ones have it. Fixed by adding Windows' own symbol font as a fallback.
- **One test failed on Windows only**, because it compared paths as text and Windows uses `\` where Linux uses `/`.
- **The Steam API key could end up in the log file**, because it is part of the web address and error messages quote the address. The address is now stripped from those errors.
- **Downloading covers held everything else up.** The cover batch ran on the one worker thread, so clicking Play during it had to wait, and closing the window left the program running until the batch finished. It now has its own thread and stops when the app closes.
- **Running a command in a terminal while the window was tracking a game ended that game's session early.** Crash recovery now leaves alone any session that was active in the last three minutes.
- **A failed launch marked every running game as "not running".** Only the game that failed is cleared now.
- **Cover downloads are safer:** the size limit applies while downloading, the file must really be an image, and it is saved under a temporary name first so a half-finished download is never shown.
- **Scans save in one database transaction** per store, which is faster and can't leave a half-saved library.
- **The game page and stats page asked the database for the same things every frame.** They now load once and reuse the answer until something changes.

## After M8 (added 13 Nov 2026)
The full plan with objectives, risks and completion criteria is in [[28 Expanded Development Roadmap]]. Summary:

| Phase | Milestones | Goal | Status |
|---|---|---|---|
| 0 Stabilise | M8 (finish write-up), **M9** acceptance + v0.1 release | Prove what exists works; ship it | 🔜 **next** |
| 1 Universal Library | M10 provider registry + workspace · M11 games/installations/ownerships · M12 IGDB + duplicates · M13 more stores · M14 Steam account data · M15 organising | One correct library across stores | 🔜 |
| 2 Modding Hub | M16–M19 | Browse mods, work with MO2/Vortex/Prism, native BepInEx + drop-in | 🔭 |
| 3 Discovery & Progression (local) | M20–M23 | Wishlist, prices, play-next, Stardust v2, profile card | 🔭 |
| 4 Identity | M24–M27 | Optional accounts, public profiles, follows, activity | 🔭 |
| 5 Community | M28–M31 | Game pages, reviews, lists, shared mod collections | 🔭 |
| 6 Expansion | — | Cloud sync, mobile companion, plugins, more platforms | 🔭 possibilities |

### M9 checklist (copied here so it sits next to the "Done when" lines it covers)
- [ ] Commit the uncommitted work in sensible commits
- [ ] Run every "Done when" above; record results in an **Acceptance Test Log** note
- [ ] Verify GOG, Epic launch links + `.egstore` fallback, a 10-minute Steam and Epic session, file pickers, the three live APIs
- [ ] Fix failures (with a regression test where possible)
- [ ] GitHub Actions on `windows-latest` (fmt, clippy, test)
- [ ] Choose a licence
- [ ] Tag v0.1.0 with `gamelib.exe` attached
