---
tags: [gamelib, changes]
date: 2026-11-13
status: record
---
# 29 Code Change Notes (12–13 Nov 2026)

Back to [[00 Home]] · Milestones: [[09 Milestones]]

Everything changed in the **code** (`gamelib/`) since it was pulled from GitHub, in the order it was done. **None of it is committed yet.** Changes to the plan itself are recorded separately in [[18 Plan Review 2026-11-13]].

> [!info] State after these changes
> - `cargo test`: **148 tests pass** (132 before, one of them failing on Windows). `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.
> - `cargo build --release` makes one `gamelib.exe` (about 20 MB).
> - Checked by running the real app on this PC: the window, the icon, the remembered size, the start-up error box, every screen at three window sizes in both looks, and Epic detection finding Fortnite.

## 1. M8 polish

| Change | Where | Notes |
|---|---|---|
| **App icon** | `assets/icon/` (new), `ui/window.rs` (new), `build.rs` (new) | Amber square with a play triangle. Shown in the title bar and taskbar, and embedded in the exe. Drawn by `assets/icon/make_icon.py`, so it can be regenerated. If the resource compiler is missing the build still works and only the exe's file icon is lost. |
| **Window size remembered** | `ui/window.rs`, `ui/mod.rs` | Saved as the `ui.window` setting when the app closes, e.g. `1500x950` or `1500x950 maximized`. Odd saved values are ignored. The position is deliberately not saved (it could put the window off-screen). |
| **Release build is a window program** | `main.rs`, `Cargo.toml` | No console window behind the app. If it can't start, a message box says why. Terminal commands still work from the same exe, but the terminal doesn't wait for them. Debug builds (`cargo run`) keep the console. |
| **Smaller release exe** | `Cargo.toml` | `[profile.release]` with thin LTO, one codegen unit and stripped symbols. |
| **Log file capped** | `cli.rs` | Over about 1 MB it is set aside as `gamelib.log.old` and a new one started. |
| **README rewritten** | `README.md`, `docs/screenshots/` (new) | Features, five screenshots of the real app with a made-up library, how to build the finished program, an architecture diagram, what has and hasn't been checked on Windows, known limits. |

## 2. Problems found by reading the code, and fixed

| Problem | Fix | Where |
|---|---|---|
| **The Steam API key could end up in the log file and status bar.** It is part of the web address, and error messages quote the address. | The address is stripped from those errors. | `friends.rs` |
| **Downloading covers held everything else up.** The batch ran on the one worker thread, so Play had to wait for it, and closing the window left the program running until it finished. | Cover batches run on their own thread and stop when the app closes. A second request while one is running makes it go round once more. | `worker.rs` |
| **A terminal command could end a game session that the window was still tracking**, losing the rest of the playtime. | Crash recovery leaves alone any session active in the last 3 minutes. | `db/sessions.rs` |
| **A failed launch marked every running game as "not running".** | The error now says which game it is about; only that one is cleared. | `worker.rs`, `ui/mod.rs` |
| **A cover download had no size limit while downloading**, could save an error page as an image, and could leave a half-written file. | The limit applies while reading; the file must be a real JPEG, PNG or WebP; it is saved under a temporary name and renamed. | `metadata.rs` |
| **Scans saved hundreds of games one at a time.** | One database transaction per store: faster, and never half-saved. Added `Db::transaction`. | `db/mod.rs`, `db/games.rs`, `sources/mod.rs` |
| **Slow saving in general.** | SQLite's `synchronous = NORMAL`, the recommended partner to WAL mode. | `db/mod.rs` |
| **The game page and the stats page asked the database the same questions every frame** (up to 60 times a second). | Loaded once, reused until something changes. | `ui/details.rs`, `ui/stats.rs`, `ui/mod.rs` |
| **Epic's folder was assumed to be on `C:`.** | Uses the `ProgramData` environment variable. | `sources/epic.rs` |
| **Re-picking the profile picture already in use could empty the file** (a file copied onto itself). | Detected and skipped. | `profile.rs` |
| **One test failed on Windows only** (it compared paths as text, `/` against `\`). | Test fixed. | `sources/folder.rs` |
| **One test had a timing race** (it checked a counter a moment too early). | Test waits for it. | `worker.rs` |

## 3. Problems found by running the app on Windows

### The Stardust star ✦ was an empty box
Neither the app's font (Inter) nor egui's built-in fonts contain ✦, so every price and balance showed a box. **Fix:** Windows' own symbol font (Segoe UI Symbol) is added as a last fallback. → `ui/theme.rs`

### Epic games weren't found (Fortnite)
**Cause:** the Epic launcher normally writes one `.item` file per installed game into `C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests`. On this PC that folder doesn't exist: Epic's own log repeats *"Failed to save install info …\Manifests\….item"*. So the launcher never recorded Fortnite on disk, and GameLib, which only looked there, saw nothing.

**Fix:** a second way of finding Epic games, used for any game the manifests don't cover. → `sources/epic_folders.rs` (new), `sources/epic.rs`
1. Look in Epic's usual library folders (`Program Files\Epic Games`, and `Epic Games` at the top of any drive) for folders with a hidden `.egstore` inside. Every Epic install has one.
2. Read the start of the build manifest in `.egstore` to get the exe the game runs.
3. Look the game up in the launcher's own settings file (`%LOCALAPPDATA%\EpicGamesLauncher\Saved\Config\…\GameUserSettings.ini`) to get the three ids its launch link needs.
4. If it's found there, the game starts through Epic like any other (so signing in and online play work). If not, GameLib runs the exe directly.

**Result on this PC:** a scan now finds Fortnite with the link `com.epicgames.launcher://apps/fn%3A…%3AFortnite?action=launch&silent=true`. "Epic Online Services" has an `.egstore` too but isn't a game, so it is skipped by name. If the launcher ever does write its manifest, the game isn't duplicated (same id).

> [!warning] Not checked yet
> That pressing Play on Fortnite actually starts it, and that its playtime is recorded. The link is built the same way as for a normal Epic game, but I haven't launched it.

Added `flate2` as a dependency (the manifest is zlib-compressed; it was already in the build through other crates).

## 4. UI review (13 Nov 2026)

Every screen was captured from the running app at **980×620** (the smallest the window goes), **1360×850** (the default) and **1920×1050**, in Ember and Constellation, and looked at for things that overlap, get cut off, repeat each other or do nothing.

### Layouts that broke in a small window
| Screen | What was wrong | Now |
|---|---|---|
| **Home** | "Continue playing" always drew 3 tiles: titles were cut to "Starfall (" and the Play buttons hung over the tile edges. | As many tiles as fit (1–3). |
| **Library** | The filter chips ran under the Sort box and off the edge. | See "Removed" below; in Constellation they wrap. |
| **Game page** | The four stat tiles were squeezed to "PLAYTIM E / 112…" and the last ran off the edge. | Tiles go two per row when there isn't room for one row. Title text is smaller in a narrow column. |
| **Stats** | The right-hand panels ran off the edge. | Two columns when there's room, otherwise one under the other. Bar widths are worked out from the space there is. |
| **Profile** | The banner took 250 px of a 620 px window; stat tiles cut "Ember R…". | Banner height follows the window (130–250 px). Tiles wrap. Showcase and Friends stack when narrow. |
| **Shop** | Category chips ran off the edge; the "How you earn" panel ran off the edge with its text overlapping; the Equip button sat on top of the description. | Chips wrap. The two panels stack when narrow. The description is left off a narrow card. |

### Layouts that looked off on a big window
| Screen | What was wrong | Now |
|---|---|---|
| **Home** | Three tiles stretched into long, mostly empty bars. | Tiles stop growing at 520 px. |
| **Game page** | A huge cover, a short column of text beside it, then a big gap; the chart panel didn't reach the right edge. | Smaller cover (170–300 px). The chart and tags moved up into the column beside the cover, filling the gap. |
| **Settings** | Four panels stretched across 1,600 px with a few short lines at the left of each. | A readable column, at most 860 px wide. |

### Removed: repeated, fake or low-value controls
| Removed | Why |
|---|---|
| **Filter chips on the Library screen (Ember)** | Every one (All, Installed, Favourites, each collection) is already in the sidebar next to it. Constellation has no sidebar, so it keeps them, and gained "Recently played" and "Hidden", which it had no way to reach before. |
| **"Change banner" button on the profile** | It opened the same window as "Edit profile" beside it. |
| **"Rescan now" in Settings** | Same as the Scan button that is always in the top bar. |
| **"Download missing covers" in Settings** | Saving a cover art key now does it; a scan already did. |
| **"Earn Stardust by playing ›" in the Shop** | Looked like a link but did nothing. The panel under it explains how Stardust is earned. |
| **The second Stardust balance in the Shop header (Ember)** | The top bar shows it a few centimetres above. Constellation, with no top bar, keeps it. |
| **"Source" tile on the game page** | The badge on the cover already says STEAM / EPIC / GOG. |
| **"Library size" tile on Stats** | The status bar already shows the game count. Replaced by **Sessions**, which wasn't shown anywhere. |
| **"N sources" and "Covers 3/16" in the status bar** | Noise. The bar is now the status on the left and the game count on the right. |
| **"Pick up to 4 games" on a full showcase** | Only shown while there's an empty slot. |

### Wording and small things
- **Scan result** reads "Scan finished: 3 new games" (or "no new games", plus "N no longer installed" if any) instead of "3 new, 10 updated, 0 no longer installed; GOG not found". A store that isn't installed is normal, not news; it's still in the log.
- **Shop rewards** read "✦ 10" instead of "+ ✦ 10".
- **Tick boxes** are a little bigger.
- **Settings** has a line saying what the store tick boxes are for, and its rows sit closer together.
- **Top bar** leaves less empty space before Home / Library / Stats / Shop in a narrow window.
- `gamelib ui game:12` opens game 12's page directly (used for taking the screenshots).

### Looked at and left alone
- **The sidebar** (Ember). It is long, but each row is a real filter with a count, and it scrolls.
- **The ⋯ menu on a game page.** Nine actions, all different, grouped by separators.
- **In Constellation**, the featured shop card has a dark outline where its corners are rounded off against the starry background. Cosmetic; needs the card drawn differently.
- **The pop-up windows** (add game, import review, edit profile, cover search, rename, new tag). I couldn't screenshot these automatically, so they were not reviewed.

## 5. Things other code relies on that changed
For anyone reading old notes or writing new code against these:
- `Db::recover_unfinished_sessions` now takes the current time: `recover_unfinished_sessions(now)`.
- `WorkerResult::Error` has a third field, `game_id: Option<i64>`.
- `ScanReport::summary()` returns different text, and no longer mentions stores that failed.
- `EpicSource` has two more fields, `library_dirs` and `settings_files`.
- New public items: `Db::transaction`, `db::STALE_SESSION_SECS`, `metadata::check_cover_bytes`, `sources::epic::default_manifests_dir` (replaces the `DEFAULT_MANIFESTS_DIR` constant), the `sources::epic_folders` module.

## 6. Still not done
- [ ] **Nothing is committed** (see section 7: Git needs a name and email first).
- [ ] **Play Fortnite from GameLib** and check it starts and its time is recorded.
- [ ] **Terminal commands from the release exe** in a real console window (they work when output is piped; the prompt may come back before the output).
- [ ] **Review the pop-up windows** the same way as the screens.
- [ ] The **coursework write-up** ([[13 Dev Log Template]]). This note and the "Found and fixed" list in [[09 Milestones]] are raw material for it.
- [ ] The `READ` file with the insult is still in the repo.

## 7. Merged in: the newer version from GitHub (13 Nov 2026, later)

> [!important] This section changes some of what sections 4 and 6 say
> The GitHub branch `claude/nice-feynman-kjrf7j` had two commits that never reached `main`, made about half an hour before the version that was pulled. They are now merged with everything above. **The top bar and sidebar that section 4 describes no longer exist**; the fixes from section 4 that still apply were carried over to the new layout.

### What the merged-in commits brought
| Change | Notes |
|---|---|
| **One layout for every look** | An animated background behind glass panels, with the icon rail on the left. The old top-bar-and-sidebar layout is gone. Its extra filters (recently played, hidden, sources, tags, new collection) are in a **More** menu in the library header. |
| **Constellation is the default** and free, as is **Ember** (now a background of its own, with drifting sparks). **Aurora** is the shop's featured item; **Synthwave** is the other one for sale. |
| **Animated backgrounds** | Twinkling and shooting stars, drifting northern lights, a rolling neon grid, with one "Animated backgrounds" switch in Settings (replaces the old twinkle switch). |
| **Artwork without any key** | New `artwork.rs`. Steam covers are copied from Steam's own library cache or its public image server; each game's icon is read out of its exe and shown on its cover. New `icons\` folder and `icon_path` column (database schema v3). |
| **The ✦ symbol is drawn** in the places that matter (balance, prices) instead of typed. The symbol-font fallback from section 3 is kept for the places that still type it (status messages). |

### How the two were combined
- **Backend:** the artwork steps (Steam cache, exe icons, Steam's image server, then SteamGridDB) now run **inside the separate cover thread** from section 2, so they don't hold up Play and they stop when the app closes. All the other backend fixes merged without conflict.
- **Library header:** the newer chips and **More** menu, made to **wrap** onto a second line in a narrow window (they ran under the Sort box).
- **Game page, Home, Stats, Profile, Shop, Settings:** today's layouts (wrapping stat tiles, chart beside the cover, stacking panels, capped Settings width) on the new look.
- **Shop header:** keeps the Stardust balance (there is no top bar to show it any more); the fake "Earn Stardust by playing ›" link stays removed.

### Dropped, because the thing they fixed is gone
- Removing the Library filter chips in Ember (there is no sidebar to duplicate now).
- The narrower top-bar spacing, and "Constellation-only" versions of the chips and the Shop balance.

### Fixed after the merge, found by looking at the new layout
| Problem | Fix |
|---|---|
| **Profile banner** ran edge to edge and faded into a flat dark colour, leaving a hard line across the starry background. | It is now a rounded card lined up with the panels below. |
| **Shop pictures had a dark frame** (the wart noted in section 4). | Only the four corner slivers are covered now, not a thick outline. |
| **Settings panels ran off the right edge** in a small window. | The column is never wider than the space there is. |

### Checked on this PC after the merge
- **157 tests pass**; clippy and fmt are clean; the release build works.
- Every screen captured again at 980×620, 1360×850 and 1920×1050.
- **Keyless artwork works for real:** in a scratch data folder, a scan plus one artwork run gave **6 of 9 Steam covers, 8 icons, and 9 descriptions from the live Steam Store** (so that web service is now verified). Fortnite got an icon but no cover (it would need a SteamGridDB key).

### State of Git
- The merge was done **in the working folder, without commits**, because Git has no name or email set on this PC. The branch is `merge-new-look`; `main` is untouched.
- A backup of the work from before the merge is kept as a Git stash ("today's work before merging the new look").
- **To do:** set a Git name and email, then commit. The two GitHub commits can be kept as real history by merging the branch properly at that point.
