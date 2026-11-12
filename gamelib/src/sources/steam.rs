//! Steam: found with the `steamlocate` crate, which reads
//! `libraryfolders.vdf` (every library, on every drive) and each
//! `appmanifest_<appid>.acf`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use steamlocate::SteamDir;

use super::GameSource;
use crate::model::{DetectedGame, LaunchTarget, Source};

/// App ids that are installed alongside games but aren't games.
const SKIP_APP_IDS: &[u32] = &[
    228980,  // Steamworks Common Redistributables
    1070560, // Steam Linux Runtime
    1391110, // Steam Linux Runtime - Soldier
    1628350, // Steam Linux Runtime - Sniper
    1493710, // Proton Experimental
    250820,  // SteamVR
];

/// Names that mark a tool rather than a game.
const SKIP_NAME_PREFIXES: &[&str] = &["steamworks", "proton ", "steam linux runtime"];

/// Should this Steam app be left out of the library?
pub fn is_tool(app_id: u32, name: &str) -> bool {
    let name = name.to_lowercase();
    SKIP_APP_IDS.contains(&app_id) || SKIP_NAME_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// The link that asks Steam to start a game.
pub fn launch_uri(app_id: u32) -> String {
    format!("steam://rungameid/{app_id}")
}

#[derive(Debug, Default)]
pub struct SteamSource {
    /// Where Steam is installed. `None` means find it automatically.
    pub steam_dir: Option<PathBuf>,
}

impl GameSource for SteamSource {
    fn name(&self) -> &str {
        "Steam"
    }

    fn source(&self) -> Source {
        Source::Steam
    }

    fn detect(&self) -> Result<Vec<DetectedGame>> {
        let steam = match &self.steam_dir {
            Some(dir) => SteamDir::from_dir(dir),
            None => SteamDir::locate(),
        }
        .context("Steam doesn't seem to be installed")?;

        let mut games = Vec::new();
        for library in steam
            .libraries()
            .context("couldn't read Steam's library list")?
        {
            // One unreadable library or manifest is skipped, not fatal.
            let library = match library {
                Ok(library) => library,
                Err(err) => {
                    log::warn!("skipping a Steam library: {err}");
                    continue;
                }
            };
            for app in library.apps() {
                let app = match app {
                    Ok(app) => app,
                    Err(err) => {
                        log::warn!("skipping a Steam app manifest: {err}");
                        continue;
                    }
                };
                let title = app.name.clone().unwrap_or_else(|| app.install_dir.clone());
                if is_tool(app.app_id, &title) {
                    continue;
                }
                games.push(DetectedGame {
                    title,
                    source: Source::Steam,
                    source_id: app.app_id.to_string(),
                    install_dir: Some(library.resolve_app_dir(&app)),
                    launch: LaunchTarget::Uri(launch_uri(app.app_id)),
                });
            }
        }
        Ok(games)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn tools_and_redistributables_are_skipped() {
        assert!(is_tool(228980, "Steamworks Common Redistributables"));
        assert!(is_tool(999, "Proton 9.0"));
        assert!(is_tool(999, "Steam Linux Runtime 3.0 (sniper)"));
        assert!(!is_tool(570, "Dota 2"));
        assert!(
            !is_tool(999, "Protonaut"),
            "a game that merely starts with 'Proton'"
        );
    }

    #[test]
    fn launch_link() {
        assert_eq!(launch_uri(570), "steam://rungameid/570");
    }

    fn write_manifest(steamapps: &std::path::Path, app_id: u32, name: &str, dir: &str) {
        let acf = format!(
            "\"AppState\"\n{{\n\t\"appid\"\t\t\"{app_id}\"\n\t\"name\"\t\t\"{name}\"\n\t\"installdir\"\t\t\"{dir}\"\n}}\n"
        );
        fs::write(steamapps.join(format!("appmanifest_{app_id}.acf")), acf).unwrap();
    }

    #[test]
    fn reads_a_fake_steam_install() {
        let root = tempfile::tempdir().unwrap();
        let steamapps = root.path().join("steamapps");
        fs::create_dir_all(steamapps.join("common")).unwrap();
        // steamlocate looks for the library list here.
        let vdf = format!(
            "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t\t\"apps\"\n\t\t{{\n\t\t\t\"570\"\t\t\"1\"\n\t\t\t\"228980\"\t\t\"1\"\n\t\t}}\n\t}}\n}}\n",
            root.path().to_string_lossy().replace('\\', "\\\\")
        );
        fs::write(steamapps.join("libraryfolders.vdf"), vdf).unwrap();
        write_manifest(&steamapps, 570, "Dota 2", "dota 2 beta");
        write_manifest(
            &steamapps,
            228980,
            "Steamworks Common Redistributables",
            "Steamworks Shared",
        );

        let source = SteamSource {
            steam_dir: Some(root.path().to_path_buf()),
        };
        let games = source.detect().unwrap();
        assert_eq!(
            games.len(),
            1,
            "the redistributables are skipped: {games:?}"
        );
        assert_eq!(games[0].title, "Dota 2");
        assert_eq!(games[0].source_id, "570");
        assert_eq!(
            games[0].launch,
            LaunchTarget::Uri("steam://rungameid/570".into())
        );
        assert_eq!(
            games[0].install_dir,
            Some(steamapps.join("common").join("dota 2 beta"))
        );
    }

    #[test]
    fn a_folder_without_steam_is_an_error() {
        let empty = tempfile::tempdir().unwrap();
        let source = SteamSource {
            steam_dir: Some(empty.path().to_path_buf()),
        };
        assert!(source.detect().is_err());
    }
}
