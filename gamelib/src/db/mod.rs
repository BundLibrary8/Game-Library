//! Saving and loading with SQLite.
//!
//! [`Db`] wraps one database connection. Its methods are spread over a few
//! files by topic (`games`, `tags`, `sessions`, `settings`), and the feature
//! modules (`stats`, `profile`, `shop`, `friends`) add their own.
//!
//! Tests use [`Db::open_in_memory`], so they never touch the real library.

mod games;
mod sessions;
mod settings;
mod tags;

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;

pub use games::{GameQuery, SortBy, UpsertOutcome};
pub use sessions::SessionSummary;
pub use settings::keys;

/// Each entry upgrades the schema by one version. `PRAGMA user_version`
/// remembers how many have run, so on startup only the new ones are applied.
/// **Never edit an entry that has shipped; add a new one instead.**
const MIGRATIONS: &[&str] = &[
    // v1: the core library
    "
    CREATE TABLE games(
        id INTEGER PRIMARY KEY,
        title TEXT NOT NULL,
        source TEXT NOT NULL,            -- 'steam' | 'epic' | 'gog' | 'folder' | 'manual'
        source_id TEXT NOT NULL,
        install_dir TEXT,
        launch_kind TEXT NOT NULL,       -- 'exe' | 'uri'
        launch_value TEXT NOT NULL,      -- exe path or URI
        launch_args TEXT,                -- JSON array
        launch_working_dir TEXT,
        favourite INTEGER NOT NULL DEFAULT 0,
        hidden INTEGER NOT NULL DEFAULT 0,
        installed INTEGER NOT NULL DEFAULT 1,
        playtime_secs INTEGER NOT NULL DEFAULT 0,
        last_played INTEGER,
        added_at INTEGER NOT NULL,
        cover_path TEXT,
        cover_manual INTEGER NOT NULL DEFAULT 0,  -- set by hand: scans leave it alone
        description TEXT,
        genres TEXT,                     -- JSON array
        release_year INTEGER,
        UNIQUE(source, source_id)        -- stops duplicates on rescan
    );

    CREATE TABLE tags(
        id INTEGER PRIMARY KEY,
        name TEXT UNIQUE NOT NULL COLLATE NOCASE,
        is_collection INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE game_tags(
        game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
        tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
        PRIMARY KEY(game_id, tag_id)
    );

    CREATE TABLE play_sessions(
        id INTEGER PRIMARY KEY,
        game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
        started_at INTEGER NOT NULL,
        ended_at INTEGER,
        duration_secs INTEGER NOT NULL DEFAULT 0,
        closed INTEGER NOT NULL DEFAULT 0     -- 1 once added to games.playtime_secs
    );
    CREATE INDEX play_sessions_game ON play_sessions(game_id);

    CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
    ",
    // v2: profile, friends, Stardust and shop
    "
    CREATE TABLE profile(
        id INTEGER PRIMARY KEY CHECK (id = 1),   -- always exactly one row
        display_name TEXT NOT NULL,
        name_chosen INTEGER NOT NULL DEFAULT 0,  -- 0 until the free first rename is used
        bio TEXT,
        avatar_path TEXT,
        banner TEXT,             -- 'upload:<path>' | 'game:<game_id>' | 'item:<shop item id>'
        steam_id TEXT,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE profile_showcase(
        slot INTEGER PRIMARY KEY CHECK (slot BETWEEN 1 AND 4),
        game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE
    );
    CREATE TABLE profile_badges(
        slot INTEGER PRIMARY KEY CHECK (slot BETWEEN 1 AND 3),
        item_id TEXT NOT NULL
    );
    CREATE TABLE friends_cache(
        steam_id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        avatar_url TEXT,
        state TEXT NOT NULL,
        playing TEXT,
        updated_at INTEGER NOT NULL
    );

    CREATE TABLE stardust_ledger(
        id INTEGER PRIMARY KEY,
        amount INTEGER NOT NULL,   -- + earned, - spent
        reason TEXT NOT NULL,      -- 'playtime' | 'finished' | 'daily' | 'purchase'
        game_id INTEGER,
        item_id TEXT,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE owned_items(
        item_id TEXT PRIMARY KEY,
        acquired_at INTEGER NOT NULL,
        how TEXT NOT NULL          -- 'bought' | 'earned' | 'gift'
    );
    ",
    // v3: each game's own icon
    "ALTER TABLE games ADD COLUMN icon_path TEXT;",
    // v4: completion status, ratings, franchises, save and mod folders,
    // and system requirements
    "
    ALTER TABLE games ADD COLUMN status TEXT;
    ALTER TABLE games ADD COLUMN rating INTEGER;
    ALTER TABLE games ADD COLUMN franchise TEXT;
    ALTER TABLE games ADD COLUMN franchise_order INTEGER;
    ALTER TABLE games ADD COLUMN save_dir TEXT;
    ALTER TABLE games ADD COLUMN mods_dir TEXT;
    ALTER TABLE games ADD COLUMN requirements_min TEXT;
    ALTER TABLE games ADD COLUMN requirements_rec TEXT;
    ",
    // v5: when artwork was last looked for online, so games with none
    // anywhere aren't looked up again on every start
    "ALTER TABLE games ADD COLUMN art_checked_at INTEGER;",
    // v6: covers are now looked for in more places, so games marked as
    // having none anywhere are given another try
    "UPDATE games SET art_checked_at = NULL WHERE cover_path IS NULL;",
    // v7: how long a game takes to beat, your notes and review, and the
    // process to watch when it doesn't run from its install folder
    "
    ALTER TABLE games ADD COLUMN minutes_to_beat INTEGER;
    ALTER TABLE games ADD COLUMN notes TEXT;
    ALTER TABLE games ADD COLUMN review TEXT;
    ALTER TABLE games ADD COLUMN process_name TEXT;
    ",
];

/// A connection to the library database.
pub struct Db {
    pub(crate) conn: Connection,
}

impl Db {
    /// Open (or create) the database file and bring its schema up to date.
    pub fn open(path: &Path) -> Result<Db> {
        let conn = Connection::open(path)
            .with_context(|| format!("couldn't open database {}", path.display()))?;
        // WAL lets the UI read while a worker thread writes; the timeout makes
        // two writers queue up instead of failing with "database is locked".
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Db::init(conn)
    }

    /// A throwaway database that lives only in memory. For tests.
    pub fn open_in_memory() -> Result<Db> {
        Db::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Db> {
        conn.pragma_update(None, "foreign_keys", true)?;
        let db = Db { conn };
        db.migrate()?;
        Ok(db)
    }

    /// The schema version this database is at.
    pub fn schema_version(&self) -> Result<usize> {
        let v: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        Ok(v as usize)
    }

    fn migrate(&self) -> Result<()> {
        let current = self.schema_version()?;
        anyhow::ensure!(
            current <= MIGRATIONS.len(),
            "this library was made by a newer version of GameLib (schema v{current})"
        );
        for (index, sql) in MIGRATIONS.iter().enumerate().skip(current) {
            let version = index + 1;
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(sql)
                .with_context(|| format!("database upgrade to v{version} failed"))?;
            tx.pragma_update(None, "user_version", version as i64)?;
            tx.commit()?;
            log::info!("database upgraded to schema v{version}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_database_is_at_latest_version() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len());
    }

    #[test]
    fn reopening_keeps_data_and_does_not_rerun_migrations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        {
            let db = Db::open(&path).unwrap();
            db.set_setting("hello", "world").unwrap();
        }
        let db = Db::open(&path).unwrap();
        assert_eq!(db.get_setting("hello").unwrap().as_deref(), Some("world"));
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len());
    }

    #[test]
    fn refuses_a_database_from_the_future() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        {
            let db = Db::open(&path).unwrap();
            db.conn.pragma_update(None, "user_version", 999).unwrap();
        }
        assert!(Db::open(&path).is_err());
    }
}
