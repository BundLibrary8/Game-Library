//! GOG: one registry key per installed game, under
//! `HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\GOG.com\Games\`.
//!
//! The registry only exists on Windows, so the part that reads it is
//! compiled on Windows only. Turning the values into a game
//! ([`game_from_entry`]) is ordinary code that is tested everywhere.

use std::path::PathBuf;

use anyhow::Result;

use super::GameSource;
use crate::model::{DetectedGame, LaunchTarget, Source};

/// The registry values for one game, as text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GogEntry {
    pub game_id: String,
    pub game_name: String,
    /// The install folder.
    pub path: String,
    /// Full path of the exe to run.
    pub exe: String,
    pub working_dir: String,
    pub launch_param: String,
    /// Set for DLC: the id of the game it belongs to.
    pub depends_on: String,
}

/// Turn registry values into a game. Returns `None` for DLC and for
/// entries too incomplete to launch.
pub fn game_from_entry(entry: &GogEntry) -> Option<DetectedGame> {
    if !entry.depends_on.trim().is_empty() {
        return None; // DLC
    }
    if entry.game_id.is_empty() || entry.game_name.is_empty() || entry.exe.is_empty() {
        return None;
    }
    let non_empty = |text: &str| (!text.trim().is_empty()).then(|| PathBuf::from(text));
    let install_dir = non_empty(&entry.path);
    Some(DetectedGame {
        title: entry.game_name.clone(),
        source: Source::Gog,
        source_id: entry.game_id.clone(),
        install_dir: install_dir.clone(),
        // GOG games are DRM-free, so the exe can be run directly.
        launch: LaunchTarget::Exe {
            path: PathBuf::from(&entry.exe),
            args: split_args(&entry.launch_param),
            working_dir: non_empty(&entry.working_dir).or(install_dir),
        },
    })
}

/// Split a launch-parameter string into arguments: on spaces, except
/// inside double quotes. `-conf "my file.conf" -x` → 3 arguments.
pub fn split_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut has_token = false;
    for c in text.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_token {
                    args.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            c => {
                current.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        args.push(current);
    }
    args
}

#[cfg(windows)]
fn read_registry() -> Result<Vec<GogEntry>> {
    use anyhow::Context;
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;

    let games_key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\WOW6432Node\GOG.com\Games")
        .context("no GOG games found in the registry")?;

    let mut entries = Vec::new();
    for name in games_key.enum_keys() {
        // One unreadable key is skipped, not fatal.
        let Ok(name) = name else { continue };
        let Ok(key) = games_key.open_subkey(&name) else {
            log::warn!("skipping unreadable GOG registry key {name}");
            continue;
        };
        let text = |value: &str| key.get_value::<String, _>(value).unwrap_or_default();
        entries.push(GogEntry {
            game_id: text("gameID"),
            game_name: text("gameName"),
            path: text("path"),
            exe: text("exe"),
            working_dir: text("workingDir"),
            launch_param: text("launchParam"),
            depends_on: text("dependsOn"),
        });
    }
    Ok(entries)
}

#[cfg(not(windows))]
fn read_registry() -> Result<Vec<GogEntry>> {
    anyhow::bail!("GOG detection only works on Windows")
}

#[derive(Debug, Default)]
pub struct GogSource;

impl GameSource for GogSource {
    fn name(&self) -> &str {
        "GOG"
    }

    fn source(&self) -> Source {
        Source::Gog
    }

    fn detect(&self) -> Result<Vec<DetectedGame>> {
        Ok(read_registry()?
            .iter()
            .filter_map(game_from_entry)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> GogEntry {
        GogEntry {
            game_id: "1207658924".into(),
            game_name: "Old Classic".into(),
            path: r"D:\GOG\Old Classic".into(),
            exe: r"D:\GOG\Old Classic\classic.exe".into(),
            working_dir: r"D:\GOG\Old Classic".into(),
            launch_param: String::new(),
            depends_on: String::new(),
        }
    }

    #[test]
    fn a_registry_entry_becomes_an_exe_game() {
        let game = game_from_entry(&entry()).unwrap();
        assert_eq!(game.title, "Old Classic");
        assert_eq!(game.source, Source::Gog);
        assert_eq!(game.source_id, "1207658924");
        assert_eq!(game.install_dir, Some(PathBuf::from(r"D:\GOG\Old Classic")));
        assert_eq!(
            game.launch,
            LaunchTarget::Exe {
                path: PathBuf::from(r"D:\GOG\Old Classic\classic.exe"),
                args: vec![],
                working_dir: Some(PathBuf::from(r"D:\GOG\Old Classic")),
            }
        );
    }

    #[test]
    fn launch_parameters_become_arguments() {
        let mut e = entry();
        e.launch_param = r#"-conf "my file.conf" -noconsole"#.into();
        let LaunchTarget::Exe { args, .. } = game_from_entry(&e).unwrap().launch else {
            panic!("expected an exe");
        };
        assert_eq!(args, ["-conf", "my file.conf", "-noconsole"]);
    }

    #[test]
    fn working_dir_falls_back_to_the_install_folder() {
        let mut e = entry();
        e.working_dir.clear();
        let LaunchTarget::Exe { working_dir, .. } = game_from_entry(&e).unwrap().launch else {
            panic!("expected an exe");
        };
        assert_eq!(working_dir, Some(PathBuf::from(r"D:\GOG\Old Classic")));
    }

    #[test]
    fn dlc_and_incomplete_entries_are_skipped() {
        let mut dlc = entry();
        dlc.depends_on = "1207658924".into();
        assert_eq!(game_from_entry(&dlc), None);

        let mut no_exe = entry();
        no_exe.exe.clear();
        assert_eq!(game_from_entry(&no_exe), None);

        assert_eq!(game_from_entry(&GogEntry::default()), None);
    }

    #[test]
    fn splitting_arguments() {
        assert!(split_args("").is_empty());
        assert!(split_args("   ").is_empty());
        assert_eq!(split_args("-a  -b"), ["-a", "-b"]);
        assert_eq!(split_args(r#""one two" three"#), ["one two", "three"]);
        assert_eq!(split_args(r#"--name="a b""#), ["--name=a b"]);
        assert_eq!(split_args(r#""""#), [""]);
    }
}
