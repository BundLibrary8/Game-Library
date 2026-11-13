//! The mod library: every game's mods in one place.
//!
//! A game's mods live in its mods folder (set on the game's page, or
//! suggested from common places like `Mods`, `BepInEx\plugins`). Each file
//! or folder directly inside counts as one mod.
//!
//! - **Turning a mod off** moves it into a `.gamelib-disabled` folder
//!   inside the mods folder, where the game won't load it. Turning it on
//!   moves it back. Nothing is deleted.
//! - **Installing** takes a `.zip` (unpacked into the mods folder), or any
//!   other file or folder (copied in).
//! - **Removing** deletes the mod for good, so the UI asks first.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use walkdir::WalkDir;

/// The folder (inside a mods folder) that switched-off mods are kept in.
pub const DISABLED_DIR: &str = ".gamelib-disabled";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModItem {
    pub name: String,
    pub enabled: bool,
    pub is_folder: bool,
    pub bytes: u64,
    /// When it was last changed (Unix time), if known.
    pub modified: Option<i64>,
    pub path: PathBuf,
}

fn size_of(path: &Path) -> u64 {
    WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

fn items_in(dir: &Path, enabled: bool) -> Vec<ModItem> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            if name.starts_with('.') {
                return None; // hidden files, and the disabled folder itself
            }
            let path = entry.path();
            Some(ModItem {
                is_folder: path.is_dir(),
                bytes: size_of(&path),
                modified: entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64),
                name,
                enabled,
                path,
            })
        })
        .collect()
}

/// Every mod in a mods folder, on and off, sorted by name.
pub fn list(mods_dir: &Path) -> Result<Vec<ModItem>> {
    ensure!(
        mods_dir.is_dir(),
        "the mods folder {} isn't there",
        mods_dir.display()
    );
    let mut mods = items_in(mods_dir, true);
    mods.extend(items_in(&mods_dir.join(DISABLED_DIR), false));
    mods.sort_by_key(|m| m.name.to_lowercase());
    Ok(mods)
}

/// A name that's safe to use inside the mods folder: no path separators
/// and no going up a folder.
fn safe_name(name: &str) -> Result<&str> {
    ensure!(
        !name.is_empty()
            && name != "."
            && name != ".."
            && !name.contains(['/', '\\'])
            && name != DISABLED_DIR,
        "\"{name}\" isn't a mod name"
    );
    Ok(name)
}

/// Turn a mod on or off.
pub fn set_enabled(mods_dir: &Path, name: &str, enabled: bool) -> Result<()> {
    let name = safe_name(name)?;
    let disabled_dir = mods_dir.join(DISABLED_DIR);
    let (from, to) = if enabled {
        (disabled_dir.join(name), mods_dir.join(name))
    } else {
        (mods_dir.join(name), disabled_dir.join(name))
    };
    ensure!(from.exists(), "{name} isn't there any more");
    ensure!(
        !to.exists(),
        "there's already a mod called {name} {}",
        if enabled {
            "switched on"
        } else {
            "switched off"
        }
    );
    if !enabled {
        fs::create_dir_all(&disabled_dir)?;
    }
    fs::rename(&from, &to)
        .with_context(|| format!("couldn't move {name} (is the game running and using it?)"))
}

/// Delete a mod for good.
pub fn remove(mods_dir: &Path, name: &str, enabled: bool) -> Result<()> {
    let name = safe_name(name)?;
    let path = if enabled {
        mods_dir.join(name)
    } else {
        mods_dir.join(DISABLED_DIR).join(name)
    };
    ensure!(path.exists(), "{name} isn't there any more");
    if path.is_dir() {
        fs::remove_dir_all(&path)
    } else {
        fs::remove_file(&path)
    }
    .with_context(|| format!("couldn't delete {name}"))
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    for entry in WalkDir::new(from) {
        let entry = entry?;
        let dest = to.join(entry.path().strip_prefix(from).unwrap_or(entry.path()));
        if entry.file_type().is_dir() {
            fs::create_dir_all(&dest)?;
        } else if entry.file_type().is_file() {
            fs::copy(entry.path(), &dest)
                .with_context(|| format!("couldn't copy {}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// Install a mod: unpack a zip into the mods folder, or copy a file or
/// folder in. Returns the names of the mods that appeared.
pub fn install(mods_dir: &Path, source: &Path) -> Result<Vec<String>> {
    fs::create_dir_all(mods_dir)?;
    let before: Vec<String> = items_in(mods_dir, true)
        .into_iter()
        .map(|m| m.name)
        .collect();
    let is_zip = source
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    if is_zip {
        let file = fs::File::open(source)
            .with_context(|| format!("couldn't open {}", source.display()))?;
        let mut archive = zip::ZipArchive::new(file).context("that zip file can't be read")?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            // `enclosed_name` refuses names that would land outside the
            // folder ("../../Windows/…").
            let Some(relative) = entry.enclosed_name() else {
                bail!(
                    "the zip contains an unsafe path ({}); not installed",
                    entry.name().map(|n| n.into_owned()).unwrap_or_default()
                );
            };
            let dest = mods_dir.join(relative);
            if entry.is_dir() {
                fs::create_dir_all(&dest)?;
            } else {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut out = fs::File::create(&dest)
                    .with_context(|| format!("couldn't write {}", dest.display()))?;
                std::io::copy(&mut entry, &mut out)?;
            }
        }
    } else {
        let name = source
            .file_name()
            .context("that isn't a file or folder")?
            .to_string_lossy()
            .into_owned();
        let dest = mods_dir.join(&name);
        ensure!(!dest.exists(), "there's already a mod called {name}");
        if source.is_dir() {
            copy_tree(source, &dest)?;
        } else {
            fs::copy(source, &dest).with_context(|| format!("couldn't copy {name}"))?;
        }
    }
    let mut added: Vec<String> = items_in(mods_dir, true)
        .into_iter()
        .map(|m| m.name)
        .filter(|name| !before.contains(name))
        .collect();
    added.sort();
    Ok(added)
}

/// Where a game's mods probably go, if any of the usual folders exist in
/// its install folder.
pub fn suggest_mods_dir(install_dir: &Path) -> Option<PathBuf> {
    const USUAL: [&str; 9] = [
        "Mods",
        "mods",
        "BepInEx/plugins",
        "MelonLoader/Mods",
        "Data/Mods",
        "Content/Paks/~mods",
        "Content/Paks/Mods",
        "addons",
        "Plugins",
    ];
    USUAL
        .iter()
        .map(|sub| install_dir.join(sub))
        .find(|dir| dir.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn mods_are_listed_switched_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let mods = dir.path().join("Mods");
        write(&mods.join("BetterUI/ui.pak"), "1234");
        write(&mods.join("cheats.dll"), "12");
        write(&mods.join(".hidden"), "x");

        let all = list(&mods).unwrap();
        assert_eq!(
            all.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
            ["BetterUI", "cheats.dll"]
        );
        assert_eq!(all[0].bytes, 4);
        assert!(all[0].is_folder && all[0].enabled);

        set_enabled(&mods, "cheats.dll", false).unwrap();
        assert!(mods.join(DISABLED_DIR).join("cheats.dll").exists());
        assert!(!list(&mods).unwrap()[1].enabled);
        assert!(
            set_enabled(&mods, "cheats.dll", false).is_err(),
            "already off"
        );
        set_enabled(&mods, "cheats.dll", true).unwrap();
        assert!(mods.join("cheats.dll").exists());

        assert!(set_enabled(&mods, "../escape", false).is_err());
        assert!(remove(&mods, "..", true).is_err());
        remove(&mods, "BetterUI", true).unwrap();
        assert_eq!(list(&mods).unwrap().len(), 1);
    }

    #[test]
    fn zips_files_and_folders_install() {
        let dir = tempfile::tempdir().unwrap();
        let mods = dir.path().join("Mods");

        let zip_path = dir.path().join("pack.zip");
        let mut writer = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("NewMaps/map1.dat", options).unwrap();
        writer.write_all(b"map").unwrap();
        writer.start_file("readme.txt", options).unwrap();
        writer.write_all(b"hi").unwrap();
        writer.finish().unwrap();
        assert_eq!(
            install(&mods, &zip_path).unwrap(),
            ["NewMaps", "readme.txt"]
        );
        assert_eq!(
            fs::read_to_string(mods.join("NewMaps/map1.dat")).unwrap(),
            "map"
        );

        let single = dir.path().join("speed.dll");
        fs::write(&single, "x").unwrap();
        assert_eq!(install(&mods, &single).unwrap(), ["speed.dll"]);
        assert!(install(&mods, &single).is_err(), "already there");

        let folder = dir.path().join("Skins");
        write(&folder.join("a/b.png"), "png");
        assert_eq!(install(&mods, &folder).unwrap(), ["Skins"]);
        assert!(mods.join("Skins/a/b.png").exists());
    }

    #[test]
    fn unsafe_zips_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("evil.zip");
        let mut writer = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        writer
            .start_file(
                "../../outside.txt",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"x").unwrap();
        writer.finish().unwrap();
        assert!(install(&dir.path().join("Mods"), &zip_path).is_err());
        assert!(!dir.path().join("outside.txt").exists());
    }

    #[test]
    fn the_usual_mods_folders_are_suggested() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(suggest_mods_dir(dir.path()), None);
        fs::create_dir_all(dir.path().join("BepInEx/plugins")).unwrap();
        assert_eq!(
            suggest_mods_dir(dir.path()),
            Some(dir.path().join("BepInEx/plugins"))
        );
    }
}
