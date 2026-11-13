//! Queries for the stats page and the profile's stat tiles.

use anyhow::Result;

use crate::db::{Db, GameQuery, SortBy};
use crate::model::{Game, Source};

const SECS_PER_DAY: i64 = 86_400;

/// The headline numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryStats {
    pub game_count: u64,
    pub installed_count: u64,
    pub total_playtime_secs: u64,
    pub session_count: u64,
    pub finished_count: u64,
    /// Title and playtime of the most-played game, if anything has been played.
    pub most_played: Option<(String, u64)>,
}

/// Playtime on one calendar day (UTC).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayPlaytime {
    /// Unix timestamp of the start of the day.
    pub day_start: i64,
    pub secs: u64,
}

impl Db {
    pub fn library_stats(&self) -> Result<LibraryStats> {
        let (game_count, installed_count, total_playtime_secs): (i64, i64, i64) = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(installed), 0), COALESCE(SUM(playtime_secs), 0) FROM games",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let session_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM play_sessions WHERE closed = 1",
            [],
            |row| row.get(0),
        )?;
        let most_played = self
            .top_played(1)?
            .into_iter()
            .next()
            .map(|g| (g.title, g.playtime_secs));

        Ok(LibraryStats {
            game_count: game_count as u64,
            installed_count: installed_count as u64,
            total_playtime_secs: total_playtime_secs as u64,
            session_count: session_count as u64,
            finished_count: self.finished_count()?,
            most_played,
        })
    }

    /// The most-played games, most first. Games never played are left out.
    pub fn top_played(&self, limit: usize) -> Result<Vec<Game>> {
        let games = self.query_games(&GameQuery {
            include_hidden: true,
            sort: SortBy::Playtime,
            ..GameQuery::default()
        })?;
        Ok(games
            .into_iter()
            .filter(|g| g.playtime_secs > 0)
            .take(limit)
            .collect())
    }

    /// Playtime per day for sessions that started in `since..until`, oldest
    /// day first. Days with no play are left out. A session counts towards
    /// the day it started on.
    pub fn playtime_by_day(&self, since: i64, until: i64) -> Result<Vec<DayPlaytime>> {
        let mut stmt = self.conn.prepare(
            "SELECT started_at / ?3 AS day, SUM(duration_secs)
             FROM play_sessions
             WHERE started_at >= ?1 AND started_at < ?2
             GROUP BY day ORDER BY day",
        )?;
        let days = stmt
            .query_map([since, until, SECS_PER_DAY], |row| {
                Ok(DayPlaytime {
                    day_start: row.get::<_, i64>(0)? * SECS_PER_DAY,
                    secs: row.get::<_, i64>(1)?.max(0) as u64,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(days)
    }

    /// Playtime for each of the last `weeks` weeks, oldest first, ending
    /// with the week that finishes at `now`. Pass a game id for one game,
    /// or `None` for the whole library. Weeks with no play are 0, so the
    /// result always has `weeks` entries (handy for drawing a chart).
    pub fn weekly_playtime(
        &self,
        game_id: Option<i64>,
        now: i64,
        weeks: usize,
    ) -> Result<Vec<u64>> {
        const WEEK: i64 = 7 * SECS_PER_DAY;
        let start = now - weeks as i64 * WEEK;
        let mut stmt = self.conn.prepare(
            "SELECT started_at, duration_secs FROM play_sessions
             WHERE started_at > ?1 AND started_at <= ?2 AND (?3 IS NULL OR game_id = ?3)",
        )?;
        let rows = stmt.query_map(rusqlite::params![start, now, game_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut totals = vec![0u64; weeks];
        for row in rows {
            let (started_at, duration) = row?;
            // started_at is in (start, now], so the index is 0..weeks.
            let index = ((started_at - start - 1) / WEEK) as usize;
            if let Some(total) = totals.get_mut(index) {
                *total += duration.max(0) as u64;
            }
        }
        Ok(totals)
    }

    /// Every finished session since `since`, as `(game_id, started_at,
    /// duration_secs)`, oldest first.
    pub fn session_rows(&self, since: i64) -> Result<Vec<(i64, i64, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT game_id, started_at, duration_secs FROM play_sessions
             WHERE closed = 1 AND started_at >= ?1 ORDER BY started_at",
        )?;
        let rows = stmt
            .query_map([since], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get::<_, i64>(2)?.max(0) as u64,
                ))
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// How many finished play sessions a game has had.
    pub fn session_count(&self, game_id: i64) -> Result<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM play_sessions WHERE game_id = ?1 AND closed = 1",
            [game_id],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    /// Total playtime per source (Steam, Epic…), biggest first. Sources with
    /// no playtime are left out.
    pub fn playtime_by_source(&self) -> Result<Vec<(Source, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT source, SUM(playtime_secs) AS total FROM games
             GROUP BY source HAVING total > 0 ORDER BY total DESC",
        )?;
        let rows: Vec<(String, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        rows.into_iter()
            .map(|(source, total)| Ok((Source::parse(&source)?, total as u64)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DetectedGame, LaunchTarget};

    fn add(db: &Db, source: Source, id: &str, title: &str) -> i64 {
        let game = DetectedGame {
            title: title.into(),
            source,
            source_id: id.into(),
            install_dir: None,
            launch: LaunchTarget::Uri(format!("test://{id}")),
        };
        db.upsert_detected(&game, 1).unwrap().game_id
    }

    fn play(db: &Db, game: i64, start: i64, secs: i64) {
        let s = db.start_session(game, start).unwrap();
        db.finish_session(s, start + secs).unwrap();
    }

    #[test]
    fn empty_library() {
        let db = Db::open_in_memory().unwrap();
        let stats = db.library_stats().unwrap();
        assert_eq!(stats.game_count, 0);
        assert_eq!(stats.total_playtime_secs, 0);
        assert_eq!(stats.most_played, None);
        assert!(db.top_played(10).unwrap().is_empty());
    }

    #[test]
    fn totals_and_top_played_order() {
        let db = Db::open_in_memory().unwrap();
        let a = add(&db, Source::Steam, "1", "A");
        let b = add(&db, Source::Steam, "2", "B");
        let c = add(&db, Source::Epic, "3", "C");
        let _never = add(&db, Source::Gog, "4", "Never played");
        play(&db, a, 1000, 100);
        play(&db, b, 2000, 300);
        play(&db, b, 3000, 300);
        play(&db, c, 4000, 250);
        db.set_installed(c, false).unwrap();

        let stats = db.library_stats().unwrap();
        assert_eq!(stats.game_count, 4);
        assert_eq!(stats.installed_count, 3);
        assert_eq!(stats.total_playtime_secs, 950);
        assert_eq!(stats.session_count, 4);
        assert_eq!(stats.most_played, Some(("B".to_string(), 600)));

        let top: Vec<String> = db
            .top_played(10)
            .unwrap()
            .into_iter()
            .map(|g| g.title)
            .collect();
        assert_eq!(top, ["B", "C", "A"]);
        assert_eq!(db.top_played(2).unwrap().len(), 2);

        assert_eq!(
            db.playtime_by_source().unwrap(),
            vec![(Source::Steam, 700), (Source::Epic, 250)]
        );
    }

    #[test]
    fn playtime_grouped_by_day() {
        let db = Db::open_in_memory().unwrap();
        let a = add(&db, Source::Steam, "1", "A");
        let day = |n: i64| n * SECS_PER_DAY;
        play(&db, a, day(100) + 10, 60);
        play(&db, a, day(100) + 5000, 40);
        play(&db, a, day(102) + 10, 500);
        play(&db, a, day(200), 999); // outside the range asked for

        let days = db.playtime_by_day(day(100), day(103)).unwrap();
        assert_eq!(
            days,
            vec![
                DayPlaytime {
                    day_start: day(100),
                    secs: 100
                },
                DayPlaytime {
                    day_start: day(102),
                    secs: 500
                },
            ]
        );
    }

    #[test]
    fn weekly_playtime_buckets() {
        let db = Db::open_in_memory().unwrap();
        let a = add(&db, Source::Steam, "1", "A");
        let b = add(&db, Source::Steam, "2", "B");
        let week = 7 * SECS_PER_DAY;
        let now = 100 * week;
        play(&db, a, now - 10, 5); // this week
        play(&db, a, now - week + 100, 30); // this week (just inside)
        play(&db, a, now - week - 100, 200); // last week
        play(&db, b, now - 2 * week - 100, 1000); // two weeks ago, other game
        play(&db, a, now - 9 * week, 7777); // too old for a 4-week chart

        assert_eq!(
            db.weekly_playtime(Some(a), now, 4).unwrap(),
            [0, 0, 200, 35]
        );
        assert_eq!(
            db.weekly_playtime(None, now, 4).unwrap(),
            [0, 1000, 200, 35]
        );
        assert_eq!(db.weekly_playtime(Some(b), now, 2).unwrap(), [0, 0]);
        assert_eq!(db.session_count(a).unwrap(), 4);
    }
}
