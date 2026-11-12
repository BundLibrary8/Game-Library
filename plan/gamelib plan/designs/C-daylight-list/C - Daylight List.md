---
tags: [gamelib, design, ui, variant]
variant: C
status: draft-needs-review
---
# Variant C: Daylight List

Back to [[15 Design Variants]] · [[00 Home]]

A **light theme** built around a **sortable table** instead of a cover grid. It's denser and more practical, closer to a file manager than a store. It also shows the **side-panel** version of game details, so you can compare it with the full page in [[14 UI Designs]].

![[gamelib-C1-list-with-side-panel.png]]
![[gamelib-C2-settings.png]]

## What's different
- **Navigation is top tabs only** (Home, Library, Stats, Settings), with no sidebar.
- **List view**: small cover thumbnail, title, source badge, playtime, last played, genres, and a Play button **on every row**. Click a column heading to sort.
- Uninstalled games show **Locate** instead of Play.
- **Details in a right-hand panel**: select a row and its info appears without leaving the list.
- **Settings screen** (not designed anywhere else yet):
  - Source toggles, each showing whether that launcher was **Found** on this PC
  - Scan folders with game counts
  - SteamGridDB key with a **Test** button
  - Startup options
  - Data folder, export backup, reset
  - It notes that **EA app and Xbox aren't supported yet**. Those games can be added by hand or by folder scan (see [[04 Game Sources]]).

## Colours
| Token | Value |
|---|---|
| Background / panel | `#f5f6f8` / `#ffffff` |
| Border | `#e4e7ec` |
| Text / muted | `#14171f` / `#6b7280` |
| Accent (indigo) | `#4f46e5` |
| Source badges | Steam `#2f5bd3`, Epic `#3f4350`, GOG `#7a3fd1`, Local `#1f8a57` |

## egui notes
- Light mode is built in: `ctx.set_visuals(egui::Visuals::light())`, then change the accent colours.
- The table: `egui_extras::TableBuilder` gives sortable, resizable columns, and only draws the visible rows, so it stays fast with hundreds of games.
- Side panel: `egui::SidePanel::right("details")`.
- Toggle switches: egui has checkboxes built in, and the egui demo includes a ready-made **toggle switch** example to copy.
