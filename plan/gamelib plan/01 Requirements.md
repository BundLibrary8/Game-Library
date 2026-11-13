---
tags: [gamelib, design]
status: agreed (v0.1) + long-term proposed
updated: 2026-11-13
---
# 01 Requirements

Back to [[00 Home]]

> [!info] Plan review, 13 Nov 2026: what changed on this page
> - The v0.1 requirements below are **unchanged** and are all built ✅ (still to be checked by hand: [[09 Milestones]]).
> - The assumption "**local only**" becomes "**local-first**": the app must keep working with no internet and no account, but optional online features are now in the long-term plan.
> - New section: [[#Long-term requirements (proposed)]]. Some "Out of scope (for now)" items move into later phases; each is marked below.
> - Why: [[18 Plan Review 2026-11-13]] · Vision: [[19 Product Vision]]

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
- [x] **Profile**: editable name, picture, bio, **banners**, badges, showcase games ([[16 Profile and Friends]])
- [x] **Friends list**: proposed as **Steam friends, read-only** ([[16 Profile and Friends]])
- [x] **Shop**: library backgrounds, profile banners, badges, name changes. Proposed currency: **Stardust earned by playing** ([[17 Shop]])

**Tech**
- Rust, with an egui UI (chosen over Tauri and Slint/Iced; see [[12 Decisions Log]])
- Starting with **no prior Rust experience**

## Assumptions (correct these if wrong)
- **Windows only.**
- **Local only**: no GameLib accounts, no cloud sync. Everything lives on my PC. *(Exception: the friends list reads from Steam's API; see [[16 Profile and Friends]].)*
  → **Changed 13 Nov 2026 to local-first** for the long term: still true for v0.1 and for every core feature; optional online features come in Phase 4+.
- One user. *(Still true on the PC; online accounts are per person.)*

## Success criteria: "done" looks like
1. I open the app and see **my real games**, found automatically, with **cover art**.
2. I click Play and the game **launches**, whatever store it's from.
3. When I close the game, its **playtime and last-played date update**.
4. I can **search**, **favourite**, **tag**, and put games into **collections**.
5. The **stats page** shows total playtime and my most-played games.
6. The code is cleanly organised, with tests for the core logic, so it holds up as coursework.

## Out of scope (for now)
*(Annotated 13 Nov 2026 with where each item now sits in [[28 Expanded Development Roadmap]].)*
- Mac and Linux → still out; Linux is a Phase 6 *possibility* only
- Installing or uninstalling games, or buying games → still out. GameLib links to the store's own install/buy pages instead
- Achievements → 🔜 **Phase 1 (M14)**, where APIs allow (Steam first)
- Cloud saves → still out (stores already do this)
- Chat or messaging friends → still out; not planned
- GameLib's own online accounts or friend system → 🔭 **Phase 4**, optional
- **Real-money purchases** → still out, for the whole roadmap ([[17 Shop#Why no real money (for now)]], [[26 Progression and Stardust Economy]])
- Syncing between PCs → 🔭 **Phase 6** (cloud sync of user decisions)

## Long-term requirements (proposed)
> [!abstract] 🔭 Proposed 13 Nov 2026. Not built. Order and detail: [[28 Expanded Development Roadmap]].

### Non-negotiable constraints
- **Local-first:** library, launching, playtime, organising, mods and the local profile work with no internet and **no GameLib account**.
- **Windows-first**, Rust + egui, building on the existing code (no rewrite).
- **Complement stores, don't compete:** no storefront, no selling games/keys; launch through original launchers where needed.
- **Cosmetic-only rewards:** no real money, gambling or trading.
- **Respect provider terms:** official APIs where possible; no scraping behind logins; no storing other services' passwords.
- **Achievable solo:** every milestone ends in a working app; online/community features wait until the local product is strong.

### A. Universal library (Phase 1)
- [ ] One **canonical game** with several **installations** and **ownerships** ([[20 Game Identity and Installation Model]])
- [ ] Duplicate detection with user-confirmed merge and undo
- [ ] New stores through a provider registry: Ubisoft Connect 🟡, EA app 🔬, Xbox/Microsoft Store 🔬 ([[27 Future Integrations]])
- [ ] Owned-but-not-installed games from a linked Steam account
- [ ] Metadata (description, genres, dates) for **all** games, not only Steam (IGDB)
- [ ] Achievements where APIs permit; store-reported playtime shown beside GameLib's
- [ ] Status (backlog/playing/finished), list view, saved filters

### B. Modding hub (Phase 2)
- [ ] Browse mods (Thunderstore, Modrinth, Nexus) for games in the library
- [ ] Work *with* MO2/Vortex/Prism: detect them, show profiles, launch with a profile
- [ ] Native install/update/uninstall only for simple methods (BepInEx/Thunderstore, folder drop-in), with exact uninstall
- [ ] Mod profiles per installation; collections exported as files ([[22 Modding Architecture]])

### C. Discovery & progression, local (Phase 3)
- [ ] Wishlist, release tracking, price comparison across stores, "what to play next" ([[25 Game Discovery]])
- [ ] XP/levels, idle-filtered and capped rewards, themes, shareable profile card ([[26 Progression and Stardust Economy]])

### D. Identity (Phase 4, optional)
- [ ] GameLib accounts separate from linked store accounts; public/private profile sections; follows/friends; opt-in activity; block/report ([[23 Social Platform Architecture]])

### E. Communities (Phase 5)
- [ ] Game community pages, reviews, shared lists and mod collections, screenshots, guides, with moderation tools first

### Success criteria for the long-term plan
1. Someone with Steam, Epic and GOG installed opens GameLib instead of them, because it shows **one** entry per game with all their playtime.
2. Modding a supported game takes fewer steps in GameLib than without it, and never breaks the game install.
3. Nothing a user did locally is lost when online features arrive, and nothing is shared without them choosing to.
