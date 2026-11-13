//! EA app (and older Origin installs).
//!
//! Games the EA app installs add an entry to Windows' list of installed
//! programs, and put an `__Installer\installerdata.xml` file in their
//! install folder that names the game's EA "offer id". The EA app starts
//! a game from its offer id with an `origin2://` link.
//!
//! EA games bought on Steam also say "Electronic Arts" in that list, but
//! Steam uninstalls them (with a `steam://` link), so they are skipped
//! here and stay Steam games.

use anyhow::Result;

use super::GameSource;
use super::uninstall::{self, UninstallEntry};
use crate::model::{DetectedGame, LaunchTarget, Source};
use crate::sources::folder;

/// The first offer id (`<contentID>`) in an `installerdata.xml` file.
pub fn offer_id_from_installer_data(xml: &str) -> Option<String> {
    let start = xml.find("<contentID>")? + "<contentID>".len();
    let end = start + xml[start..].find("</contentID>")?;
    let id = xml[start..end].trim();
    // Offer ids look like `Origin.OFR.50.0002694`: letters, digits, dots.
    (!id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_'))
    .then(|| id.to_string())
}

/// The EA app link that starts a game.
pub fn launch_uri(offer_id: &str) -> String {
    format!("origin2://game/launch?offerIds={offer_id}&autoDownload=1")
}

/// Program names that are the launcher itself, not a game.
const NOT_GAMES: [&str; 4] = ["ea app", "ea", "origin", "ea desktop"];

/// Turn an installed-programs entry into a game, if it is an EA app game.
/// The game's folder is looked in for its `installerdata.xml`.
pub fn game_from_entry(entry: &UninstallEntry) -> Option<DetectedGame> {
    let command = entry.uninstall_string.to_ascii_lowercase();
    if command.starts_with("steam://")
        || command.contains("steam.exe")
        || command.contains("epicgameslauncher")
    {
        return None; // bought elsewhere
    }
    let title = entry.display_name.trim();
    if title.is_empty() || NOT_GAMES.contains(&title.to_ascii_lowercase().as_str()) {
        return None;
    }
    let install_dir = entry.install_dir()?;
    let installer_data =
        std::fs::read_to_string(install_dir.join("__Installer").join("installerdata.xml")).ok();
    let by_ea = entry
        .publisher
        .to_ascii_lowercase()
        .contains("electronic arts")
        || command.contains("eainstaller")
        || command.contains("ea desktop")
        || command.contains("origin");
    // Something that looks like an EA game but has no installer data is
    // more likely a stray tool, so it needs both signs.
    if !by_ea || installer_data.is_none() {
        return None;
    }
    let offer_id = installer_data
        .as_deref()
        .and_then(offer_id_from_installer_data);
    let launch = match &offer_id {
        Some(id) => LaunchTarget::Uri(launch_uri(id)),
        None => {
            let name = install_dir.file_name()?.to_str()?;
            LaunchTarget::exe(folder::pick_best_exe(
                name,
                &folder::find_exes(&install_dir),
            )?)
        }
    };
    Some(DetectedGame {
        title: title.to_string(),
        source: Source::Ea,
        source_id: offer_id.unwrap_or_else(|| title.to_ascii_lowercase()),
        install_dir: Some(install_dir),
        launch,
    })
}

#[derive(Debug, Default)]
pub struct EaSource;

impl GameSource for EaSource {
    fn name(&self) -> &str {
        "EA app"
    }

    fn source(&self) -> Source {
        Source::Ea
    }

    fn detect(&self) -> Result<Vec<DetectedGame>> {
        Ok(uninstall::read_entries()?
            .iter()
            .filter_map(game_from_entry)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    const INSTALLER_DATA: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<DiPManifest version="4.0">
  <contentIDs>
    <contentID>Origin.OFR.50.0002694</contentID>
    <contentID>Origin.OFR.50.0002695</contentID>
  </contentIDs>
  <gameTitles><gameTitle locale="en_US">Apex Legends</gameTitle></gameTitles>
</DiPManifest>"#;

    fn ea_game(dir: &Path, name: &str, data: Option<&str>) -> UninstallEntry {
        let folder = dir.join(name);
        fs::create_dir_all(folder.join("__Installer")).unwrap();
        fs::write(folder.join(format!("{name}.exe")), b"x").unwrap();
        if let Some(xml) = data {
            fs::write(folder.join("__Installer").join("installerdata.xml"), xml).unwrap();
        }
        UninstallEntry {
            key: name.to_string(),
            display_name: name.to_string(),
            publisher: "Electronic Arts".into(),
            install_location: folder.to_string_lossy().into(),
            uninstall_string: format!(
                r#""C:\Program Files\Common Files\EAInstaller\{name}\Cleanup.exe" uninstall_game"#
            ),
            display_icon: String::new(),
        }
    }

    #[test]
    fn reads_the_offer_id() {
        assert_eq!(
            offer_id_from_installer_data(INSTALLER_DATA).as_deref(),
            Some("Origin.OFR.50.0002694")
        );
        assert_eq!(
            offer_id_from_installer_data("<contentIDs></contentIDs>"),
            None
        );
        assert_eq!(
            offer_id_from_installer_data("<contentID>bad id\"&x</contentID>"),
            None
        );
    }

    #[test]
    fn an_ea_game_launches_through_the_ea_app() {
        let dir = tempfile::tempdir().unwrap();
        let game = game_from_entry(&ea_game(dir.path(), "Apex", Some(INSTALLER_DATA))).unwrap();
        assert_eq!(game.title, "Apex");
        assert_eq!(game.source, Source::Ea);
        assert_eq!(game.source_id, "Origin.OFR.50.0002694");
        assert_eq!(
            game.launch,
            LaunchTarget::Uri(
                "origin2://game/launch?offerIds=Origin.OFR.50.0002694&autoDownload=1".into()
            )
        );
        assert_eq!(game.install_dir, Some(dir.path().join("Apex")));
    }

    #[test]
    fn without_an_offer_id_the_game_exe_is_used() {
        let dir = tempfile::tempdir().unwrap();
        let game =
            game_from_entry(&ea_game(dir.path(), "Old Game", Some("<DiPManifest/>"))).unwrap();
        assert_eq!(
            game.launch,
            LaunchTarget::exe(dir.path().join("Old Game").join("Old Game.exe"))
        );
        assert_eq!(game.source_id, "old game");
    }

    #[test]
    fn steam_copies_the_ea_app_and_unrelated_programs_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let mut steam_copy = ea_game(dir.path(), "Battlefield", Some(INSTALLER_DATA));
        steam_copy.uninstall_string = "steam://uninstall/1238810".into();
        assert_eq!(game_from_entry(&steam_copy), None);

        let mut app = ea_game(dir.path(), "EA app", Some(INSTALLER_DATA));
        app.display_name = "EA app".into();
        assert_eq!(game_from_entry(&app), None);

        // Says EA, but there's no installer data: not treated as a game.
        assert_eq!(game_from_entry(&ea_game(dir.path(), "EA Tool", None)), None);

        let mut other = ea_game(dir.path(), "Paint", Some(INSTALLER_DATA));
        other.publisher = "Someone Else".into();
        other.uninstall_string = r"C:\Paint\uninstall.exe".into();
        assert_eq!(game_from_entry(&other), None);
    }
}
