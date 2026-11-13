//! When and how you play, worked out from play sessions: by hour, by
//! weekday, by month, streaks, the longest session. Used by the expanded
//! stats page and by GameLib Wrapped, the look back at a year.
//!
//! Times are converted to local time with a fixed UTC offset (in seconds)
//! passed in by the caller, so the results don't depend on the machine
//! running the tests.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use chrono::{DateTime, Datelike, FixedOffset, TimeZone, Timelike};

use crate::db::Db;
use crate::model::Game;

/// A finished session: `(game_id, started_at, duration_secs)`.
pub type SessionRow = (i64, i64, u64);

fn local(at: i64, offset: i32) -> DateTime<FixedOffset> {
    let zone = FixedOffset::east_opt(offset).unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
    zone.timestamp_opt(at, 0)
        .single()
        .unwrap_or_else(|| zone.timestamp_opt(0, 0).unwrap())
}

/// Days since the epoch, in local time.
fn day_number(at: i64, offset: i32) -> i64 {
    (at + offset as i64).div_euclid(86_400)
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Patterns {
    pub total_secs: u64,
    pub sessions: usize,
    pub days_played: usize,
    /// Playtime by the hour sessions started, 0-23 local time.
    pub by_hour: [u64; 24],
    /// Playtime by weekday, Monday first.
    pub by_weekday: [u64; 7],
    /// Playtime by month, January first.
    pub by_month: [u64; 12],
    pub by_game: HashMap<i64, u64>,
    /// `(game_id, duration_secs, started_at)`.
    pub longest_session: Option<(i64, u64, i64)>,
    /// The most days in a row with some play.
    pub longest_streak: u32,
    /// Days in a row up to today (or yesterday) with some play.
    pub current_streak: u32,
}

impl Patterns {
    pub fn average_session_secs(&self) -> u64 {
        if self.sessions == 0 {
            0
        } else {
            self.total_secs / self.sessions as u64
        }
    }

    /// The games played most, most first.
    pub fn top_games(&self, limit: usize) -> Vec<(i64, u64)> {
        let mut games: Vec<(i64, u64)> = self.by_game.iter().map(|(g, s)| (*g, *s)).collect();
        games.sort_by_key(|(g, s)| (std::cmp::Reverse(*s), *g));
        games.truncate(limit);
        games
    }
}

pub fn patterns(sessions: &[SessionRow], now: i64, offset: i32) -> Patterns {
    let mut p = Patterns::default();
    let mut days: HashSet<i64> = HashSet::new();
    for &(game_id, at, secs) in sessions {
        p.total_secs += secs;
        p.sessions += 1;
        let when = local(at, offset);
        p.by_hour[when.hour() as usize] += secs;
        p.by_weekday[when.weekday().num_days_from_monday() as usize] += secs;
        p.by_month[when.month0() as usize] += secs;
        *p.by_game.entry(game_id).or_default() += secs;
        days.insert(day_number(at, offset));
        if p.longest_session
            .is_none_or(|(_, longest, _)| secs > longest)
        {
            p.longest_session = Some((game_id, secs, at));
        }
    }
    p.days_played = days.len();

    let mut sorted: Vec<i64> = days.iter().copied().collect();
    sorted.sort_unstable();
    let mut run = 0u32;
    let mut previous: Option<i64> = None;
    for day in &sorted {
        run = if previous == Some(day - 1) {
            run + 1
        } else {
            1
        };
        p.longest_streak = p.longest_streak.max(run);
        previous = Some(*day);
    }
    let today = day_number(now, offset);
    let mut day = if days.contains(&today) {
        today
    } else {
        today - 1
    };
    while days.contains(&day) {
        p.current_streak += 1;
        day -= 1;
    }
    p
}

/// Hours per genre, from playtime per game, most first. A game with two
/// genres counts towards both.
pub fn genre_hours(games: &[Game], by_game: &HashMap<i64, u64>) -> Vec<(String, u64)> {
    let mut genres: HashMap<String, u64> = HashMap::new();
    for game in games {
        let secs = by_game.get(&game.id).copied().unwrap_or(0);
        if secs == 0 {
            continue;
        }
        for genre in &game.genres {
            *genres.entry(genre.clone()).or_default() += secs;
        }
    }
    let mut out: Vec<(String, u64)> = genres.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}

pub const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
pub const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

// --- Wrapped -------------------------------------------------------------------

/// A year of play, looked back on.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Wrapped {
    pub year: i32,
    pub patterns: Patterns,
    /// `(game_id, secs)`, most first.
    pub top_games: Vec<(i64, u64)>,
    pub top_genre: Option<(String, u64)>,
    /// Different games played.
    pub games_played: usize,
    /// Games added to the library this year.
    pub games_added: usize,
    /// Games marked finished this year.
    pub games_finished: usize,
    /// The busiest month (0 = January) and its playtime.
    pub busiest_month: Option<(usize, u64)>,
    /// The busiest weekday (0 = Monday) and its playtime.
    pub busiest_weekday: Option<(usize, u64)>,
    /// The hour you most often played.
    pub peak_hour: Option<usize>,
    /// A one-word description of how you play.
    pub persona: &'static str,
}

/// The start of a year, as a Unix timestamp in local time.
pub fn year_start(year: i32, offset: i32) -> i64 {
    let zone = FixedOffset::east_opt(offset).unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
    zone.with_ymd_and_hms(year, 1, 1, 0, 0, 0)
        .single()
        .map(|t| t.timestamp())
        .unwrap_or(0)
}

pub fn year_of(at: i64, offset: i32) -> i32 {
    local(at, offset).year()
}

fn busiest<const N: usize>(values: &[u64; N]) -> Option<(usize, u64)> {
    values
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, v)| *v > 0)
        .max_by_key(|(i, v)| (*v, std::cmp::Reverse(*i)))
}

/// Look back at `year`. `sessions` may cover any range; only those that
/// started in the year count. `finished` is `(game_id, finished_at)`.
pub fn wrapped(
    year: i32,
    games: &[Game],
    sessions: &[SessionRow],
    finished: &[(i64, i64)],
    now: i64,
    offset: i32,
) -> Wrapped {
    let start = year_start(year, offset);
    let end = year_start(year + 1, offset);
    let in_year = |at: i64| at >= start && at < end;
    let rows: Vec<SessionRow> = sessions.iter().copied().filter(|s| in_year(s.1)).collect();
    let patterns = patterns(&rows, now.min(end - 1), offset);
    let top_games = patterns.top_games(5);
    let top_genre = genre_hours(games, &patterns.by_game).into_iter().next();
    let peak_hour = busiest(&patterns.by_hour).map(|(h, _)| h);
    let night = patterns.by_hour[22..]
        .iter()
        .chain(&patterns.by_hour[..4])
        .sum::<u64>();
    let weekend = patterns.by_weekday[5] + patterns.by_weekday[6];
    let total = patterns.total_secs.max(1);
    let persona = if patterns.sessions == 0 {
        "Newcomer"
    } else if night * 3 > total {
        "Night Owl"
    } else if weekend * 2 > total {
        "Weekend Warrior"
    } else if top_games.first().is_some_and(|(_, s)| s * 2 > total) {
        "Devoted"
    } else if patterns.by_game.len() >= 15 {
        "Explorer"
    } else if patterns.average_session_secs() >= 3 * 3600 {
        "Marathoner"
    } else {
        "Steady Player"
    };
    Wrapped {
        year,
        games_played: patterns.by_game.len(),
        games_added: games.iter().filter(|g| in_year(g.added_at)).count(),
        games_finished: finished.iter().filter(|(_, at)| in_year(*at)).count(),
        busiest_month: busiest(&patterns.by_month),
        busiest_weekday: busiest(&patterns.by_weekday),
        top_games,
        top_genre,
        peak_hour,
        persona,
        patterns,
    }
}

impl Db {
    /// When each game was marked finished (from the Stardust reward for it).
    pub fn finished_dates(&self) -> Result<Vec<(i64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT game_id, MIN(created_at) FROM stardust_ledger
             WHERE reason = 'finished' AND game_id IS NOT NULL GROUP BY game_id",
        )?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;
    // 2026-01-05 was a Monday.
    const MON: i64 = 1_767_571_200;

    #[test]
    fn hours_weekdays_and_the_longest_session() {
        let sessions = vec![
            (1, MON + 20 * 3600, 3600),
            (2, MON + DAY + 9 * 3600, 7200),
            (1, MON + 5 * DAY + 20 * 3600, 600),
        ];
        let p = patterns(&sessions, MON + 6 * DAY, 0);
        assert_eq!(p.total_secs, 11_400);
        assert_eq!(p.by_hour[20], 4200);
        assert_eq!(p.by_weekday[0], 3600);
        assert_eq!(p.by_weekday[5], 600, "Saturday");
        assert_eq!(p.longest_session, Some((2, 7200, MON + DAY + 9 * 3600)));
        assert_eq!(p.days_played, 3);
        assert_eq!(p.top_games(1), vec![(2, 7200)]);
    }

    #[test]
    fn the_offset_moves_sessions_into_local_time() {
        let p = patterns(&[(1, MON + 23 * 3600, 60)], MON, 2 * 3600);
        assert_eq!(p.by_hour[1], 60, "23:00 UTC is 01:00 at UTC+2");
        assert_eq!(p.by_weekday[1], 60, "…on Tuesday");
    }

    #[test]
    fn streaks() {
        let at = |d: i64| (1, MON + d * DAY + 3600, 60);
        let sessions = vec![at(0), at(1), at(2), at(5), at(6)];
        let p = patterns(&sessions, MON + 6 * DAY + 7200, 0);
        assert_eq!(p.longest_streak, 3);
        assert_eq!(p.current_streak, 2);
        // Played yesterday but not yet today still counts.
        let p = patterns(&sessions, MON + 7 * DAY + 7200, 0);
        assert_eq!(p.current_streak, 2);
        let p = patterns(&sessions, MON + 9 * DAY, 0);
        assert_eq!(p.current_streak, 0);
    }

    #[test]
    fn wrapped_only_counts_the_year() {
        let start = year_start(2026, 0);
        let sessions = vec![
            (1, start - 10, 99_999),
            (1, start + 22 * 3600, 3 * 3600),
            (2, start + 40 * DAY + 23 * 3600, 3600),
        ];
        let game = |id: i64, genres: &[&str]| Game {
            id,
            title: format!("G{id}"),
            source: crate::model::Source::Steam,
            source_id: id.to_string(),
            install_dir: None,
            launch: crate::model::LaunchTarget::Uri(String::new()),
            favourite: false,
            hidden: false,
            installed: true,
            playtime_secs: 0,
            last_played: None,
            added_at: start + 5,
            cover_path: None,
            icon_path: None,
            description: None,
            genres: genres.iter().map(|g| g.to_string()).collect(),
            release_year: None,
        };
        let games = vec![game(1, &["RPG"]), game(2, &["Puzzle"])];
        let w = wrapped(
            2026,
            &games,
            &sessions,
            &[(1, start + DAY)],
            start + 100 * DAY,
            0,
        );
        assert_eq!(w.patterns.total_secs, 4 * 3600);
        assert_eq!(w.top_games, vec![(1, 3 * 3600), (2, 3600)]);
        assert_eq!(w.top_genre, Some(("RPG".into(), 3 * 3600)));
        assert_eq!((w.games_added, w.games_finished, w.games_played), (2, 1, 2));
        assert_eq!(w.busiest_month, Some((0, 3 * 3600)));
        assert_eq!(w.persona, "Night Owl");
    }

    #[test]
    fn finished_dates_come_from_the_ledger() {
        let db = Db::open_in_memory().unwrap();
        let id = db
            .upsert_detected(&crate::model::DetectedGame::manual("A", "/g/a.exe"), 1)
            .unwrap()
            .game_id;
        db.conn
            .execute("UPDATE games SET playtime_secs = 99999 WHERE id = ?1", [id])
            .unwrap();
        db.finish_game(id, 500).unwrap();
        assert_eq!(db.finished_dates().unwrap(), vec![(id, 500)]);
    }
}
