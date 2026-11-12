//! Epic Games: one JSON file per installed game, named `*.item`, in the
//! launcher's `Manifests` folder.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use super::GameSource;
use crate::model::{DetectedGame, LaunchTarget, Source};

/// Where the Epic Games Launcher keeps its manifests.
pub const DEFAULT_MANIFESTS_DIR: &str = r"C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests";

/// The parts of a `.item` file we use. serde ignores every other field.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Manifest {
    display_name: String,
    install_location: String,
    app_name: String,
    #[serde(default)]
    catalog_namespace: String,
    #[serde(default)]
    catalog_item_id: String,
    /// False for DLC and add-ons, which aren't launchable on their own.
    #[serde(rename = "bIsApplication", default = "yes")]
    is_application: bool,
}

fn yes() -> bool {
    true
}

/// The link that asks the Epic launcher to start a game. Launching through
/// Epic (rather than the exe) keeps online play and DRM working.
pub fn launch_uri(namespace: &str, item_id: &str, app_name: &str) -> String {
    format!(
        "com.epicgames.launcher://apps/{namespace}%3A{item_id}%3A{app_name}?action=launch&silent=true"
    )
}

/// Parse the text of one `.item` file. Returns `None` for things that
/// aren't games (DLC).
pub fn parse_manifest(json: &str) -> Result<Option<DetectedGame>> {
    let manifest: Manifest = serde_json::from_str(json).context("not a valid Epic manifest")?;
    if !manifest.is_application {
        return Ok(None);
    }
    // Older manifests may lack the catalogue ids; the app name alone works too.
    let uri = if manifest.catalog_namespace.is_empty() || manifest.catalog_item_id.is_empty() {
        format!(
            "com.epicgames.launcher://apps/{}?action=launch&silent=true",
            manifest.app_name
        )
    } else {
        launch_uri(
            &manifest.catalog_namespace,
            &manifest.catalog_item_id,
            &manifest.app_name,
        )
    };
    Ok(Some(DetectedGame {
        title: manifest.display_name,
        source: Source::Epic,
        source_id: manifest.app_name,
        install_dir: Some(PathBuf::from(manifest.install_location)),
        launch: LaunchTarget::Uri(uri),
    }))
}

/// Read every `.item` file in a folder. A corrupt file is logged and
/// skipped; it doesn't stop the others.
pub fn read_manifests_dir(dir: &Path) -> Result<Vec<DetectedGame>> {
    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("Epic's manifest folder wasn't found ({})", dir.display()))?;

    let mut games = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("item") {
            continue;
        }
        let parsed = std::fs::read_to_string(&path)
            .map_err(anyhow::Error::from)
            .and_then(|text| parse_manifest(&text));
        match parsed {
            Ok(Some(game)) => games.push(game),
            Ok(None) => {}
            Err(err) => log::warn!("skipping Epic manifest {}: {err:#}", path.display()),
        }
    }
    games.sort_by(|a, b| a.title.cmp(&b.title));
    Ok(games)
}

#[derive(Debug)]
pub struct EpicSource {
    pub manifests_dir: PathBuf,
}

impl Default for EpicSource {
    fn default() -> Self {
        EpicSource {
            manifests_dir: PathBuf::from(DEFAULT_MANIFESTS_DIR),
        }
    }
}

impl GameSource for EpicSource {
    fn name(&self) -> &str {
        "Epic Games"
    }

    fn source(&self) -> Source {
        Source::Epic
    }

    fn detect(&self) -> Result<Vec<DetectedGame>> {
        read_manifests_dir(&self.manifests_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAME: &str = include_str!("../../tests/fixtures/epic_game.item");
    const DLC: &str = include_str!("../../tests/fixtures/epic_dlc.item");

    #[test]
    fn parses_title_folder_and_launch_link() {
        let game = parse_manifest(GAME).unwrap().expect("a game");
        assert_eq!(game.title, "Ember Road");
        assert_eq!(game.source, Source::Epic);
        assert_eq!(game.source_id, "a1b2c3EmberRoad");
        assert_eq!(game.install_dir, Some(PathBuf::from(r"D:\Epic\EmberRoad")));
        assert_eq!(
            game.launch,
            LaunchTarget::Uri(
                "com.epicgames.launcher://apps/emberns%3A0123456789abcdef%3Aa1b2c3EmberRoad?action=launch&silent=true"
                    .into()
            )
        );
    }

    #[test]
    fn dlc_is_not_a_game() {
        assert_eq!(parse_manifest(DLC).unwrap(), None);
    }

    #[test]
    fn manifest_without_catalogue_ids_falls_back_to_the_app_name() {
        let json = r#"{"DisplayName":"Old","InstallLocation":"C:\\Old","AppName":"OldApp"}"#;
        let game = parse_manifest(json).unwrap().unwrap();
        assert_eq!(
            game.launch,
            LaunchTarget::Uri(
                "com.epicgames.launcher://apps/OldApp?action=launch&silent=true".into()
            )
        );
    }

    #[test]
    fn corrupt_manifest_is_an_error() {
        assert!(parse_manifest("{ this is not json").is_err());
        assert!(parse_manifest(r#"{"DisplayName":"No other fields"}"#).is_err());
    }

    #[test]
    fn a_folder_of_manifests_skips_corrupt_files_dlc_and_other_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("AAA.item"), GAME).unwrap();
        std::fs::write(dir.path().join("BBB.item"), DLC).unwrap();
        std::fs::write(dir.path().join("CCC.item"), "corrupt!").unwrap();
        std::fs::write(dir.path().join("notes.txt"), GAME).unwrap();

        let games = read_manifests_dir(dir.path()).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].title, "Ember Road");
    }

    #[test]
    fn missing_folder_means_epic_is_not_installed() {
        let source = EpicSource {
            manifests_dir: PathBuf::from("/definitely/not/here"),
        };
        assert!(source.detect().is_err());
    }
}
