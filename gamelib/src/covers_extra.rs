//! More places to find cover art, for games that aren't on Steam
//! (Fortnite, Valorant, League of Legends…). Neither needs a key.
//!
//! - **The Epic Games Launcher's catalogue.** The launcher keeps a copy of
//!   Epic's catalogue on the PC (`catcache.bin`: base64-encoded JSON). It
//!   lists every game's official artwork, including the tall box art.
//! - **Wikipedia.** A game's article almost always opens with its cover
//!   art or logo. Wikipedia's search API returns that picture's address.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::Deserialize;

/// Where the Epic launcher keeps its catalogue cache.
pub const EPIC_CATCACHE: &str = r"C:\ProgramData\Epic\EpicGamesLauncher\Data\Catalog\catcache.bin";

/// Picture types in Epic's catalogue, most cover-like first.
const EPIC_IMAGE_TYPES: [&str; 6] = [
    "DieselGameBoxTall",
    "OfferImageTall",
    "DieselStoreFrontTall",
    "DieselGameBox",
    "OfferImageWide",
    "Thumbnail",
];

/// Decode standard base64 (with or without padding and line breaks).
pub fn base64_decode(text: &str) -> Result<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    };
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0;
    for c in text.bytes() {
        if c == b'=' {
            break;
        }
        if c.is_ascii_whitespace() {
            continue;
        }
        let v = value(c).context("the file isn't valid base64")?;
        buffer = (buffer << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
struct CatalogItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default, rename = "keyImages")]
    key_images: Vec<KeyImage>,
    #[serde(default, rename = "releaseInfo")]
    release_info: Vec<ReleaseInfo>,
}

#[derive(Deserialize)]
struct KeyImage {
    #[serde(rename = "type")]
    kind: String,
    url: String,
}

#[derive(Deserialize)]
struct ReleaseInfo {
    #[serde(default, rename = "appId")]
    app_id: String,
}

fn simplify(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Find a game's best cover picture in Epic's catalogue (the decoded JSON).
/// It's matched by Epic app name (what GameLib stores as the source id),
/// then by catalogue item id, then by title.
pub fn epic_cover_url(
    catalog_json: &str,
    app_name: &str,
    item_id: Option<&str>,
    title: &str,
) -> Result<Option<String>> {
    let items: Vec<CatalogItem> =
        serde_json::from_str(catalog_json).context("Epic's catalogue isn't readable")?;
    let by_app = items.iter().find(|item| {
        item.release_info
            .iter()
            .any(|r| !r.app_id.is_empty() && r.app_id.eq_ignore_ascii_case(app_name))
    });
    let by_id = || item_id.and_then(|id| items.iter().find(|item| item.id == id));
    let wanted = simplify(title);
    let by_title = || {
        items
            .iter()
            .find(|item| !wanted.is_empty() && simplify(&item.title) == wanted)
    };
    let Some(item) = by_app.or_else(by_id).or_else(by_title) else {
        return Ok(None);
    };
    Ok(EPIC_IMAGE_TYPES.iter().find_map(|kind| {
        item.key_images
            .iter()
            .find(|image| image.kind == *kind && image.url.starts_with("https://"))
            .map(|image| image.url.clone())
    }))
}

/// The catalogue item id in an Epic launch link
/// (`…/apps/<namespace>%3A<item id>%3A<app name>?…`).
pub fn epic_item_id(launch_uri: &str) -> Option<String> {
    let apps = launch_uri.split("/apps/").nth(1)?;
    let ids = apps.split('?').next()?;
    let parts: Vec<&str> = ids.split("%3A").collect();
    (parts.len() == 3).then(|| parts[1].to_string())
}

/// Read and decode Epic's catalogue cache, if the launcher is installed.
pub fn read_epic_catalog(path: &Path) -> Result<String> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("couldn't read {}", path.display()))?;
    let bytes = base64_decode(text.trim())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn epic_catcache_path() -> PathBuf {
    PathBuf::from(EPIC_CATCACHE)
}

/// Epic's catalogue, read from disk the first time it's needed and kept
/// (it's several megabytes, and the same for every game).
pub fn epic_catalog() -> Option<&'static str> {
    static CATALOG: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    CATALOG
        .get_or_init(|| match read_epic_catalog(&epic_catcache_path()) {
            Ok(text) => Some(text),
            Err(err) => {
                log::info!("no Epic catalogue: {err:#}");
                None
            }
        })
        .as_deref()
}

// --- Wikipedia -------------------------------------------------------------

#[derive(Deserialize)]
struct WikiReply {
    #[serde(default)]
    query: Option<WikiQuery>,
}

#[derive(Deserialize)]
struct WikiQuery {
    #[serde(default)]
    pages: Vec<WikiPage>,
}

#[derive(Deserialize)]
struct WikiPage {
    title: String,
    #[serde(default)]
    index: u32,
    #[serde(default)]
    original: Option<WikiImage>,
}

#[derive(Deserialize)]
struct WikiImage {
    source: String,
}

/// Pick the lead picture of the article that is about this game. The
/// article's title must start with the game's name, which keeps out
/// articles that merely mention it.
pub fn parse_wikipedia(json: &str, title: &str) -> Result<Option<String>> {
    let reply: WikiReply =
        serde_json::from_str(json).context("Wikipedia sent back something unexpected")?;
    let wanted = simplify(title);
    let mut pages = reply.query.map(|q| q.pages).unwrap_or_default();
    pages.sort_by_key(|p| p.index);
    Ok(pages
        .into_iter()
        .filter(|p| !wanted.is_empty() && simplify(&p.title).starts_with(&wanted))
        .filter_map(|p| p.original.map(|o| o.source))
        // SVG logos can't be drawn as covers.
        .find(|url| {
            let lower = url.to_lowercase();
            url.starts_with("https://") && !lower.ends_with(".svg")
        }))
}

/// Look for a game's article on Wikipedia and return its lead picture.
pub fn wikipedia_cover_url(client: &Client, title: &str) -> Result<Option<String>> {
    let search = format!("{title} video game");
    let json = client
        .get("https://en.wikipedia.org/w/api.php")
        .query(&[
            ("action", "query"),
            ("format", "json"),
            ("formatversion", "2"),
            ("generator", "search"),
            ("gsrsearch", search.as_str()),
            ("gsrlimit", "5"),
            ("prop", "pageimages"),
            ("piprop", "original"),
            ("pilicense", "any"),
        ])
        .send()
        .context("couldn't reach Wikipedia")?
        .error_for_status()?
        .text()?;
    parse_wikipedia(&json, title)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATALOG: &str = include_str!("../tests/fixtures/epic_catalog.json");
    const WIKI: &str = include_str!("../tests/fixtures/wikipedia_valorant.json");

    #[test]
    fn base64_round_trips() {
        assert_eq!(base64_decode("aGVsbG8gd29ybGQ=").unwrap(), b"hello world");
        assert_eq!(base64_decode("aGVs\nbG8").unwrap(), b"hello");
        assert!(base64_decode("not*base64").is_err());
    }

    #[test]
    fn epic_covers_are_found_by_app_name_id_or_title() {
        assert_eq!(
            epic_cover_url(CATALOG, "Fortnite", None, "Fortnite")
                .unwrap()
                .as_deref(),
            Some("https://cdn1.epicgames.com/fortnite-tall.jpg")
        );
        assert_eq!(
            epic_cover_url(CATALOG, "unknown", Some("abc123"), "x")
                .unwrap()
                .as_deref(),
            Some("https://cdn1.epicgames.com/rl-wide.jpg"),
            "no tall picture, so the wide box art"
        );
        assert_eq!(
            epic_cover_url(CATALOG, "unknown", None, "rocket league")
                .unwrap()
                .as_deref(),
            Some("https://cdn1.epicgames.com/rl-wide.jpg")
        );
        assert_eq!(epic_cover_url(CATALOG, "nope", None, "Nope").unwrap(), None);
    }

    #[test]
    fn epic_launch_links_give_the_item_id() {
        assert_eq!(
            epic_item_id("com.epicgames.launcher://apps/fn%3A4fe75bbc%3AFortnite?action=launch")
                .as_deref(),
            Some("4fe75bbc")
        );
        assert_eq!(
            epic_item_id("com.epicgames.launcher://apps/Fortnite?action=launch"),
            None
        );
    }

    #[test]
    fn wikipedia_picks_the_games_own_article() {
        assert_eq!(
            parse_wikipedia(WIKI, "VALORANT").unwrap().as_deref(),
            Some("https://upload.wikimedia.org/wikipedia/en/b/ba/Valorant_cover.jpg")
        );
        assert_eq!(parse_wikipedia(WIKI, "League of Legends").unwrap(), None);
        assert_eq!(
            parse_wikipedia(r#"{"batchcomplete":true}"#, "x").unwrap(),
            None
        );
    }
}
