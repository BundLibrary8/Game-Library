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

/// Covers are saved at this size: big enough to look sharp on a card,
/// small enough that hundreds of them don't fill the graphics memory.
pub const COVER_WIDTH: u32 = 360;
pub const COVER_HEIGHT: u32 = 540;

/// Addresses on Steam's public image servers where a game's portrait
/// cover usually is, most likely first. No API key is needed. Not every
/// game has one there (very old and very new games may not).
pub fn steam_cdn_cover_urls(app_id: &str) -> Vec<String> {
    vec![
        format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{app_id}/library_600x900.jpg"),
        format!(
            "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/{app_id}/library_600x900.jpg"
        ),
        format!("https://cdn.akamai.steamstatic.com/steam/apps/{app_id}/library_600x900.jpg"),
    ]
}

/// Is a picture this shape a portrait cover (about 2 wide to 3 tall)?
fn is_cover_shaped(width: u32, height: u32) -> bool {
    width >= 200 && height * 10 >= width * 13 && height * 10 <= width * 17
}

/// Find a game's portrait cover in Steam's library cache, given the
/// folder Steam is installed in. Steam has changed how it stores these
/// over the years, so three things are tried:
/// 1. `appcache/librarycache/<appid>_library_600x900.jpg` (the old way)
/// 2. `appcache/librarycache/<appid>/…/library_600x900.jpg`
/// 3. any cover-shaped picture in `appcache/librarycache/<appid>/`, for
///    newer Steam versions that give the files meaningless names
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
    let pictures: Vec<PathBuf> = WalkDir::new(&per_app)
        .max_depth(3)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| {
            path.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "png"))
        })
        .collect();
    let by_name = pictures.iter().find(|path| {
        path.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| STEAM_COVER_NAMES.contains(&name))
    });
    if let Some(path) = by_name {
        return Some(path.clone());
    }
    // No recognisable name: look at the pictures' shapes instead, and take
    // the biggest cover-shaped one. Only the file header is read for this.
    pictures
        .into_iter()
        .filter_map(|path| {
            let (width, height) = image::image_dimensions(&path).ok()?;
            is_cover_shaped(width, height).then_some((width * height, path))
        })
        .max_by_key(|(pixels, _)| *pixels)
        .map(|(_, path)| path)
}

/// Turn any picture into a cover: a JPEG of exactly
/// [`COVER_WIDTH`] × [`COVER_HEIGHT`].
///
/// A portrait picture is simply resized (trimming the edges if its shape
/// is slightly off). A wide picture, like a store banner, would be ruined
/// by squashing, so it is placed whole near the top over a blurred,
/// darkened copy of itself. That is how Steam shows games that have no
/// portrait art.
pub fn make_cover(picture: &[u8]) -> Result<Vec<u8>> {
    use image::imageops::{self, FilterType};

    let picture = image::load_from_memory(picture).context("the picture couldn't be read")?;
    let (width, height) = (picture.width(), picture.height());
    anyhow::ensure!(width > 0 && height > 0, "the picture is empty");

    let cover = if height * 10 >= width * 12 {
        picture
            .resize_to_fill(COVER_WIDTH, COVER_HEIGHT, FilterType::Lanczos3)
            .to_rgb8()
    } else {
        // Background: shrink a lot, blur, and blow back up. Blurring a tiny
        // picture is quick and gives a soft, even blur.
        let small = picture
            .resize_to_fill(COVER_WIDTH / 6, COVER_HEIGHT / 6, FilterType::Triangle)
            .to_rgb8();
        let blurred = imageops::fast_blur(&small, 3.0);
        let mut cover = imageops::resize(&blurred, COVER_WIDTH, COVER_HEIGHT, FilterType::Triangle);
        for pixel in cover.pixels_mut() {
            for channel in &mut pixel.0 {
                *channel = (*channel as f32 * 0.55) as u8;
            }
        }
        // Foreground: the whole picture, as wide as the cover.
        let front_height = (height as u64 * COVER_WIDTH as u64 / width as u64).max(1) as u32;
        let front = picture
            .resize_exact(
                COVER_WIDTH,
                front_height.min(COVER_HEIGHT),
                FilterType::Lanczos3,
            )
            .to_rgb8();
        let top = (COVER_HEIGHT - front.height()) as f32 * 0.22;
        imageops::overlay(&mut cover, &front, 0, top as i64);
        cover
    };

    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 88)
        .encode_image(&cover)
        .context("the cover couldn't be saved as a JPEG")?;
    Ok(jpeg)
}

/// Where a game's automatically found cover is saved.
pub fn cover_path(covers_dir: &Path, game_id: i64) -> PathBuf {
    covers_dir.join(format!("{game_id}.jpg"))
}

/// Make a cover from a picture's bytes and save it for a game. Returns
/// where it was saved.
pub fn save_cover(picture: &[u8], covers_dir: &Path, game_id: i64) -> Result<PathBuf> {
    let dest = cover_path(covers_dir, game_id);
    std::fs::write(&dest, make_cover(picture)?)
        .with_context(|| format!("couldn't save {}", dest.display()))?;
    Ok(dest)
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
    let picture =
        std::fs::read(&cached).with_context(|| format!("couldn't read {}", cached.display()))?;
    save_cover(&picture, covers_dir, game.id).map(Some)
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
    fn steam_cdn_addresses_use_the_app_id() {
        let urls = steam_cdn_cover_urls("570");
        assert_eq!(
            urls[0],
            "https://cdn.cloudflare.steamstatic.com/steam/apps/570/library_600x900.jpg"
        );
        assert!(
            urls.iter()
                .all(|url| url.contains("/570/") && url.starts_with("https://"))
        );
    }

    /// A picture file in memory: one flat colour, any size and format.
    fn picture(width: u32, height: u32, colour: [u8; 3], format: image::ImageFormat) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(width, height, image::Rgb(colour));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut bytes), format)
            .unwrap();
        bytes
    }

    #[test]
    fn newer_steam_caches_with_meaningless_names_are_matched_by_shape() {
        let steam = tempfile::tempdir().unwrap();
        let folder = steam
            .path()
            .join("appcache")
            .join("librarycache")
            .join("999");
        fs::create_dir_all(&folder).unwrap();
        let jpeg = image::ImageFormat::Jpeg;
        fs::write(folder.join("aaaa.jpg"), picture(460, 215, [9, 9, 9], jpeg)).unwrap(); // banner
        fs::write(folder.join("bbbb.jpg"), picture(300, 450, [9, 9, 9], jpeg)).unwrap(); // small cover
        fs::write(folder.join("cccc.jpg"), picture(600, 900, [9, 9, 9], jpeg)).unwrap(); // big cover
        fs::write(
            folder.join("dddd.png"),
            picture(64, 64, [9, 9, 9], image::ImageFormat::Png),
        )
        .unwrap(); // icon
        fs::write(folder.join("notes.txt"), b"not a picture").unwrap();

        assert_eq!(
            steam_cache_cover(steam.path(), "999"),
            Some(folder.join("cccc.jpg"))
        );
    }

    #[test]
    fn cover_shapes() {
        assert!(is_cover_shaped(600, 900));
        assert!(is_cover_shaped(300, 450));
        assert!(!is_cover_shaped(460, 215), "a banner");
        assert!(!is_cover_shaped(512, 512), "a square");
        assert!(!is_cover_shaped(64, 96), "too small to be a cover");
        assert!(!is_cover_shaped(300, 900), "far too tall");
    }

    fn decode(jpeg: &[u8]) -> image::RgbImage {
        image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg)
            .unwrap()
            .to_rgb8()
    }

    fn close(a: [u8; 3], b: [u8; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 12)
    }

    #[test]
    fn a_portrait_picture_becomes_a_cover_of_the_standard_size() {
        let red = [200, 30, 40];
        let cover = decode(&make_cover(&picture(600, 900, red, image::ImageFormat::Png)).unwrap());
        assert_eq!((cover.width(), cover.height()), (COVER_WIDTH, COVER_HEIGHT));
        assert!(close(cover.get_pixel(180, 270).0, red));
        assert!(
            close(cover.get_pixel(5, 530).0, red),
            "it fills the whole cover"
        );
    }

    #[test]
    fn a_wide_picture_is_kept_whole_over_a_darkened_backdrop() {
        let green = [40, 200, 80];
        let cover =
            decode(&make_cover(&picture(460, 215, green, image::ImageFormat::Png)).unwrap());
        assert_eq!((cover.width(), cover.height()), (COVER_WIDTH, COVER_HEIGHT));
        // The banner itself, full brightness, sits in the upper part…
        assert!(close(cover.get_pixel(180, 160).0, green));
        // …and below it is the same colour, darkened.
        let below = cover.get_pixel(180, 500).0;
        assert!(below[1] < 140 && below[1] > 60, "{below:?}");
        assert!(
            below[1] > below[0] && below[1] > below[2],
            "still green: {below:?}"
        );
    }

    #[test]
    fn make_cover_refuses_things_that_are_not_pictures() {
        assert!(make_cover(b"<html>404 not found</html>").is_err());
        assert!(make_cover(b"").is_err());
    }

    #[test]
    fn a_cached_steam_cover_is_copied_at_the_standard_size() {
        let steam = tempfile::tempdir().unwrap();
        let cache = steam.path().join("appcache").join("librarycache");
        fs::create_dir_all(&cache).unwrap();
        fs::write(
            cache.join("570_library_600x900.jpg"),
            picture(600, 900, [10, 20, 200], image::ImageFormat::Jpeg),
        )
        .unwrap();
        let covers = tempfile::tempdir().unwrap();

        let mut game = sample_game(steam.path());
        game.id = 42;
        game.source_id = "570".into();
        let saved = copy_steam_cache_cover(steam.path(), &game, covers.path())
            .unwrap()
            .unwrap();
        assert_eq!(saved, covers.path().join("42.jpg"));
        assert_eq!(
            image::image_dimensions(&saved).unwrap(),
            (COVER_WIDTH, COVER_HEIGHT)
        );

        // Not a Steam game, or nothing cached: nothing copied.
        game.source_id = "571".into();
        assert_eq!(
            copy_steam_cache_cover(steam.path(), &game, covers.path()).unwrap(),
            None
        );
        game.source = Source::Epic;
        game.source_id = "570".into();
        assert_eq!(
            copy_steam_cache_cover(steam.path(), &game, covers.path()).unwrap(),
            None
        );
    }

    fn sample_game(install_dir: &Path) -> Game {
        Game {
            id: 1,
            title: "Ember Road".into(),
            source: Source::Steam,
            source_id: "1".into(),
            install_dir: Some(install_dir.to_path_buf()),
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
        }
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

        let mut game = sample_game(&install);
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
