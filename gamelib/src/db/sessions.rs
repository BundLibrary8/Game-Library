//! The `play_sessions` table.
//!
//! Playtime is stored twice on purpose: once per session here (for stats),
//! and as a running total in `games.playtime_secs` (so the grid loads fast).
//! A session's time is added to the total exactly once, when it is closed.

use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use super::Db;
use crate::model::PlaySession;
use crate::shop;

/// What closing a session did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionSummary {
    pub session_id: i64,
    pub game_id: i64,
    pub duration_secs: u64,
    /// Stardust earned for this session's playtime.
    pub stardust: u32,
}

fn session_from_row(row: &rusqlite::Row) -> rusqlite::Result<PlaySession> {
    Ok(PlaySession {
        id: row.get(0)?,
        game_id: row.get(1)?,
        started_at: row.get(2)?,
        ended_at: row.get(3)?,
        duration_secs: row.get::<_, i64>(4)?.max(0) as u64,
    })
}

/// Add an already-timed session to its game's totals and pay the Stardust.
/// Runs inside the caller's transaction.
fn close_session(conn: &Connection, session_id: i64) -> Result<SessionSummary> {
    let row: Option<(i64, i64, Option<i64>, i64, bool)> = conn
        .query_row(
            "SELECT game_id, started_at, ended_at, duration_secs, closed
             FROM play_sessions WHERE id = ?1",
            [session_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    let Some((game_id, started_at, ended_at, duration, closed)) = row else {
        bail!("there is no play session with id {session_id}");
    };
    if closed {
        bail!("play session {session_id} has already been closed");
    }
    let ended_at = ended_at.unwrap_or(started_at);
    let duration_secs = duration.max(0) as u64;

    conn.execute(
        "UPDATE play_sessions SET closed = 1 WHERE id = ?1",
        [session_id],
    )?;
    conn.execute(
        "UPDATE games SET playtime_secs = playtime_secs + ?2,
            last_played = MAX(COALESCE(last_played, 0), ?3)
         WHERE id = ?1",
        params![game_id, duration_secs as i64, ended_at],
    )?;
    let stardust = shop::award_playtime(conn, game_id, duration_secs, ended_at)?;

    Ok(SessionSummary {
        session_id,
        game_id,
        duration_secs,
        stardust,
    })
}

impl Db {
    /// The game has been seen running: open a session.
    pub fn start_session(&self, game_id: i64, started_at: i64) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO play_sessions(game_id, started_at) VALUES (?1, ?2)",
            [game_id, started_at],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Save progress on a running session (done about once a minute), so a
    /// crash or power cut loses at most that much playtime.
    pub fn checkpoint_session(&self, session_id: i64, now: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE play_sessions
             SET ended_at = ?2, duration_secs = MAX(0, ?2 - started_at)
             WHERE id = ?1 AND closed = 0",
            [session_id, now],
        )?;
        Ok(())
    }

    /// The game has closed: finish the session, add its time to the game's
    /// total, set "last played", and pay the Stardust. All in one
    /// transaction, so either everything is saved or nothing is.
    pub fn finish_session(&self, session_id: i64, ended_at: i64) -> Result<SessionSummary> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE play_sessions
             SET ended_at = ?2, duration_secs = MAX(0, ?2 - started_at)
             WHERE id = ?1 AND closed = 0",
            [session_id, ended_at],
        )?;
        let summary = close_session(&tx, session_id)?;
        tx.commit()?;
        Ok(summary)
    }

    /// Run at startup. Sessions left open by a crash are closed using their
    /// last checkpoint, so the time played up to then still counts.
    pub fn recover_unfinished_sessions(&self) -> Result<Vec<SessionSummary>> {
        let tx = self.conn.unchecked_transaction()?;
        let open_ids: Vec<i64> = {
            let mut stmt =
                tx.prepare("SELECT id FROM play_sessions WHERE closed = 0 ORDER BY id")?;
            stmt.query_map([], |row| row.get(0))?
                .collect::<Result<_, _>>()?
        };
        let mut summaries = Vec::new();
        for session_id in open_ids {
            summaries.push(close_session(&tx, session_id)?);
        }
        tx.commit()?;
        Ok(summaries)
    }

    /// A game's sessions, newest first.
    pub fn sessions_for_game(&self, game_id: i64) -> Result<Vec<PlaySession>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, game_id, started_at, ended_at, duration_secs
             FROM play_sessions WHERE game_id = ?1 ORDER BY started_at DESC, id DESC",
        )?;
        let sessions = stmt
            .query_map([game_id], session_from_row)?
            .collect::<Result<_, _>>()?;
        Ok(sessions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DetectedGame;

    fn db_with_game() -> (Db, i64) {
        let db = Db::open_in_memory().unwrap();
        let id = db
            .upsert_detected(
                &DetectedGame::manual("Ember Road", "/g/ember/ember.exe"),
                100,
            )
            .unwrap()
            .game_id;
        (db, id)
    }

    #[test]
    fn finishing_a_session_updates_totals_and_last_played() {
        let (db, game) = db_with_game();
        let s1 = db.start_session(game, 1000).unwrap();
        let summary = db.finish_session(s1, 1600).unwrap();
        assert_eq!(summary.game_id, game);
        assert_eq!(summary.duration_secs, 600);

        let s2 = db.start_session(game, 5000).unwrap();
        db.finish_session(s2, 5300).unwrap();

        let g = db.require_game(game).unwrap();
        assert_eq!(g.playtime_secs, 900);
        assert_eq!(g.last_played, Some(5300));

        let sessions = db.sessions_for_game(game).unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].id, s2, "newest first");
        assert_eq!(sessions[1].duration_secs, 600);
    }

    #[test]
    fn a_session_cannot_be_counted_twice() {
        let (db, game) = db_with_game();
        let s = db.start_session(game, 1000).unwrap();
        db.finish_session(s, 1600).unwrap();
        assert!(db.finish_session(s, 9999).is_err());
        assert_eq!(db.require_game(game).unwrap().playtime_secs, 600);
        assert_eq!(db.sessions_for_game(game).unwrap()[0].ended_at, Some(1600));
    }

    #[test]
    fn checkpoints_do_not_touch_the_total_until_the_session_closes() {
        let (db, game) = db_with_game();
        let s = db.start_session(game, 1000).unwrap();
        db.checkpoint_session(s, 1060).unwrap();
        db.checkpoint_session(s, 1120).unwrap();
        assert_eq!(db.require_game(game).unwrap().playtime_secs, 0);
        assert_eq!(db.sessions_for_game(game).unwrap()[0].duration_secs, 120);

        db.finish_session(s, 1150).unwrap();
        assert_eq!(db.require_game(game).unwrap().playtime_secs, 150);
    }

    #[test]
    fn crash_recovery_counts_time_up_to_the_last_checkpoint() {
        let (db, game) = db_with_game();
        let s = db.start_session(game, 1000).unwrap();
        db.checkpoint_session(s, 1300).unwrap();
        // …the PC crashes here, and the app starts again:
        let recovered = db.recover_unfinished_sessions().unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].duration_secs, 300);

        let g = db.require_game(game).unwrap();
        assert_eq!(g.playtime_secs, 300);
        assert_eq!(g.last_played, Some(1300));

        // Running recovery again changes nothing.
        assert!(db.recover_unfinished_sessions().unwrap().is_empty());
        assert_eq!(db.require_game(game).unwrap().playtime_secs, 300);
    }

    #[test]
    fn clock_going_backwards_never_gives_negative_time() {
        let (db, game) = db_with_game();
        let s = db.start_session(game, 1000).unwrap();
        let summary = db.finish_session(s, 900).unwrap();
        assert_eq!(summary.duration_secs, 0);
        assert_eq!(db.require_game(game).unwrap().playtime_secs, 0);
    }

    #[test]
    fn finishing_an_unknown_session_is_an_error() {
        let (db, _) = db_with_game();
        assert!(db.finish_session(77, 1).is_err());
    }
}
