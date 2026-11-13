---
tags: [gamelib, design, infrastructure]
status: proposed
created: 2026-11-13
phase: 4+
---
# 24 Backend and Cloud Infrastructure

Back to [[00 Home]] · Social: [[23 Social Platform Architecture]] · Data: [[03 Data Model#Local vs online data]] · Roadmap: [[28 Expanded Development Roadmap#Phase 4 – GameLib Identity (first backend)]]

> [!abstract] Status: 🔭 future; **no backend is needed until Phase 4**
> Phases 0–3 (library, mods, local discovery, Stardust v2) run entirely on the PC. The existing app has no server and doesn't need one. This page is about choosing one **when** accounts arrive, without paying for anything up front.

## Technology review: what stays and why
| Technology | Verdict | Reasoning |
|---|---|---|
| **Rust** | ✅ Keep | Already 13.8k lines with 148 tests ✅. Good for long-running background work, Windows APIs, safe parsing of untrusted files (manifests, mod archives). Also a solid choice for a backend (axum) if self-hosting |
| **egui / eframe** | ✅ Keep | Built UI matches the designs; immediate mode keeps state simple. Limits to watch: **rich text/Markdown** (guides, reviews) and **long scrolling feeds** need care (virtualised lists via `ScrollArea::show_rows`; `egui_commonmark` for Markdown 🔬 evaluate), **accessibility** (AccessKit support exists in egui; 🔬 test with Narrator), **embedded web content** (none; open the browser instead). None justify a rewrite |
| **SQLite (rusqlite, bundled)** | ✅ Keep for the desktop | Perfect for a local-first single-user library. WAL + one connection per thread already ✅. Will comfortably hold 10k+ games, sessions and mod manifests |
| **Blocking reqwest on worker threads** | ✅ Keep for the desktop | No need for an async runtime in the app. Add the per-provider rate limiter + disk cache ([[21 Provider Architecture#Rate limits and politeness (shared, not per provider)]]) |
| **Existing providers** (steamlocate, Epic manifests + `.egstore` fallback, GOG registry, SteamGridDB, Steam Store, Steam Web API) | ✅ Keep, move behind traits | Already tested against fixtures ✅; still need live checks (Phase 0) |
| **Windows Credential Manager** for secrets | 🔜 Add | Today API keys are plain text in `library.db` ([[12 Decisions Log#Still open]]) |

## When a backend becomes necessary
Only these features truly need one: GameLib accounts, public profiles, follows/friends between GameLib users, activity feeds, reviews/lists/guides/screenshots shared with others, cloud sync between PCs, a mobile companion. Price comparison and IGDB metadata **don't** (they can be called from the desktop with the user's own key or a free key).

## Backend options (all have a free or very cheap start)
| Option | What you get | Free tier / cost | Pros | Cons |
|---|---|---|---|---|
| **A. Supabase** (hosted, open source) | Postgres, auth (email/OAuth), storage, row-level security, auto REST API | Researched Nov 2026 (third-party summaries; confirm on supabase.com/pricing): 2 active free projects, **500 MB DB**, **1 GB file storage**, **5 GB egress/month**, **50k monthly active users**, **projects pause after 7 days without activity** | Fastest to working accounts; auth done properly by people who specialise in it; can be **self-hosted later** (open source) | Not Rust; learning RLS; pause on inactivity during development; egress cap matters for images |
| **B. PocketBase** (self-hosted, single Go binary) | SQLite-backed DB, auth, file storage, admin UI, realtime | Free software; needs a host (a free-tier VM or ~£4–5/month VPS) | One file to run; simple; SQLite like the desktop | You run the server (updates, backups); scaling limits (fine for small) |
| **C. Own Rust server** (`axum` + Postgres or SQLite) | Exactly what you write | Same hosting as B | Most Rust learning; shares types with the desktop (`gamelib-api-types` crate) | You write auth/session security yourself (highest risk), plus everything else |
| **D. Static/Git-based sharing** (no server) | Profile cards & lists as files, shared via GitHub Gist/Pages | Free | Zero ops | No social graph; only covers "share" use cases |

**Recommendation (to confirm):** start with **D in Phase 3** (shareable profile card and list files, no server) → **spike A vs B** at the start of Phase 4 (milestone M23, one week each at most, judged on: auth flow from a desktop app, privacy rules, image upload, cost at 100 and 10,000 users) → pick one. Keep the desktop talking to the backend through a single `gamelib-api` client module so the backend can change later. Option C only for small Rust services *beside* A/B if needed (e.g. a metadata proxy).

## Authentication
- Desktop sign-in through the system browser (OAuth 2.0 Authorization Code + **PKCE**, loopback redirect `http://127.0.0.1:<port>/callback`). No embedded web view.
- Refresh token in Windows Credential Manager; access token in memory.
- Steam linking through "Sign in through Steam" (OpenID 2.0) handled **by the backend**, which records the verified SteamID.

## Media storage
| Need | Plan | Free/low-cost option |
|---|---|---|
| Covers/artwork for *your* library | Stays local ✅ (`%APPDATA%\gamelib\covers`) | — |
| Avatars, banners (published) | Resize to fixed sizes server-side (e.g. 256 px avatar, 1500×500 banner), WebP | Supabase/PocketBase storage; or an S3-compatible bucket with free egress such as Cloudflare R2 (🔬 confirm current free allowance) |
| Screenshots (Phase 5b) | Size cap (e.g. 5 MB), resize, strip EXIF location data | Same; monitor quota |
| Clips/video | **Link out** (YouTube, Medal…). Don't host | — |
| Mod files | **Never hosted.** Links to the repository | — |

## Cloud sync (Phase 6)
- Sync **user decisions**, not files: favourites, tags, collections, status, notes, mod profile definitions (JSON), wishlist, profile. Not covers (re-downloadable), not installations (PC-specific), not raw logs.
- Each syncable row gets a stable `uuid` and `updated_at` (add these columns early, in schema v3, so sync isn't a painful migration later: see [[03 Data Model#Proposed schema v3+ (not built)]]).
- Conflict rule: last-writer-wins per field with a change log; tags/collections merge as sets.
- Sessions sync as append-only events (they never change once closed ✅ `closed` flag), so totals are recomputed, never merged.

## Provider rules, limits and data ownership
- **Steam Web API:** Terms of Use cap at **100,000 calls/day** per key, with conditions on use of the data. A public GameLib must use **each user's own key or a GameLib server key under Valve's terms** 🔬 read the terms before Phase 4.
- **IGDB:** free with a Twitch developer app; **4 requests/second**; commercial use reportedly needs a partnership and attribution 🔬. A public GameLib should call IGDB through a **small server-side proxy with caching** so the client secret isn't in the exe.
- **Nexus:** public apps must be registered ([[22 Modding Architecture#Provider facts and restrictions (researched 13 Nov 2026)]]).
- **Data ownership:** the user's library and play history belong to the user. Export everything to JSON at any time (local) and from the server (account). No selling or sharing of data with third parties.

## Costs ceiling for the solo developer
Target **£0/month** through Phase 4's private beta; at most a cheap VPS (~£5/month) if self-hosting. Anything beyond that (CDN, image moderation services, paid DB tier) is a **decision to log** in [[12 Decisions Log]] with the free alternative written next to it.

## Out of scope until explicitly decided
- Kubernetes, microservices, message queues, multi-region anything.
- Payments, ads, analytics that track users. (Crash reporting only if opt-in.)
