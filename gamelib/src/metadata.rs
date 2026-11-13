//! Cover art and game info.
//!
//! - **Covers** (portrait, 600×900) come from SteamGridDB, which needs a
//!   free API key entered in Settings.
//! - **Description, genres and release year** for Steam games come from
//!   the Steam Store, which needs no key.
//!
//! Everything here that touches the network is slow: call it from the
//! worker thread. The `parse_*` functions are pure and are tested against
//! saved responses in `tests/fixtures/`; tests never call the real APIs.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::Url;
use reqwest::blocking::Client;
use serde::Deserialize;

use crate::model::{Game, Source};

const SGDB_API: &str = "https://www.steamgriddb.com/api/v2";
const STEAM_STORE_API: &str = "https://store.steampowered.com/api/appdetails";
const COVER_DIMENSIONS: &str = "600x900";
/// Wait this long between requests, to be polite and avoid rate limits.
pub const REQUEST_PAUSE: Duration = Duration::from_millis(500);
/// Refuse cover downloads bigger than this.
const MAX_COVER_BYTES: usize = 20 * 1024 * 1024;

/// An HTTP client set up for GameLib's requests.
pub fn http_client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent(concat!("gamelib/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()?)
}

// --- SteamGridDB ---------------------------------------------------------

#[derive(Deserialize)]
struct SgdbResponse<T> {
    success: bool,
    #[serde(default = "Vec::new")]
    data: Vec<T>,
}

#[derive(Deserialize)]
struct SgdbGrid {
    url: String,
}

/// One result of a SteamGridDB title search.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SgdbGame {
    pub id: u64,
    pub name: String,
}

/// The image URLs in a SteamGridDB "grids" response, best first.
pub fn parse_grids(json: &str) -> Result<Vec<String>> {
    let parsed: SgdbResponse<SgdbGrid> =
        serde_json::from_str(json).context("unexpected reply from SteamGridDB")?;
    if !parsed.success {
        bail!("SteamGridDB reported a failure");
    }
    Ok(parsed.data.into_iter().map(|grid| grid.url).collect())
}

/// The games in a SteamGridDB search response, best match first.
pub fn parse_search(json: &str) -> Result<Vec<SgdbGame>> {
    let parsed: SgdbResponse<SgdbGame> =
        serde_json::from_str(json).context("unexpected reply from SteamGridDB")?;
    if !parsed.success {
        bail!("SteamGridDB reported a failure");
    }
    Ok(parsed.data)
}

/// Build a SteamGridDB URL from path segments, escaping each one (game
/// titles contain spaces, slashes, question marks…).
fn sgdb_url(segments: &[&str]) -> Result<Url> {
    let mut url = Url::parse(SGDB_API)?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("bad SteamGridDB base URL"))?
        .extend(segments);
    Ok(url)
}

fn sgdb_get(client: &Client, api_key: &str, url: Url) -> Result<String> {
    let response = client
        .get(url)
        .bearer_auth(api_key)
        .send()
        .context("couldn't reach SteamGridDB")?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        bail!("SteamGridDB rejected the API key (check it in Settings)");
    }
    Ok(response.error_for_status()?.text()?)
}

fn grids_for(client: &Client, api_key: &str, segments: &[&str]) -> Result<Vec<String>> {
    let mut url = sgdb_url(segments)?;
    url.query_pairs_mut()
        .append_pair("dimensions", COVER_DIMENSIONS);
    parse_grids(&sgdb_get(client, api_key, url)?)
}

/// Find a cover image for a game. Steam games are looked up by app id (an
/// exact match); everything else by searching for the title.
/// `search_title` overrides the title ("Search again" with a better name).
/// Returns `None` if SteamGridDB has nothing.
pub fn find_cover_url(
    client: &Client,
    api_key: &str,
    game: &Game,
    search_title: Option<&str>,
) -> Result<Option<String>> {
    if game.source == Source::Steam && search_title.is_none() {
        // A 404 here just means "no art for that app id": fall through to
        // the title search rather than failing.
        if let Ok(urls) = grids_for(client, api_key, &["grids", "steam", &game.source_id])
            && let Some(url) = urls.into_iter().next()
        {
            return Ok(Some(url));
        }
    }

    let title = search_title.unwrap_or(&game.title).trim();
    if title.is_empty() {
        return Ok(None);
    }
    let search = sgdb_get(
        client,
        api_key,
        sgdb_url(&["search", "autocomplete", title])?,
    )?;
    let Some(best) = parse_search(&search)?.into_iter().next() else {
        return Ok(None);
    };
    let urls = grids_for(client, api_key, &["grids", "game", &best.id.to_string()])?;
    Ok(urls.into_iter().next())
}

/// Download a file into memory, refusing anything suspiciously large.
pub fn download_bytes(client: &Client, url: &str) -> Result<Vec<u8>> {
    let bytes = client
        .get(url)
        .send()
        .with_context(|| format!("couldn't reach {url}"))?
        .error_for_status()?
        .bytes()?;
    if bytes.is_empty() || bytes.len() > MAX_COVER_BYTES {
        bail!(
            "the download was an unexpected size ({} bytes)",
            bytes.len()
        );
    }
    Ok(bytes.to_vec())
}

/// Download a picture and save it as a game's cover (resized to the
/// standard cover size). Returns where it was saved.
pub fn download_cover(
    client: &Client,
    url: &str,
    covers_dir: &Path,
    game_id: i64,
) -> Result<PathBuf> {
    let picture = download_bytes(client, url)?;
    crate::artwork::save_cover(&picture, covers_dir, game_id)
}

// --- Steam Store ---------------------------------------------------------

/// Description, genres and release year from the Steam Store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreInfo {
    pub description: Option<String>,
    pub genres: Vec<String>,
    pub release_year: Option<u16>,
    /// The game's wide store banner: a fallback when it has no cover.
    pub header_image: Option<String>,
    /// Minimum and recommended PC requirements, as plain text.
    pub requirements_min: Option<String>,
    pub requirements_rec: Option<String>,
}

#[derive(Deserialize)]
struct AppDetailsEntry {
    success: bool,
    data: Option<AppDetails>,
}

#[derive(Deserialize)]
struct AppDetails {
    short_description: Option<String>,
    #[serde(default)]
    genres: Vec<Genre>,
    release_date: Option<ReleaseDate>,
    header_image: Option<String>,
    /// An object with "minimum" and "recommended" HTML, or an empty list
    /// when the store has none, so it is read loosely.
    #[serde(default)]
    pc_requirements: serde_json::Value,
}

#[derive(Deserialize)]
struct Genre {
    description: String,
}

#[derive(Deserialize)]
struct ReleaseDate {
    #[serde(default)]
    date: String,
}

/// Find a plausible year in a date like "9 Jul, 2013" or "2013年7月9日".
pub fn parse_year(date: &str) -> Option<u16> {
    date.split(|c: char| !c.is_ascii_digit())
        .filter(|part| part.len() == 4)
        .filter_map(|part| part.parse::<u16>().ok())
        .find(|year| (1970..=2100).contains(year))
}

/// One part ("minimum" or "recommended") of the store's requirements, as
/// plain text without its "Minimum:" heading.
fn requirements_text(requirements: &serde_json::Value, part: &str) -> Option<String> {
    let html = requirements.get(part)?.as_str()?;
    let text = crate::hardware::html_to_text(html);
    let text = text
        .lines()
        .filter(|line| !matches!(line.to_lowercase().as_str(), "minimum:" | "recommended:"))
        .collect::<Vec<_>>()
        .join("\n");
    (!text.is_empty()).then_some(text)
}

/// Parse a Steam Store `appdetails` response. Returns `None` when the
/// store has no page for the app (delisted games, tools).
pub fn parse_store_details(json: &str, app_id: &str) -> Result<Option<StoreInfo>> {
    let mut parsed: std::collections::HashMap<String, AppDetailsEntry> =
        serde_json::from_str(json).context("unexpected reply from the Steam Store")?;
    let Some(entry) = parsed.remove(app_id) else {
        return Ok(None);
    };
    let (true, Some(data)) = (entry.success, entry.data) else {
        return Ok(None);
    };
    Ok(Some(StoreInfo {
        description: data
            .short_description
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty()),
        genres: data.genres.into_iter().map(|g| g.description).collect(),
        release_year: data.release_date.and_then(|d| parse_year(&d.date)),
        // Only a proper web address is kept; it will be downloaded later.
        header_image: data.header_image.filter(|url| url.starts_with("https://")),
        requirements_min: requirements_text(&data.pc_requirements, "minimum"),
        requirements_rec: requirements_text(&data.pc_requirements, "recommended"),
    }))
}

/// Fetch a Steam game's store info.
pub fn fetch_store_info(client: &Client, app_id: &str) -> Result<Option<StoreInfo>> {
    let json = client
        .get(STEAM_STORE_API)
        .query(&[("appids", app_id), ("l", "english")])
        .send()
        .context("couldn't reach the Steam Store")?
        .error_for_status()?
        .text()?;
    parse_store_details(&json, app_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRIDS: &str = include_str!("../tests/fixtures/sgdb_grids.json");
    const SEARCH: &str = include_str!("../tests/fixtures/sgdb_search.json");
    const APPDETAILS: &str = include_str!("../tests/fixtures/steam_appdetails.json");

    #[test]
    fn parses_grid_urls_in_order() {
        assert_eq!(
            parse_grids(GRIDS).unwrap(),
            [
                "https://cdn2.steamgriddb.com/grid/aaaa.png",
                "https://cdn2.steamgriddb.com/grid/bbbb.jpg"
            ]
        );
    }

    #[test]
    fn parses_search_results() {
        let games = parse_search(SEARCH).unwrap();
        assert_eq!(
            games[0],
            SgdbGame {
                id: 4242,
                name: "Ember Road".into()
            }
        );
        assert_eq!(games.len(), 2);
    }

    #[test]
    fn empty_and_failed_responses() {
        assert!(
            parse_grids(r#"{"success":true,"data":[]}"#)
                .unwrap()
                .is_empty()
        );
        assert!(parse_grids(r#"{"success":false,"errors":["nope"]}"#).is_err());
        assert!(parse_search("<html>rate limited</html>").is_err());
    }

    #[test]
    fn titles_are_escaped_in_urls() {
        let url = sgdb_url(&["search", "autocomplete", "What? A/B & 100%"]).unwrap();
        assert_eq!(
            url.as_str(),
            "https://www.steamgriddb.com/api/v2/search/autocomplete/What%3F%20A%2FB%20&%20100%25"
        );
    }

    #[test]
    fn parses_steam_store_details() {
        let info = parse_store_details(APPDETAILS, "570").unwrap().unwrap();
        assert!(info.description.unwrap().starts_with("Every day, millions"));
        assert_eq!(info.genres, ["Action", "Strategy", "Free To Play"]);
        assert_eq!(info.release_year, Some(2013));
        assert_eq!(
            info.header_image.as_deref(),
            Some("https://cdn.akamai.steamstatic.com/steam/apps/570/header.jpg")
        );
        let minimum = info.requirements_min.unwrap();
        assert!(minimum.contains("Memory: 4 GB RAM"), "{minimum}");
        assert!(!minimum.contains("Minimum"), "the heading is dropped");
        assert_eq!(info.requirements_rec, None);
    }

    #[test]
    fn store_has_no_page_for_some_apps() {
        assert_eq!(
            parse_store_details(r#"{"123":{"success":false}}"#, "123").unwrap(),
            None
        );
        assert_eq!(parse_store_details(APPDETAILS, "999").unwrap(), None);
        // A game with no requirements sends an empty list instead.
        let bare = r#"{"5":{"success":true,"data":{"pc_requirements":[]}}}"#;
        let info = parse_store_details(bare, "5").unwrap().unwrap();
        assert_eq!(info.requirements_min, None);
        assert!(parse_store_details("[]", "570").is_err());
    }

    #[test]
    fn years_from_dates() {
        assert_eq!(parse_year("9 Jul, 2013"), Some(2013));
        assert_eq!(parse_year("2020"), Some(2020));
        assert_eq!(parse_year("Q4 2027"), Some(2027));
        assert_eq!(parse_year("Coming soon"), None);
        assert_eq!(parse_year(""), None);
        assert_eq!(parse_year("12345"), None);
    }
}
