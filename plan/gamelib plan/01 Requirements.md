---
tags: [gamelib, design]
status: agreed
---
# 01 Requirements

Back to [[00 Home]]

## Purpose
Both of these:
- **Learn Rust** and have a strong **coursework / portfolio** project with a clear structure and write-up.
- End up with a tool I **actually use** instead of opening Steam, Epic and GOG separately.

## What I said I want
**Where games come from**
- [x] Auto-detect **Steam** games
- [x] Auto-detect **Epic Games** and **GOG** games
- [x] **Folder scan** (e.g. `D:\Games`) for loose executables
- [x] **Manual add**: pick an `.exe` and fill in details

**Features**
- [x] **Cover art and info**: box art, description, genres, from an online database
- [x] **Playtime tracking**: total time and last played
- [x] **Search, tags, favourites**: filter and organise
- [x] **Collections and a stats page**: groups like "Backlog", plus most played and totals

**Tech**
- Rust, with an egui UI (chosen over Tauri and Slint/Iced; see [[12 Decisions Log]])
- Starting with **no prior Rust experience**

## Assumptions (correct these if wrong)
- **Windows only.**
- **Local only**: no accounts, no cloud sync. Everything lives on my PC.
- One user.

## Success criteria: "done" looks like
1. I open the app and see **my real games**, found automatically, with **cover art**.
2. I click Play and the game **launches**, whatever store it's from.
3. When I close the game, its **playtime and last-played date update**.
4. I can **search**, **favourite**, **tag**, and put games into **collections**.
5. The **stats page** shows total playtime and my most-played games.
6. The code is cleanly organised, with tests for the core logic, so it holds up as coursework.

## Out of scope (for now)
- Mac and Linux
- Installing or uninstalling games, or buying games
- Achievements, friends, cloud saves
- Syncing between PCs
