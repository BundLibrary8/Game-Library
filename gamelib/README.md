# GameLib backend

The non-UI half of GameLib: one library for Steam, Epic, GOG, loose folders
and hand-added games, with launching, playtime tracking, cover art, stats,
a local profile, Steam friends, and the Stardust shop.

The window (`src/ui/`) follows the designs in
[`../plan/gamelib plan/`](../plan/gamelib%20plan/00%20Home.md) and has two
looks that share the same screens:

- **Ember** (the default): dark panels, amber accent, top bar and sidebar.
- **Constellation**: a star map behind glass panels, an icon rail, and an
  ice-blue to violet accent. It switches on while the Constellation library
  background from the Shop is equipped.

The same program also has command-line commands (`src/cli.rs`).

## Try it

```powershell
cargo run                              # open the window
cargo run -- help                      # list the command-line commands
cargo run -- scan                      # find Steam, Epic and GOG games
cargo run -- list
```

The first build takes several minutes; later ones are quick.

In the window: **Scan** finds launcher games, **+ Add game** adds one by
hand. Click a game to open its page; hover a cover and press the play
button to start it straight from the grid. Home, Library, Stats, Shop,
Profile and Settings are in the top bar and sidebar (or the icon rail).

To see a look without owning it, set the `GAMELIB_THEME` environment
variable to `constellation`, `aurora`, `synthwave` or `ember`:

```powershell
$env:GAMELIB_THEME = "constellation"; cargo run
```

Data is kept in `%APPDATA%\gamelib\` (`library.db`, `covers\`,
`gamelib.log`). Set the `GAMELIB_DATA_DIR` environment variable to use a
different folder while experimenting. If the window fails to open, try
setting `GAMELIB_RENDERER=glow`.

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
| `main.rs`, `cli.rs` | The program's entry point and the command-line commands |
| `ui/` | The window: `mod.rs` (state and the frame around every screen), `theme.rs`, `paint.rs`, one file per screen, and `dialogs.rs` |
| `assets/fonts/` | The Inter font (SIL Open Font License, see `LICENSE-Inter.txt`) |

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
- The window itself, and its file-picker dialogs (the window was run on Linux only)
- SteamGridDB, Steam Store and Steam Web API calls (the parsers are tested
  against sample responses; the live endpoints were never called)
