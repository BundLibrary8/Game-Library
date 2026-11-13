//! Riot Client (League of Legends, VALORANT, Legends of Runeterra, 2XKO).
//!
//! Each game adds an entry to Windows' list of installed programs whose
//! uninstall command is the Riot Client with
//! `--uninstall-product=valorant --uninstall-patchline=live`. The Riot
//! Client starts the same game with `--launch-product` and
//! `--launch-patchline`, which is exactly what Riot's own shortcuts do.

use anyhow::Result;

use super::GameSource;
use super::uninstall::{self, UninstallEntry, command_option, command_program};
use crate::model::{DetectedGame, LaunchTarget, Source};

/// Turn an installed-programs entry into a game, if it is a Riot game.
pub fn game_from_entry(entry: &UninstallEntry) -> Option<DetectedGame> {
    let command = &entry.uninstall_string;
    if !command.to_ascii_lowercase().contains("riotclientservices") {
        return None;
    }
    let product = command_option(command, "uninstall-product")?;
    if product.starts_with("riot_client") {
        return None; // the Riot Client itself
    }
    let patchline =
        command_option(command, "uninstall-patchline").unwrap_or_else(|| "live".to_string());
    let client = command_program(command)?;
    let title = entry.display_name.trim();
    let title = if title.is_empty() {
        product.as_str()
    } else {
        title
    };
    Some(DetectedGame {
        title: title.to_string(),
        source: Source::Riot,
        source_id: format!("{product}.{patchline}"),
        install_dir: entry.install_dir(),
        launch: LaunchTarget::Exe {
            // Started in the Riot Client's own folder (the launcher's default).
            working_dir: None,
            path: client,
            args: vec![
                format!("--launch-product={product}"),
                format!("--launch-patchline={patchline}"),
            ],
        },
    })
}

#[derive(Debug, Default)]
pub struct RiotSource;

impl GameSource for RiotSource {
    fn name(&self) -> &str {
        "Riot Client"
    }

    fn source(&self) -> Source {
        Source::Riot
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
    use std::path::PathBuf;

    fn entry(name: &str, product: &str) -> UninstallEntry {
        UninstallEntry {
            key: format!("Riot Game {product}.live"),
            display_name: name.to_string(),
            publisher: "Riot Games, Inc".into(),
            install_location: format!(r"C:\Riot Games\{name}"),
            uninstall_string: format!(
                r#""C:\Riot Games\Riot Client\RiotClientServices.exe" --uninstall-product={product} --uninstall-patchline=live"#
            ),
            display_icon: String::new(),
        }
    }

    #[test]
    fn a_riot_game_launches_through_the_riot_client() {
        let game = game_from_entry(&entry("VALORANT", "valorant")).unwrap();
        assert_eq!(game.title, "VALORANT");
        assert_eq!(game.source, Source::Riot);
        assert_eq!(game.source_id, "valorant.live");
        assert_eq!(
            game.install_dir,
            Some(PathBuf::from(r"C:\Riot Games\VALORANT"))
        );
        assert_eq!(
            game.launch,
            LaunchTarget::Exe {
                path: PathBuf::from(r"C:\Riot Games\Riot Client\RiotClientServices.exe"),
                args: vec![
                    "--launch-product=valorant".into(),
                    "--launch-patchline=live".into()
                ],
                working_dir: None,
            }
        );
    }

    #[test]
    fn other_programs_and_the_client_itself_are_skipped() {
        assert_eq!(game_from_entry(&entry("Riot Client", "riot_client")), None);
        let other = UninstallEntry {
            display_name: "Discord".into(),
            uninstall_string: r"C:\Users\me\AppData\Local\Discord\Update.exe --uninstall".into(),
            ..Default::default()
        };
        assert_eq!(game_from_entry(&other), None);
    }

    #[test]
    fn the_patchline_is_kept_apart() {
        let mut pbe = entry("League of Legends (PBE)", "league_of_legends");
        pbe.uninstall_string = pbe
            .uninstall_string
            .replace("patchline=live", "patchline=pbe");
        let game = game_from_entry(&pbe).unwrap();
        assert_eq!(game.source_id, "league_of_legends.pbe");
        let LaunchTarget::Exe { args, .. } = game.launch else {
            panic!()
        };
        assert_eq!(args[1], "--launch-patchline=pbe");
    }
}
