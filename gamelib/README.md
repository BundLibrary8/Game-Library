# GameLib backend

The non-UI half of GameLib: one library for Steam, Epic, GOG, loose folders
and hand-added games, with launching, playtime tracking, cover art, stats,
a local profile, Steam friends, and the Stardust shop.

The egui window isn't built yet. Until it is, a small command-line tool
(`src/main.rs`) lets you try everything. The design lives in
[`../plan/gamelib plan/`](../plan/gamelib%20plan/00%20Home.md).

## Try it

```powershell
cargo run -- help
cargo run -- scan                      # find Steam, Epic and GOG games
cargo run -- scan-folder D:\Games      # find loose games
cargo run -- list
cargo run -- launch 1                  # start game 1 and track playtime
cargo run -- stats
```

Data is kept in `%APPDATA%\gamelib\` (`library.db`, `covers\`,
`gamelib.log`). Set the `GAMELIB_DATA_DIR` environment variable to use a
different folder while experimenting.

## Checks

```powershell
cargo test       # all the tests
cargo clippy --all-targets -- -D warnings
cargo fmt
```

## How the code is organised

| File | What it does |
|---|---|
| `model.rs` | Core types: `Game`, `Source`, `LaunchTarget`, `DetectedGame` |
| `db/` | SQLite storage: schema and upgrades (`mod.rs`), `games`, `tags`, `sessions`, `settings` |
| `sources/` | The `GameSource` trait and one file each for Steam, Epic, GOG and folder scan |
| `launcher.rs` | Starts a game (an exe, or a `steam://` / Epic link) |
| `playtime.rs` | Decides when a game is running; the pure `step` function plus the tracking loop |
| `metadata.rs` | Covers from SteamGridDB, info from the Steam Store |
| `stats.rs` | Totals, most played, playtime per day |
| `profile.rs` | Name, bio, banner, badges, showcase, earned banners |
| `friends.rs` | Read-only Steam friends list, with a cache |
| `shop.rs` | Stardust ledger, rewards, catalogue, buying and equipping |
| `worker.rs` | The background thread: the UI sends a `Job`, gets a `WorkerResult` back |
| `paths.rs` | Where files live, and path comparison helpers |

The UI will only need `Db` (for quick reads and edits) and `Worker` (for
anything slow). See the example at the top of `worker.rs`.

## Where this differs from the plan notes

- **Extra columns in `games`:** `installed` (for "not installed" games),
  `cover_manual` (so a hand-picked cover is never overwritten) and
  `launch_working_dir`. `play_sessions` has `closed`, so a crash can't
  make a session count twice.
- **Launcher links are opened with `rundll32 url.dll,FileProtocolHandler`**
  rather than `cmd /C start`, because `cmd` breaks on the `&` in Epic's links.
- **A session ends at the moment the game was last seen**, not 15 seconds
  later, so the "is it really closed?" wait isn't counted as playtime.
- **Name changes are credits:** buying one in the shop adds a credit, and
  renaming spends it.
- **Days are counted in UTC** for the daily reward and the per-day chart.

## Not yet checked on Windows

This was written and tested on Linux. These parts only do real work on
Windows and need the manual checklist on a real PC:

- Steam detection against a real Steam install (tested here with a fake one)
- Epic detection against real `.item` files (tested with sample files)
- GOG registry reading (compiles for Windows; never run)
- Opening `steam://` and Epic links
- SteamGridDB, Steam Store and Steam Web API calls (the parsers are tested
  against sample responses; the live endpoints were never called)
