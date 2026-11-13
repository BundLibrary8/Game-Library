//! Release and update management: which version this is, and whether a
//! newer one has been released on GitHub.
//!
//! Releases are made by pushing a tag like `v0.2.0`; the GitHub Actions
//! workflow in `.github/workflows/release.yml` builds `gamelib.exe` and
//! attaches it to a GitHub release. This module asks GitHub for the newest
//! release and compares version numbers. It never downloads or installs
//! anything by itself: it tells you, and links to the release page.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// This build's version, from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Where releases are published.
pub const REPOSITORY: &str = "BundLibrary8/Game-Library";

/// A release on GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub url: String,
    /// The release notes.
    pub notes: String,
    /// A direct download of the Windows build, if the release has one.
    pub download: Option<String>,
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

/// Ask GitHub for the newest release.
pub fn check() -> Result<UpdateCheck> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("gamelib/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()?;
    let response = client
        .get(format!(
            "https://api.github.com/repos/{REPOSITORY}/releases"
        ))
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
        assert_eq!(parse_releases("[]").unwrap(), None);
        assert!(parse_releases("{").is_err());
    }
}
