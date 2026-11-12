//! Where GameLib keeps its files: `%APPDATA%\gamelib\` on Windows.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Set this environment variable to keep GameLib's files somewhere else
/// (handy for trying things out without touching the real library).
pub const DATA_DIR_ENV: &str = "GAMELIB_DATA_DIR";

/// The data folder, created if it doesn't exist yet.
pub fn data_dir() -> Result<PathBuf> {
    let dir = match std::env::var_os(DATA_DIR_ENV) {
        Some(custom) if !custom.is_empty() => PathBuf::from(custom),
        _ => directories::BaseDirs::new()
            .context("couldn't find the user's home folder")?
            .data_dir()
            .join("gamelib"),
    };
    std::fs::create_dir_all(&dir).with_context(|| format!("couldn't create {}", dir.display()))?;
    Ok(dir)
}

pub fn db_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("library.db"))
}

pub fn log_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("gamelib.log"))
}

/// Folder for downloaded cover images, created if needed.
pub fn covers_dir() -> Result<PathBuf> {
    subdir("covers")
}

/// Folder for icons read out of games' exes, created if needed.
pub fn icons_dir() -> Result<PathBuf> {
    subdir("icons")
}

/// Folder for the profile picture and uploaded banners, created if needed.
pub fn profile_dir() -> Result<PathBuf> {
    subdir("profile")
}

fn subdir(name: &str) -> Result<PathBuf> {
    let dir = data_dir()?.join(name);
    std::fs::create_dir_all(&dir).with_context(|| format!("couldn't create {}", dir.display()))?;
    Ok(dir)
}

/// Is `path` inside the folder `dir` (or the folder itself)?
///
/// Compares whole path components, so `D:\Games\Foo2\a.exe` is *not*
/// inside `D:\Games\Foo`. On Windows the comparison ignores upper/lower
/// case, because Windows file names do.
pub fn is_inside(path: &Path, dir: &Path) -> bool {
    if cfg!(windows) {
        let lower = |p: &Path| PathBuf::from(p.to_string_lossy().to_lowercase());
        lower(path).starts_with(lower(dir))
    } else {
        path.starts_with(dir)
    }
}

/// The real, full form of a path: `..` resolved and links followed, so two
/// different spellings of the same folder compare equal. Falls back to the
/// path as given if it doesn't exist.
///
/// On Windows the system's answer starts with `\\?\`; that prefix is
/// removed, because the paths Windows reports for running programs don't
/// have it.
pub fn real_path(path: &Path) -> PathBuf {
    match path.canonicalize() {
        Ok(full) => PathBuf::from(strip_verbatim_prefix(&full.to_string_lossy())),
        Err(_) => path.to_path_buf(),
    }
}

/// `\\?\C:\Games` → `C:\Games`, and `\\?\UNC\server\share` →
/// `\\server\share`. Anything else is returned unchanged.
pub fn strip_verbatim_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_inside_compares_whole_components() {
        let dir = Path::new("/games/Foo");
        assert!(is_inside(Path::new("/games/Foo/bin/foo.exe"), dir));
        assert!(is_inside(Path::new("/games/Foo"), dir));
        assert!(!is_inside(Path::new("/games/Foo2/foo.exe"), dir));
        assert!(!is_inside(Path::new("/games"), dir));
        assert!(!is_inside(Path::new("/other/Foo/foo.exe"), dir));
    }

    #[test]
    fn verbatim_prefixes_are_stripped() {
        assert_eq!(strip_verbatim_prefix(r"\\?\C:\Games\Foo"), r"C:\Games\Foo");
        assert_eq!(strip_verbatim_prefix(r"\\?\UNC\nas\games"), r"\\nas\games");
        assert_eq!(strip_verbatim_prefix(r"C:\Games"), r"C:\Games");
        assert_eq!(strip_verbatim_prefix("/games/foo"), "/games/foo");
    }

    #[test]
    fn real_path_resolves_dot_dot_and_tolerates_missing_paths() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("a")).unwrap();
        std::fs::create_dir(dir.path().join("b")).unwrap();
        let roundabout = dir.path().join("a").join("..").join("b");
        assert_eq!(real_path(&roundabout), real_path(&dir.path().join("b")));
        assert!(!real_path(&roundabout).to_string_lossy().contains(".."));

        let missing = Path::new("/definitely/not/here");
        assert_eq!(real_path(missing), missing);
    }
}
