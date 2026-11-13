//! The extra things you can record about a game: how far through it you
//! are, your rating, the franchise it belongs to, where its saves and
//! mods live, and its system requirements.
//!
//! These are kept apart from [`crate::model::Game`] so the core type
//! stays small; ask for them with [`Db::extras`] or [`Db::all_extras`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use rusqlite::params;

use crate::db::Db;

/// How far through a game you are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    Backlog,
    Playing,
    Finished,
    OnHold,
    Abandoned,
}

impl Status {
    pub const ALL: [Status; 5] = [
        Status::Backlog,
        Status::Playing,
        Status::Finished,
        Status::OnHold,
        Status::Abandoned,
    ];

    /// The text stored in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Backlog => "backlog",
            Status::Playing => "playing",
            Status::Finished => "finished",
            Status::OnHold => "on_hold",
            Status::Abandoned => "abandoned",
        }
    }

    pub fn parse(text: &str) -> Option<Status> {
        Status::ALL.into_iter().find(|s| s.as_str() == text)
    }

    /// The name shown to the user.
    pub fn label(self) -> &'static str {
        match self {
            Status::Backlog => "Backlog",
            Status::Playing => "Playing",
            Status::Finished => "Finished",
            Status::OnHold => "On hold",
            Status::Abandoned => "Abandoned",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GameExtras {
    pub status: Option<Status>,
    /// Your rating, 1 to 5 stars.
    pub rating: Option<u8>,
    pub franchise: Option<String>,
    /// Where it comes in the franchise's playing order (1 is first).
    pub franchise_order: Option<i64>,
    /// The folder the game keeps its saves in.
    pub save_dir: Option<PathBuf>,
    /// The folder the game's mods are installed in.
    pub mods_dir: Option<PathBuf>,
    /// Minimum and recommended system requirements, as plain text.
    pub requirements_min: Option<String>,
    pub requirements_rec: Option<String>,
    /// How long the game takes to beat, in minutes, as you entered it.
    pub minutes_to_beat: Option<u32>,
    /// Private notes: where you got to, tips, anything.
    pub notes: Option<String>,
    /// Your review of the game.
    pub review: Option<String>,
    /// The name of the game's exe (like `eldenring.exe`), for games whose
    /// process doesn't run from their install folder.
    pub process_name: Option<String>,
}

const EXTRA_COLUMNS: &str = "status, rating, franchise, franchise_order, save_dir, mods_dir,
    requirements_min, requirements_rec, minutes_to_beat, notes, review, process_name";

fn extras_from_row(row: &rusqlite::Row, first: usize) -> rusqlite::Result<GameExtras> {
    Ok(GameExtras {
        status: row
            .get::<_, Option<String>>(first)?
            .and_then(|text| Status::parse(&text)),
        rating: row.get(first + 1)?,
        franchise: row.get(first + 2)?,
        franchise_order: row.get(first + 3)?,
        save_dir: row.get::<_, Option<String>>(first + 4)?.map(PathBuf::from),
        mods_dir: row.get::<_, Option<String>>(first + 5)?.map(PathBuf::from),
        requirements_min: row.get(first + 6)?,
        requirements_rec: row.get(first + 7)?,
        minutes_to_beat: row.get(first + 8)?,
        notes: row.get(first + 9)?,
        review: row.get(first + 10)?,
        process_name: row.get(first + 11)?,
    })
}

fn path_text(path: Option<&Path>) -> Option<String> {
    path.map(|p| p.to_string_lossy().into_owned())
}

impl Db {
    /// One game's extras. A game with nothing recorded gives the default.
    pub fn extras(&self, game_id: i64) -> Result<GameExtras> {
        Ok(self.conn.query_row(
            &format!("SELECT {EXTRA_COLUMNS} FROM games WHERE id = ?1"),
            [game_id],
            |row| extras_from_row(row, 0),
        )?)
    }

    /// Every game's extras, by game id.
    pub fn all_extras(&self) -> Result<HashMap<i64, GameExtras>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT id, {EXTRA_COLUMNS} FROM games"))?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, extras_from_row(row, 1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn set_status(&self, game_id: i64, status: Option<Status>) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET status = ?2 WHERE id = ?1",
            params![game_id, status.map(Status::as_str)],
        )?;
        Ok(())
    }

    /// Set (or clear) your rating. It must be 1 to 5.
    pub fn set_rating(&self, game_id: i64, rating: Option<u8>) -> Result<()> {
        if let Some(stars) = rating {
            anyhow::ensure!((1..=5).contains(&stars), "a rating is 1 to 5 stars");
        }
        self.conn.execute(
            "UPDATE games SET rating = ?2 WHERE id = ?1",
            params![game_id, rating],
        )?;
        Ok(())
    }

    /// Put a game in a franchise (or take it out with `None`), with its
    /// place in the playing order if known.
    pub fn set_franchise(
        &self,
        game_id: i64,
        franchise: Option<&str>,
        order: Option<i64>,
    ) -> Result<()> {
        let franchise = franchise.map(str::trim).filter(|name| !name.is_empty());
        let order = if franchise.is_some() { order } else { None };
        self.conn.execute(
            "UPDATE games SET franchise = ?2, franchise_order = ?3 WHERE id = ?1",
            params![game_id, franchise, order],
        )?;
        Ok(())
    }

    pub fn set_save_dir(&self, game_id: i64, dir: Option<&Path>) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET save_dir = ?2 WHERE id = ?1",
            params![game_id, path_text(dir)],
        )?;
        Ok(())
    }

    pub fn set_mods_dir(&self, game_id: i64, dir: Option<&Path>) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET mods_dir = ?2 WHERE id = ?1",
            params![game_id, path_text(dir)],
        )?;
        Ok(())
    }

    pub fn set_requirements(
        &self,
        game_id: i64,
        minimum: Option<&str>,
        recommended: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET requirements_min = ?2, requirements_rec = ?3 WHERE id = ?1",
            params![game_id, minimum, recommended],
        )?;
        Ok(())
    }

    /// Set (or clear) how long the game takes to beat, in minutes.
    pub fn set_minutes_to_beat(&self, game_id: i64, minutes: Option<u32>) -> Result<()> {
        let minutes = minutes.filter(|m| *m > 0);
        self.conn.execute(
            "UPDATE games SET minutes_to_beat = ?2 WHERE id = ?1",
            params![game_id, minutes],
        )?;
        Ok(())
    }

    /// Save your notes. Blank text clears them.
    pub fn set_notes(&self, game_id: i64, notes: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET notes = ?2 WHERE id = ?1",
            params![game_id, blank_to_none(notes)],
        )?;
        Ok(())
    }

    /// Save your review. Blank text clears it.
    pub fn set_review(&self, game_id: i64, review: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET review = ?2 WHERE id = ?1",
            params![game_id, blank_to_none(review)],
        )?;
        Ok(())
    }

    /// Set the exe name to watch for when tracking playtime, like
    /// `eldenring.exe`. Only the file name is kept.
    pub fn set_process_name(&self, game_id: i64, name: Option<&str>) -> Result<()> {
        let name = blank_to_none(name)
            .map(|name| name.rsplit(['/', '\\']).next().unwrap_or(name).to_string());
        self.conn.execute(
            "UPDATE games SET process_name = ?2 WHERE id = ?1",
            params![game_id, name],
        )?;
        Ok(())
    }
}

fn blank_to_none(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DetectedGame;

    fn db_with_game() -> (Db, i64) {
        let db = Db::open_in_memory().unwrap();
        let id = db
            .upsert_detected(&DetectedGame::manual("Ember Road", "/g/ember/ember.exe"), 1)
            .unwrap()
            .game_id;
        (db, id)
    }

    #[test]
    fn status_text_round_trips() {
        for status in Status::ALL {
            assert_eq!(Status::parse(status.as_str()), Some(status));
        }
        assert_eq!(Status::parse("nonsense"), None);
    }

    #[test]
    fn a_new_game_has_no_extras() {
        let (db, id) = db_with_game();
        assert_eq!(db.extras(id).unwrap(), GameExtras::default());
    }

    #[test]
    fn extras_save_load_and_clear() {
        let (db, id) = db_with_game();
        db.set_status(id, Some(Status::Playing)).unwrap();
        db.set_rating(id, Some(4)).unwrap();
        db.set_franchise(id, Some("  Ember  "), Some(2)).unwrap();
        db.set_save_dir(id, Some(Path::new("/saves/ember")))
            .unwrap();
        db.set_mods_dir(id, Some(Path::new("/g/ember/mods")))
            .unwrap();
        db.set_requirements(id, Some("Memory: 8 GB RAM"), None)
            .unwrap();

        let extras = db.extras(id).unwrap();
        assert_eq!(extras.status, Some(Status::Playing));
        assert_eq!(extras.rating, Some(4));
        assert_eq!(extras.franchise.as_deref(), Some("Ember"));
        assert_eq!(extras.franchise_order, Some(2));
        assert_eq!(extras.save_dir, Some(PathBuf::from("/saves/ember")));
        assert_eq!(extras.mods_dir, Some(PathBuf::from("/g/ember/mods")));
        assert_eq!(extras.requirements_min.as_deref(), Some("Memory: 8 GB RAM"));
        assert_eq!(db.all_extras().unwrap()[&id], extras);

        db.set_status(id, None).unwrap();
        db.set_rating(id, None).unwrap();
        db.set_franchise(id, Some("   "), Some(9)).unwrap();
        let cleared = db.extras(id).unwrap();
        assert_eq!(cleared.status, None);
        assert_eq!(cleared.rating, None);
        assert_eq!(cleared.franchise, None);
        assert_eq!(cleared.franchise_order, None, "no franchise means no order");
    }

    #[test]
    fn ratings_must_be_one_to_five() {
        let (db, id) = db_with_game();
        assert!(db.set_rating(id, Some(0)).is_err());
        assert!(db.set_rating(id, Some(6)).is_err());
        assert!(db.set_rating(id, Some(5)).is_ok());
    }

    #[test]
    fn a_rescan_keeps_the_extras() {
        let (db, id) = db_with_game();
        db.set_status(id, Some(Status::Finished)).unwrap();
        db.set_rating(id, Some(5)).unwrap();
        db.upsert_detected(&DetectedGame::manual("Ember Road", "/g/ember/ember.exe"), 9)
            .unwrap();
        let extras = db.extras(id).unwrap();
        assert_eq!(
            (extras.status, extras.rating),
            (Some(Status::Finished), Some(5))
        );
    }

    #[test]
    fn notes_reviews_lengths_and_process_names_are_kept() {
        let (db, id) = db_with_game();
        db.set_minutes_to_beat(id, Some(600)).unwrap();
        db.set_notes(id, Some("  Got to the second boss  "))
            .unwrap();
        db.set_review(id, Some("Loved it")).unwrap();
        db.set_process_name(id, Some(r"C:\Games\Ember\ember-win64.exe"))
            .unwrap();
        let extras = db.extras(id).unwrap();
        assert_eq!(extras.minutes_to_beat, Some(600));
        assert_eq!(extras.notes.as_deref(), Some("Got to the second boss"));
        assert_eq!(extras.review.as_deref(), Some("Loved it"));
        assert_eq!(extras.process_name.as_deref(), Some("ember-win64.exe"));

        db.set_minutes_to_beat(id, Some(0)).unwrap();
        db.set_notes(id, Some("   ")).unwrap();
        db.set_process_name(id, None).unwrap();
        let cleared = db.extras(id).unwrap();
        assert_eq!(cleared.minutes_to_beat, None);
        assert_eq!(cleared.notes, None);
        assert_eq!(cleared.process_name, None);
    }
}
