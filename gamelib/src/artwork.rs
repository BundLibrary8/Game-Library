//! Real artwork for games, found on this PC without any API key.
//!
//! - **Steam covers** are copied from Steam's own library cache (the
//!   pictures Steam already downloaded for its library), or fetched from
//!   Steam's public image server.
//! - **Icons** are read out of a game's `.exe`. Windows programs carry
//!   their icon inside the file; this pulls out the largest one and saves
//!   it as a PNG.
//!
//! The icon reader understands the exe file format directly, so it works
//! (and is tested) on any operating system.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use walkdir::WalkDir;

use crate::model::{Game, LaunchTarget, Source};
use crate::sources::folder;

/// File names Steam uses for a game's portrait cover in its cache.
const STEAM_COVER_NAMES: [&str; 2] = ["library_600x900.jpg", "library_600x900_2x.jpg"];

/// Where Steam's public image server keeps a game's portrait cover.
/// No API key is needed. Not every game has one (older ones may 404).
pub fn steam_cdn_cover_url(app_id: &str) -> String {
    format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{app_id}/library_600x900.jpg")
}

/// Find a game's portrait cover in Steam's library cache, given the
/// folder Steam is installed in. Steam has used two layouts over the
/// years, and both are checked:
/// - `appcache/librarycache/<appid>_library_600x900.jpg`
/// - `appcache/librarycache/<appid>/…/library_600x900.jpg`
pub fn steam_cache_cover(steam_root: &Path, app_id: &str) -> Option<PathBuf> {
    // An app id is a number; refuse anything else so it can't be used to
    // wander around the disk.
    if app_id.is_empty() || !app_id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let cache = steam_root.join("appcache").join("librarycache");
    for name in STEAM_COVER_NAMES {
        let flat = cache.join(format!("{app_id}_{name}"));
        if flat.is_file() {
            return Some(flat);
        }
    }
    let per_app = cache.join(app_id);
    if !per_app.is_dir() {
        return None;
    }
    WalkDir::new(&per_app)
        .max_depth(3)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .find(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| STEAM_COVER_NAMES.contains(&name))
        })
        .map(|entry| entry.into_path())
}

/// The exe whose icon best represents a game: the one it launches, or for
/// launcher games (Steam, Epic) the most likely game exe in its install
/// folder.
pub fn icon_source_exe(game: &Game) -> Option<PathBuf> {
    if let LaunchTarget::Exe { path, .. } = &game.launch
        && path.is_file()
    {
        return Some(path.clone());
    }
    let install_dir = game.install_dir.as_deref().filter(|dir| dir.is_dir())?;
    let folder_name = install_dir.file_name()?.to_str()?;
    folder::pick_best_exe(folder_name, &folder::find_exes(install_dir))
}

/// Turn the bytes of a Windows `.ico` file into PNG bytes, using the
/// largest picture inside it.
pub fn ico_to_png(ico: &[u8]) -> Result<Vec<u8>> {
    let picture = image::load_from_memory_with_format(ico, image::ImageFormat::Ico)
        .context("the icon couldn't be read")?;
    let mut png = Vec::new();
    picture
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .context("the icon couldn't be saved as a PNG")?;
    Ok(png)
}

/// Read the main icon out of a Windows exe as `.ico` bytes. Returns
/// `Ok(None)` if the file has no icon.
pub fn read_exe_icon(exe: &Path) -> Result<Option<Vec<u8>>> {
    // Game exes can be very large, so the file is mapped rather than read
    // into memory: only the parts that are looked at get loaded.
    let file =
        pelite::FileMap::open(exe).with_context(|| format!("couldn't open {}", exe.display()))?;
    let pe = pelite::PeFile::from_bytes(&file)
        .map_err(|err| anyhow::anyhow!("{} isn't a Windows program: {err}", exe.display()))?;
    let Ok(resources) = pe.resources() else {
        return Ok(None); // no resources at all, so no icon
    };
    // An exe can hold several icon groups; the first is the one Windows
    // shows for the file.
    let Some(Ok((_, group))) = resources.icons().next() else {
        return Ok(None);
    };
    let mut ico = Vec::new();
    group
        .write(&mut ico)
        .context("the icon inside the exe couldn't be read")?;
    Ok(Some(ico))
}

/// Pull the icon out of `exe` and save it as a PNG at `dest`. Returns
/// false (and writes nothing) if the exe has no icon.
pub fn save_exe_icon(exe: &Path, dest: &Path) -> Result<bool> {
    let Some(ico) = read_exe_icon(exe)? else {
        return Ok(false);
    };
    let png = ico_to_png(&ico)?;
    std::fs::write(dest, png).with_context(|| format!("couldn't save {}", dest.display()))?;
    Ok(true)
}

/// Copy a Steam game's cover out of Steam's cache into GameLib's covers
/// folder (the cache can be cleared by Steam at any time). Returns where
/// it was saved, or `None` if Steam has no cover cached for it.
pub fn copy_steam_cache_cover(
    steam_root: &Path,
    game: &Game,
    covers_dir: &Path,
) -> Result<Option<PathBuf>> {
    if game.source != Source::Steam {
        return Ok(None);
    }
    let Some(cached) = steam_cache_cover(steam_root, &game.source_id) else {
        return Ok(None);
    };
    let dest = covers_dir.join(format!("{}.jpg", game.id));
    std::fs::copy(&cached, &dest)
        .with_context(|| format!("couldn't copy {} to {}", cached.display(), dest.display()))?;
    Ok(Some(dest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_covers_in_both_steam_cache_layouts() {
        let steam = tempfile::tempdir().unwrap();
        let cache = steam.path().join("appcache").join("librarycache");
        fs::create_dir_all(&cache).unwrap();
        assert_eq!(steam_cache_cover(steam.path(), "570"), None);

        // Old layout: one flat folder.
        fs::write(cache.join("570_library_600x900.jpg"), b"old").unwrap();
        fs::write(cache.join("570_header.jpg"), b"wrong picture").unwrap();
        assert_eq!(
            steam_cache_cover(steam.path(), "570"),
            Some(cache.join("570_library_600x900.jpg"))
        );

        // New layout: a folder per game, sometimes with a subfolder.
        let nested = cache.join("730").join("0a1b2c3d");
        fs::create_dir_all(&nested).unwrap();
        fs::write(cache.join("730").join("header.jpg"), b"wrong picture").unwrap();
        fs::write(nested.join("library_600x900.jpg"), b"new").unwrap();
        assert_eq!(
            steam_cache_cover(steam.path(), "730"),
            Some(nested.join("library_600x900.jpg"))
        );
    }

    #[test]
    fn odd_app_ids_are_refused() {
        let steam = tempfile::tempdir().unwrap();
        fs::create_dir_all(steam.path().join("appcache").join("librarycache")).unwrap();
        for bad in ["", "..", "../570", "57a", "5/7"] {
            assert_eq!(steam_cache_cover(steam.path(), bad), None, "{bad:?}");
        }
    }

    #[test]
    fn steam_cdn_address() {
        assert_eq!(
            steam_cdn_cover_url("570"),
            "https://cdn.cloudflare.steamstatic.com/steam/apps/570/library_600x900.jpg"
        );
    }

    /// Make a real `.ico` file in memory holding one solid-colour square.
    fn make_ico(size: u32) -> Vec<u8> {
        let picture = image::RgbaImage::from_pixel(size, size, image::Rgba([200, 40, 90, 255]));
        let mut ico = Vec::new();
        image::DynamicImage::ImageRgba8(picture)
            .write_to(&mut std::io::Cursor::new(&mut ico), image::ImageFormat::Ico)
            .unwrap();
        ico
    }

    #[test]
    fn an_ico_becomes_a_png_of_the_same_size_and_colour() {
        let png = ico_to_png(&make_ico(48)).unwrap();
        let picture = image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap();
        assert_eq!((picture.width(), picture.height()), (48, 48));
        assert_eq!(picture.to_rgba8().get_pixel(10, 10).0, [200, 40, 90, 255]);
    }

    #[test]
    fn rubbish_is_an_error_not_a_crash() {
        assert!(ico_to_png(b"this is not an icon").is_err());

        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("fake.exe");
        fs::write(&fake, b"MZ but not really a program").unwrap();
        assert!(read_exe_icon(&fake).is_err());
        assert!(read_exe_icon(&dir.path().join("missing.exe")).is_err());
        let dest = dir.path().join("icon.png");
        assert!(save_exe_icon(&fake, &dest).is_err());
        assert!(!dest.exists(), "nothing is written when reading fails");
    }

    #[test]
    fn icon_comes_from_the_launch_exe_or_the_install_folder() {
        let dir = tempfile::tempdir().unwrap();
        let install = dir.path().join("Ember Road");
        fs::create_dir_all(install.join("bin")).unwrap();
        let exe = install.join("bin").join("EmberRoad.exe");
        fs::write(&exe, vec![0u8; 500]).unwrap();
        fs::write(install.join("unins000.exe"), vec![0u8; 9000]).unwrap();

        let mut game = Game {
            id: 1,
            title: "Ember Road".into(),
            source: Source::Steam,
            source_id: "1".into(),
            install_dir: Some(install.clone()),
            launch: LaunchTarget::Uri("steam://rungameid/1".into()),
            favourite: false,
            hidden: false,
            installed: true,
            playtime_secs: 0,
            last_played: None,
            added_at: 0,
            cover_path: None,
            icon_path: None,
            description: None,
            genres: vec![],
            release_year: None,
        };
        // A launcher game: the best exe in its folder (not the uninstaller).
        assert_eq!(icon_source_exe(&game), Some(exe.clone()));

        // An exe game: the exe it launches.
        let other = install.join("unins000.exe");
        game.launch = LaunchTarget::exe(&other);
        assert_eq!(icon_source_exe(&game), Some(other));

        // Nothing on disk: no icon source.
        game.launch = LaunchTarget::Uri("steam://rungameid/1".into());
        game.install_dir = Some(dir.path().join("gone"));
        assert_eq!(icon_source_exe(&game), None);
    }

    /// Checks icon reading against a real Windows program. It needs one to
    /// test with, so it only does anything when GAMELIB_TEST_EXE points at
    /// an exe that has an icon:
    /// `GAMELIB_TEST_EXE=C:\Windows\notepad.exe cargo test real_exe`
    #[test]
    fn real_exe_icon_can_be_saved() {
        let Some(exe) = std::env::var_os("GAMELIB_TEST_EXE") else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("icon.png");
        assert!(
            save_exe_icon(Path::new(&exe), &dest).unwrap(),
            "the exe has no icon"
        );
        let picture = image::open(&dest).unwrap();
        assert!(picture.width() >= 16 && picture.width() == picture.height());
        println!("icon is {}x{}", picture.width(), picture.height());
    }
}
