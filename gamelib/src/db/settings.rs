//! The `settings` table: small key/value options.

use std::path::PathBuf;

use anyhow::Result;
use rusqlite::OptionalExtension;

use super::Db;
use crate::model::Source;

/// Names of the settings the app uses.
pub mod keys {
    /// SteamGridDB API key (cover art). Never commit this to Git.
    pub const STEAMGRIDDB_KEY: &str = "steamgriddb_api_key";
    /// Steam Web API key (friends list). Never commit this to Git.
    pub const STEAM_API_KEY: &str = "steam_api_key";
    /// Anthropic API key (the assistant asks Claude). Never commit this to Git.
    pub const ANTHROPIC_KEY: &str = "anthropic_api_key";
    /// "0" to keep the assistant to its built-in answers even with a key.
    pub const ASSISTANT_USE_CLAUDE: &str = "assistant.use_claude";
    /// JSON array of folders for the folder scan.
    pub const SCAN_FOLDERS: &str = "scan_folders";
    pub const SCAN_ON_STARTUP: &str = "scan_on_startup";
    /// The equipped library background (a shop item id).
    pub const EQUIPPED_LIBRARY_BG: &str = "equipped.library_bg";
    /// Unused "Name change" purchases.
    pub const NAME_CHANGE_CREDITS: &str = "name_change_credits";
}

impl Db {
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    pub fn delete_setting(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM settings WHERE key = ?1", [key])?;
        Ok(())
    }

    /// A true/false setting, with a default for when it has never been set.
    pub fn get_bool_setting(&self, key: &str, default: bool) -> Result<bool> {
        Ok(match self.get_setting(key)?.as_deref() {
            Some("1") => true,
            Some("0") => false,
            _ => default,
        })
    }

    pub fn set_bool_setting(&self, key: &str, value: bool) -> Result<()> {
        self.set_setting(key, if value { "1" } else { "0" })
    }

    /// Folders the folder scan looks in (e.g. `D:\Games`).
    pub fn scan_folders(&self) -> Result<Vec<PathBuf>> {
        match self.get_setting(keys::SCAN_FOLDERS)? {
            Some(json) => Ok(serde_json::from_str(&json)?),
            None => Ok(Vec::new()),
        }
    }

    pub fn set_scan_folders(&self, folders: &[PathBuf]) -> Result<()> {
        self.set_setting(keys::SCAN_FOLDERS, &serde_json::to_string(folders)?)
    }

    /// Whether a source is switched on in Settings. Everything is on by default.
    pub fn source_enabled(&self, source: Source) -> Result<bool> {
        self.get_bool_setting(&format!("source.{}.enabled", source.as_str()), true)
    }

    pub fn set_source_enabled(&self, source: Source, enabled: bool) -> Result<()> {
        self.set_bool_setting(&format!("source.{}.enabled", source.as_str()), enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_save_overwrite_and_delete() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.get_setting("k").unwrap(), None);
        db.set_setting("k", "one").unwrap();
        db.set_setting("k", "two").unwrap();
        assert_eq!(db.get_setting("k").unwrap().as_deref(), Some("two"));
        db.delete_setting("k").unwrap();
        assert_eq!(db.get_setting("k").unwrap(), None);
    }

    #[test]
    fn scan_folders_round_trip() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.scan_folders().unwrap().is_empty());
        let folders = vec![PathBuf::from("/games"), PathBuf::from("/more games")];
        db.set_scan_folders(&folders).unwrap();
        assert_eq!(db.scan_folders().unwrap(), folders);
    }

    #[test]
    fn sources_are_enabled_until_switched_off() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.source_enabled(Source::Epic).unwrap());
        db.set_source_enabled(Source::Epic, false).unwrap();
        assert!(!db.source_enabled(Source::Epic).unwrap());
        assert!(db.source_enabled(Source::Steam).unwrap());
    }
}
