---
tags: [gamelib, decisions]
updated: 2026-11-13
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
| 2026-11-13 | Release builds are a **window program** (no console window) | Double-clicking `gamelib.exe` shouldn't open a black box behind it. The cost: terminal commands don't make the terminal wait |
| 2026-11-13 | The window remembers its **size** but not its **position** | A remembered position can put the window off-screen when a monitor is unplugged |
| 2026-11-13 | The ✦ symbol comes from **Windows' symbol font**, not a bundled one | It's on every Windows PC, and the app is Windows only |
| 2026-11-13 | Crash recovery ignores sessions active in the **last 3 minutes** | Stops a second copy of GameLib (a terminal command) ending a game that the window is still tracking |

## Answered by the code: confirm or change
These were open questions. The code had to pick an answer to be built at all, so each one below is **what the app does today**. Tick it if I'm happy with it; otherwise it becomes a change to make.

- [ ] Are **collections just tags** with a sidebar flag? → **Yes.** A collection is a tag with `is_collection` set. ([[03 Data Model]])
- [ ] Info for non-Steam games? → **Only Steam games get a description, genres and year** (from the Steam Store). Others have none; IGDB isn't used. ([[06 Metadata and Cover Art]])
- [ ] Is "Windows only, local only, single user" right? → **Yes**, that is how it is built. ([[01 Requirements]])
- [ ] Keep the 15 s "game closed" and 3 min "never started" timings? → **Kept.** One change: the session ends at the moment the game was last seen, so the 15 s wait isn't counted as playtime. ([[05 Launching and Playtime]])
- [ ] Track games launched outside GameLib? → **No.** Only games started with GameLib's Play button are tracked.
- [ ] Game details as a **full page** or a **side panel**? → **Full page.** ([[14 UI Designs#Open decision]])
- [ ] Add a **Home** screen? → **Yes**, it's built. ([[14 UI Designs]])
- [ ] Which look first, and which become themes? → **Ember** is the default. **Constellation**, **Aurora** and **Synthwave Grid** are library backgrounds bought in the shop. **Daylight List** and **Couch Mode** are not built. ([[15 Design Variants#Suggested plan (to confirm)]])
- [ ] Constellation **twinkling**, with an off switch? → **Yes**, with a switch in Settings. It is **off** by default. ([[B - Constellation]])
- [ ] Friends list: Steam, local, or an online server? → **Steam friends, read-only.** ([[16 Profile and Friends#Friends list: the options]])
- [ ] Shop currency: **Stardust earned by playing** (no real money)? → **Yes.** ([[17 Shop]])
- [ ] Prices and reward rates? → **As proposed:** ✦10 per hour, ✦50 per finished game (needs 1 h played), ✦5 for the first game each day; items cost ✦150–800.
- [ ] Should name changes cost Stardust? → **The first is free; each later one costs ✦500** (bought as a "Name change" in the shop).
- [ ] Sections 2–5 approved overall? → Built as drafted, with the differences listed in the README.

## Proposed in the plan review (13 Nov 2026): confirm or change
These come from [[18 Plan Review 2026-11-13]]. They're **recommendations**, not decided until ticked.

| # | Proposal | Why | Where |
|---|---|---|---|
| P1 | **Keep Rust, egui/eframe and SQLite**; no rewrite | They fit a local-first desktop app; nothing in the vision needs a different UI framework | [[24 Backend and Cloud Infrastructure#Technology review: what stays and why]] |
| P2 | Long-term principle changes from **local-only** to **local-first** | Allows optional online features without ever requiring them | [[01 Requirements]], [[19 Product Vision]] |
| P3 | Split **game / installation / ownership**, keeping existing game ids | Same game on two stores should be one entry | [[20 Game Identity and Installation Model]] |
| P4 | Only **exact external-ID** matches merge automatically; everything else is a suggestion the user confirms; merges can be undone | A wrong merge is worse than a visible duplicate | [[20 Game Identity and Installation Model#Matching: how scans decide "same game or new game?"]] |
| P5 | Store-reported playtime is **shown beside** GameLib's, never added to it | Steam also counts sessions GameLib launched | [[20 Game Identity and Installation Model#Playtime with several sources]] |
| P6 | Replace the `Source` enum with **`ProviderId` + capability traits + a registry** | New providers without touching 13 files | [[21 Provider Architecture]] |
| P7 | Convert to a **Cargo workspace** (`gamelib-core` + `gamelib-desktop`) at M10; no crate per provider | Faster rebuilds, enforced UI separation, future server can share types | [[21 Provider Architecture#Single crate or workspace?]] |
| P8 | **No formal plugin system** before Phase 3 ends; providers compiled in | Avoid over-engineering; keep traits protocol-friendly | [[21 Provider Architecture#A formal plugin system? Not yet.]] |
| P9 | **IGDB** as the metadata source for non-Steam games | Broad coverage, cross-store IDs | [[27 Future Integrations#Metadata and artwork]] |
| P10 | Modding: **integrate with MO2/Vortex/Prism**, native installs only for BepInEx/Thunderstore and drop-in games | Don't rebuild mature tools | [[22 Modding Architecture]] |
| P11 | Roadmap order: **Library → Mods ∥ local Discovery & Progression → Identity → Community → Expansion** | Local features first; server-dependent features when the product earns them | [[28 Expanded Development Roadmap#Order and why it changed]] |
| P12 | **No backend before Phase 4**; then a time-boxed spike of Supabase vs PocketBase (own axum server only beside them) | Free to start; auth done properly | [[24 Backend and Cloud Infrastructure#Backend options (all have a free or very cheap start)]] |
| P13 | Playtime rewards get an **idle filter and daily cap**; finished reward keyed on external ID | Fix exploits found in the review | [[26 Progression and Stardust Economy]] |
| P14 | **No storefront, affiliate links, real money, loot boxes or trading** anywhere in the roadmap | Keeps the project honest, legal and solo-sized | [[19 Product Vision#What GameLib is not]] |
| P15 | Moderation tools (report, block, remove) ship **before** any user-generated content is public | Safety and UK Online Safety Act duties | [[23 Social Platform Architecture#Moderation, safety and law (must-haves before Phase 5 goes public)]] |

- [ ] P1 · [ ] P2 · [ ] P3 · [ ] P4 · [ ] P5 · [ ] P6 · [ ] P7 · [ ] P8 · [ ] P9 · [ ] P10 · [ ] P11 · [ ] P12 · [ ] P13 · [ ] P14 · [ ] P15

## Still open
- [ ] **Where should API keys be kept?** Today they are plain text in `library.db`. Windows Credential Manager would be safer, but it's more code. *(Plan review suggests: move them in M10, before account tokens exist.)*
- [ ] **Should playtime carry on being tracked after GameLib is closed?** Today a session is counted up to its last one-minute checkpoint. Fixing it properly means a small helper that stays running in the background.
- [ ] **Build Daylight List and Couch Mode**, or drop them from the plan?
- [ ] **Info for non-Steam games:** add IGDB, allow typing it in by hand, or leave it?

### Added by the plan review (13 Nov 2026): need my input
- [ ] **Licence** for GameLib (MIT/Apache-2.0 vs GPL-3.0). Matters before publishing releases, and GPL tools (MO2, Vortex) can't have their code copied into a non-GPL GameLib. ([[22 Modding Architecture#Licensing]])
- [ ] **Who is GameLib for after the coursework?** Just me, friends, or the public? This decides when Nexus app registration, IGDB terms, a backend and legal work become necessary.
- [ ] **Is the coursework deadline tied to the current v0.1 scope?** If yes, nothing from Phase 1 should start before the write-up is handed in.
- [ ] **Which stores do I actually use** besides Steam/Epic/GOG (Ubisoft, EA, Xbox/Game Pass, Battle.net…)? Decides M13's order.
- [ ] **Which games do I mod?** Decides the first mod adapter (Thunderstore/BepInEx vs MO2 hand-off vs Minecraft/Prism).
- [ ] **Nexus Premium?** Direct mod downloads through the API are Premium-only.
- [ ] **Backend appetite:** happy to rely on a hosted service (Supabase), or prefer self-hosting (PocketBase / own Rust server) even if slower?
- [ ] **Age/ownership:** an adult needs to own any public online service (accounts, user content), as already noted for payments in [[17 Shop]].
- [ ] **Phases 2 and 3 order:** modding first, or local discovery/progression first?
