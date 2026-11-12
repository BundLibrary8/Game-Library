---
tags: [gamelib, design, ui, moc]
status: draft-needs-review
created: 2026-11-12
---
# 15 Design Variants

Back to [[00 Home]] · Main designs: [[14 UI Designs]]

Four different directions for how GameLib could look. Each variant has its own folder inside `designs/`, with a note explaining it.

| Variant | Folder | Feel | Key idea |
|---|---|---|---|
| **A: Ember** (main) | `designs/` | Dark, amber accent | Sidebar + cover grid, full-page details. See [[14 UI Designs]] |
| **B: Constellation** | `designs/B-constellation/` | Deep space, glass panels | **Star-map background**, icon rail, playtime drawn as a constellation. See [[B - Constellation]] |
| **C: Daylight List** | `designs/C-daylight-list/` | Light, compact, practical | Sortable **table view** + **side-panel** details + **Settings** screen. See [[C - Daylight List]] |
| **D: Couch Mode** | `designs/D-couch-mode/` | Full-screen TV style | **Controller-friendly** carousel with big art. See [[D - Couch Mode]] |

## Quick look
![[gamelib-01-library-grid.png|420]] ![[gamelib-B1-constellation-library.png|420]]
![[gamelib-C1-list-with-side-panel.png|420]] ![[gamelib-D1-couch-library.png|420]]

## They can be combined
These don't have to be either/or:
- **A + B are the same layout** with different colours and background. They could ship as **theme options**: Dark (A), Constellation (B), and Light (from C's colours).
- **C's list view** can be the "☰" toggle in A's library (A already has the grid/list buttons), and **C's Settings screen** can be used whichever look wins.
- **D** is a separate mode for later (stretch goal after M8).

## Suggested plan (to confirm)
1. Build **A's layout** first (M1–M5).
2. Add **C's list view** and **Settings screen** inside it.
3. Add **B (Constellation)** as a **library background you unlock in the [[17 Shop]]** (M7). The star background is a fun, self-contained bit of painter code.
4. Add **D (Couch mode)** only if there's time, after M8.

Profile and Shop screens are in `designs/E-profile-and-shop/`; see [[16 Profile and Friends]] and [[17 Shop]].

Decisions to make are in [[12 Decisions Log]].
