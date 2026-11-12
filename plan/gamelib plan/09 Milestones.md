---
tags: [gamelib, plan]
status: draft-needs-review
---
# 09 Milestones

Back to [[00 Home]]

Each milestone ends with an app that **runs and does something useful**. Don't start the next one until the "Done when" list is all ticked. A detailed step-by-step plan is written for each milestone when it's time to start it.

## M0: Setup and Rust basics
Learn: [[11 Rust Learning Path#Before M0/M1]] · Tools: [[10 Setup Checklist]]
- [ ] Rust, VS Code and Git installed; `cargo run` shows an empty egui window titled "GameLib"
- [ ] Repo on GitHub with a README and `.gitignore` (`/target`)

## M1: Core library (manual games)
- [ ] `model.rs`, plus `db.rs` with `games` table and schema versioning
- [ ] Add game manually (file picker → title)
- [ ] Grid of games (placeholder tiles), details panel, Play button launches `.exe`
- [ ] Data survives closing and reopening the app
- [ ] Tests: database insert/load/update
- **Done when:** I can add 3 games by hand, close the app, reopen it, and launch them.

## M2: Auto-detection
Learn: traits, threads and channels
- [ ] `GameSource` trait + `worker.rs` (background scanning, status bar progress)
- [ ] Steam → Epic → GOG (one at a time, each with its own commit)
- [ ] Folder scan + import review window
- [ ] Rescan without duplicates; missing games marked "not installed"
- [ ] Settings screen: enable sources, scan folders
- **Done when:** one click of Scan finds all my installed games and nothing appears twice.

## M3: Playtime tracking
- [ ] `play_sessions` table; pure state logic + tests
- [ ] Tracker thread using `sysinfo`; checkpoints every minute
- [ ] Details panel shows playtime + last played; sort by them
- **Done when:** I play a Steam game for 10 minutes and the app records about 10 minutes.

## M4: Cover art and info
- [ ] Settings field for the SteamGridDB key
- [ ] Cover download queue; covers show in the grid
- [ ] Steam Store info (description, genres, year)
- [ ] "Change cover…" / "Search again"
- **Done when:** almost every game has the right cover without me doing anything.

## M5: Organising and stats
- [ ] Search box + sort options
- [ ] Favourites, tags, collections (sidebar)
- [ ] Hide/remove games
- [ ] Stats page with charts
- **Done when:** I can find any game in a couple of seconds and the stats page looks good in a screenshot.

## M6: Polish and write-up
- [ ] App icon, window size remembered, nice empty states ("No games yet, click Scan")
- [ ] `cargo build --release` → a single `gamelib.exe`
- [ ] README with screenshots, features and architecture diagram (reuse [[02 Architecture]])
- [ ] Coursework write-up from [[13 Dev Log Template]] entries
