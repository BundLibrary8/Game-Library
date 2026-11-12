---
tags: [gamelib, design]
status: draft-needs-review
section: 4
---
# 07 UI Screens (Section 4, draft)

Back to [[00 Home]] · Prev: [[06 Metadata and Cover Art]] · Next: [[08 Errors and Testing]]

> [!warning] Draft
> Not agreed yet.

> [!example] Mockups
> Rendered designs for every screen are in [[14 UI Designs]]. They add a **Home** screen, and suggest a **full-page game details** view instead of the side panel sketched below.

## Main window layout
```
┌───────────────────────────────────────────────────────────────┐
│ GameLib   [ 🔍 search…        ]  Sort: [Title ▾]  [Scan] [+Add] │  ← top bar
├──────────────┬────────────────────────────────┬───────────────┤
│ LIBRARY      │ ┌────┐ ┌────┐ ┌────┐ ┌────┐   │  [ cover ]    │
│  All games   │ │    │ │    │ │    │ │    │   │  Hades        │
│  ★ Favourites│ │    │ │    │ │    │ │    │   │  [ ▶ PLAY ]   │
│  Recent      │ └────┘ └────┘ └────┘ └────┘   │  41h 12m      │
│ COLLECTIONS  │  Hades  Celeste ...            │  Last: 2 days │
│  Backlog     │ ┌────┐ ┌────┐ ┌────┐ ┌────┐   │  Tags: [+]    │
│  Co-op       │ │    │ │    │ │    │ │    │   │  ★ Favourite  │
│ SOURCES      │ └────┘ └────┘ └────┘ └────┘   │  Edit · Hide  │
│  Steam (84)  │                                │               │
│  Epic (12)   │                                │               │
│  GOG (6)     │                                │               │
│ ──────────── │                                │               │
│  📊 Stats    │                                │               │
│  ⚙ Settings  │                                │               │
├──────────────┴────────────────────────────────┴───────────────┤
│ Status: Found 102 games · Downloading covers 23/102…          │  ← status bar
└───────────────────────────────────────────────────────────────┘
```

## Screens
- **Library (centre)**: a grid of cover cards that resizes with the window. Click to select, double-click to play. A list view toggle can come later.
  - **Search** filters by title as you type. **Sort**: title, last played, playtime, recently added.
  - The **sidebar** filters the grid: favourites, recently played, a collection, or a source.
- **Details panel (right)**: cover, title, Play button, playtime, last played, tags (add or remove), favourite star, collections, *Edit* (title, exe, args, cover), *Hide*, and *Remove*.
- **Import review (pop-up window)**: shown after a folder scan. A list of what was found, with checkboxes, editable titles and exe pickers, plus "Add selected".
- **Stats**:
  - Total playtime and number of games
  - Top 10 most played (bar chart, `egui_plot`)
  - Played in the last 7 / 30 days (from `play_sessions`)
  - Games per source
  - "Never played" count, which is good for shaming the backlog
- **Settings**: enable or disable each source, add or remove scan folders, SteamGridDB API key, "scan on startup", and "Rescan now".

## How the UI code is organised
- `AppState` (in `app.rs`) holds the loaded games, current filter, search text, selected game, the open screen, and the channel ends.
- Each screen is a function such as `fn show_library(ui: &mut egui::Ui, state: &mut AppState)`. Because egui is immediate mode, there are **no callbacks**: `if ui.button("Play").clicked() { … }` runs right there.
- The grid reads from an **in-memory list** loaded from the DB, not the database itself, so it stays fast.

> [!tip] Explore egui first
> The egui demo at **egui.rs** shows every widget running in your browser, with source code. Spend 20 minutes there before Milestone 1.
