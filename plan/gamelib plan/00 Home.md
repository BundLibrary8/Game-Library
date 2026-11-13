---
tags: [gamelib, moc]
status: built (v0.1 candidate) + long-term plan proposed
created: 2026-11-12
updated: 2026-11-13
---
# GameLib — Home

**Today:** a Windows desktop app, written in **Rust** with **egui**, that gathers every game I own (Steam, Epic, GOG, loose folders, manual adds) into one library, launches them, and tracks how long I play.

**Long term (proposed):** a local-first **universal gaming platform**: one library across every store, a modding hub, an optional gaming identity, game-centred communities and discovery, built *beside* Steam, Epic and GOG rather than replacing them. → [[19 Product Vision]]

> [!success] Where we are (13 Nov 2026)
> **Stage:** built. Milestones **M0–M7 are coded**, and the polish half of **M8** is done (icon, remembered window size, release build, README with screenshots).
> The app runs on my Windows 11 PC: it opens, finds games and starts them. **All 148 automated tests pass** (re-run during the plan review).
>
> **Still to do (Phase 0, see [[28 Expanded Development Roadmap#Phase 0 – Stabilise and release v0.1]]):**
> - My own "Done when" checks for each milestone in [[09 Milestones]] (playing a game for 10 minutes and checking the time, covers with a real API key, the friends list…)
> - **Commit** the current uncommitted work (Epic folder fallback, icon, window size, README…)
> - The **coursework write-up** and the dev log entries it is built from ([[13 Dev Log Template]])
> - Review the answers the code gave to the open questions in [[12 Decisions Log]]
> - Tag **v0.1.0**

> [!info] Plan review, 13 Nov 2026
> The plan was reviewed against the code and expanded with a long-term vision. **What changed and why:** [[18 Plan Review 2026-11-13]].
> **Recommended next milestone:** M9 (acceptance tests + v0.1 release), then M10 (provider registry).

## Status key (used on every page)
| Mark | Meaning |
|---|---|
| ✅ **Built** | Exists in the code and its automated tests pass. *Not* the same as checked by hand: see [[09 Milestones]] |
| 🔜 **Planned next** | Phase 0–1: designed in detail, will be built next |
| 🔭 **Future** | Phases 2–6: direction agreed in principle, re-planned before building |
| 🔬 **Needs research** | Not known to be possible or allowed yet; needs a spike or reading official docs |

The code is in `gamelib/` next to this vault; its `README.md` has the features, screenshots, how to run it and how the code is organised.

## Design notes for what's built (v0.1)
These were written before the code. Where the code ended up different, the README's "Where this differs from the plan notes" section says how.

1. [[01 Requirements]]: what I'm building and why (now with long-term requirements)
2. [[02 Architecture]]: how the code is organised (as built + where it's heading)
3. [[03 Data Model]]: schema as built (v2) + proposed v3+
4. [[04 Game Sources]]: Steam, Epic (+ folder fallback), GOG, folder scan, manual; planned stores
5. [[05 Launching and Playtime]]: starting games and tracking time
6. [[06 Metadata and Cover Art]]: downloading covers and info
7. [[07 UI Screens]]: what each screen looks like
8. [[08 Errors and Testing]]: what happens when things go wrong, and how it's tested
9. [[14 UI Designs]]: **mockups** of every main screen, inspired by Steam, Epic, EA and Xbox
10. [[15 Design Variants]]: alternative looks: **Constellation** (built), **Daylight List** and **Couch Mode** (not built)
11. [[16 Profile and Friends]]: profile editing, banners, badges, Steam friends list
12. [[17 Shop]]: spend **Stardust ✦** earned by playing on backgrounds, banners, badges and name changes

## Long-term plan (proposed 13 Nov 2026)
| Note | What it covers | Status |
|---|---|---|
| [[18 Plan Review 2026-11-13]] | Change notes: what was reviewed, found and changed in the plan | record |
| [[19 Product Vision]] | Why GameLib, the five pillars, principles, what it is not | 🔭 |
| [[20 Game Identity and Installation Model]] | One game, many installations and ownerships; matching and merging | 🔜 Phase 1 |
| [[21 Provider Architecture]] | Provider traits, registry, workspace split, plugin-system verdict | 🔜 Phase 1 |
| [[22 Modding Architecture]] | Mod repositories, existing mod managers, game adapters, profiles | 🔭 Phase 2 |
| [[25 Game Discovery]] | Wishlist, releases, prices, recommendations (local first) | 🔭 Phase 3 |
| [[26 Progression and Stardust Economy]] | XP, levels, anti-exploit rules, themes; cosmetic only | 🔭 Phase 3 |
| [[23 Social Platform Architecture]] | Accounts, profiles, privacy, friends, activity, communities, moderation | 🔭 Phases 4–5 |
| [[24 Backend and Cloud Infrastructure]] | Tech review, backend options, auth, media, sync, costs | 🔭 Phase 4+ |
| [[27 Future Integrations]] | Research catalogue: stores, metadata, mods, prices, with confidence levels | 🔬 |
| [[28 Expanded Development Roadmap]] | **Phases 0–6 with milestones M9–M31** | plan |

## Building it
- [[09 Milestones]]: M0–M8, with what is done ticked off; the rest continues in [[28 Expanded Development Roadmap]]
- [[10 Setup Checklist]]: installing Rust and the tools
- [[11 Rust Learning Path]]: what to learn before each milestone
- [[12 Decisions Log]]: decisions made, proposals to confirm, and questions still open
- [[13 Dev Log Template]]: copy for each work session (useful for coursework evidence)

## Tech at a glance
| Area | Choice today | Long-term |
|---|---|---|
| Language | Rust | Keep |
| UI | `eframe` / `egui` (pure Rust, immediate mode) | Keep ([[24 Backend and Cloud Infrastructure#Technology review: what stays and why]]) |
| Storage | SQLite via `rusqlite` (bundled) | Keep locally; a server DB only from Phase 4 |
| Code layout | One crate (library + binary) | Cargo workspace from M10 ([[21 Provider Architecture#Single crate or workspace?]]) |
| Platform | Windows only | Windows first |
| Data location | `%APPDATA%\gamelib\` | Same; secrets move to Windows Credential Manager |
| Online | Only SteamGridDB, Steam Store, Steam Web API calls | Optional backend from Phase 4; never required |
| Finished program | `cargo build --release` → one `gamelib.exe`, about 20 MB | Same |
