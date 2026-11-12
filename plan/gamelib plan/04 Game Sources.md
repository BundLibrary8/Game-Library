---
tags: [gamelib, design]
status: draft-needs-review
section: 3
---
# 04 Game Sources (Section 3, draft)

Back to [[00 Home]] · Prev: [[03 Data Model]] · Next: [[05 Launching and Playtime]]

> [!warning] Draft
> Not agreed yet. The file locations below are where these launchers normally store data. **Check them on your own PC** before relying on them.

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
