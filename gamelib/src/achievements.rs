//! GameLib's own achievements, earned across your whole library: finish
//! ten games, play a four-hour session, play seven days in a row…
//!
//! They're worked out fresh from the library each time, so there's
//! nothing to store and nothing to get out of step.

use std::collections::{HashMap, HashSet};

use crate::extras::{GameExtras, Status};
use crate::model::Game;
use crate::patterns::{Patterns, SessionRow};

const HOUR: u64 = 3600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Achievement {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// How far along you are, and the goal.
    pub progress: u64,
    pub goal: u64,
}

impl Achievement {
    pub fn unlocked(&self) -> bool {
        self.progress >= self.goal
    }

    pub fn fraction(&self) -> f32 {
        (self.progress as f32 / self.goal.max(1) as f32).min(1.0)
    }
}

/// Every achievement, with your progress towards each. `patterns` covers
/// all your sessions (with local times).
pub fn achievements(
    games: &[Game],
    extras: &HashMap<i64, GameExtras>,
    sessions: &[SessionRow],
    patterns: &Patterns,
) -> Vec<Achievement> {
    let finished = extras
        .values()
        .filter(|e| e.status == Some(Status::Finished))
        .count() as u64;
    let rated = extras.values().filter(|e| e.rating.is_some()).count() as u64;
    let reviewed = extras.values().filter(|e| e.review.is_some()).count() as u64;
    let played_games = games.iter().filter(|g| g.playtime_secs > 0).count() as u64;
    let most_in_one = games.iter().map(|g| g.playtime_secs).max().unwrap_or(0) / HOUR;
    let total_hours = games.iter().map(|g| g.playtime_secs).sum::<u64>() / HOUR;
    let genres: HashSet<String> = games
        .iter()
        .filter(|g| g.playtime_secs >= HOUR)
        .flat_map(|g| g.genres.iter().map(|x| x.to_lowercase()))
        .collect();
    let sources: HashSet<_> = games.iter().map(|g| g.source).collect();
    let longest = patterns.longest_session.map(|s| s.1).unwrap_or(0);
    let late = patterns.by_hour[..5].iter().any(|s| *s > 0) as u64;
    let early = patterns.by_hour[5..8].iter().any(|s| *s > 0) as u64;
    let franchise_done = {
        // A franchise where every game you own is finished (at least 2).
        let mut by_name: HashMap<&str, (u32, u32)> = HashMap::new();
        for e in extras.values() {
            if let Some(name) = &e.franchise {
                let entry = by_name.entry(name).or_default();
                entry.0 += 1;
                entry.1 += (e.status == Some(Status::Finished)) as u32;
            }
        }
        by_name
            .values()
            .any(|(owned, done)| *owned >= 2 && owned == done) as u64
    };

    let a = |id, name, description, progress: u64, goal: u64| Achievement {
        id,
        name,
        description,
        progress: progress.min(goal),
        goal,
    };
    vec![
        a(
            "first_session",
            "First Steps",
            "Play a game through GameLib",
            sessions.len() as u64,
            1,
        ),
        a(
            "collector",
            "Collector",
            "Have 25 games in your library",
            games.len() as u64,
            25,
        ),
        a(
            "hoarder",
            "Hoarder",
            "Have 100 games in your library",
            games.len() as u64,
            100,
        ),
        a("the_end", "The End", "Finish a game", finished, 1),
        a(
            "completionist",
            "Completionist",
            "Finish 10 games",
            finished,
            10,
        ),
        a(
            "backlog_slayer",
            "Backlog Slayer",
            "Finish 25 games",
            finished,
            25,
        ),
        a(
            "sampler",
            "Sampler",
            "Play 10 different games",
            played_games,
            10,
        ),
        a(
            "explorer",
            "Genre Explorer",
            "Play 5 genres for an hour each",
            genres.len() as u64,
            5,
        ),
        a(
            "marathon",
            "Marathon",
            "Play for 4 hours in one go",
            longest / 60,
            240,
        ),
        a(
            "devoted",
            "Devoted",
            "Put 100 hours into one game",
            most_in_one,
            100,
        ),
        a(
            "centurion",
            "Centurion",
            "Play 100 hours in total",
            total_hours,
            100,
        ),
        a(
            "thousand",
            "Lifer",
            "Play 1,000 hours in total",
            total_hours,
            1000,
        ),
        a(
            "streak_7",
            "On a Roll",
            "Play on 7 days in a row",
            patterns.longest_streak as u64,
            7,
        ),
        a(
            "streak_30",
            "Unstoppable",
            "Play on 30 days in a row",
            patterns.longest_streak as u64,
            30,
        ),
        a(
            "night_owl",
            "Night Owl",
            "Play between midnight and 5am",
            late,
            1,
        ),
        a(
            "early_bird",
            "Early Bird",
            "Play between 5am and 8am",
            early,
            1,
        ),
        a("critic", "Critic", "Rate 10 games", rated, 10),
        a("reviewer", "Reviewer", "Write 3 reviews", reviewed, 3),
        a(
            "multi_launcher",
            "Launcher Juggler",
            "Own games on 3 launchers",
            sources.len() as u64,
            3,
        ),
        a(
            "series_complete",
            "Series Complete",
            "Finish a whole franchise",
            franchise_done,
            1,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::patterns;

    fn game(id: i64, hours: u64) -> Game {
        Game {
            id,
            title: format!("G{id}"),
            source: crate::model::Source::Steam,
            source_id: id.to_string(),
            install_dir: None,
            launch: crate::model::LaunchTarget::Uri(String::new()),
            favourite: false,
            hidden: false,
            installed: true,
            playtime_secs: hours * HOUR,
            last_played: None,
            added_at: 0,
            cover_path: None,
            icon_path: None,
            description: None,
            genres: vec!["RPG".into()],
            release_year: None,
        }
    }

    #[test]
    fn progress_is_counted_and_capped() {
        let games = vec![game(1, 120), game(2, 0)];
        let extras = HashMap::from([(
            1,
            GameExtras {
                status: Some(Status::Finished),
                ..GameExtras::default()
            },
        )]);
        let sessions = vec![(1, 2 * 3600, 5 * HOUR)];
        let p = patterns(&sessions, 3 * 86_400, 0);
        let all = achievements(&games, &extras, &sessions, &p);
        let get = |id: &str| all.iter().find(|a| a.id == id).unwrap().clone();
        assert!(get("first_session").unlocked());
        assert!(get("the_end").unlocked());
        assert_eq!(get("completionist").progress, 1);
        assert!(get("marathon").unlocked());
        assert!(get("devoted").unlocked());
        assert_eq!(get("devoted").progress, 100, "capped at the goal");
        assert!(get("night_owl").unlocked());
        assert!(!get("early_bird").unlocked());
        assert!((get("collector").fraction() - 0.08).abs() < 0.001);
        let ids: HashSet<&str> = all.iter().map(|a| a.id).collect();
        assert_eq!(ids.len(), all.len(), "ids are unique");
    }
}
