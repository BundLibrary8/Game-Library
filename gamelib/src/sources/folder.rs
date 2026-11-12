//! Folder scan: finding loose games in folders like `D:\Games`.
//!
//! Each top-level subfolder is treated as one game. This is guesswork, so
//! unlike the launcher sources its results aren't saved straight away:
//! they go to the import review screen, where a person ticks the ones to
//! add and can fix titles.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use walkdir::WalkDir;

use super::GameSource;
use crate::model::{DetectedGame, LaunchTarget, Source};
use crate::paths::{is_inside, real_path};

/// How many folders deep to look for exes inside a game's folder.
pub const MAX_DEPTH: usize = 3;

/// If an exe's name contains one of these, it isn't the game.
const JUNK_WORDS: &[&str] = &[
    "unins",
    "setup",
    "install",
    "vc_redist",
    "vcredist",
    "dxsetup",
    "dxwebsetup",
    "directx",
    "dotnet",
    "crash",
    "launcher_helper",
    "helper",
    "updater",
    "patcher",
    "redist",
    "benchmark",
    "config",
    "report",
    "easyanticheat",
    "battleye",
    "cleanup",
    "notification_helper",
];

/// Is this exe an installer, crash handler or other hanger-on, rather
/// than the game? `name` is the file name, e.g. `UnityCrashHandler64.exe`.
pub fn is_junk_exe(name: &str) -> bool {
    let name = name.to_lowercase();
    JUNK_WORDS.iter().any(|word| name.contains(word))
}

/// Letters and digits only, lower case: "Ember Road" and "ember-road"
/// both become "emberroad", so names can be compared loosely.
fn simplify(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Choose the most likely game exe from the candidates (path, size in
/// bytes): one whose name matches the folder's name if there is one,
/// otherwise the biggest. Junk exes are never chosen.
pub fn pick_best_exe(folder_name: &str, candidates: &[(PathBuf, u64)]) -> Option<PathBuf> {
    let wanted = simplify(folder_name);
    let real: Vec<&(PathBuf, u64)> = candidates
        .iter()
        .filter(|(path, _)| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| !is_junk_exe(n))
        })
        .collect();

    let stem = |path: &Path| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(simplify)
            .unwrap_or_default()
    };
    let by_name = real
        .iter()
        .filter(|(path, _)| !wanted.is_empty() && stem(path) == wanted)
        .max_by_key(|(_, size)| *size);
    let chosen = by_name.or_else(|| real.iter().max_by_key(|(_, size)| *size));
    chosen.map(|(path, _)| path.clone())
}

/// Every `.exe` inside a game folder, up to [`MAX_DEPTH`] levels down,
/// with its size.
pub fn find_exes(game_dir: &Path) -> Vec<(PathBuf, u64)> {
    WalkDir::new(game_dir)
        .max_depth(MAX_DEPTH)
        .into_iter()
        .filter_map(|entry| entry.ok()) // unreadable folders are skipped
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
        })
        .map(|entry| {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            (entry.into_path(), size)
        })
        .collect()
}

/// Scan one folder: each subfolder with a plausible exe becomes a
/// candidate game. Subfolders that are (or contain, or are inside) one of
/// `skip_dirs` are left out; pass the install folders of Steam, Epic and
/// GOG games so nothing appears twice.
pub fn scan_folder(root: &Path, skip_dirs: &[PathBuf]) -> Result<Vec<DetectedGame>> {
    // Use the folder's real path, so the exe paths saved as ids are the
    // same however the folder was typed.
    let root = &real_path(root);
    let entries = std::fs::read_dir(root)
        .with_context(|| format!("couldn't open the folder {}", root.display()))?;

    let mut games = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let game_dir = entry.path();
        if !game_dir.is_dir() {
            continue;
        }
        let already_known = skip_dirs
            .iter()
            .any(|known| is_inside(known, &game_dir) || is_inside(&game_dir, known));
        if already_known {
            continue;
        }
        let Some(folder_name) = game_dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(exe) = pick_best_exe(folder_name, &find_exes(&game_dir)) else {
            continue;
        };
        games.push(DetectedGame {
            title: folder_name.to_string(),
            source: Source::Folder,
            source_id: exe.to_string_lossy().into_owned(),
            install_dir: Some(game_dir),
            launch: LaunchTarget::exe(exe),
        });
    }
    games.sort_by_key(|g| g.title.to_lowercase());
    Ok(games)
}

/// Folder scan as a [`GameSource`], over every folder set in Settings.
#[derive(Debug, Default)]
pub struct FolderSource {
    pub roots: Vec<PathBuf>,
    pub skip_dirs: Vec<PathBuf>,
}

impl GameSource for FolderSource {
    fn name(&self) -> &str {
        "Folder scan"
    }

    fn source(&self) -> Source {
        Source::Folder
    }

    fn detect(&self) -> Result<Vec<DetectedGame>> {
        let mut games = Vec::new();
        for root in &self.roots {
            // A folder that has gone (unplugged drive) doesn't stop the rest.
            match scan_folder(root, &self.skip_dirs) {
                Ok(found) => games.extend(found),
                Err(err) => log::warn!("{err:#}"),
            }
        }
        Ok(games)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn candidates(list: &[(&str, u64)]) -> Vec<(PathBuf, u64)> {
        list.iter().map(|(p, s)| (PathBuf::from(p), *s)).collect()
    }

    #[test]
    fn junk_exes_are_recognised() {
        for junk in [
            "unins000.exe",
            "Setup.exe",
            "vc_redist.x64.exe",
            "DXSETUP.exe",
            "UnityCrashHandler64.exe",
            "CrashReportClient.exe",
            "launcher_helper.exe",
            "Game_Installer.exe",
        ] {
            assert!(is_junk_exe(junk), "{junk}");
        }
        for real in ["EmberRoad.exe", "game.exe", "hl2.exe", "Launcher.exe"] {
            assert!(!is_junk_exe(real), "{real}");
        }
    }

    #[test]
    fn prefers_the_exe_named_like_the_folder_even_if_smaller() {
        let c = candidates(&[
            ("/g/Ember Road/engine.exe", 9_000),
            ("/g/Ember Road/bin/ember-road.exe", 500),
        ]);
        assert_eq!(
            pick_best_exe("Ember Road", &c),
            Some(PathBuf::from("/g/Ember Road/bin/ember-road.exe"))
        );
    }

    #[test]
    fn otherwise_picks_the_largest_exe() {
        let c = candidates(&[
            ("/g/X/a.exe", 10),
            ("/g/X/big.exe", 5_000),
            ("/g/X/b.exe", 20),
        ]);
        assert_eq!(
            pick_best_exe("Something Else", &c),
            Some(PathBuf::from("/g/X/big.exe"))
        );
    }

    #[test]
    fn junk_is_never_picked_even_when_it_is_the_biggest() {
        let c = candidates(&[("/g/X/setup.exe", 99_999), ("/g/X/play.exe", 10)]);
        assert_eq!(pick_best_exe("X", &c), Some(PathBuf::from("/g/X/play.exe")));

        let only_junk = candidates(&[("/g/X/unins000.exe", 5), ("/g/X/vc_redist.x64.exe", 9)]);
        assert_eq!(pick_best_exe("X", &only_junk), None);
        assert_eq!(pick_best_exe("X", &[]), None);
    }

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0u8; bytes]).unwrap();
    }

    #[test]
    fn scans_a_folder_of_games() {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        write(&r.join("Ember Road/EmberRoad.exe"), 100);
        write(&r.join("Ember Road/unins000.exe"), 5000);
        write(&r.join("Deep Game/a/b/deep.EXE"), 10); // 3 levels: found
        write(&r.join("Too Deep/a/b/c/hidden.exe"), 10); // 4 levels: not found
        write(&r.join("Just Docs/readme.txt"), 10);
        write(&r.join("Only Junk/setup.exe"), 10);
        write(&r.join("loose.exe"), 10); // not in a subfolder
        write(&r.join("Steam Game/game.exe"), 10);

        let skip = vec![r.join("Steam Game")];
        // Ask with a roundabout spelling: the results use the real path.
        let games = scan_folder(&r.join("Ember Road").join(".."), &skip).unwrap();
        let r = &real_path(r);
        let titles: Vec<&str> = games.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles, ["Deep Game", "Ember Road"]);

        let ember = &games[1];
        assert_eq!(ember.source, Source::Folder);
        assert_eq!(ember.install_dir, Some(r.join("Ember Road")));
        assert_eq!(
            ember.launch,
            LaunchTarget::exe(r.join("Ember Road/EmberRoad.exe"))
        );
        assert_eq!(
            ember.source_id,
            r.join("Ember Road/EmberRoad.exe").to_string_lossy()
        );
    }

    #[test]
    fn a_missing_folder_is_an_error_for_one_scan_but_not_for_the_source() {
        assert!(scan_folder(Path::new("/definitely/not/here"), &[]).is_err());

        let root = tempfile::tempdir().unwrap();
        write(&root.path().join("Game/game.exe"), 10);
        let source = FolderSource {
            roots: vec![
                PathBuf::from("/definitely/not/here"),
                root.path().to_path_buf(),
            ],
            skip_dirs: vec![],
        };
        assert_eq!(source.detect().unwrap().len(), 1);
    }
}
