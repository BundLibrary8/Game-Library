//! Release and update management: which version this is, and whether a
//! newer one has been released on GitHub.
//!
//! Every change merged into `main` is built by the GitHub Actions workflow
//! in `.github/workflows/release.yml`, which publishes `GameLib.exe` as a
//! GitHub release numbered `v<major>.<minor>.<build>`. That build has its
//! version baked in (`GAMELIB_VERSION`), and when it opens it asks GitHub
//! for the newest release, downloads a newer `GameLib.exe`, swaps it for
//! itself and restarts ([`install`], [`restart`]).
//!
//! A copy built from source (`cargo run`) has no baked-in version and never
//! replaces itself; it updates with `git pull`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// This build's version: the release number for a published build,
/// otherwise the one in `Cargo.toml`.
pub const VERSION: &str = match option_env!("GAMELIB_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};
/// Is this a published build (one that may update itself)?
pub const PUBLISHED: bool = option_env!("GAMELIB_VERSION").is_some();
/// Where releases are published.
pub const REPOSITORY: &str = "BundLibrary8/Game-Library";
/// The program attached to every release.
pub const EXE_NAME: &str = "GameLib.exe";
/// The setting that turns updating when GameLib opens on or off.
pub const AUTO_KEY: &str = "updates.auto";

/// A link that always downloads the newest GameLib: the one to share.
pub fn share_link() -> String {
    format!("https://github.com/{REPOSITORY}/releases/latest/download/{EXE_NAME}")
}

/// A release on GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub url: String,
    /// The release notes.
    pub notes: String,
    /// A direct download of the Windows build, if the release has one.
    pub download: Option<String>,
    /// The program itself (`GameLib.exe`), for updating in place.
    pub exe: Option<String>,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

/// "v1.2.3" or "1.2" → [1, 2, 3] / [1, 2, 0]. Anything after a `-` (like
/// `-beta`) is ignored.
pub fn parse_version(text: &str) -> Option<[u64; 3]> {
    let text = text.trim().trim_start_matches(['v', 'V']);
    let text = text.split(['-', '+']).next()?;
    let mut parts = [0u64; 3];
    let mut count = 0;
    for (i, part) in text.split('.').enumerate() {
        if i >= 3 {
            return None;
        }
        parts[i] = part.parse().ok()?;
        count += 1;
    }
    (count > 0).then_some(parts)
}

/// Is `candidate` newer than `current`?
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// Read GitHub's list of releases (newest first) and pick the newest
/// proper (not draft, not pre-release) one.
pub fn parse_releases(json: &str) -> Result<Option<Release>> {
    let releases: Vec<ApiRelease> =
        serde_json::from_str(json).context("GitHub sent back something unexpected")?;
    let newest = releases
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease && parse_version(&r.tag_name).is_some())
        .max_by_key(|r| parse_version(&r.tag_name));
    Ok(newest.map(|r| Release {
        version: r.tag_name.trim_start_matches(['v', 'V']).to_string(),
        download: r
            .assets
            .iter()
            .find(|a| {
                let name = a.name.to_lowercase();
                name.ends_with(".zip") || name.ends_with(".exe")
            })
            .map(|a| a.browser_download_url.clone()),
        exe: r
            .assets
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(EXE_NAME))
            .map(|a| a.browser_download_url.clone()),
        url: r.html_url,
        notes: r.body.unwrap_or_default(),
    }))
}

/// What checking for updates found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheck {
    UpToDate,
    Available(Release),
    /// No releases have been published yet.
    NoReleases,
}

/// GitHub's list of releases. `GAMELIB_RELEASES_URL` points it somewhere
/// else, for testing updates against a pretend GitHub.
fn releases_url() -> String {
    std::env::var("GAMELIB_RELEASES_URL")
        .unwrap_or_else(|_| format!("https://api.github.com/repos/{REPOSITORY}/releases"))
}

/// Ask GitHub for the newest release.
pub fn check() -> Result<UpdateCheck> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("gamelib/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()?;
    let response = client
        .get(releases_url())
        .header("Accept", "application/vnd.github+json")
        .send()
        .context("couldn't reach GitHub (check your internet connection)")?;
    match response.status().as_u16() {
        200 => {}
        404 => bail!(
            "GitHub didn't show the releases. If the repository is private, check the releases page on GitHub yourself."
        ),
        403 | 429 => bail!("GitHub is limiting requests right now. Try again in an hour."),
        code => bail!("GitHub answered with an error ({code})"),
    }
    Ok(match parse_releases(&response.text()?)? {
        None => UpdateCheck::NoReleases,
        Some(release) if is_newer(&release.version, VERSION) => UpdateCheck::Available(release),
        Some(_) => UpdateCheck::UpToDate,
    })
}

/// For a published build: if GitHub has a newer one, download it and put
/// it in place of this program. Returns the version installed, if any.
pub fn update_itself() -> Result<Option<String>> {
    if !PUBLISHED {
        return Ok(None);
    }
    let UpdateCheck::Available(release) = check()? else {
        return Ok(None);
    };
    if release.exe.is_none() {
        return Ok(None);
    }
    let exe = std::env::current_exe().context("couldn't find GameLib's own program file")?;
    install(&release, &exe)?;
    Ok(Some(release.version))
}

/// Where a downloaded update waits, and where the old program goes while
/// the new one takes its place (Windows lets a running program be renamed,
/// but not overwritten).
fn side_paths(exe: &Path) -> (PathBuf, PathBuf) {
    (exe.with_extension("new.exe"), exe.with_extension("old.exe"))
}

/// Does this look like a Windows program, and not (say) an error page?
pub fn looks_like_program(bytes: &[u8]) -> bool {
    bytes.len() > 512 * 1024 && bytes.starts_with(b"MZ")
}

/// Download `release`'s `GameLib.exe` and put it in place of `exe`. The
/// running program carries on; the new one runs from the next start.
pub fn install(release: &Release, exe: &Path) -> Result<()> {
    let url = release
        .exe
        .as_deref()
        .context("the release has no GameLib.exe attached")?;
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("gamelib/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(600))
        .build()?;
    let response = client
        .get(url)
        .send()
        .context("couldn't download the update")?;
    if !response.status().is_success() {
        bail!(
            "GitHub answered the download with an error ({})",
            response.status()
        );
    }
    let bytes = response.bytes().context("the download was cut off")?;
    if !looks_like_program(&bytes) {
        bail!("the download isn't a Windows program");
    }
    let (new, _) = side_paths(exe);
    std::fs::write(&new, &bytes)
        .with_context(|| format!("couldn't save the update next to {}", exe.display()))?;
    swap_in(exe, &new)
}

/// Put `new` where `exe` is, keeping the old one as `….old.exe` until the
/// next start. If the second step fails, the old program is put back.
pub fn swap_in(exe: &Path, new: &Path) -> Result<()> {
    let (_, old) = side_paths(exe);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old).context("couldn't move the old GameLib aside")?;
    if let Err(err) = std::fs::rename(new, exe) {
        let _ = std::fs::rename(&old, exe);
        return Err(err).context("couldn't put the new GameLib in place");
    }
    Ok(())
}

/// Tidy up after an update: the old program and any half-finished download.
pub fn clean_up(exe: &Path) {
    let (new, old) = side_paths(exe);
    let _ = std::fs::remove_file(old);
    let _ = std::fs::remove_file(new);
}

/// [`clean_up`] in the background. Just after an update the old program
/// may still be closing, and Windows won't delete a program that's
/// running, so this tries again for a little while.
pub fn clean_up_soon(exe: PathBuf) {
    let _ = std::thread::Builder::new()
        .name("update-clean-up".into())
        .spawn(move || {
            let (_, old) = side_paths(&exe);
            for _ in 0..40 {
                clean_up(&exe);
                if !old.exists() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        });
}

/// Start GameLib again (the updated program), on its own.
pub fn restart(exe: &Path) -> Result<()> {
    let mut command = std::process::Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    command
        .spawn()
        .with_context(|| format!("couldn't start {}", exe.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert_eq!(parse_version("v1.2.3"), Some([1, 2, 3]));
        assert_eq!(parse_version("0.10"), Some([0, 10, 0]));
        assert_eq!(parse_version("2.0.0-beta.1"), Some([2, 0, 0]));
        assert_eq!(parse_version("latest"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("nonsense", "0.1.0"));
    }

    #[test]
    fn the_newest_real_release_is_picked() {
        let json = r#"[
            {"tag_name":"v0.4.0-rc1","html_url":"u4","prerelease":true,"draft":false,"assets":[]},
            {"tag_name":"v0.3.0","html_url":"u3","body":"Notes","draft":false,"prerelease":false,
             "assets":[{"name":"checksums.txt","browser_download_url":"c"},
                       {"name":"gamelib-windows.zip","browser_download_url":"z"}]},
            {"tag_name":"v0.10.0","html_url":"u10","draft":true,"prerelease":false,"assets":[]},
            {"tag_name":"v0.2.1","html_url":"u2","draft":false,"prerelease":false,"assets":[]}
        ]"#;
        let release = parse_releases(json).unwrap().unwrap();
        assert_eq!(release.version, "0.3.0");
        assert_eq!(release.url, "u3");
        assert_eq!(release.notes, "Notes");
        assert_eq!(release.download.as_deref(), Some("z"));
        assert_eq!(release.exe, None);
        assert_eq!(parse_releases("[]").unwrap(), None);
        assert!(parse_releases("{").is_err());
    }

    #[test]
    fn the_program_itself_is_found_among_the_files() {
        let json = r#"[{"tag_name":"v0.2.7","html_url":"u","draft":false,"prerelease":false,
            "assets":[{"name":"GameLib.exe","browser_download_url":"e"}]}]"#;
        let release = parse_releases(json).unwrap().unwrap();
        assert_eq!(release.exe.as_deref(), Some("e"));
        assert_eq!(release.download.as_deref(), Some("e"));
        assert!(share_link().ends_with("/releases/latest/download/GameLib.exe"));
    }

    #[test]
    fn only_a_real_program_is_installed() {
        let mut exe = b"MZ".to_vec();
        exe.resize(600 * 1024, 0);
        assert!(looks_like_program(&exe));
        assert!(!looks_like_program(b"MZ tiny"));
        assert!(!looks_like_program(&vec![b'<'; 600 * 1024]));
    }

    #[test]
    fn the_new_program_takes_the_old_ones_place() {
        let dir = std::env::temp_dir().join(format!("gamelib-update-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("GameLib.exe");
        let new = dir.join("GameLib.new.exe");
        std::fs::write(&exe, "old").unwrap();
        std::fs::write(&new, "new").unwrap();
        swap_in(&exe, &new).unwrap();
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), "new");
        assert_eq!(
            std::fs::read_to_string(dir.join("GameLib.old.exe")).unwrap(),
            "old"
        );
        assert!(!new.exists());
        // If the new one is missing, the old one is put back.
        assert!(swap_in(&exe, &new).is_err());
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), "new");
        clean_up(&exe);
        assert!(!dir.join("GameLib.old.exe").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
