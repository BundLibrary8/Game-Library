//! Battle.net (Blizzard and Activision games).
//!
//! Each game Battle.net installs adds an entry to Windows' list of
//! installed programs, uninstalled by `Blizzard Uninstaller.exe` with a
//! `--uid=` option naming the game (`prometheus` is Overwatch).
//!
//! Battle.net starts a game with `Battle.net.exe --exec="launch <code>"`,
//! where the code is a different name again (`Pro` for Overwatch). The
//! table below knows the codes of the well-known games; any other game is
//! started from its own exe, which usually opens Battle.net to start it.

use std::path::{Path, PathBuf};

use anyhow::Result;

use super::GameSource;
use super::uninstall::{self, UninstallEntry, command_option};
use crate::model::{DetectedGame, LaunchTarget, Source};
use crate::sources::folder;

/// Battle.net's game id (`--uid=`) → the code it launches the game with.
const LAUNCH_CODES: &[(&str, &str)] = &[
    ("wow", "WoW"),
    ("wow_classic", "WoWC"),
    ("diablo3", "D3"),
    ("fenris", "Fen"),
    ("osi", "OSI"),
    ("anbs", "ANBS"),
    ("hs_beta", "WTCG"),
    ("heroes", "Hero"),
    ("prometheus", "Pro"),
    ("s1", "S1"),
    ("s2", "S2"),
    ("w3", "W3"),
    ("wlby", "WLBY"),
    ("rtro", "RTRO"),
    ("viper", "VIPR"),
    ("odin", "ODIN"),
    ("lazarus", "LAZR"),
    ("zeus", "ZEUS"),
    ("fore", "FORE"),
    ("auks", "AUKS"),
];

pub fn launch_code(uid: &str) -> Option<&'static str> {
    let uid = uid.to_ascii_lowercase();
    LAUNCH_CODES
        .iter()
        .find(|(id, _)| *id == uid)
        .map(|(_, code)| *code)
}

/// Where Battle.net.exe usually is.
pub const DEFAULT_CLIENT: &str = r"C:\Program Files (x86)\Battle.net\Battle.net.exe";

/// Is this entry a game installed by Battle.net?
fn is_battlenet_game(entry: &UninstallEntry) -> bool {
    let command = entry.uninstall_string.to_ascii_lowercase();
    command.contains("blizzard uninstaller")
        || (command.contains("battle.net") && command.contains("--uid="))
}

/// Where Battle.net itself is installed, if its own entry says.
pub fn client_from_entries(entries: &[UninstallEntry]) -> Option<PathBuf> {
    entries
        .iter()
        .find(|e| e.display_name.trim().eq_ignore_ascii_case("Battle.net"))
        .and_then(UninstallEntry::install_dir)
        .map(|dir| dir.join("Battle.net.exe"))
}

/// Turn an installed-programs entry into a game, if it is a Battle.net
/// game. `client` is Battle.net.exe.
pub fn game_from_entry(entry: &UninstallEntry, client: &Path) -> Option<DetectedGame> {
    if !is_battlenet_game(entry) {
        return None;
    }
    let uid = command_option(&entry.uninstall_string, "uid")?;
    if uid.eq_ignore_ascii_case("battle.net") || uid.eq_ignore_ascii_case("bna") {
        return None; // Battle.net itself
    }
    let title = Some(entry.display_name.trim().to_string())
        .filter(|name| !name.is_empty())
        .or_else(|| command_option(&entry.uninstall_string, "displayname"))?;
    let install_dir = entry.install_dir();
    let launch = match launch_code(&uid) {
        Some(code) => LaunchTarget::Exe {
            path: client.to_path_buf(),
            args: vec![format!("--exec=launch {code}")],
            // Started in Battle.net's own folder (the launcher's default).
            working_dir: None,
        },
        // No known code: start the game's own exe. It hands over to
        // Battle.net if it needs to.
        None => {
            let dir = install_dir.as_deref()?;
            let name = dir.file_name()?.to_str()?;
            LaunchTarget::exe(folder::pick_best_exe(name, &folder::find_exes(dir))?)
        }
    };
    Some(DetectedGame {
        title,
        source: Source::BattleNet,
        source_id: uid.to_ascii_lowercase(),
        install_dir,
        launch,
    })
}

/// All the Battle.net games in a list of installed programs.
pub fn games_from_entries(entries: &[UninstallEntry]) -> Vec<DetectedGame> {
    let client = client_from_entries(entries).unwrap_or_else(|| PathBuf::from(DEFAULT_CLIENT));
    entries
        .iter()
        .filter_map(|e| game_from_entry(e, &client))
        .collect()
}

#[derive(Debug, Default)]
pub struct BattleNetSource;

impl GameSource for BattleNetSource {
    fn name(&self) -> &str {
        "Battle.net"
    }

    fn source(&self) -> Source {
        Source::BattleNet
    }

    fn detect(&self) -> Result<Vec<DetectedGame>> {
        Ok(games_from_entries(&uninstall::read_entries()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, uid: &str, location: &str) -> UninstallEntry {
        UninstallEntry {
            key: name.to_string(),
            display_name: name.to_string(),
            publisher: "Blizzard Entertainment".into(),
            install_location: location.to_string(),
            uninstall_string: format!(
                r#""C:\ProgramData\Battle.net\Agent\Blizzard Uninstaller.exe" --lang=enUS --uid={uid} --displayname="{name}""#
            ),
            display_icon: String::new(),
        }
    }

    #[test]
    fn a_known_game_launches_through_battle_net() {
        let client = PathBuf::from(r"D:\Battle.net\Battle.net.exe");
        let game = game_from_entry(
            &entry(
                "Overwatch",
                "prometheus",
                r"C:\Program Files (x86)\Overwatch",
            ),
            &client,
        )
        .unwrap();
        assert_eq!(game.title, "Overwatch");
        assert_eq!(game.source, Source::BattleNet);
        assert_eq!(game.source_id, "prometheus");
        assert_eq!(
            game.install_dir,
            Some(PathBuf::from(r"C:\Program Files (x86)\Overwatch"))
        );
        assert_eq!(
            game.launch,
            LaunchTarget::Exe {
                path: client.clone(),
                args: vec!["--exec=launch Pro".into()],
                working_dir: None,
            }
        );
    }

    #[test]
    fn an_unknown_game_launches_its_own_exe() {
        let dir = tempfile::tempdir().unwrap();
        let game_dir = dir.path().join("New Game");
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::write(game_dir.join("NewGame.exe"), b"x").unwrap();
        let entry = entry("New Game", "brandnew", &game_dir.to_string_lossy());
        let game = game_from_entry(&entry, Path::new(DEFAULT_CLIENT)).unwrap();
        assert_eq!(game.launch, LaunchTarget::exe(game_dir.join("NewGame.exe")));

        // …and with nothing to start, it isn't listed.
        let empty = UninstallEntry {
            install_location: dir.path().join("nothing").to_string_lossy().into(),
            ..entry
        };
        assert_eq!(game_from_entry(&empty, Path::new(DEFAULT_CLIENT)), None);
    }

    #[test]
    fn battle_net_itself_and_other_programs_are_skipped() {
        let mut client = entry(
            "Battle.net",
            "battle.net",
            r"C:\Program Files (x86)\Battle.net",
        );
        client.uninstall_string =
            r#""C:\Program Files (x86)\Battle.net\Battle.net Uninstaller.exe""#.into();
        let other = UninstallEntry {
            display_name: "Notepad++".into(),
            uninstall_string: r"C:\Program Files\Notepad++\uninstall.exe".into(),
            ..Default::default()
        };
        let games = games_from_entries(&[
            client,
            other,
            entry("Diablo IV", "fenris", r"C:\Games\Diablo IV"),
        ]);
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].title, "Diablo IV");
        // Battle.net's own entry says where its exe is.
        let LaunchTarget::Exe { path, args, .. } = &games[0].launch else {
            panic!()
        };
        assert_eq!(
            path,
            &PathBuf::from(r"C:\Program Files (x86)\Battle.net").join("Battle.net.exe")
        );
        assert_eq!(args, &["--exec=launch Fen"]);
    }

    #[test]
    fn launch_codes() {
        assert_eq!(launch_code("prometheus"), Some("Pro"));
        assert_eq!(launch_code("WOW"), Some("WoW"));
        assert_eq!(launch_code("unknown"), None);
    }
}
