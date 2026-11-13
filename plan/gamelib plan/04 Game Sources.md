---
tags: [gamelib, design]
status: built (live checks pending) + planned sources
section: 3
updated: 2026-11-13
---
# 04 Game Sources (Section 3)

Back to [[00 Home]] · Prev: [[03 Data Model]] · Next: [[05 Launching and Playtime]]

> [!info] Plan review, 13 Nov 2026: what changed on this page
> - Was marked "draft"; everything in the original sections below is **built** ✅ (code + fixture tests). Live checks still pending: **GOG** registry and **Epic launch links** ([[09 Milestones]]).
> - Added: [[#Epic fallback: finding games by their folders (built, uncommitted)]], which was in the code but not in the plan.
> - Added: [[#Planned sources (Phase 1)]] and a pointer to the full research in [[27 Future Integrations]].
> - The trait below becomes `LibraryProvider` in Phase 1 ([[21 Provider Architecture]]). Change notes: [[18 Plan Review 2026-11-13]].

> [!warning] Original note (still true)
> The file locations below are where these launchers normally store data. **Check them on your own PC** before relying on them.

## One trait, many sources
```rust
pub struct DetectedGame {
    pub title: String,
    pub source: Source,
    pub source_id: String,
    pub install_dir: Option<PathBuf>,
    pub launch: LaunchTarget,
}

pub trait GameSource {
    fn name(&self) -> &str;
    fn detect(&self) -> anyhow::Result<Vec<DetectedGame>>;
}
```
"Scan all" loops over every enabled source. **If one source fails (e.g. Epic isn't installed), the others still run**, and the failure appears in the status bar.

## Steam (`steam.rs`)
- Use the **`steamlocate`** crate. It finds the Steam install, reads `libraryfolders.vdf` to get every library (C:, D:…), and reads each `appmanifest_<appid>.acf`.
- Gives: app id, name, install folder (`steamapps\common\<installdir>`).
- Skip things that aren't games, such as "Steamworks Common Redistributables" and Proton/SDK tools. Keep a small skip-list of app ids or names.
- Launch: `Uri("steam://rungameid/<appid>")`.

## Epic Games (`epic.rs`)
- Read every `*.item` file in `C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests\`. Each one is **JSON**.
- Useful fields: `DisplayName`, `InstallLocation`, `LaunchExecutable`, `AppName`, `CatalogNamespace`, `CatalogItemId`.
- Parse with `serde` into a struct containing only the fields we need.
- Launch through Epic so that online games and DRM work:
  `com.epicgames.launcher://apps/<CatalogNamespace>%3A<CatalogItemId>%3A<AppName>?action=launch&silent=true`
- A good first parser to write tests for, using a sample `.item` file in `tests/fixtures/`.

## GOG (`gog.rs`)
- Read the registry under `HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\GOG.com\Games\`, with one subkey per game, using the **`winreg`** crate.
- Useful values: `gameName`, `gameID`, `path`, `exe`, `launchParam`/`launchCommand`.
- Launch: `Exe { path: exe, working_dir: path }`. GOG games are usually DRM-free, so they run directly.

## Folder scan (`folder.rs`)
- The user adds folders in Settings (e.g. `D:\Games`).
- Each **top-level subfolder = one game**. Look inside it, up to 3 levels deep with `walkdir`, for `.exe` files.
- **Ignore junk executables:** names containing `unins`, `setup`, `install`, `vc_redist`, `dxsetup`, `crash`, `UnityCrashHandler`, `launcher_helper`, etc.
- Pick the most likely exe: the one whose name matches the folder name, otherwise the largest.
- **Skip folders that are already a Steam, Epic or GOG install**, so games don't appear twice.
- Results go to an **import review screen** where you tick which ones to add and can fix titles and exes. Folder scan guesses, so a person confirms. See [[07 UI Screens]].

## Manual add
- An "Add game" button opens a native file picker (`rfd`). Choose the `.exe`, type a title, and it's saved as `Source::Manual` with `source_id` set to the exe path.

## Rescanning
- Runs on the worker thread (see [[02 Architecture]]).
- Updates existing rows by `(source, source_id)` and adds new ones. Games that have disappeared are marked "not installed" and are never deleted. See [[03 Data Model]].
- Option: a "scan on startup" setting.

## Epic fallback: finding games by their folders (built, uncommitted)
✅ in `sources/epic_folders.rs` (617 lines with tests), added while testing on my PC; **not yet committed** to Git.
- **Why:** on some PCs the Epic launcher never writes the `.item` manifests (its log says "Failed to save install info"), so the manifest scan finds nothing even though games are installed.
- **How:** every Epic install has a hidden `.egstore` folder with the binary **build manifest**. The fallback (1) looks for `.egstore` folders in the usual Epic library folders, (2) reads the launch exe from the manifest header (with strict size limits, errors instead of panics), (3) looks the game up in the launcher's own settings `.ini` to get the namespace/item id/app name for the launch link.
- If step 3 finds nothing, the game is launched by running its exe directly.
- Results are merged with the manifest scan so a game found both ways appears once.
- Tests: corrupt manifests, the ini parser, matching. 🔬 To verify live: that launching via the exe still works for games that need the Epic launcher for DRM/online.

## Planned sources (Phase 1)
> [!todo] 🔜 Planned in [[28 Expanded Development Roadmap#M13: More stores (M, plus time-boxed spikes)]]. Not built. Confidence levels and sources in [[27 Future Integrations#Stores and libraries]].

| Source | How (expected) | Confidence | Plan |
|---|---|---|---|
| **Ubisoft Connect** | Registry `HKLM\SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\<id>` → install dir; launch `uplay://launch/<id>/0` | 🟡 community-documented | Build in M13 |
| **Installed programs** | Uninstall registry entries + Start Menu shortcuts → the existing **import review** screen (guesses, so a person confirms) | 🟡 | Build in M13 |
| **EA app** | `ProgramData\EA Desktop\InstallData`; launch URI unclear | 🔬 | 2-session spike; fallback = folder scan |
| **Xbox / Microsoft Store** | Windows `PackageManager` API, `XboxGames` folders, `shell:AppsFolder` launch | 🔬 | 2-session spike; playtime tracking may not work for `WindowsApps` |
| **GOG Galaxy database** | Read-only `galaxy-2.0.db` (all Galaxy integrations) | 🔬 undocumented | Spike; "experimental" only |
| **Steam owned games** | Steam Web API `GetOwnedGames` | 🟢 official | M14: owned-not-installed + Steam playtime |
| Battle.net, Amazon, itch.io | Local files/DBs | 🔬 | Later (Phase 6) |

With the Phase 1 data model, each of these produces **installations and ownerships** attached to a canonical game instead of a new `games` row per store ([[20 Game Identity and Installation Model#Matching: how scans decide "same game or new game?"]]).
