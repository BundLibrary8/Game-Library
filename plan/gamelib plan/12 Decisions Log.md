---
tags: [gamelib, decisions]
---
# 12 Decisions Log

Back to [[00 Home]]

## Decided
| Date | Decision | Why |
|---|---|---|
| 2026-11-12 | Build a **game collection app** (not a game engine crate or mini-games) | What I want to make |
| 2026-11-12 | Purpose: **learning/coursework and daily use** | Both matter |
| 2026-11-12 | Sources: **Steam, Epic, GOG, folder scan, manual** | Want everything in one place |
| 2026-11-12 | Features: **covers/info, playtime, search/tags/favourites, collections/stats** | Full launcher experience |
| 2026-11-12 | UI: **egui (pure Rust)** over Tauri (Rust + React) and Slint/Iced | One language and simplest setup; more Rust practice |
| 2026-11-12 | Build in **milestones**, each ending in a working app | Big scope for a first Rust project |

## Open: decide when reviewing the drafts
- [ ] Are **collections just tags** with a sidebar flag? (proposed in [[03 Data Model]])
- [ ] Info for non-Steam games: typed in by hand for now, IGDB later? (proposed in [[06 Metadata and Cover Art]])
- [ ] Is "Windows only, local only, single user" right? ([[01 Requirements]])
- [ ] Keep the 15 s "game closed" and 3 min "never started" timings? ([[05 Launching and Playtime]])
- [ ] Track games launched outside GameLib? (proposed: not in the first version)
- [ ] Game details as a **full page** or a **side panel**? ([[14 UI Designs#Open decision]])
- [ ] Add a **Home** screen ("Continue playing" + "Recently added")? ([[14 UI Designs]])
- [ ] Which look to build first, and which become **themes** or **later modes**? ([[15 Design Variants#Suggested plan (to confirm)]])
- [ ] Constellation background: include **twinkling**, with an off switch in Settings? ([[B - Constellation]])
- [ ] Friends list: **Steam friends (read-only)**, a **local list**, or a full **online server** later? ([[16 Profile and Friends#Friends list: the options]]) *Suggested: Steam friends.*
- [ ] Shop currency: **Stardust earned by playing** (no real money)? ([[17 Shop]])
- [ ] Are the prices and reward rates about right? (✦10/hour, ✦50 per finished game, items ✦150–800)
- [ ] Should name changes cost Stardust, or always be free?
- [ ] Sections 2–5 approved overall?
