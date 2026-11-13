//! The `games` table.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use rusqlite::types::Value;
use rusqlite::{OptionalExtension, params, params_from_iter};

use super::Db;
use crate::model::{DetectedGame, Game, LaunchTarget, Source};

const GAME_COLUMNS: &str = "id, title, source, source_id, install_dir, launch_kind, launch_value,
    launch_args, launch_working_dir, favourite, hidden, installed, playtime_secs, last_played,
    added_at, cover_path, description, genres, release_year, icon_path";

/// How to order a list of games.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortBy {
    #[default]
    Title,
    /// Most recently played first; never-played games last.
    LastPlayed,
    /// Most played first.
    Playtime,
    /// Newest additions first.
    RecentlyAdded,
}

/// Filters for [`Db::query_games`]. `GameQuery::default()` means
/// "every visible game, A to Z".
#[derive(Debug, Clone, Default)]
pub struct GameQuery {
    /// Text that must appear somewhere in the title (ignoring case).
    pub search: Option<String>,
    /// Only games with this tag or in this collection.
    pub tag_id: Option<i64>,
    pub source: Option<Source>,
    pub favourites_only: bool,
    pub installed_only: bool,
    /// Hidden games are left out unless this is set.
    pub include_hidden: bool,
    pub sort: SortBy,
}

/// What [`Db::upsert_detected`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpsertOutcome {
    pub game_id: i64,
    /// True if the game was new, false if an existing row was refreshed.
    pub inserted: bool,
}

fn path_to_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn game_from_row(row: &rusqlite::Row) -> rusqlite::Result<Game> {
    // Small helper: turn "this column held something unexpected" into an error
    // rusqlite understands, so a bad row is reported instead of panicking.
    fn bad(
        index: usize,
        err: impl Into<Box<dyn std::error::Error + Send + Sync>>,
    ) -> rusqlite::Error {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, err.into())
    }

    let source_text: String = row.get(2)?;
    let source = Source::parse(&source_text).map_err(|e| bad(2, e))?;

    let launch_kind: String = row.get(5)?;
    let launch_value: String = row.get(6)?;
    let launch = match launch_kind.as_str() {
        "uri" => LaunchTarget::Uri(launch_value),
        "exe" => {
            let args_json: Option<String> = row.get(7)?;
            let args = match args_json {
                Some(json) => serde_json::from_str(&json).map_err(|e| bad(7, e))?,
                None => Vec::new(),
            };
            let working_dir: Option<String> = row.get(8)?;
            LaunchTarget::Exe {
                path: PathBuf::from(launch_value),
                args,
                working_dir: working_dir.map(PathBuf::from),
            }
        }
        other => return Err(bad(5, format!("unknown launch kind '{other}'"))),
    };

    let genres_json: Option<String> = row.get(17)?;
    let genres = match genres_json {
        Some(json) => serde_json::from_str(&json).map_err(|e| bad(17, e))?,
        None => Vec::new(),
    };

    Ok(Game {
        id: row.get(0)?,
        title: row.get(1)?,
        source,
        source_id: row.get(3)?,
        install_dir: row.get::<_, Option<String>>(4)?.map(PathBuf::from),
        launch,
        favourite: row.get(9)?,
        hidden: row.get(10)?,
        installed: row.get(11)?,
        playtime_secs: row.get::<_, i64>(12)?.max(0) as u64,
        last_played: row.get(13)?,
        added_at: row.get(14)?,
        cover_path: row.get::<_, Option<String>>(15)?.map(PathBuf::from),
        icon_path: row.get::<_, Option<String>>(19)?.map(PathBuf::from),
        description: row.get(16)?,
        genres,
        release_year: row.get(18)?,
    })
}

/// The four launch columns for a target: kind, value, args (JSON), working dir.
fn launch_columns(
    launch: &LaunchTarget,
) -> Result<(&'static str, String, Option<String>, Option<String>)> {
    Ok(match launch {
        LaunchTarget::Uri(uri) => ("uri", uri.clone(), None, None),
        LaunchTarget::Exe {
            path,
            args,
            working_dir,
        } => (
            "exe",
            path_to_text(path),
            Some(serde_json::to_string(args)?),
            working_dir.as_deref().map(path_to_text),
        ),
    })
}

impl Db {
    /// Save a detected game. If a game with the same `(source, source_id)` is
    /// already there, its title, folder and launch info are refreshed and it
    /// is marked installed again, but **favourites, tags, playtime and covers
    /// are kept**. This is what makes rescanning safe.
    pub fn upsert_detected(&self, game: &DetectedGame, now: i64) -> Result<UpsertOutcome> {
        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM games WHERE source = ?1 AND source_id = ?2",
                params![game.source.as_str(), game.source_id],
                |row| row.get(0),
            )
            .optional()?;
        let (kind, value, args, working_dir) = launch_columns(&game.launch)?;
        let install_dir = game.install_dir.as_deref().map(path_to_text);

        match existing {
            Some(game_id) => {
                self.conn.execute(
                    "UPDATE games SET title = ?2, install_dir = ?3, launch_kind = ?4,
                        launch_value = ?5, launch_args = ?6, launch_working_dir = ?7, installed = 1
                     WHERE id = ?1",
                    params![
                        game_id,
                        game.title,
                        install_dir,
                        kind,
                        value,
                        args,
                        working_dir
                    ],
                )?;
                Ok(UpsertOutcome {
                    game_id,
                    inserted: false,
                })
            }
            None => {
                self.conn.execute(
                    "INSERT INTO games(title, source, source_id, install_dir, launch_kind,
                        launch_value, launch_args, launch_working_dir, added_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        game.title,
                        game.source.as_str(),
                        game.source_id,
                        install_dir,
                        kind,
                        value,
                        args,
                        working_dir,
                        now
                    ],
                )?;
                Ok(UpsertOutcome {
                    game_id: self.conn.last_insert_rowid(),
                    inserted: true,
                })
            }
        }
    }

    /// Add a game by hand: an exe and a title.
    pub fn add_manual_game(&self, title: &str, exe: &Path, now: i64) -> Result<i64> {
        let title = title.trim();
        anyhow::ensure!(!title.is_empty(), "the game needs a title");
        Ok(self
            .upsert_detected(&DetectedGame::manual(title, exe), now)?
            .game_id)
    }

    pub fn get_game(&self, game_id: i64) -> Result<Option<Game>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {GAME_COLUMNS} FROM games WHERE id = ?1"),
                [game_id],
                game_from_row,
            )
            .optional()?)
    }

    /// Like [`Db::get_game`], but a missing game is an error.
    pub fn require_game(&self, game_id: i64) -> Result<Game> {
        match self.get_game(game_id)? {
            Some(game) => Ok(game),
            None => bail!("there is no game with id {game_id}"),
        }
    }

    /// Every game, including hidden ones, A to Z.
    pub fn list_games(&self) -> Result<Vec<Game>> {
        self.query_games(&GameQuery {
            include_hidden: true,
            ..GameQuery::default()
        })
    }

    /// Games matching the filters, in the requested order.
    pub fn query_games(&self, query: &GameQuery) -> Result<Vec<Game>> {
        let mut conditions: Vec<&str> = Vec::new();
        let mut values: Vec<Value> = Vec::new();

        if let Some(search) = query
            .search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            // instr() rather than LIKE so that % and _ in the search are literal.
            conditions.push("instr(lower(title), lower(?)) > 0");
            values.push(Value::Text(search.to_string()));
        }
        if let Some(tag_id) = query.tag_id {
            conditions.push("id IN (SELECT game_id FROM game_tags WHERE tag_id = ?)");
            values.push(Value::Integer(tag_id));
        }
        if let Some(source) = query.source {
            conditions.push("source = ?");
            values.push(Value::Text(source.as_str().to_string()));
        }
        if query.favourites_only {
            conditions.push("favourite = 1");
        }
        if query.installed_only {
            conditions.push("installed = 1");
        }
        if !query.include_hidden {
            conditions.push("hidden = 0");
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let order = match query.sort {
            SortBy::Title => "title COLLATE NOCASE",
            SortBy::LastPlayed => "last_played IS NULL, last_played DESC, title COLLATE NOCASE",
            SortBy::Playtime => "playtime_secs DESC, title COLLATE NOCASE",
            SortBy::RecentlyAdded => "added_at DESC, id DESC",
        };

        let sql = format!("SELECT {GAME_COLUMNS} FROM games {where_clause} ORDER BY {order}");
        let mut stmt = self.conn.prepare(&sql)?;
        let games = stmt
            .query_map(params_from_iter(values), game_from_row)?
            .collect::<Result<_, _>>()?;
        Ok(games)
    }

    pub fn set_favourite(&self, game_id: i64, favourite: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET favourite = ?2 WHERE id = ?1",
            params![game_id, favourite],
        )?;
        Ok(())
    }

    pub fn set_hidden(&self, game_id: i64, hidden: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET hidden = ?2 WHERE id = ?1",
            params![game_id, hidden],
        )?;
        Ok(())
    }

    pub fn set_title(&self, game_id: i64, title: &str) -> Result<()> {
        let title = title.trim();
        anyhow::ensure!(!title.is_empty(), "the game needs a title");
        self.conn.execute(
            "UPDATE games SET title = ?2 WHERE id = ?1",
            params![game_id, title],
        )?;
        Ok(())
    }

    /// Change how a game is started ("Locate exe…"). Marks it installed.
    pub fn set_launch(&self, game_id: i64, launch: &LaunchTarget) -> Result<()> {
        let (kind, value, args, working_dir) = launch_columns(launch)?;
        self.conn.execute(
            "UPDATE games SET launch_kind = ?2, launch_value = ?3, launch_args = ?4,
                launch_working_dir = ?5, installed = 1
             WHERE id = ?1",
            params![game_id, kind, value, args, working_dir],
        )?;
        Ok(())
    }

    /// Remove a game for good, along with its tags and play sessions.
    pub fn delete_game(&self, game_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM games WHERE id = ?1", [game_id])?;
        Ok(())
    }

    /// Record a downloaded cover. Does nothing (and returns false) if the
    /// user has already chosen a cover by hand.
    pub fn set_cover_auto(&self, game_id: i64, cover: &Path) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE games SET cover_path = ?2 WHERE id = ?1 AND cover_manual = 0",
            params![game_id, path_to_text(cover)],
        )?;
        Ok(changed > 0)
    }

    /// Record a cover the user picked ("Change cover…"). Scans and downloads
    /// never overwrite it afterwards.
    pub fn set_cover_manual(&self, game_id: i64, cover: &Path) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET cover_path = ?2, cover_manual = 1 WHERE id = ?1",
            params![game_id, path_to_text(cover)],
        )?;
        Ok(())
    }

    /// Games with no cover that are due an online lookup: ones never
    /// looked up, or last looked up more than `retry_after_secs` ago.
    /// Pass 0 to get every game without a cover.
    pub fn games_due_cover_lookup(&self, now: i64, retry_after_secs: i64) -> Result<Vec<Game>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {GAME_COLUMNS} FROM games
             WHERE cover_path IS NULL
               AND (art_checked_at IS NULL OR art_checked_at <= ?1)
             ORDER BY id"
        ))?;
        let games = stmt
            .query_map([now - retry_after_secs], game_from_row)?
            .collect::<Result<_, _>>()?;
        Ok(games)
    }

    /// Note that a game's artwork has just been looked for online.
    pub fn mark_art_checked(&self, game_id: i64, now: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET art_checked_at = ?2 WHERE id = ?1",
            params![game_id, now],
        )?;
        Ok(())
    }

    /// Record the icon read out of a game's exe.
    pub fn set_icon(&self, game_id: i64, icon: &Path) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET icon_path = ?2 WHERE id = ?1",
            params![game_id, path_to_text(icon)],
        )?;
        Ok(())
    }

    /// Games that have no icon yet.
    pub fn games_without_icon(&self) -> Result<Vec<Game>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {GAME_COLUMNS} FROM games WHERE icon_path IS NULL ORDER BY id"
        ))?;
        let games = stmt
            .query_map([], game_from_row)?
            .collect::<Result<_, _>>()?;
        Ok(games)
    }

    /// Save description, genres and release year.
    pub fn set_game_info(
        &self,
        game_id: i64,
        description: Option<&str>,
        genres: &[String],
        release_year: Option<u16>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET description = ?2, genres = ?3, release_year = ?4 WHERE id = ?1",
            params![
                game_id,
                description,
                serde_json::to_string(genres)?,
                release_year
            ],
        )?;
        Ok(())
    }

    /// Games that still need a cover downloading.
    pub fn games_without_cover(&self) -> Result<Vec<Game>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {GAME_COLUMNS} FROM games WHERE cover_path IS NULL ORDER BY id"
        ))?;
        let games = stmt
            .query_map([], game_from_row)?
            .collect::<Result<_, _>>()?;
        Ok(games)
    }

    /// After a successful scan of `source`: every game from that source that
    /// the scan did **not** find is marked "not installed". Nothing is
    /// deleted, so playtime history survives. Returns how many were marked.
    pub fn mark_missing(&self, source: Source, found_source_ids: &[String]) -> Result<usize> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, source_id FROM games WHERE source = ?1 AND installed = 1")?;
        let installed: Vec<(i64, String)> = stmt
            .query_map([source.as_str()], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;

        let found: std::collections::HashSet<&str> =
            found_source_ids.iter().map(String::as_str).collect();
        let mut marked = 0;
        for (game_id, source_id) in installed {
            if !found.contains(source_id.as_str()) {
                self.set_installed(game_id, false)?;
                marked += 1;
            }
        }
        Ok(marked)
    }

    pub fn set_installed(&self, game_id: i64, installed: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET installed = ?2 WHERE id = ?1",
            params![game_id, installed],
        )?;
        Ok(())
    }

    /// For games that run an exe directly: mark each one installed or not
    /// depending on whether its exe still exists. Returns how many changed.
    pub fn refresh_exe_installed(&self) -> Result<usize> {
        let mut changed = 0;
        for game in self.list_games()? {
            if let LaunchTarget::Exe { path, .. } = &game.launch {
                let exists = path.is_file();
                if exists != game.installed {
                    self.set_installed(game.id, exists)?;
                    changed += 1;
                }
            }
        }
        Ok(changed)
    }

    /// Install folders of games from the given sources. The folder scan uses
    /// this to avoid listing a Steam/Epic/GOG game a second time.
    pub fn install_dirs(&self, sources: &[Source]) -> Result<Vec<PathBuf>> {
        Ok(self
            .list_games()?
            .into_iter()
            .filter(|g| sources.contains(&g.source))
            .filter_map(|g| g.install_dir)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steam_game(app_id: &str, title: &str) -> DetectedGame {
        DetectedGame {
            title: title.to_string(),
            source: Source::Steam,
            source_id: app_id.to_string(),
            install_dir: Some(PathBuf::from(format!("/steam/common/{title}"))),
            launch: LaunchTarget::Uri(format!("steam://rungameid/{app_id}")),
        }
    }

    fn titles(games: &[Game]) -> Vec<&str> {
        games.iter().map(|g| g.title.as_str()).collect()
    }

    #[test]
    fn insert_and_load_a_uri_game() {
        let db = Db::open_in_memory().unwrap();
        let outcome = db
            .upsert_detected(&steam_game("570", "Dota 2"), 1000)
            .unwrap();
        assert!(outcome.inserted);

        let game = db.require_game(outcome.game_id).unwrap();
        assert_eq!(game.title, "Dota 2");
        assert_eq!(game.source, Source::Steam);
        assert_eq!(game.source_id, "570");
        assert_eq!(
            game.launch,
            LaunchTarget::Uri("steam://rungameid/570".into())
        );
        assert_eq!(game.added_at, 1000);
        assert!(game.installed);
        assert!(!game.favourite);
        assert_eq!(game.playtime_secs, 0);
        assert_eq!(game.last_played, None);
    }

    #[test]
    fn exe_launch_target_round_trips_with_args_and_working_dir() {
        let db = Db::open_in_memory().unwrap();
        let detected = DetectedGame {
            title: "Old Game".into(),
            source: Source::Gog,
            source_id: "1207658924".into(),
            install_dir: Some(PathBuf::from("/gog/Old Game")),
            launch: LaunchTarget::Exe {
                path: PathBuf::from("/gog/Old Game/game.exe"),
                args: vec!["-windowed".into(), "--name=a b".into()],
                working_dir: Some(PathBuf::from("/gog/Old Game")),
            },
        };
        let id = db.upsert_detected(&detected, 1).unwrap().game_id;
        assert_eq!(db.require_game(id).unwrap().launch, detected.launch);
    }

    #[test]
    fn missing_game_is_none_or_an_error() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.get_game(42).unwrap().is_none());
        assert!(db.require_game(42).is_err());
    }

    #[test]
    fn rescan_updates_details_but_keeps_user_data() {
        let db = Db::open_in_memory().unwrap();
        let id = db
            .upsert_detected(&steam_game("570", "Dota"), 1000)
            .unwrap()
            .game_id;
        db.set_favourite(id, true).unwrap();
        db.set_cover_manual(id, Path::new("/covers/1.jpg")).unwrap();
        let tag = db.ensure_tag("MOBA", false).unwrap();
        db.tag_game(id, tag.id).unwrap();
        let session = db.start_session(id, 2000).unwrap();
        db.finish_session(session, 2600).unwrap();

        // The same game is found again, with a new title.
        let again = db
            .upsert_detected(&steam_game("570", "Dota 2"), 9999)
            .unwrap();
        assert!(!again.inserted);
        assert_eq!(again.game_id, id);

        let games = db.list_games().unwrap();
        assert_eq!(games.len(), 1, "no duplicate row");
        let game = &games[0];
        assert_eq!(game.title, "Dota 2");
        assert_eq!(game.added_at, 1000);
        assert!(game.favourite);
        assert_eq!(game.playtime_secs, 600);
        assert_eq!(game.cover_path, Some(PathBuf::from("/covers/1.jpg")));
        assert_eq!(db.tags_for_game(id).unwrap().len(), 1);
    }

    #[test]
    fn same_id_from_different_sources_are_different_games() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_detected(&steam_game("100", "A"), 1).unwrap();
        let mut epic = steam_game("100", "B");
        epic.source = Source::Epic;
        db.upsert_detected(&epic, 1).unwrap();
        assert_eq!(db.list_games().unwrap().len(), 2);
    }

    #[test]
    fn games_missing_from_a_scan_are_marked_not_deleted() {
        let db = Db::open_in_memory().unwrap();
        let kept = db
            .upsert_detected(&steam_game("1", "Kept"), 1)
            .unwrap()
            .game_id;
        let gone = db
            .upsert_detected(&steam_game("2", "Gone"), 1)
            .unwrap()
            .game_id;
        let manual = db
            .add_manual_game("Manual", Path::new("/g/m.exe"), 1)
            .unwrap();

        let marked = db.mark_missing(Source::Steam, &["1".to_string()]).unwrap();
        assert_eq!(marked, 1);
        assert!(db.require_game(kept).unwrap().installed);
        assert!(!db.require_game(gone).unwrap().installed);
        assert!(
            db.require_game(manual).unwrap().installed,
            "other sources untouched"
        );

        // Finding it again brings it back.
        db.upsert_detected(&steam_game("2", "Gone"), 5).unwrap();
        assert!(db.require_game(gone).unwrap().installed);
    }

    #[test]
    fn exe_games_follow_whether_the_file_exists() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("game.exe");
        std::fs::write(&exe, b"x").unwrap();

        let db = Db::open_in_memory().unwrap();
        let id = db.add_manual_game("Game", &exe, 1).unwrap();
        assert_eq!(db.refresh_exe_installed().unwrap(), 0);

        std::fs::remove_file(&exe).unwrap();
        assert_eq!(db.refresh_exe_installed().unwrap(), 1);
        assert!(!db.require_game(id).unwrap().installed);
    }

    #[test]
    fn search_ignores_case_and_treats_wildcards_literally() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_detected(&steam_game("1", "Ember Road"), 1)
            .unwrap();
        db.upsert_detected(&steam_game("2", "100% Orange"), 1)
            .unwrap();
        db.upsert_detected(&steam_game("3", "Roadside"), 1).unwrap();

        let find = |text: &str| {
            let games = db
                .query_games(&GameQuery {
                    search: Some(text.to_string()),
                    ..GameQuery::default()
                })
                .unwrap();
            games.into_iter().map(|g| g.title).collect::<Vec<_>>()
        };
        assert_eq!(find("road"), ["Ember Road", "Roadside"]);
        assert_eq!(find("%"), ["100% Orange"]);
        assert_eq!(find("  "), ["100% Orange", "Ember Road", "Roadside"]);
        assert!(find("zzz").is_empty());
    }

    #[test]
    fn filters_favourites_hidden_tag_and_source() {
        let db = Db::open_in_memory().unwrap();
        let a = db
            .upsert_detected(&steam_game("1", "A"), 1)
            .unwrap()
            .game_id;
        let b = db
            .upsert_detected(&steam_game("2", "B"), 1)
            .unwrap()
            .game_id;
        let c = db.add_manual_game("C", Path::new("/g/c.exe"), 1).unwrap();
        db.set_favourite(a, true).unwrap();
        db.set_hidden(b, true).unwrap();
        let backlog = db.ensure_tag("Backlog", true).unwrap();
        db.tag_game(c, backlog.id).unwrap();

        let q = |query: GameQuery| titles(&db.query_games(&query).unwrap()).join(",");
        assert_eq!(q(GameQuery::default()), "A,C", "hidden left out by default");
        assert_eq!(
            q(GameQuery {
                include_hidden: true,
                ..Default::default()
            }),
            "A,B,C"
        );
        assert_eq!(
            q(GameQuery {
                favourites_only: true,
                ..Default::default()
            }),
            "A"
        );
        assert_eq!(
            q(GameQuery {
                tag_id: Some(backlog.id),
                ..Default::default()
            }),
            "C"
        );
        assert_eq!(
            q(GameQuery {
                source: Some(Source::Manual),
                ..Default::default()
            }),
            "C"
        );
    }

    #[test]
    fn sort_orders() {
        let db = Db::open_in_memory().unwrap();
        let a = db
            .upsert_detected(&steam_game("1", "alpha"), 10)
            .unwrap()
            .game_id;
        let b = db
            .upsert_detected(&steam_game("2", "Beta"), 30)
            .unwrap()
            .game_id;
        let c = db
            .upsert_detected(&steam_game("3", "Gamma"), 20)
            .unwrap()
            .game_id;
        // a: 100 s ending at 1100; c: 500 s ending at 700; b never played.
        let s = db.start_session(a, 1000).unwrap();
        db.finish_session(s, 1100).unwrap();
        let s = db.start_session(c, 200).unwrap();
        db.finish_session(s, 700).unwrap();
        let _ = b;

        let order = |sort: SortBy| {
            titles(
                &db.query_games(&GameQuery {
                    sort,
                    ..Default::default()
                })
                .unwrap(),
            )
            .join(",")
        };
        assert_eq!(order(SortBy::Title), "alpha,Beta,Gamma");
        assert_eq!(order(SortBy::LastPlayed), "alpha,Gamma,Beta");
        assert_eq!(order(SortBy::Playtime), "Gamma,alpha,Beta");
        assert_eq!(order(SortBy::RecentlyAdded), "Beta,Gamma,alpha");
    }

    #[test]
    fn auto_cover_never_overwrites_a_manual_one() {
        let db = Db::open_in_memory().unwrap();
        let id = db
            .upsert_detected(&steam_game("1", "A"), 1)
            .unwrap()
            .game_id;
        assert_eq!(db.games_without_cover().unwrap().len(), 1);

        assert!(
            db.set_cover_auto(id, Path::new("/covers/auto.jpg"))
                .unwrap()
        );
        assert!(db.games_without_cover().unwrap().is_empty());

        db.set_cover_manual(id, Path::new("/covers/mine.png"))
            .unwrap();
        assert!(
            !db.set_cover_auto(id, Path::new("/covers/auto2.jpg"))
                .unwrap()
        );
        assert_eq!(
            db.require_game(id).unwrap().cover_path,
            Some(PathBuf::from("/covers/mine.png"))
        );
    }

    #[test]
    fn cover_lookups_are_not_repeated_until_the_retry_time() {
        let db = Db::open_in_memory().unwrap();
        let a = db
            .upsert_detected(&steam_game("1", "A"), 1)
            .unwrap()
            .game_id;
        let b = db
            .upsert_detected(&steam_game("2", "B"), 1)
            .unwrap()
            .game_id;
        let week = 7 * 86_400;
        let due = |now: i64, retry: i64| -> Vec<i64> {
            db.games_due_cover_lookup(now, retry)
                .unwrap()
                .iter()
                .map(|g| g.id)
                .collect()
        };
        assert_eq!(due(1000, week), [a, b], "never looked up: both due");

        db.mark_art_checked(a, 1000).unwrap();
        assert_eq!(due(2000, week), [b], "A was only just checked");
        assert_eq!(
            due(1000 + week, week),
            [a, b],
            "a week later A is due again"
        );
        assert_eq!(due(2000, 0), [a, b], "a retry time of 0 means everything");

        // A game that has a cover is never due.
        db.set_cover_auto(b, Path::new("/covers/2.jpg")).unwrap();
        assert_eq!(due(2000, 0), [a]);
    }

    #[test]
    fn icons_are_saved_and_listed() {
        let db = Db::open_in_memory().unwrap();
        let a = db
            .upsert_detected(&steam_game("1", "A"), 1)
            .unwrap()
            .game_id;
        let b = db
            .upsert_detected(&steam_game("2", "B"), 1)
            .unwrap()
            .game_id;
        assert_eq!(db.games_without_icon().unwrap().len(), 2);

        db.set_icon(a, Path::new("/icons/1.png")).unwrap();
        assert_eq!(
            db.require_game(a).unwrap().icon_path,
            Some(PathBuf::from("/icons/1.png"))
        );
        let without: Vec<i64> = db
            .games_without_icon()
            .unwrap()
            .iter()
            .map(|g| g.id)
            .collect();
        assert_eq!(without, [b]);

        // A rescan keeps the icon.
        db.upsert_detected(&steam_game("1", "A"), 9).unwrap();
        assert!(db.require_game(a).unwrap().icon_path.is_some());
    }

    #[test]
    fn game_info_round_trips() {
        let db = Db::open_in_memory().unwrap();
        let id = db
            .upsert_detected(&steam_game("1", "A"), 1)
            .unwrap()
            .game_id;
        let genres = vec!["Action".to_string(), "RPG".to_string()];
        db.set_game_info(id, Some("A game."), &genres, Some(2013))
            .unwrap();
        let game = db.require_game(id).unwrap();
        assert_eq!(game.description.as_deref(), Some("A game."));
        assert_eq!(game.genres, genres);
        assert_eq!(game.release_year, Some(2013));
    }

    #[test]
    fn deleting_a_game_removes_its_sessions_and_tags() {
        let db = Db::open_in_memory().unwrap();
        let id = db
            .upsert_detected(&steam_game("1", "A"), 1)
            .unwrap()
            .game_id;
        let tag = db.ensure_tag("T", false).unwrap();
        db.tag_game(id, tag.id).unwrap();
        db.start_session(id, 5).unwrap();

        db.delete_game(id).unwrap();
        assert!(db.get_game(id).unwrap().is_none());
        assert_eq!(db.tag_game_count(tag.id).unwrap(), 0);
        assert!(db.sessions_for_game(id).unwrap().is_empty());
    }

    #[test]
    fn manual_game_needs_a_title() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.add_manual_game("   ", Path::new("/g/a.exe"), 1).is_err());
    }
}
