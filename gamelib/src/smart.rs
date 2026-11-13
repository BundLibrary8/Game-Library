//! Smart collections: saved filters that keep themselves up to date.
//! "Short and unplayed", "Rated 4+ and unfinished", "RPGs over 10 hours".
//! Each is a set of rules; a game is in the collection while it matches
//! all of them. They're kept as JSON in the settings table.

use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::backlog;
use crate::db::Db;
use crate::extras::{GameExtras, Status};
use crate::model::{Game, Source};

const KEY: &str = "smart_collections";

/// Whether a game has been played.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Played {
    #[default]
    Any,
    Never,
    Played,
}

/// The rules. Anything left empty or `None` matches every game.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Rules {
    /// Source names ("steam", "epic"…). Empty means any source.
    pub sources: Vec<String>,
    /// Status names ("backlog", "playing"…), and "none" for games with no
    /// status. Empty means any status.
    pub statuses: Vec<String>,
    /// A genre the game must have (part of the name is enough).
    pub genre: String,
    /// A tag or collection the game must be in.
    pub tag: String,
    /// Words the title must contain.
    pub title: String,
    pub played: Played,
    pub min_hours: Option<f32>,
    pub max_hours: Option<f32>,
    /// At most this long to beat (your number, or the genre guess).
    pub max_length_hours: Option<f32>,
    pub min_rating: Option<u8>,
    /// Not played in at least this many days (never-played games count).
    pub idle_days: Option<u32>,
    pub installed_only: bool,
    pub favourites_only: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartCollection {
    pub name: String,
    pub rules: Rules,
}

impl Rules {
    /// Does this game match every rule?
    pub fn matches(&self, game: &Game, extras: &GameExtras, tags: &[String], now: i64) -> bool {
        let hours = game.playtime_secs as f32 / 3600.0;
        let contains = |haystack: &str, needle: &str| {
            haystack
                .to_lowercase()
                .contains(&needle.trim().to_lowercase())
        };
        if !self.sources.is_empty() && !self.sources.iter().any(|s| s == game.source.as_str()) {
            return false;
        }
        if !self.statuses.is_empty() {
            let status = extras.status.map(Status::as_str).unwrap_or("none");
            if !self.statuses.iter().any(|s| s == status) {
                return false;
            }
        }
        if !self.genre.trim().is_empty() && !game.genres.iter().any(|g| contains(g, &self.genre)) {
            return false;
        }
        if !self.tag.trim().is_empty()
            && !tags.iter().any(|t| t.eq_ignore_ascii_case(self.tag.trim()))
        {
            return false;
        }
        if !self.title.trim().is_empty() && !contains(&game.title, &self.title) {
            return false;
        }
        match self.played {
            Played::Any => {}
            Played::Never if game.playtime_secs > 0 => return false,
            Played::Played if game.playtime_secs == 0 => return false,
            _ => {}
        }
        if self.min_hours.is_some_and(|min| hours < min) {
            return false;
        }
        if self.max_hours.is_some_and(|max| hours > max) {
            return false;
        }
        if let Some(max) = self.max_length_hours
            && backlog::length(game, extras).minutes as f32 > max * 60.0
        {
            return false;
        }
        if let Some(min) = self.min_rating
            && extras.rating.is_none_or(|r| r < min)
        {
            return false;
        }
        if let Some(days) = self.idle_days
            && game
                .last_played
                .is_some_and(|at| now - at < days as i64 * 86_400)
        {
            return false;
        }
        if self.installed_only && !game.installed {
            return false;
        }
        if self.favourites_only && !game.favourite {
            return false;
        }
        true
    }

    /// The rules in a few words, for a tooltip.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if !self.sources.is_empty() {
            let names: Vec<String> = self
                .sources
                .iter()
                .filter_map(|s| Source::parse(s).ok())
                .map(|s| format!("{s:?}"))
                .collect();
            parts.push(format!("from {}", names.join(" or ")));
        }
        if !self.statuses.is_empty() {
            let names: Vec<&str> = self
                .statuses
                .iter()
                .map(|s| Status::parse(s).map(Status::label).unwrap_or("no status"))
                .collect();
            parts.push(names.join(" or "));
        }
        if !self.genre.trim().is_empty() {
            parts.push(format!("genre {}", self.genre.trim()));
        }
        if !self.tag.trim().is_empty() {
            parts.push(format!("tagged {}", self.tag.trim()));
        }
        if !self.title.trim().is_empty() {
            parts.push(format!("title has \"{}\"", self.title.trim()));
        }
        match self.played {
            Played::Never => parts.push("never played".into()),
            Played::Played => parts.push("played".into()),
            Played::Any => {}
        }
        if let Some(h) = self.min_hours {
            parts.push(format!("at least {h} h played"));
        }
        if let Some(h) = self.max_hours {
            parts.push(format!("at most {h} h played"));
        }
        if let Some(h) = self.max_length_hours {
            parts.push(format!("beatable in {h} h"));
        }
        if let Some(r) = self.min_rating {
            parts.push(format!("rated {r}+"));
        }
        if let Some(d) = self.idle_days {
            parts.push(format!("not played for {d} days"));
        }
        if self.installed_only {
            parts.push("installed".into());
        }
        if self.favourites_only {
            parts.push("favourites".into());
        }
        if parts.is_empty() {
            "Every game".into()
        } else {
            parts.join(", ")
        }
    }
}

/// Ready-made smart collections to start from.
pub fn presets() -> Vec<SmartCollection> {
    vec![
        SmartCollection {
            name: "Short and unplayed".into(),
            rules: Rules {
                played: Played::Never,
                max_length_hours: Some(8.0),
                installed_only: true,
                ..Rules::default()
            },
        },
        SmartCollection {
            name: "Loved but unfinished".into(),
            rules: Rules {
                min_rating: Some(4),
                statuses: vec!["none".into(), "playing".into(), "on_hold".into()],
                ..Rules::default()
            },
        },
        SmartCollection {
            name: "Gathering dust".into(),
            rules: Rules {
                played: Played::Played,
                idle_days: Some(90),
                statuses: vec!["none".into(), "playing".into(), "on_hold".into()],
                ..Rules::default()
            },
        },
        SmartCollection {
            name: "Big time sinks".into(),
            rules: Rules {
                min_hours: Some(50.0),
                ..Rules::default()
            },
        },
    ]
}

/// Which games are in a smart collection.
pub fn filter<'g>(
    rules: &Rules,
    games: &'g [Game],
    extras: &HashMap<i64, GameExtras>,
    tags: &HashMap<i64, Vec<String>>,
    now: i64,
) -> Vec<&'g Game> {
    let none = Vec::new();
    games
        .iter()
        .filter(|g| {
            let e = extras.get(&g.id).cloned().unwrap_or_default();
            rules.matches(g, &e, tags.get(&g.id).unwrap_or(&none), now)
        })
        .collect()
}

impl Db {
    pub fn smart_collections(&self) -> Result<Vec<SmartCollection>> {
        Ok(match self.get_setting(KEY)? {
            Some(json) => serde_json::from_str(&json).unwrap_or_else(|err| {
                log::warn!("couldn't read smart collections: {err}");
                Vec::new()
            }),
            None => Vec::new(),
        })
    }

    pub fn set_smart_collections(&self, collections: &[SmartCollection]) -> Result<()> {
        self.set_setting(KEY, &serde_json::to_string(collections)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LaunchTarget;

    fn game(id: i64, hours: u64, genres: &[&str]) -> Game {
        Game {
            id,
            title: format!("Game {id}"),
            source: Source::Steam,
            source_id: id.to_string(),
            install_dir: None,
            launch: LaunchTarget::Uri(String::new()),
            favourite: false,
            hidden: false,
            installed: true,
            playtime_secs: hours * 3600,
            last_played: (hours > 0).then_some(0),
            added_at: 0,
            cover_path: None,
            icon_path: None,
            description: None,
            genres: genres.iter().map(|g| g.to_string()).collect(),
            release_year: None,
        }
    }

    #[test]
    fn empty_rules_match_everything() {
        assert!(Rules::default().matches(&game(1, 0, &[]), &GameExtras::default(), &[], 0));
        assert_eq!(Rules::default().describe(), "Every game");
    }

    #[test]
    fn rules_combine() {
        let rules = Rules {
            genre: "rpg".into(),
            played: Played::Played,
            min_hours: Some(5.0),
            ..Rules::default()
        };
        let extras = GameExtras::default();
        assert!(rules.matches(&game(1, 10, &["Action RPG"]), &extras, &[], 0));
        assert!(!rules.matches(&game(2, 2, &["RPG"]), &extras, &[], 0));
        assert!(!rules.matches(&game(3, 10, &["Puzzle"]), &extras, &[], 0));
    }

    #[test]
    fn status_none_and_ratings() {
        let rules = Rules {
            statuses: vec!["none".into()],
            min_rating: Some(4),
            ..Rules::default()
        };
        let rated = |rating, status| GameExtras {
            rating,
            status,
            ..GameExtras::default()
        };
        let g = game(1, 1, &[]);
        assert!(rules.matches(&g, &rated(Some(5), None), &[], 0));
        assert!(!rules.matches(&g, &rated(Some(3), None), &[], 0));
        assert!(!rules.matches(&g, &rated(None, None), &[], 0));
        assert!(!rules.matches(&g, &rated(Some(5), Some(Status::Finished)), &[], 0));
    }

    #[test]
    fn idle_days_and_tags() {
        let rules = Rules {
            idle_days: Some(30),
            tag: "co-op".into(),
            ..Rules::default()
        };
        let mut g = game(1, 3, &[]);
        g.last_played = Some(0);
        let tags = vec!["Co-op".to_string()];
        assert!(rules.matches(&g, &GameExtras::default(), &tags, 40 * 86_400));
        assert!(!rules.matches(&g, &GameExtras::default(), &tags, 10 * 86_400));
        assert!(!rules.matches(&g, &GameExtras::default(), &[], 40 * 86_400));
    }

    #[test]
    fn collections_are_saved() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.smart_collections().unwrap().is_empty());
        db.set_smart_collections(&presets()).unwrap();
        assert_eq!(db.smart_collections().unwrap(), presets());
    }
}
