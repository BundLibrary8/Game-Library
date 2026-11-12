---
tags: [gamelib, design, ui]
status: draft-needs-review
created: 2026-11-12
---
# 14 UI Designs

Back to [[00 Home]] · Related: [[07 UI Screens]] · Other looks: [[15 Design Variants]]

> [!note] This is **Variant A: Ember**
> Alternative designs (Constellation, Daylight List, Couch Mode) are in [[15 Design Variants]].

Simple mockups for GameLib, based on the **layout ideas** that the big launchers share. The look is our own: no logos, art or branding copied. Game names and covers are made up; real covers come from [[06 Metadata and Cover Art]].

> [!warning] Draft
> These are for discussing the design, not pixel-perfect specs. One open decision is listed at the bottom.

## What we're borrowing from each launcher
| Launcher | Idea we're borrowing | Where it shows up |
|---|---|---|
| **Steam** | Left sidebar with your library, collections and filters. Game page with a large **hero banner** and a **play bar** showing Play, last played and playtime. A home shelf of recent games. | Sidebar, Game details, Home |
| **Epic Games** | A clean **grid of portrait covers** with little else in the way. Filter and sort controls above the grid. | Library grid |
| **EA app** | Big, bold tiles. Hovering a tile shows a **Play** button. Quick "installed / not installed" filtering. | Library grid hover, filter chips |
| **Xbox app** | **Filter chips** across the top, a **grid/list toggle**, and a **"jump back in"** row of recently played games. | Filter chips, Home "Continue playing" |

**What we're deliberately leaving out:** store pages, friends lists, news feeds, ads, achievements and downloads. GameLib is only a library and launcher (see [[01 Requirements#Out of scope (for now)]]).

## Visual style
| Token | Colour | Used for |
|---|---|---|
| Background | `#0f1115` | window |
| Panel | `#161920` / `#1d212a` | sidebar, cards, top bar |
| Border | `#2a2f3a` | dividers |
| Text / muted | `#e8eaf0` / `#8b93a7` | labels |
| **Accent (amber)** | `#f5a524` | Play buttons, selected item, favourites |
| Success (green) | `#4cc38a` | "installed", "up to date" |

- Dark theme throughout, with rounded corners (6–10 px) and one accent colour, so **Play is always the most obvious thing on screen**.
- Each source has its own colour dot (Steam blue, Epic grey, GOG purple, Local green), used the same way everywhere.
- A small source badge sits in the top-left of every cover, and a ★ in the top-right for favourites.

---

## 1. Library grid
![[gamelib-01-library-grid.png]]
- **Sidebar** (Steam/Xbox idea): Library filters, then Collections, then Sources, each with a count. Settings sits at the bottom.
- **Filter chips** (Xbox idea): All · Installed · Not installed · Never played.
- **Sort** dropdown and **grid/list toggle** on the right.
- **Cover grid** (Epic idea). Hovering a cover outlines it and shows a Play button (EA idea). Uninstalled games are **greyed out**.
- **Status bar**: scan and cover-download progress.

## 2. Game details
![[gamelib-02-game-details.png]]
- **Hero banner + play bar** (Steam idea): a big Play button, then last played, playtime, source and install path, with ★ and a ⋯ menu (Edit, Hide, Remove, Open folder).
- **About**: description and tags, with "+ Tag".
- **Last 14 days**: a small playtime chart from `play_sessions`.
- **Recent sessions** and **Collections** checkboxes, to add or remove this game from a collection.

## 3. Home
![[gamelib-03-home.png]]
- **Continue playing** (Xbox "jump back in" / Steam recent games): the 3 most recently played games, as wide tiles with a Play button.
- **Recently added**: the newest games from scans.
- Opens on launch, so you can click straight into a game.

## 4. Stats
![[gamelib-04-stats.png]]
- **4 headline numbers**: total playtime, library size, this month, never played.
- **Most played** (horizontal bars), **weekly playtime** (12 weeks), **games by source**.
- All of it comes from simple SQL queries on `games` and `play_sessions` (see [[03 Data Model]]).

## 5. Folder-scan import review
![[gamelib-05-import-review.png]]
- Appears after a folder scan, because folder scanning **guesses** (see [[04 Game Sources#Folder scan (`folder.rs`)]]).
- Every row can be edited: tick box, title, and which exe to use.
- Status column: **New**, **Check exe** (more than one found), **Already found via Steam** (skipped automatically), **No game exe found**.
- Nothing is saved until you click **Add N games**.

---

## Building these in egui
| Design element | egui approach |
|---|---|
| Top bar / sidebar / status bar | `TopBottomPanel::top`, `SidePanel::left`, `TopBottomPanel::bottom` |
| Cover grid | `ScrollArea` + `ui.horizontal_wrapped` (or `Grid`), each card an `Image` inside a `Frame` with rounding |
| Hover Play button | `response.hovered()`, then paint a circle + ▶ on top with `ui.painter()` |
| Filter chips | `ui.selectable_label(selected, "Installed")` styled with rounding |
| Hero banner | the game's cover image stretched and darkened, or a solid colour. Fancy CSS-style gradients aren't built in, so keep it simple |
| Charts | `egui_plot::BarChart` |
| Import review | `egui::Window` or `egui::Modal` containing a `Grid` of checkboxes, `TextEdit`s and `ComboBox`es |
| Colours | set once in `ctx.set_visuals(...)` / `Style`, using the tokens above |

> [!tip] Placeholder covers
> Until real covers download, draw a coloured tile with the game's title, like the mockups. Pick the colour from a hash of the title so it never changes.

## Open decision
- [ ] **Game details as a full page (design 2) or as a right-hand side panel (sketch in [[07 UI Screens]])?** A full page has room for charts and sessions, which suits Stats-minded features. A side panel keeps the grid visible. *Suggestion: full page*, opened by clicking a cover, with double-click (or the hover ▶) to play straight from the grid.
