---
tags: [gamelib, design, architecture]
status: proposed
created: 2026-11-13
phase: 1
---
# 21 Provider Architecture

Back to [[00 Home]] · Current architecture: [[02 Architecture]] · Roadmap: [[28 Expanded Development Roadmap#Phase 1 – Universal Library]]

> [!todo] Status: 🔜 planned next (Phase 1, milestone M10)
> Describes how the code should grow so a new store, metadata source or mod site doesn't need changes all over the app. Nothing here is built. The existing `GameSource` trait ✅ is the starting point.

## Where we are (✅ as built)
- `sources/mod.rs` defines `trait GameSource { name, source, detect }`. Steam, Epic, GOG and folder scan implement it. **This is a good seed.**
- But `Source` is a **closed enum** (`Steam, Epic, Gog, Folder, Manual`). Adding Ubisoft means editing `model.rs`, `db/games.rs`, `db/settings.rs`, `sources/mod.rs` (`enabled_sources`), `ui/theme.rs` (name, badge text, colour), `ui/settings.rs`, `stats.rs`, `metadata.rs`, `worker.rs`: **13 files** reference `Source::` variants.
- Metadata is hard-wired: SteamGridDB for covers and the Steam Store for info, called directly from `worker.rs`/`metadata.rs`.
- Friends are hard-wired to the Steam Web API in `friends.rs`.

None of this is wrong for v0.1. It just won't scale to 10+ providers.

## Goal
> Adding a provider = **one new file** (or folder) implementing a trait + **one line** in the registry. The UI, database and worker don't change.

## Provider kinds (capability traits)
Each trait is small and synchronous (blocking), because providers run on worker threads, exactly like today. A provider can implement several traits (Steam: library + metadata + activity + social).

```rust
/// Every provider describes itself; the UI uses this instead of matching on an enum.
pub struct ProviderInfo {
    pub id: ProviderId,              // "steam", "gog", "ubisoft", "igdb", "nexus"…  (stored in the DB)
    pub display_name: &'static str,  // "Ubisoft Connect"
    pub badge: &'static str,         // "UBI"
    pub colour: [u8; 3],             // source dot colour
    pub needs: &'static [Requirement], // e.g. ApiKey("steam_web_api_key"), Account, InstalledClient
}

/// A. Stores and other places games come from.
pub trait LibraryProvider {
    fn info(&self) -> &ProviderInfo;
    /// Games installed on this PC (what `GameSource::detect` does today).
    fn detect_installed(&self, ctx: &ProviderCtx) -> Result<Vec<DetectedInstallation>>;
    /// Games owned on a linked account, installed or not. Default: not supported.
    fn list_owned(&self, _ctx: &ProviderCtx) -> Result<Option<Vec<DetectedOwnership>>> { Ok(None) }
    /// How to ask the store to install a game it owns (e.g. a steam://install link). Optional.
    fn install_uri(&self, _external_id: &str) -> Option<String> { None }
}

/// Descriptions, genres, dates, cross-store ids, artwork.
pub trait MetadataProvider {
    fn info(&self) -> &ProviderInfo;
    fn lookup(&self, ctx: &ProviderCtx, query: &MetadataQuery) -> Result<Option<GameMetadata>>;
    fn artwork(&self, _ctx: &ProviderCtx, _game: &MetadataQuery, _kind: ArtKind) -> Result<Vec<ArtCandidate>> { Ok(vec![]) }
}

/// Playtime / achievements reported by a store account.
pub trait ActivityProvider {
    fn info(&self) -> &ProviderInfo;
    fn playtime(&self, ctx: &ProviderCtx) -> Result<Vec<ProviderPlaytime>>;
    fn achievements(&self, _ctx: &ProviderCtx, _external_id: &str) -> Result<Option<Vec<Achievement>>> { Ok(None) }
}

/// Friends from an external network (Steam today).
pub trait SocialProvider {
    fn info(&self) -> &ProviderInfo;
    fn friends(&self, ctx: &ProviderCtx) -> Result<Vec<ExternalFriend>>;
}

// Modding traits (ModRepository, ModManagerIntegration, GameModAdapter) are in [[22 Modding Architecture]].
```

`ProviderCtx` carries what a provider may use: an HTTP client (with GameLib's User-Agent and per-provider rate limiter), its own settings/API key, a cache directory, and a cancellation flag. **Providers never get the database connection**: they return plain data and the core saves it. That keeps them testable with fixtures, as today's parsers are ✅.

### The registry
```rust
pub struct Providers {
    pub library:  Vec<Box<dyn LibraryProvider + Send + Sync>>,
    pub metadata: Vec<Box<dyn MetadataProvider + Send + Sync>>,
    pub activity: Vec<Box<dyn ActivityProvider + Send + Sync>>,
    pub social:   Vec<Box<dyn SocialProvider + Send + Sync>>,
    // + mod repositories, mod managers, game mod adapters (Phase 2)
}

pub fn builtin_providers() -> Providers { /* the one place every provider is listed */ }
```
- Settings shows one row per provider from `info()`, with enable/disable and any API key fields from `needs`.
- Theme colours and badges come from `info()`, so `ui/theme.rs` stops matching on `Source`.
- `ProviderId` is a `&'static str` newtype stored as text (the DB already stores `'steam'`, `'epic'`… ✅, so **no data migration** for this change).

### Rate limits and politeness (shared, not per provider)
- A small **token-bucket limiter per provider** in `ProviderCtx` (e.g. IGDB 4 req/s, Modrinth 300/min, Nexus daily+hourly quotas, Steam ~100k/day). Today there's a fixed 500 ms pause ✅ (`metadata::REQUEST_PAUSE`); this generalises it.
- A **response cache** on disk with per-provider TTL (metadata: weeks; prices: hours; friends: 5 min as today ✅).
- All requests send a descriptive `User-Agent` (Modrinth requires one).

## Migration path from today's code (small steps, each a working app)
1. Add `ProviderInfo` and have the existing four sources return one. UI reads names/colours from it. (`Source` enum still exists.)
2. Rename `GameSource` → `LibraryProvider` with a default `list_owned`. Implement a `builtin_providers()` registry; `enabled_sources()` filters it.
3. Replace `Source` with `ProviderId` in `model.rs` and the DB layer. `Folder` and `Manual` become providers too (`"folder"`, `"manual"`), with no network access.
4. Move SteamGridDB and Steam Store into `MetadataProvider` implementations; the cover thread iterates metadata providers in priority order.
5. Move Steam friends into a `SocialProvider`.

Each step keeps all 148 tests ✅ passing, plus new tests for the registry.

## Single crate or workspace?
**Today (✅):** one crate with a library (`lib.rs`) and a binary (`main.rs`). The UI is already isolated in `src/ui/`. That already gives most of the benefit of a split.

**Recommendation:** convert to a **Cargo workspace at the start of Phase 1 (M10)**, with few crates:

```
gamelib/                      (workspace root, Cargo.toml with [workspace])
├─ crates/
│  ├─ gamelib-core/           model, db + migrations, providers traits + built-in providers,
│  │                          playtime, launcher, stats, profile, shop/progression, worker
│  └─ gamelib-desktop/        eframe/egui app + main.rs + cli.rs + build.rs (icon)
│  (later)
│  ├─ gamelib-mods/           Phase 2, only if mod code gets large or pulls heavy deps (archives, 7z)
│  ├─ gamelib-api-types/      Phase 4: serde types shared by desktop and server
│  └─ gamelib-server/         Phase 4: the backend, if self-hosted
```
| Benefit | Real for us? |
|---|---|
| Faster rebuilds: changing UI code doesn't recompile core, and core tests don't build egui | **Yes**: egui/eframe/wgpu are the slowest dependencies |
| Compiler-enforced "core never uses UI" | Yes (currently only a convention) |
| A backend can reuse the API types without pulling in egui or Windows-only crates | Yes, from Phase 4 |
| Cost | `Cargo.toml` paths, `pub` visibility fixes, moving `tests/`; about one session of work |

**Don't** split per provider (one crate per store). That is ceremony without benefit for a solo project. Use Cargo **features** for optional heavy providers instead (e.g. `xbox` pulling in the `windows` crate).

## A formal plugin system? Not yet.
| Option | What it is | Verdict |
|---|---|---|
| **Compile-time providers** (above) | Traits + a registry in the core | ✅ **Do this.** Gives 90% of the extensibility, none of the risk |
| Dynamic libraries (`.dll` plugins) | Load Rust/C plugins at runtime | ❌ Rust has no stable ABI; unsafe; version hell |
| **WASM plugins** (e.g. wasmtime / Extism) | Third-party providers in a sandbox | 🔭 Future possibility (Phase 6) if outside contributors appear. Sandbox is good for security; host API design is a big job |
| **Out-of-process plugins** (JSON over stdin/stdout) | A provider is any exe speaking a small protocol | 🔭 Future possibility: simplest for community providers written in any language |
| Scripting (Lua/Rhai) | Embedded scripts | ❌ Not needed |

Decision: **revisit after Phase 3**, only if there's demand from other people. Design the traits so they *could* be exposed over a protocol later (plain data in, plain data out, no DB access): that's the cheap insurance.

## What stays the same
- egui UI thread + worker threads + channels ✅ ([[02 Architecture]]): providers run inside the existing worker/cover threads.
- Blocking `reqwest` ✅: no need for async/tokio in the desktop app. Async only becomes interesting for the server (Phase 4).
- SQLite with one connection per thread and WAL ✅.
