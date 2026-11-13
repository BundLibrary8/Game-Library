//! Windows' list of installed programs: the entries behind "Apps &
//! features" (and the older "Add or remove programs").
//!
//! Battle.net, the Riot Client and the EA app don't keep a simple list of
//! their games anywhere readable, but every game they install adds an
//! entry here, and the entry says which launcher it belongs to. So all
//! three are found by reading this list once and picking out their games.
//!
//! The registry only exists on Windows, so reading it is compiled on
//! Windows only. Everything that interprets the entries is ordinary code,
//! tested everywhere.

use std::path::PathBuf;

use anyhow::Result;

/// One installed program, as Windows records it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UninstallEntry {
    /// The registry key's name, e.g. `Riot Game valorant.live`.
    pub key: String,
    pub display_name: String,
    pub publisher: String,
    pub install_location: String,
    /// The command Windows runs to uninstall it. Launchers put useful
    /// details in here, like which of their games it is.
    pub uninstall_string: String,
    pub display_icon: String,
}

impl UninstallEntry {
    /// The install folder, if one is recorded.
    pub fn install_dir(&self) -> Option<PathBuf> {
        let trimmed = self.install_location.trim().trim_matches('"').trim();
        (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
    }
}

/// The program a command runs: `"C:\a b\x.exe" --flag` → `C:\a b\x.exe`.
pub fn command_program(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    let program = match command.strip_prefix('"') {
        Some(rest) => rest.split('"').next()?,
        None => command.split_whitespace().next()?,
    };
    (!program.is_empty()).then(|| PathBuf::from(program))
}

/// The value of a `--name=value` (or `--name="value"`) option in a command.
pub fn command_option(command: &str, name: &str) -> Option<String> {
    let start = command.find(&format!("--{name}="))? + name.len() + 3;
    let rest = &command[start..];
    let value = match rest.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next()?,
        None => rest.split_whitespace().next()?,
    };
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(windows)]
pub fn read_entries() -> Result<Vec<UninstallEntry>> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};

    const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    // 64-bit programs, 32-bit programs, and programs installed just for
    // this user (Riot puts its games here).
    let places = [
        (
            HKEY_LOCAL_MACHINE,
            UNINSTALL.to_string(),
            KEY_READ | KEY_WOW64_64KEY,
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall".to_string(),
            KEY_READ,
        ),
        (HKEY_CURRENT_USER, UNINSTALL.to_string(), KEY_READ),
    ];

    let mut entries = Vec::new();
    for (root, path, flags) in places {
        // A place that doesn't exist (or can't be read) just has no entries.
        let Ok(list) = RegKey::predef(root).open_subkey_with_flags(&path, flags) else {
            continue;
        };
        for name in list.enum_keys().flatten() {
            let Ok(key) = list.open_subkey_with_flags(&name, flags) else {
                continue;
            };
            let text = |value: &str| key.get_value::<String, _>(value).unwrap_or_default();
            entries.push(UninstallEntry {
                key: name.clone(),
                display_name: text("DisplayName"),
                publisher: text("Publisher"),
                install_location: text("InstallLocation"),
                uninstall_string: text("UninstallString"),
                display_icon: text("DisplayIcon"),
            });
        }
    }
    Ok(entries)
}

#[cfg(not(windows))]
pub fn read_entries() -> Result<Vec<UninstallEntry>> {
    anyhow::bail!("this launcher can only be found on Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_program_a_command_runs() {
        assert_eq!(
            command_program(
                r#""C:\Riot Games\Riot Client\RiotClientServices.exe" --uninstall-product=valorant"#
            ),
            Some(PathBuf::from(
                r"C:\Riot Games\Riot Client\RiotClientServices.exe"
            ))
        );
        assert_eq!(
            command_program(r"C:\Tools\uninstall.exe /S"),
            Some(PathBuf::from(r"C:\Tools\uninstall.exe"))
        );
        assert_eq!(command_program("   "), None);
        assert_eq!(command_program(r#""""#), None);
    }

    #[test]
    fn options_in_a_command() {
        let command = r#""C:\ProgramData\Battle.net\Agent\Blizzard Uninstaller.exe" --lang=enUS --uid=prometheus --displayname="Overwatch 2""#;
        assert_eq!(
            command_option(command, "uid").as_deref(),
            Some("prometheus")
        );
        assert_eq!(
            command_option(command, "displayname").as_deref(),
            Some("Overwatch 2")
        );
        assert_eq!(command_option(command, "lang").as_deref(), Some("enUS"));
        assert_eq!(command_option(command, "missing"), None);
        assert_eq!(command_option("x --uid=", "uid"), None);
    }

    #[test]
    fn install_folders_are_tidied() {
        let entry = |location: &str| UninstallEntry {
            install_location: location.to_string(),
            ..Default::default()
        };
        assert_eq!(
            entry(r#" "C:\Games\X" "#).install_dir(),
            Some(PathBuf::from(r"C:\Games\X"))
        );
        assert_eq!(entry("").install_dir(), None);
    }
}
