//! Backlog Intelligence: what you own versus what you've played, how long
//! things take, and what to play with the time you have.
//!
//! Everything here is worked out from the local library. How long a game
//! takes to beat comes from the number you entered on its page; without
//! one, a rough guess is made from its genres (and marked as a guess).
//! HowLongToBeat has no public API, so GameLib doesn't fetch from it.

use std::collections::{HashMap, HashSet};

use crate::extras::{GameExtras, Status};
use crate::insights::Recommendation;
use crate::model::Game;

const HOUR: u64 = 3600;
const DAY: i64 = 86_400;

// --- The summary ---------------------------------------------------------

/// "You own 146 games. You've played 38, completed 12, and haven't
/// launched 72."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    pub owned: usize,
    /// Played at least a little.
    pub played: usize,
    /// Marked as finished.
    pub completed: usize,
    /// Never started.
    pub never_launched: usize,
    pub playing: usize,
    /// Roughly how many hours it would take to beat everything you've
    /// started or not touched (not finished or abandoned).
    pub hours_left: u64,
}

impl Summary {
    /// The summary as one sentence.
    pub fn sentence(&self) -> String {
        format!(
            "You own {} {}. You've played {}, completed {}, and haven't launched {}.",
            self.owned,
            if self.owned == 1 { "game" } else { "games" },
            self.played,
            self.completed,
            self.never_launched
        )
    }
}

fn extra(extras: &HashMap<i64, GameExtras>, id: i64) -> GameExtras {
    extras.get(&id).cloned().unwrap_or_default()
}

fn never_launched(game: &Game) -> bool {
    game.playtime_secs == 0 && game.last_played.is_none()
}

pub fn summary(games: &[Game], extras: &HashMap<i64, GameExtras>) -> Summary {
    let mut summary = Summary::default();
    for game in games {
        let e = extra(extras, game.id);
        summary.owned += 1;
        if !never_launched(game) {
            summary.played += 1;
        } else {
            summary.never_launched += 1;
        }
        match e.status {
            Some(Status::Finished) => summary.completed += 1,
            Some(Status::Abandoned) => {}
            status => {
                if status == Some(Status::Playing) {
                    summary.playing += 1;
                }
                summary.hours_left += remaining_minutes(game, &e) as u64 / 60;
            }
        }
    }
    summary
}

// --- How long games take ---------------------------------------------------

/// How long a game takes to beat, and whether that's your number or a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Length {
    pub minutes: u32,
    pub guessed: bool,
}

/// Typical hours to beat, by genre. The longest matching genre wins.
const GENRE_HOURS: &[(&str, u32)] = &[
    ("massively multiplayer", 80),
    ("rpg", 45),
    ("role-playing", 45),
    ("strategy", 30),
    ("simulation", 25),
    ("open world", 35),
    ("survival", 25),
    ("adventure", 14),
    ("action", 12),
    ("shooter", 10),
    ("fps", 10),
    ("roguelike", 20),
    ("roguelite", 20),
    ("platformer", 9),
    ("racing", 10),
    ("sports", 12),
    ("fighting", 8),
    ("horror", 8),
    ("puzzle", 7),
    ("casual", 5),
    ("visual novel", 10),
    ("indie", 8),
];

/// How long this game takes: your number if you entered one, otherwise a
/// guess from its genres.
pub fn length(game: &Game, extras: &GameExtras) -> Length {
    if let Some(minutes) = extras.minutes_to_beat {
        return Length {
            minutes,
            guessed: false,
        };
    }
    let genres: Vec<String> = game.genres.iter().map(|g| g.to_lowercase()).collect();
    let hours = GENRE_HOURS
        .iter()
        .filter(|(genre, _)| genres.iter().any(|g| g.contains(genre)))
        .map(|(_, hours)| *hours)
        .max()
        .unwrap_or(15);
    Length {
        minutes: hours * 60,
        guessed: true,
    }
}

/// Roughly how many minutes are left before it's beaten. Finished games
/// have none left.
pub fn remaining_minutes(game: &Game, extras: &GameExtras) -> u32 {
    if extras.status == Some(Status::Finished) {
        return 0;
    }
    let total = length(game, extras).minutes;
    let played = (game.playtime_secs / 60) as u32;
    if played >= total {
        // Played longer than expected but not finished: say there's a
        // little left rather than none.
        (total / 10).max(30)
    } else {
        total - played
    }
}

/// Played well past a length that was only guessed: the guess was wrong
/// (or the game never really ends), so how much is left isn't known.
pub fn outgrew_guess(game: &Game, extras: &GameExtras) -> bool {
    let len = length(game, extras);
    len.guessed && game.playtime_secs / 60 >= len.minutes as u64
}

/// How far through the game you are, 0.0 to 1.0. Finished is 1.0.
pub fn progress(game: &Game, extras: &GameExtras) -> f32 {
    if extras.status == Some(Status::Finished) {
        return 1.0;
    }
    let total = length(game, extras).minutes.max(1) as f32;
    ((game.playtime_secs as f32 / 60.0) / total).min(0.95)
}

/// "2 h", "45 min", "1.5 h".
pub fn format_minutes(minutes: u32) -> String {
    if minutes < 60 {
        format!("{minutes} min")
    } else if minutes.is_multiple_of(60) || minutes >= 600 {
        format!("{} h", (minutes + 30) / 60)
    } else {
        format!("{:.1} h", minutes as f32 / 60.0)
    }
}

// --- Habits ----------------------------------------------------------------

/// How you tend to play, learned from your sessions.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Habits {
    pub sessions: usize,
    /// Typical (median) session length, in minutes.
    pub typical_session_minutes: u32,
    /// Sessions per week over the last eight weeks.
    pub sessions_per_week: f32,
    /// The hour of the day (0-23, UTC) you most often start playing.
    pub favourite_hour: Option<u32>,
}

/// Work out your habits from `(game_id, started_at, duration_secs)` rows.
pub fn habits(sessions: &[(i64, i64, u64)], now: i64) -> Habits {
    if sessions.is_empty() {
        return Habits::default();
    }
    let mut lengths: Vec<u64> = sessions.iter().map(|s| s.2).collect();
    lengths.sort_unstable();
    let median = lengths[lengths.len() / 2];
    let recent = sessions
        .iter()
        .filter(|s| s.1 > now - 56 * DAY && s.1 <= now)
        .count();
    let mut by_hour = [0usize; 24];
    for session in sessions {
        by_hour[(session.1.rem_euclid(DAY) / 3600) as usize] += 1;
    }
    let favourite_hour = (0..24).max_by_key(|h| (by_hour[*h], std::cmp::Reverse(*h)));
    Habits {
        sessions: sessions.len(),
        typical_session_minutes: (median / 60) as u32,
        sessions_per_week: recent as f32 / 8.0,
        favourite_hour: favourite_hour.map(|h| h as u32),
    }
}

// --- What to play tonight --------------------------------------------------

/// What you're in the mood for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Goal {
    /// Whatever suits the time best.
    #[default]
    Anything,
    /// Get something finished.
    Finish,
    /// Start something you haven't played.
    New,
}

impl Goal {
    pub const ALL: [Goal; 3] = [Goal::Anything, Goal::Finish, Goal::New];

    pub fn label(self) -> &'static str {
        match self {
            Goal::Anything => "Anything",
            Goal::Finish => "Finish something",
            Goal::New => "Try something new",
        }
    }
}

/// A suggestion for the time you have.
#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub game_id: i64,
    pub score: f32,
    pub reason: String,
}

/// Suggest games for a sitting of `free_minutes`.
///
/// It weighs how well a game matches your tastes (from
/// [`crate::insights::recommend`]), whether you could finish it or make
/// real progress in the time, how long your sessions usually are, and how
/// big a commitment the game is.
pub fn tonight(
    games: &[Game],
    extras: &HashMap<i64, GameExtras>,
    taste: &[Recommendation],
    habits: &Habits,
    free_minutes: u32,
    goal: Goal,
    limit: usize,
) -> Vec<Pick> {
    let free = free_minutes.max(15);
    let taste: HashMap<i64, f32> = taste.iter().map(|r| (r.game_id, r.score)).collect();
    let mut picks: Vec<Pick> = games
        .iter()
        .filter(|g| g.installed && !g.hidden)
        .filter_map(|game| {
            let e = extra(extras, game.id);
            if matches!(e.status, Some(Status::Finished | Status::Abandoned)) {
                return None;
            }
            let fresh = never_launched(game);
            if goal == Goal::New && !fresh {
                return None;
            }
            let left = remaining_minutes(game, &e);
            let len = length(game, &e);
            let mut score = taste.get(&game.id).copied().unwrap_or(0.0).max(0.0);
            let reason;
            if !fresh && outgrew_guess(game, &e) {
                let played = format_minutes((game.playtime_secs / 60) as u32);
                if e.status == Some(Status::Playing) {
                    score += 0.8;
                    reason = format!("You're playing this: {played} in so far");
                } else {
                    score += 0.1;
                    reason = format!("An old favourite: {played} played so far");
                }
            } else if !fresh && left <= free {
                score += 1.5;
                reason = format!(
                    "You could finish it tonight: about {} left",
                    format_minutes(left)
                );
            } else if fresh && len.minutes <= free {
                score += 1.0;
                reason = format!(
                    "A short one: beat it in about {}",
                    format_minutes(len.minutes)
                );
            } else if e.status == Some(Status::Playing) {
                score += 0.8;
                let share = (free as f32 / left as f32 * 100.0).round().min(99.0);
                reason = format!(
                    "You're playing this. Tonight gets you about {share}% closer to the end"
                );
            } else if fresh {
                score += 0.2;
                reason = if len.minutes > free * 8 {
                    format!(
                        "A big one (about {}): a good night to make a start",
                        format_minutes(len.minutes)
                    )
                } else {
                    "You haven't tried this yet".to_string()
                };
            } else {
                score += 0.3;
                reason = format!("Pick it back up: about {} left", format_minutes(left));
            }
            if goal == Goal::Finish {
                // Closest to the end first.
                if fresh {
                    return None;
                }
                score += 2.0 * progress(game, &e);
            }
            // Big games suit long evenings; short evenings suit games you
            // can make sense of in one sitting.
            let commitment = len.minutes as f32 / free as f32;
            if commitment > 20.0 && free < 90 {
                score -= 0.4;
            }
            // Sittings much shorter than you usually play suit games you
            // already know.
            if habits.typical_session_minutes > 0
                && free * 2 < habits.typical_session_minutes
                && fresh
            {
                score -= 0.2;
            }
            if e.status == Some(Status::OnHold) {
                score -= 0.2;
            }
            if let Some(stars) = e.rating {
                score += (stars as f32 - 3.0) * 0.1;
            }
            Some(Pick {
                game_id: game.id,
                score,
                reason,
            })
        })
        .collect();
    picks.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.game_id.cmp(&b.game_id)));
    picks.truncate(limit);
    picks
}

// --- Other questions about the backlog --------------------------------------

/// Games you have neglected: bought and never launched (oldest first), then
/// games you started and haven't touched in 30 days or more.
pub fn neglected(
    games: &[Game],
    extras: &HashMap<i64, GameExtras>,
    now: i64,
    limit: usize,
) -> Vec<(i64, String)> {
    let mut untouched: Vec<&Game> = games
        .iter()
        .filter(|g| !g.hidden && never_launched(g))
        .collect();
    untouched.sort_by_key(|g| (g.added_at, g.id));
    let mut out: Vec<(i64, String)> = untouched
        .into_iter()
        .map(|g| {
            let days = (now - g.added_at).max(0) / DAY;
            let why = if days >= 1 {
                format!("Never launched, owned for {days} days")
            } else {
                "Never launched".to_string()
            };
            (g.id, why)
        })
        .collect();

    let mut stale: Vec<(&Game, i64)> = games
        .iter()
        .filter(|g| !g.hidden)
        .filter(|g| {
            !matches!(
                extra(extras, g.id).status,
                Some(Status::Finished | Status::Abandoned)
            )
        })
        .filter_map(|g| g.last_played.map(|at| (g, (now - at) / DAY)))
        .filter(|(_, days)| *days >= 30)
        .collect();
    stale.sort_by_key(|(g, days)| (std::cmp::Reverse(*days), g.id));
    out.extend(stale.into_iter().map(|(g, days)| {
        (
            g.id,
            format!(
                "Not played for {days} days, {} in",
                format_minutes((g.playtime_secs / 60) as u32)
            ),
        )
    }));
    out.truncate(limit);
    out
}

/// What to finish next: games you're playing or have on hold, the ones
/// with the least left first.
pub fn complete_next(
    games: &[Game],
    extras: &HashMap<i64, GameExtras>,
    limit: usize,
) -> Vec<(i64, u32)> {
    let mut started: Vec<(i64, u32)> = games
        .iter()
        .filter(|g| !g.hidden && !never_launched(g))
        .filter(|g| {
            let status = extra(extras, g.id).status;
            match status {
                Some(Status::Playing | Status::OnHold) => true,
                // Not marked, but played for a while and not past its
                // guessed length (a game you put 300 hours into without
                // finishing probably doesn't have an end).
                None => g.playtime_secs >= HOUR && !outgrew_guess(g, &extra(extras, g.id)),
                _ => false,
            }
        })
        .map(|g| (g.id, remaining_minutes(g, &extra(extras, g.id))))
        .collect();
    started.sort_by_key(|(id, left)| (*left, *id));
    started.truncate(limit);
    started
}

/// Games ordered by how far through them you are (finished first).
pub fn by_progress(
    games: &[Game],
    extras: &HashMap<i64, GameExtras>,
    limit: usize,
) -> Vec<(i64, f32)> {
    let mut all: Vec<(i64, f32)> = games
        .iter()
        .filter(|g| !g.hidden && !never_launched(g))
        .map(|g| (g.id, progress(g, &extra(extras, g.id))))
        .collect();
    all.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    all.truncate(limit);
    all
}

/// Games most like `target`, by shared genres, tags and franchise.
pub fn similar_to(
    target: &Game,
    games: &[Game],
    extras: &HashMap<i64, GameExtras>,
    tags: &HashMap<i64, Vec<String>>,
    limit: usize,
) -> Vec<(i64, f32)> {
    let labels = |game: &Game| -> HashSet<String> {
        let mut set: HashSet<String> = game.genres.iter().map(|g| g.to_lowercase()).collect();
        set.extend(
            tags.get(&game.id)
                .into_iter()
                .flatten()
                .map(|t| t.to_lowercase()),
        );
        set
    };
    let mine = labels(target);
    let my_franchise = extra(extras, target.id).franchise;
    let (my_series, _) = crate::insights::series_of(&target.title);
    let mut scored: Vec<(i64, f32)> = games
        .iter()
        .filter(|g| g.id != target.id && !g.hidden)
        .filter_map(|game| {
            let theirs = labels(game);
            let shared = mine.intersection(&theirs).count() as f32;
            let total = mine.union(&theirs).count().max(1) as f32;
            let mut score = shared / total;
            let franchise = extra(extras, game.id).franchise;
            if my_franchise.is_some() && franchise == my_franchise {
                score += 1.0;
            } else if crate::insights::series_of(&game.title).0 == my_series {
                score += 0.8;
            }
            (score > 0.0).then_some((game.id, score))
        })
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.truncate(limit);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LaunchTarget, Source};

    fn game(id: i64, title: &str, hours: u64, genres: &[&str]) -> Game {
        Game {
            id,
            title: title.into(),
            source: Source::Steam,
            source_id: id.to_string(),
            install_dir: None,
            launch: LaunchTarget::Uri(format!("steam://rungameid/{id}")),
            favourite: false,
            hidden: false,
            installed: true,
            playtime_secs: hours * HOUR,
            last_played: (hours > 0).then_some(1_000_000),
            added_at: 0,
            cover_path: None,
            icon_path: None,
            description: None,
            genres: genres.iter().map(|g| g.to_string()).collect(),
            release_year: None,
        }
    }

    fn with_status(status: Status) -> GameExtras {
        GameExtras {
            status: Some(status),
            ..GameExtras::default()
        }
    }

    #[test]
    fn the_summary_counts_played_completed_and_untouched() {
        let games = vec![
            game(1, "A", 10, &["RPG"]),
            game(2, "B", 3, &["Puzzle"]),
            game(3, "C", 0, &["Action"]),
            game(4, "D", 0, &[]),
        ];
        let extras = HashMap::from([(2, with_status(Status::Finished))]);
        let s = summary(&games, &extras);
        assert_eq!(
            (s.owned, s.played, s.completed, s.never_launched),
            (4, 2, 1, 2)
        );
        assert_eq!(
            s.sentence(),
            "You own 4 games. You've played 2, completed 1, and haven't launched 2."
        );
        // RPG 45 h - 10 played, + Action 12 h + unknown 15 h.
        assert_eq!(s.hours_left, 35 + 12 + 15);
    }

    #[test]
    fn your_own_length_beats_the_genre_guess() {
        let g = game(1, "A", 0, &["Indie", "RPG"]);
        assert_eq!(
            length(&g, &GameExtras::default()),
            Length {
                minutes: 45 * 60,
                guessed: true
            }
        );
        let mine = GameExtras {
            minutes_to_beat: Some(90),
            ..GameExtras::default()
        };
        assert_eq!(length(&g, &mine).minutes, 90);
        assert!(!length(&g, &mine).guessed);
    }

    #[test]
    fn remaining_time_never_goes_below_a_little() {
        let g = game(1, "A", 20, &["Puzzle"]); // 7 h game, 20 h played
        assert_eq!(remaining_minutes(&g, &GameExtras::default()), 42);
        assert_eq!(remaining_minutes(&g, &with_status(Status::Finished)), 0);
        assert!(progress(&g, &GameExtras::default()) < 1.0);
        assert_eq!(progress(&g, &with_status(Status::Finished)), 1.0);
    }

    #[test]
    fn tonight_prefers_what_you_can_finish_in_the_time() {
        let mut nearly = game(1, "Nearly Done", 6, &["Puzzle"]); // 1 h left
        nearly.last_played = Some(100);
        let games = vec![
            nearly,
            game(2, "Huge RPG", 0, &["RPG"]),
            game(3, "Short", 0, &["Casual"]), // 5 h
        ];
        let extras = HashMap::from([(1, with_status(Status::Playing))]);
        let picks = tonight(
            &games,
            &extras,
            &[],
            &Habits::default(),
            90,
            Goal::Anything,
            3,
        );
        assert_eq!(picks[0].game_id, 1);
        assert!(
            picks[0].reason.contains("finish it tonight"),
            "{}",
            picks[0].reason
        );

        let new_only = tonight(&games, &extras, &[], &Habits::default(), 90, Goal::New, 3);
        assert!(new_only.iter().all(|p| p.game_id != 1));

        let finish = tonight(
            &games,
            &extras,
            &[],
            &Habits::default(),
            90,
            Goal::Finish,
            3,
        );
        assert_eq!(finish.len(), 1);
    }

    #[test]
    fn neglected_lists_untouched_then_stale_games() {
        let day = DAY;
        let mut old = game(1, "Old", 4, &[]);
        old.last_played = Some(0);
        let mut fresh = game(2, "Fresh", 4, &[]);
        fresh.last_played = Some(100 * day);
        let mut never = game(3, "Never", 0, &[]);
        never.added_at = 10 * day;
        let found = neglected(&[old, fresh, never], &HashMap::new(), 100 * day, 10);
        assert_eq!(found.iter().map(|f| f.0).collect::<Vec<_>>(), vec![3, 1]);
        assert!(found[0].1.contains("90 days"));
    }

    #[test]
    fn complete_next_puts_the_closest_to_done_first() {
        let games = vec![
            game(1, "Long", 2, &["RPG"]),
            game(2, "Short", 2, &["Puzzle"]),
        ];
        let extras = HashMap::from([
            (1, with_status(Status::Playing)),
            (2, with_status(Status::Playing)),
        ]);
        let next = complete_next(&games, &extras, 5);
        assert_eq!(next[0], (2, 5 * 60));
    }

    #[test]
    fn similar_games_share_genres_or_series() {
        let games = vec![
            game(1, "Elden Ring", 50, &["Action", "RPG"]),
            game(2, "Dark Souls III", 0, &["Action", "RPG"]),
            game(3, "Tetris", 0, &["Puzzle"]),
            game(4, "Elden Ring Nightreign", 0, &["Action"]),
        ];
        let found = similar_to(&games[0], &games, &HashMap::new(), &HashMap::new(), 5);
        let ids: Vec<i64> = found.iter().map(|f| f.0).collect();
        assert_eq!(ids, vec![2, 4]);
    }

    #[test]
    fn habits_come_from_sessions() {
        let sessions = vec![
            (1, 20 * 3600, 3600),
            (1, 20 * 3600 + DAY, 7200),
            (2, 9 * 3600, 600),
        ];
        let h = habits(&sessions, 2 * DAY);
        assert_eq!(h.sessions, 3);
        assert_eq!(h.typical_session_minutes, 60);
        assert_eq!(h.favourite_hour, Some(20));
    }

    #[test]
    fn minutes_read_nicely() {
        assert_eq!(format_minutes(45), "45 min");
        assert_eq!(format_minutes(120), "2 h");
        assert_eq!(format_minutes(90), "1.5 h");
        assert_eq!(format_minutes(45 * 60 + 10), "45 h");
    }
}
