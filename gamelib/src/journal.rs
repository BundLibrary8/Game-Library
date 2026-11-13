//! Session journals and playtime goals.
//!
//! - **Journal:** each play session can have a note ("beat the second
//!   boss", "tried the new build"), so a game's sessions read like a diary.
//! - **Goals:** a weekly playtime goal for the whole library ("play 8 hours
//!   a week", or as a limit, "no more than 10"), and an hours goal per game
//!   ("put 20 hours into this").

use anyhow::{Result, ensure};
use chrono::{Datelike, FixedOffset, TimeZone};
use rusqlite::params;

use crate::db::Db;

/// One session, as it appears in a game's journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    pub session_id: i64,
    pub started_at: i64,
    pub duration_secs: u64,
    pub note: Option<String>,
}

const WEEKLY_GOAL_KEY: &str = "goal.weekly_minutes";
const WEEKLY_LIMIT_KEY: &str = "goal.weekly_is_limit";

/// The weekly goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeeklyGoal {
    pub minutes: u32,
    /// A limit to stay under, rather than a target to reach.
    pub is_limit: bool,
}

/// How a goal is going.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub done_minutes: u32,
    pub goal_minutes: u32,
}

impl Progress {
    pub fn fraction(&self) -> f32 {
        self.done_minutes as f32 / self.goal_minutes.max(1) as f32
    }

    pub fn reached(&self) -> bool {
        self.done_minutes >= self.goal_minutes
    }
}

/// When the week containing `now` started (Monday 00:00, local time).
pub fn week_start(now: i64, offset: i32) -> i64 {
    let zone = FixedOffset::east_opt(offset).unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
    let Some(local) = zone.timestamp_opt(now, 0).single() else {
        return now;
    };
    let monday =
        local.date_naive() - chrono::Days::new(local.weekday().num_days_from_monday() as u64);
    monday
        .and_hms_opt(0, 0, 0)
        .and_then(|t| zone.from_local_datetime(&t).single())
        .map(|t| t.timestamp())
        .unwrap_or(now)
}

impl Db {
    /// A game's sessions, newest first, with their notes.
    pub fn journal(&self, game_id: i64) -> Result<Vec<JournalEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, started_at, duration_secs, note FROM play_sessions
             WHERE game_id = ?1 AND closed = 1 ORDER BY started_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([game_id], |row| {
            Ok(JournalEntry {
                session_id: row.get(0)?,
                started_at: row.get(1)?,
                duration_secs: row.get::<_, i64>(2)?.max(0) as u64,
                note: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Write (or clear, with blank text) a session's note.
    pub fn set_session_note(&self, session_id: i64, note: &str) -> Result<()> {
        let note = Some(note.trim()).filter(|n| !n.is_empty());
        self.conn.execute(
            "UPDATE play_sessions SET note = ?2 WHERE id = ?1",
            params![session_id, note],
        )?;
        Ok(())
    }

    /// The newest finished session of a game, for "how did it go?".
    pub fn last_session(&self, game_id: i64) -> Result<Option<JournalEntry>> {
        Ok(self.journal(game_id)?.into_iter().next())
    }

    pub fn weekly_goal(&self) -> Result<Option<WeeklyGoal>> {
        let minutes = self
            .get_setting(WEEKLY_GOAL_KEY)?
            .and_then(|m| m.parse::<u32>().ok())
            .filter(|m| *m > 0);
        Ok(minutes.map(|minutes| WeeklyGoal {
            minutes,
            is_limit: self
                .get_bool_setting(WEEKLY_LIMIT_KEY, false)
                .unwrap_or(false),
        }))
    }

    pub fn set_weekly_goal(&self, goal: Option<WeeklyGoal>) -> Result<()> {
        match goal.filter(|g| g.minutes > 0) {
            Some(goal) => {
                ensure!(goal.minutes <= 7 * 24 * 60, "a week only has 168 hours");
                self.set_setting(WEEKLY_GOAL_KEY, &goal.minutes.to_string())?;
                self.set_bool_setting(WEEKLY_LIMIT_KEY, goal.is_limit)
            }
            None => self.delete_setting(WEEKLY_GOAL_KEY),
        }
    }

    /// Minutes played since the start of this week.
    pub fn minutes_this_week(&self, now: i64, offset: i32) -> Result<u32> {
        let since = week_start(now, offset);
        let secs: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(duration_secs), 0) FROM play_sessions
             WHERE started_at >= ?1 AND closed = 1",
            [since],
            |row| row.get(0),
        )?;
        Ok((secs.max(0) / 60) as u32)
    }

    /// The weekly goal and how it's going, if one is set.
    pub fn weekly_progress(&self, now: i64, offset: i32) -> Result<Option<(WeeklyGoal, Progress)>> {
        let Some(goal) = self.weekly_goal()? else {
            return Ok(None);
        };
        Ok(Some((
            goal,
            Progress {
                done_minutes: self.minutes_this_week(now, offset)?,
                goal_minutes: goal.minutes,
            },
        )))
    }

    /// A game's hours goal, in minutes.
    pub fn game_goal(&self, game_id: i64) -> Result<Option<u32>> {
        Ok(self.conn.query_row(
            "SELECT goal_minutes FROM games WHERE id = ?1",
            [game_id],
            |row| row.get(0),
        )?)
    }

    pub fn set_game_goal(&self, game_id: i64, minutes: Option<u32>) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET goal_minutes = ?2 WHERE id = ?1",
            params![game_id, minutes.filter(|m| *m > 0)],
        )?;
        Ok(())
    }

    /// Every game with an hours goal, and its progress.
    pub fn game_goals(&self) -> Result<Vec<(i64, Progress)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, playtime_secs, goal_minutes FROM games
             WHERE goal_minutes IS NOT NULL AND hidden = 0 ORDER BY title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                Progress {
                    done_minutes: (row.get::<_, i64>(1)?.max(0) / 60) as u32,
                    goal_minutes: row.get(2)?,
                },
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DetectedGame;

    // 2026-01-07 12:00 UTC, a Wednesday.
    const WED: i64 = 1_767_787_200;

    fn db_with_game() -> (Db, i64) {
        let db = Db::open_in_memory().unwrap();
        let id = db
            .upsert_detected(&DetectedGame::manual("A", "/g/a.exe"), 1)
            .unwrap()
            .game_id;
        (db, id)
    }

    #[test]
    fn weeks_start_on_monday_at_midnight_local_time() {
        assert_eq!(week_start(WED, 0), 1_767_571_200);
        // At UTC+3, Wednesday noon UTC is still that week; Monday 00:00
        // local is Sunday 21:00 UTC.
        assert_eq!(week_start(WED, 3 * 3600), 1_767_571_200 - 3 * 3600);
    }

    #[test]
    fn sessions_carry_notes() {
        let (db, game) = db_with_game();
        let s = db.start_session(game, 100).unwrap();
        db.finish_session(s, 3700).unwrap();
        db.set_session_note(s, "  Beat the first boss ").unwrap();
        let journal = db.journal(game).unwrap();
        assert_eq!(journal.len(), 1);
        assert_eq!(journal[0].note.as_deref(), Some("Beat the first boss"));
        assert_eq!(journal[0].duration_secs, 3600);
        db.set_session_note(s, "  ").unwrap();
        assert_eq!(db.last_session(game).unwrap().unwrap().note, None);
    }

    #[test]
    fn weekly_goals_count_this_week_only() {
        let (db, game) = db_with_game();
        let this_week = db.start_session(game, WED - 3600).unwrap();
        db.finish_session(this_week, WED - 600).unwrap();
        let last_week = db.start_session(game, WED - 7 * 86_400).unwrap();
        db.finish_session(last_week, WED - 7 * 86_400 + 3600)
            .unwrap();
        assert_eq!(db.weekly_progress(WED, 0).unwrap(), None);

        db.set_weekly_goal(Some(WeeklyGoal {
            minutes: 100,
            is_limit: true,
        }))
        .unwrap();
        let (goal, progress) = db.weekly_progress(WED, 0).unwrap().unwrap();
        assert!(goal.is_limit);
        assert_eq!(progress.done_minutes, 50);
        assert!(!progress.reached());
        assert!(
            db.set_weekly_goal(Some(WeeklyGoal {
                minutes: 20_000,
                is_limit: false
            }))
            .is_err()
        );
        db.set_weekly_goal(None).unwrap();
        assert_eq!(db.weekly_goal().unwrap(), None);
    }

    #[test]
    fn game_goals() {
        let (db, game) = db_with_game();
        db.set_game_goal(game, Some(120)).unwrap();
        assert_eq!(db.game_goal(game).unwrap(), Some(120));
        assert_eq!(db.game_goals().unwrap()[0].1.goal_minutes, 120);
        db.set_game_goal(game, Some(0)).unwrap();
        assert!(db.game_goals().unwrap().is_empty());
    }
}
