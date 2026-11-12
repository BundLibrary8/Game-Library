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
- [ ] Sections 2–5 approved overall?
