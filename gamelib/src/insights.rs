//! Working things out from your library: which games belong to the same
//! franchise, and what to play next.
//!
//! Everything here is plain calculation on data already loaded, so it is
//! easy to test and needs no internet.

use std::collections::HashMap;

use crate::extras::{GameExtras, Status};
use crate::model::Game;

// --- Franchises ----------------------------------------------------------

fn is_roman_numeral(word: &str) -> bool {
    matches!(
        word.to_ascii_uppercase().as_str(),
        "II" | "III"
            | "IV"
            | "V"
            | "VI"
            | "VII"
            | "VIII"
            | "IX"
            | "X"
            | "XI"
            | "XII"
            | "XIII"
            | "XIV"
            | "XV"
            | "XVI"
    )
}

fn roman_value(word: &str) -> Option<i64> {
    const NUMERALS: [&str; 15] = [
        "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII", "XIII", "XIV", "XV",
        "XVI",
    ];
    let upper = word.to_ascii_uppercase();
    NUMERALS
        .iter()
        .position(|n| *n == upper)
        .map(|i| i as i64 + 2)
}

/// Words that often follow a series name without being part of it.
const EDITION_WORDS: [&str; 10] = [
    "remastered",
    "definitive",
    "edition",
    "goty",
    "enhanced",
    "complete",
    "hd",
    "remake",
    "deluxe",
    "ultimate",
];

/// The series name a title most likely belongs to, and which entry it is
/// if the title says. "Borderlands 2" → ("Borderlands", Some(2));
/// "Borderlands: The Pre-Sequel" → ("Borderlands", None).
pub fn series_of(title: &str) -> (String, Option<i64>) {
    // Anything after a colon or a spaced dash is a subtitle.
    let main = title
        .split(':')
        .next()
        .unwrap_or(title)
        .split(" - ")
        .next()
        .unwrap_or(title);
    let mut words: Vec<&str> = main.split_whitespace().collect();
    let mut number = None;
    // Peel sequel numbers and edition words off the end.
    while words.len() > 1 {
        let last = words[words.len() - 1].trim_matches(|c: char| !c.is_alphanumeric() && c != '\'');
        if let Some(n) = last
            .strip_prefix('\'')
            .and_then(|year| year.parse::<i64>().ok())
        {
            number.get_or_insert(n); // "Rally '98"
        } else if let Ok(n) = last.parse::<i64>() {
            number.get_or_insert(n);
        } else if is_roman_numeral(last) {
            number = number.or(roman_value(last));
        } else if !EDITION_WORDS.contains(&last.to_lowercase().as_str()) {
            break;
        }
        words.pop();
    }
    (words.join(" "), number)
}

/// A group of games that look like one series.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FranchiseGuess {
    pub name: String,
    /// Game ids with their guessed place in the order, in playing order.
    pub games: Vec<(i64, Option<i64>)>,
}

/// Find games whose titles look like the same series. Only groups of two
/// or more are returned. Games already in a franchise are left alone.
pub fn guess_franchises(games: &[Game], extras: &HashMap<i64, GameExtras>) -> Vec<FranchiseGuess> {
    let mut groups: HashMap<String, FranchiseGuess> = HashMap::new();
    for game in games {
        if extras.get(&game.id).is_some_and(|e| e.franchise.is_some()) {
            continue;
        }
        let (name, number) = series_of(&game.title);
        if name.chars().count() < 3 {
            continue; // too short to mean anything
        }
        groups
            .entry(name.to_lowercase())
            .or_insert_with(|| FranchiseGuess {
                name,
                games: Vec::new(),
            })
            .games
            .push((game.id, number));
    }
    let mut guesses: Vec<FranchiseGuess> = groups
        .into_values()
        .filter(|g| g.games.len() >= 2)
        .collect();
    for guess in &mut guesses {
        // A first game usually has no number: treat it as 1.
        guess
            .games
            .sort_by_key(|(id, number)| (number.unwrap_or(1), *id));
    }
    guesses.sort_by_key(|a| a.name.to_lowercase());
    guesses
}

/// A franchise as recorded, with how far through it you are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Franchise {
    pub name: String,
    /// Game ids in playing order.
    pub games: Vec<i64>,
    pub finished: usize,
}

/// The franchises games have been put in, each in playing order.
pub fn franchises(games: &[Game], extras: &HashMap<i64, GameExtras>) -> Vec<Franchise> {
    let mut by_name: HashMap<String, Vec<(i64, i64, &Game)>> = HashMap::new();
    for game in games {
        let Some(extra) = extras.get(&game.id) else {
            continue;
        };
        let Some(name) = &extra.franchise else {
            continue;
        };
        // Games with no set order go by release year, then title.
        let order = extra.franchise_order.unwrap_or(i64::MAX);
        by_name
            .entry(name.clone())
            .or_default()
            .push((order, game.id, game));
    }
    let mut list: Vec<Franchise> = by_name
        .into_iter()
        .map(|(name, mut entries)| {
            entries.sort_by(|a, b| {
                (a.0, a.2.release_year.unwrap_or(u16::MAX), &a.2.title).cmp(&(
                    b.0,
                    b.2.release_year.unwrap_or(u16::MAX),
                    &b.2.title,
                ))
            });
            let finished = entries
                .iter()
                .filter(|(_, id, _)| {
                    extras
                        .get(id)
                        .is_some_and(|e| e.status == Some(Status::Finished))
                })
                .count();
            Franchise {
                name,
                games: entries.into_iter().map(|(_, id, _)| id).collect(),
                finished,
            }
        })
        .collect();
    list.sort_by_key(|a| a.name.to_lowercase());
    list
}

// --- Recommendations -----------------------------------------------------

/// A game worth playing next, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Recommendation {
    pub game_id: i64,
    pub score: f32,
    pub reason: String,
}

const HOUR: f32 = 3600.0;

/// Suggest what to play next from the games you own.
///
/// It learns what you like from what you have played: every genre and tag
/// earns points from the hours you put into games that have it, more if
/// you rated the game highly or made it a favourite, fewer if you rated it
/// low or abandoned it. Games you have barely touched are then scored by
/// how well their genres and tags match.
pub fn recommend(
    games: &[Game],
    extras: &HashMap<i64, GameExtras>,
    tags: &HashMap<i64, Vec<String>>,
    limit: usize,
) -> Vec<Recommendation> {
    let labels = |game: &Game| -> Vec<String> {
        let mut labels: Vec<String> = game.genres.clone();
        labels.extend(tags.get(&game.id).into_iter().flatten().cloned());
        labels
    };
    let extra = |id: i64| extras.get(&id).cloned().unwrap_or_default();

    // What you like, learned from what you've played.
    let mut taste: HashMap<String, f32> = HashMap::new();
    for game in games {
        let e = extra(game.id);
        // Hours count, but with less and less effect, so one 500-hour game
        // doesn't drown out everything else.
        let mut weight = (game.playtime_secs as f32 / HOUR).min(60.0).sqrt();
        weight *= match e.rating {
            Some(5) => 2.0,
            Some(4) => 1.5,
            Some(3) => 1.0,
            Some(2) => 0.4,
            Some(1) => 0.0,
            _ => 1.0,
        };
        if game.favourite {
            weight = weight * 1.5 + 1.0;
        }
        match e.status {
            Some(Status::Abandoned) => weight *= 0.2,
            Some(Status::Finished) => weight += 1.0,
            _ => {}
        }
        if weight <= 0.0 {
            continue;
        }
        for label in labels(game) {
            *taste.entry(label).or_default() += weight;
        }
    }
    let strongest = taste.values().copied().fold(0.0_f32, f32::max);

    let mut picks: Vec<Recommendation> = games
        .iter()
        .filter(|game| game.installed && !game.hidden)
        .filter(|game| game.playtime_secs < 2 * 3600)
        .filter_map(|game| {
            let e = extra(game.id);
            if matches!(e.status, Some(Status::Finished | Status::Abandoned)) {
                return None;
            }
            // The label of this game that you like most.
            let best = labels(game)
                .into_iter()
                .filter_map(|label| taste.get(&label).map(|points| (*points, label)))
                .max_by(|a, b| a.0.total_cmp(&b.0));
            let matched: f32 = labels(game)
                .iter()
                .filter_map(|label| taste.get(label))
                .sum();
            let mut score = if strongest > 0.0 {
                matched / strongest
            } else {
                0.0
            };
            let mut reason = match best {
                Some((_, label)) if score > 0.0 => format!("Because you like {label}"),
                _ => "You haven't tried this yet".to_string(),
            };
            match e.status {
                Some(Status::Backlog) => {
                    score += 0.25;
                    if score <= 0.25 {
                        reason = "From your backlog".to_string();
                    }
                }
                Some(Status::Playing) => {
                    score += 0.4;
                    reason = "Pick up where you left off".to_string();
                }
                Some(Status::OnHold) => score -= 0.1,
                _ => {}
            }
            if game.playtime_secs == 0 {
                score += 0.05; // a small nudge towards something new
            }
            Some(Recommendation {
                game_id: game.id,
                score,
                reason,
            })
        })
        .collect();
    // Best first; ties go to the most recently added game.
    picks.sort_by(|a, b| b.score.total_cmp(&a.score).then(b.game_id.cmp(&a.game_id)));
    picks.truncate(limit);
    picks
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
            playtime_secs: hours * 3600,
            last_played: None,
            added_at: id,
            cover_path: None,
            icon_path: None,
            description: None,
            genres: genres.iter().map(|g| g.to_string()).collect(),
            release_year: None,
        }
    }

    #[test]
    fn series_names_and_numbers() {
        assert_eq!(series_of("Borderlands 2"), ("Borderlands".into(), Some(2)));
        assert_eq!(
            series_of("Borderlands: The Pre-Sequel"),
            ("Borderlands".into(), None)
        );
        assert_eq!(
            series_of("Final Fantasy VII Remake"),
            ("Final Fantasy".into(), Some(7))
        );
        assert_eq!(series_of("Black Mesa"), ("Black Mesa".into(), None));
        assert_eq!(series_of("Black Myth: Wukong"), ("Black Myth".into(), None));
        assert_eq!(series_of("Portal 2"), ("Portal".into(), Some(2)));
        assert_eq!(
            series_of("Dark Souls III - Deluxe Edition"),
            ("Dark Souls".into(), Some(3))
        );
        assert_eq!(series_of("Rally '98"), ("Rally".into(), Some(98)));
        assert_eq!(
            series_of("2048"),
            ("2048".into(), None),
            "a title that is only a number"
        );
    }

    #[test]
    fn guesses_group_series_in_playing_order() {
        let games = vec![
            game(1, "Borderlands 4", 0, &[]),
            game(2, "Black Mesa", 0, &[]),
            game(3, "Borderlands 2", 0, &[]),
            game(4, "Borderlands: The Pre-Sequel", 0, &[]),
            game(5, "Black Squad", 0, &[]),
            game(6, "Portal", 0, &[]),
        ];
        let guesses = guess_franchises(&games, &HashMap::new());
        assert_eq!(guesses.len(), 1, "{guesses:?}");
        assert_eq!(guesses[0].name, "Borderlands");
        assert_eq!(guesses[0].games, [(4, None), (3, Some(2)), (1, Some(4))]);
    }

    #[test]
    fn games_already_in_a_franchise_are_not_guessed_again() {
        let games = vec![game(1, "Portal", 0, &[]), game(2, "Portal 2", 0, &[])];
        let mut extras = HashMap::new();
        extras.insert(
            1,
            GameExtras {
                franchise: Some("Portal".into()),
                ..Default::default()
            },
        );
        assert!(guess_franchises(&games, &extras).is_empty());
    }

    #[test]
    fn franchises_are_ordered_and_count_finished_games() {
        let games = vec![
            game(1, "Portal 2", 0, &[]),
            game(2, "Portal", 0, &[]),
            game(3, "Half-Life", 0, &[]),
        ];
        let mut extras = HashMap::new();
        let entry = |order, status| GameExtras {
            franchise: Some("Portal".into()),
            franchise_order: order,
            status,
            ..Default::default()
        };
        extras.insert(1, entry(Some(2), None));
        extras.insert(2, entry(Some(1), Some(Status::Finished)));

        let list = franchises(&games, &extras);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Portal");
        assert_eq!(list[0].games, [2, 1]);
        assert_eq!(list[0].finished, 1);
    }

    #[test]
    fn recommends_unplayed_games_in_genres_you_play() {
        let games = vec![
            game(1, "Played RPG", 80, &["RPG"]),
            game(2, "Played Racer", 2, &["Racing"]),
            game(3, "New RPG", 0, &["RPG"]),
            game(4, "New Racer", 0, &["Racing"]),
            game(5, "New Puzzle", 0, &["Puzzle"]),
        ];
        let picks = recommend(&games, &HashMap::new(), &HashMap::new(), 10);
        let order: Vec<i64> = picks.iter().map(|p| p.game_id).collect();
        assert_eq!(order, [3, 4, 5], "{picks:?}");
        assert_eq!(picks[0].reason, "Because you like RPG");
        assert_eq!(picks[2].reason, "You haven't tried this yet");
    }

    #[test]
    fn ratings_and_status_change_the_picks() {
        let games = vec![
            game(1, "Loved Shooter", 10, &["Shooter"]),
            game(2, "Hated RPG", 10, &["RPG"]),
            game(3, "New Shooter", 0, &["Shooter"]),
            game(4, "New RPG", 0, &["RPG"]),
            game(5, "Finished Shooter", 1, &["Shooter"]),
            game(6, "Dropped Shooter", 1, &["Shooter"]),
        ];
        let mut extras = HashMap::new();
        let with = |rating, status| GameExtras {
            rating,
            status,
            ..Default::default()
        };
        extras.insert(1, with(Some(5), None));
        extras.insert(2, with(Some(1), None));
        extras.insert(5, with(None, Some(Status::Finished)));
        extras.insert(6, with(None, Some(Status::Abandoned)));

        let picks = recommend(&games, &extras, &HashMap::new(), 10);
        let order: Vec<i64> = picks.iter().map(|p| p.game_id).collect();
        assert_eq!(
            order,
            [3, 4],
            "finished and abandoned games are never suggested"
        );
        assert!(picks[0].score > picks[1].score);
    }

    #[test]
    fn tags_count_like_genres_and_the_limit_is_respected() {
        let games = vec![
            game(1, "Played", 20, &[]),
            game(2, "A", 0, &[]),
            game(3, "B", 0, &[]),
            game(4, "C", 0, &[]),
        ];
        let mut tags = HashMap::new();
        tags.insert(1, vec!["Co-op".to_string()]);
        tags.insert(3, vec!["Co-op".to_string()]);
        let picks = recommend(&games, &HashMap::new(), &tags, 2);
        assert_eq!(picks.len(), 2);
        assert_eq!(picks[0].game_id, 3);
        assert_eq!(picks[0].reason, "Because you like Co-op");
    }

    #[test]
    fn an_empty_library_recommends_nothing() {
        assert!(recommend(&[], &HashMap::new(), &HashMap::new(), 5).is_empty());
    }
}
