---
tags: [gamelib, moc]
status: planning
created: 2026-11-12
---
# GameLib — Home

A Windows desktop app, written in **Rust** with **egui**, that gathers every game I own (Steam, Epic, GOG, loose folders, manual adds) into one library, launches them, and tracks how long I play.

> [!info] Where we are
> **Stage:** design / planning. Nothing has been coded yet.
> Section 1 (architecture) has been presented. Sections 2–5 are **drafts waiting for my review**. Once they're agreed, the next step is a step-by-step implementation plan for Milestone 1.

## Design notes
1. [[01 Requirements]]: what I'm building and why
2. [[02 Architecture]]: how the code is organised (Section 1)
3. [[03 Data Model]]: what a "game" is and how it's stored (Section 2, draft)
4. [[04 Game Sources]]: Steam, Epic, GOG, folder scan, manual (Section 3, draft)
5. [[05 Launching and Playtime]]: starting games and tracking time (Section 3, draft)
6. [[06 Metadata and Cover Art]]: downloading covers and info (Section 3, draft)
7. [[07 UI Screens]]: what each screen looks like (Section 4, draft)
8. [[08 Errors and Testing]]: what happens when things go wrong, and how it's tested (Section 5, draft)
9. [[14 UI Designs]]: **mockups** of every main screen, inspired by Steam, Epic, EA and Xbox (draft)
10. [[15 Design Variants]]: alternative looks: **Constellation** (star-map background), **Daylight List**, **Couch Mode** (draft)
11. [[16 Profile and Friends]]: profile editing, banners, badges, Steam friends list (draft)
12. [[17 Shop]]: spend **Stardust ✦** earned by playing on backgrounds, banners, badges and name changes (draft)

## Building it
- [[09 Milestones]]: the build order, each step ending in something that works
- [[10 Setup Checklist]]: installing Rust and the tools
- [[11 Rust Learning Path]]: what to learn before each milestone
- [[12 Decisions Log]]: decisions made, and questions still open
- [[13 Dev Log Template]]: copy for each work session (useful for coursework evidence)

## Tech at a glance
| Area | Choice |
|---|---|
| Language | Rust |
| UI | `eframe` / `egui` (pure Rust, immediate mode) |
| Storage | SQLite via `rusqlite` (bundled) |
| Platform | Windows only |
| Data location | `%APPDATA%\gamelib\` |
