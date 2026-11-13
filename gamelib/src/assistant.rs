//! The GameLib assistant: ask questions about your library in plain words.
//!
//! It works in two ways:
//!
//! - **Built in** ([`answer`]): no internet and no account. It recognises
//!   the common questions ("What should I play tonight?", "Which games have
//!   I neglected?", "My five most played games", "Something like Elden
//!   Ring", "What should I complete next?") and answers them from the
//!   library database.
//! - **Claude** ([`ask_claude`]): if you add an Anthropic API key in
//!   Settings, questions go to Claude along with a summary of your library
//!   (titles, hours, status, ratings, genres), so it can answer anything.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::backlog::{self, Goal, Habits, format_minutes};
use crate::extras::{GameExtras, Status};
use crate::insights::Recommendation;
use crate::model::Game;

/// Everything the assistant knows about your library.
pub struct Library<'a> {
    pub games: &'a [Game],
    pub extras: &'a HashMap<i64, GameExtras>,
    pub tags: &'a HashMap<i64, Vec<String>>,
    /// From [`crate::insights::recommend`], for every game worth suggesting.
    pub taste: &'a [Recommendation],
    pub habits: Habits,
    pub now: i64,
}

impl Library<'_> {
    fn game(&self, id: i64) -> Option<&Game> {
        self.games.iter().find(|g| g.id == id)
    }

    fn title(&self, id: i64) -> String {
        self.game(id).map(|g| g.title.clone()).unwrap_or_default()
    }

    fn extras_of(&self, id: i64) -> GameExtras {
        self.extras.get(&id).cloned().unwrap_or_default()
    }
}

/// A reply, with the games it mentions (so they can be shown as cards).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Answer {
    pub text: String,
    pub games: Vec<i64>,
}

/// Questions to suggest to someone who doesn't know what to ask.
pub const EXAMPLES: [&str; 6] = [
    "What should I play tonight?",
    "Which games have I neglected?",
    "What are my five most played games?",
    "Recommend something similar to Elden Ring",
    "Which games have the highest completion rates?",
    "What should I complete next?",
];

// --- The built-in assistant --------------------------------------------------

/// Answer a question from the library alone.
pub fn answer(question: &str, lib: &Library) -> Answer {
    let q = question.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| q.contains(w));
    let count = number_in(&q).unwrap_or(5).clamp(1, 25);

    if has(&["similar", "like ", "games like", "same as"]) && !has(&["would i like"]) {
        return similar(&q, lib, count);
    }
    if has(&[
        "neglect",
        "never played",
        "never launched",
        "haven't played",
        "havent played",
        "untouched",
        "forgotten",
        "ignored",
    ]) {
        return neglected(lib, count.max(8));
    }
    if has(&[
        "most played",
        "played the most",
        "top ",
        "favourite games",
        "favorite games",
        "most time",
    ]) {
        return most_played(lib, count);
    }
    if has(&[
        "completion rate",
        "most complete",
        "progress",
        "furthest",
        "closest to finish",
    ]) {
        return completion(lib, count);
    }
    if has(&[
        "complete next",
        "finish next",
        "should i complete",
        "should i finish",
        "what to finish",
        "what to complete",
        "close to finish",
    ]) {
        return complete_next(lib, count);
    }
    if has(&["how long", "how many hours"])
        && let Some(game) = find_title(&q, lib.games)
    {
        return about_game(game, lib);
    }
    if has(&[
        "how many",
        "summary",
        "backlog",
        "overview",
        "stats",
        "statistics",
    ]) {
        return summary(lib);
    }
    if has(&[
        "tonight",
        "play now",
        "should i play",
        "what to play",
        "recommend",
        "suggest",
        "bored",
        "free time",
        "hour",
        "minutes",
    ]) {
        let goal = if has(&["new", "haven't tried", "never tried", "fresh"]) {
            Goal::New
        } else if has(&["finish", "complete", "beat"]) {
            Goal::Finish
        } else {
            Goal::Anything
        };
        return tonight(lib, free_time_in(&q), goal);
    }
    if let Some(game) = find_title(&q, lib.games) {
        return about_game(game, lib);
    }
    Answer {
        text: format!(
            "I can answer questions about your library. Try:\n• {}\n\nFor anything else, add an Anthropic API key in Settings and I'll ask Claude.",
            EXAMPLES.join("\n• ")
        ),
        games: Vec::new(),
    }
}

fn list(lines: impl IntoIterator<Item = String>) -> String {
    lines
        .into_iter()
        .enumerate()
        .map(|(i, line)| format!("{}. {line}", i + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

fn hours(secs: u64) -> String {
    format_minutes((secs / 60) as u32)
}

fn summary(lib: &Library) -> Answer {
    let s = backlog::summary(lib.games, lib.extras);
    let mut text = s.sentence();
    if s.playing > 0 {
        text.push_str(&format!(" You're in the middle of {}.", s.playing));
    }
    text.push_str(&format!(
        " Beating everything left would take roughly {} hours.",
        s.hours_left
    ));
    Answer {
        text,
        games: Vec::new(),
    }
}

fn tonight(lib: &Library, free_minutes: Option<u32>, goal: Goal) -> Answer {
    let free = free_minutes
        .or_else(|| {
            (lib.habits.typical_session_minutes > 0).then_some(lib.habits.typical_session_minutes)
        })
        .unwrap_or(120);
    let picks = backlog::tonight(lib.games, lib.extras, lib.taste, &lib.habits, free, goal, 3);
    if picks.is_empty() {
        return Answer {
            text: "Nothing fits right now: everything installed is finished or set aside. Try scanning for games or marking one as Backlog.".into(),
            games: Vec::new(),
        };
    }
    let mut text = format!("With about {} to play, I'd go for:\n", format_minutes(free));
    text.push_str(&list(
        picks
            .iter()
            .map(|p| format!("{}: {}", lib.title(p.game_id), p.reason)),
    ));
    Answer {
        text,
        games: picks.iter().map(|p| p.game_id).collect(),
    }
}

fn neglected(lib: &Library, count: usize) -> Answer {
    let found = backlog::neglected(lib.games, lib.extras, lib.now, count);
    if found.is_empty() {
        return Answer {
            text:
                "Nothing's been neglected: you've played everything you own recently. Impressive!"
                    .into(),
            games: Vec::new(),
        };
    }
    let untouched = lib
        .games
        .iter()
        .filter(|g| !g.hidden && g.playtime_secs == 0 && g.last_played.is_none())
        .count();
    let mut text = format!(
        "{untouched} {} never been launched. The most neglected:\n",
        if untouched == 1 {
            "game has"
        } else {
            "games have"
        }
    );
    text.push_str(&list(
        found
            .iter()
            .map(|(id, why)| format!("{}: {why}", lib.title(*id))),
    ));
    Answer {
        text,
        games: found.iter().map(|f| f.0).collect(),
    }
}

fn most_played(lib: &Library, count: usize) -> Answer {
    let mut played: Vec<&Game> = lib.games.iter().filter(|g| g.playtime_secs > 0).collect();
    played.sort_by_key(|g| (std::cmp::Reverse(g.playtime_secs), g.id));
    played.truncate(count);
    if played.is_empty() {
        return Answer {
            text:
                "You haven't played anything through GameLib yet, so there's no playtime to rank."
                    .into(),
            games: Vec::new(),
        };
    }
    let total: u64 = lib.games.iter().map(|g| g.playtime_secs).sum();
    let mut text = format!("Your {} most played:\n", played.len());
    text.push_str(&list(played.iter().map(|g| {
        let share = g.playtime_secs as f64 / total.max(1) as f64 * 100.0;
        format!(
            "{}: {} ({share:.0}% of all your play)",
            g.title,
            hours(g.playtime_secs)
        )
    })));
    Answer {
        text,
        games: played.iter().map(|g| g.id).collect(),
    }
}

fn completion(lib: &Library, count: usize) -> Answer {
    let found = backlog::by_progress(lib.games, lib.extras, count);
    if found.is_empty() {
        return Answer {
            text: "Play something first and I can tell you how far through it you are.".into(),
            games: Vec::new(),
        };
    }
    let mut text = "Furthest through (from your playtime against how long each game takes, or 100% if marked finished):\n".to_string();
    text.push_str(&list(found.iter().map(|(id, p)| {
        let guessed = lib
            .game(*id)
            .map(|g| backlog::length(g, &lib.extras_of(*id)).guessed)
            .unwrap_or(true);
        let note = if guessed && *p < 1.0 {
            " (length guessed from genre)"
        } else {
            ""
        };
        format!("{}: {:.0}%{note}", lib.title(*id), p * 100.0)
    })));
    Answer {
        text,
        games: found.iter().map(|f| f.0).collect(),
    }
}

fn complete_next(lib: &Library, count: usize) -> Answer {
    let found = backlog::complete_next(lib.games, lib.extras, count.min(5));
    if found.is_empty() {
        return Answer {
            text: "You haven't got anything on the go. Mark a game as Playing on its page and I'll keep track.".into(),
            games: Vec::new(),
        };
    }
    let mut text = "Closest to the finish line:\n".to_string();
    text.push_str(&list(found.iter().map(|(id, left)| {
        format!("{}: about {} left", lib.title(*id), format_minutes(*left))
    })));
    Answer {
        text,
        games: found.iter().map(|f| f.0).collect(),
    }
}

fn similar(q: &str, lib: &Library, count: usize) -> Answer {
    let Some(target) = find_title(q, lib.games) else {
        let wanted =
            after_any(q, &["similar to ", "games like ", "like ", "same as "]).unwrap_or_default();
        return Answer {
            text: format!(
                "I couldn't find \"{}\" in your library. I can only compare games you own; with an Anthropic API key in Settings, Claude can suggest games from outside it.",
                wanted.trim_end_matches(['?', '.', '!']).trim()
            ),
            games: Vec::new(),
        };
    };
    let found = backlog::similar_to(target, lib.games, lib.extras, lib.tags, count);
    if found.is_empty() {
        return Answer {
            text: format!(
                "Nothing in your library is much like {}. Adding genres or tags to your games helps me compare them.",
                target.title
            ),
            games: Vec::new(),
        };
    }
    let mut text = format!("In your library, the closest to {}:\n", target.title);
    text.push_str(&list(found.iter().map(|(id, _)| {
        let game = lib.game(*id);
        let played = game.map(|g| g.playtime_secs).unwrap_or(0);
        let shared: Vec<String> = game
            .map(|g| {
                g.genres
                    .iter()
                    .filter(|genre| target.genres.contains(genre))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut line = lib.title(*id);
        if !shared.is_empty() {
            line.push_str(&format!(": {}", shared.join(", ")));
        }
        if played == 0 {
            line.push_str(" (not played yet)");
        }
        line
    })));
    Answer {
        text,
        games: found.iter().map(|f| f.0).collect(),
    }
}

fn about_game(game: &Game, lib: &Library) -> Answer {
    let e = lib.extras_of(game.id);
    let len = backlog::length(game, &e);
    let mut text = format!(
        "{}: you've played {}.",
        game.title,
        hours(game.playtime_secs)
    );
    match e.status {
        Some(Status::Finished) => text.push_str(" You've finished it."),
        _ => text.push_str(&format!(
            " It takes about {} to beat{}, so roughly {} left.",
            format_minutes(len.minutes),
            if len.guessed {
                " (a guess from its genre)"
            } else {
                ""
            },
            format_minutes(backlog::remaining_minutes(game, &e))
        )),
    }
    if let Some(stars) = e.rating {
        text.push_str(&format!(" You rated it {stars}/5."));
    }
    Answer {
        text,
        games: vec![game.id],
    }
}

/// The text after the first of these markers.
fn after_any<'q>(q: &'q str, markers: &[&str]) -> Option<&'q str> {
    markers
        .iter()
        .filter_map(|m| q.find(m).map(|at| &q[at + m.len()..]))
        .next()
}

/// The game whose title appears in the question (the longest wins), or
/// failing that the one whose title starts with the words after "like".
fn find_title<'g>(q: &str, games: &'g [Game]) -> Option<&'g Game> {
    let clean = |s: &str| {
        s.to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { ' ' })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let q = format!(" {} ", clean(q));
    let found = games
        .iter()
        .filter(|g| {
            let title = clean(&g.title);
            title.len() >= 3 && q.contains(&format!(" {title} "))
        })
        .max_by_key(|g| g.title.len());
    if found.is_some() {
        return found;
    }
    let wanted = clean(after_any(
        &q,
        &["similar to ", "games like ", "like ", "same as ", "about "],
    )?);
    if wanted.len() < 3 {
        return None;
    }
    games
        .iter()
        .filter(|g| clean(&g.title).starts_with(&wanted))
        .min_by_key(|g| g.title.len())
}

/// "five", "10", "a dozen" → the number.
fn number_in(q: &str) -> Option<usize> {
    const WORDS: [(&str, usize); 12] = [
        ("one", 1),
        ("two", 2),
        ("three", 3),
        ("four", 4),
        ("five", 5),
        ("six", 6),
        ("seven", 7),
        ("eight", 8),
        ("nine", 9),
        ("ten", 10),
        ("dozen", 12),
        ("twenty", 20),
    ];
    for word in q.split(|c: char| !c.is_alphanumeric()) {
        if let Ok(n) = word.parse::<usize>() {
            return Some(n);
        }
        if let Some((_, n)) = WORDS.iter().find(|(w, _)| *w == word) {
            return Some(*n);
        }
    }
    None
}

/// "I have 2 hours", "45 minutes", "an hour" → minutes.
fn free_time_in(q: &str) -> Option<u32> {
    let words: Vec<&str> = q
        .split(|c: char| !(c.is_alphanumeric() || c == '.'))
        .filter(|w| !w.is_empty())
        .collect();
    for (i, word) in words.iter().enumerate() {
        let unit = if word.starts_with("hour") || *word == "h" || *word == "hrs" {
            60.0
        } else if word.starts_with("min") {
            1.0
        } else {
            continue;
        };
        let amount = match i.checked_sub(1).map(|j| words[j]) {
            Some("an" | "a" | "one") => 1.0,
            Some("half") => 0.5,
            Some(prev) => prev
                .parse::<f32>()
                .ok()
                .or_else(|| number_in(prev).map(|n| n as f32))
                .unwrap_or(1.0),
            None => 1.0,
        };
        return Some((amount * unit).round() as u32);
    }
    None
}

// --- Claude ----------------------------------------------------------------

/// One message in a conversation with the assistant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    /// `"user"` or `"assistant"`.
    pub role: String,
    pub text: String,
}

/// The Claude model the assistant uses.
pub const MODEL: &str = "claude-opus-5-5";
const API_URL: &str = "https://api.anthropic.com/v1/messages";

const INSTRUCTIONS: &str = "You are the GameLib assistant, built into a PC game library app. \
You answer questions about the user's own game library, described below: what to play, what \
they've neglected, what to finish next, playtime, completion, and recommendations. Prefer games \
they already own; when you suggest a game they don't own, say so. Lengths marked '~' are rough \
guesses from genre, not real data. Keep answers short and friendly: a sentence or two, then a \
short numbered list when listing games. Use plain text, not Markdown headings or tables.";

/// A compact description of the library for Claude: one line per game.
pub fn library_context(lib: &Library) -> String {
    let s = backlog::summary(lib.games, lib.extras);
    let mut out = format!(
        "{}\nTypical session: {} minutes. Sessions per week lately: {:.1}.\n\n\
         Games (title | hours played | status | rating | how long to beat | last played | genres | tags):\n",
        s.sentence(),
        lib.habits.typical_session_minutes,
        lib.habits.sessions_per_week,
    );
    let mut games: Vec<&Game> = lib.games.iter().filter(|g| !g.hidden).collect();
    games.sort_by_key(|g| (std::cmp::Reverse(g.playtime_secs), g.title.to_lowercase()));
    for game in games {
        let e = lib.extras_of(game.id);
        let len = backlog::length(game, &e);
        let last = game
            .last_played
            .map(|at| format!("{} days ago", (lib.now - at).max(0) / 86_400))
            .unwrap_or_else(|| "never".into());
        out.push_str(&format!(
            "{} | {:.1} | {} | {} | {}{} | {} | {} | {}\n",
            game.title,
            game.playtime_secs as f64 / 3600.0,
            e.status.map(|s| s.label()).unwrap_or("-"),
            e.rating
                .map(|r| format!("{r}/5"))
                .unwrap_or_else(|| "-".into()),
            if len.guessed { "~" } else { "" },
            format_minutes(len.minutes),
            last,
            game.genres.join(", "),
            lib.tags
                .get(&game.id)
                .map(|t| t.join(", "))
                .unwrap_or_default(),
        ));
    }
    out
}

/// The request body sent to the Messages API.
pub fn request_body(context: &str, history: &[Turn]) -> serde_json::Value {
    let messages: Vec<serde_json::Value> = history
        .iter()
        .map(|turn| json!({"role": turn.role, "content": turn.text}))
        .collect();
    json!({
        "model": MODEL,
        "max_tokens": 16000,
        "output_config": {"effort": "medium"},
        // If Claude declines (a safety classifier), let the API retry the
        // request on a suitable fallback model instead of failing.
        "fallbacks": "default",
        "system": [
            {"type": "text", "text": INSTRUCTIONS},
            // The library rarely changes between questions, so it's cached.
            {"type": "text", "text": context, "cache_control": {"type": "ephemeral"}},
        ],
        "messages": messages,
    })
}

#[derive(Deserialize)]
struct ApiReply {
    #[serde(default)]
    content: Vec<ApiBlock>,
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct ApiBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct ApiError {
    error: ApiErrorBody,
}

#[derive(Deserialize)]
struct ApiErrorBody {
    message: String,
}

/// Turn the API's JSON reply into the assistant's text.
pub fn parse_reply(body: &str) -> Result<String> {
    let reply: ApiReply =
        serde_json::from_str(body).context("Claude sent back something unexpected")?;
    if reply.stop_reason.as_deref() == Some("refusal") {
        return Ok("Claude declined to answer that one. Try asking another way.".into());
    }
    let text: Vec<&str> = reply
        .content
        .iter()
        .filter(|block| block.kind == "text")
        .map(|block| block.text.trim())
        .filter(|text| !text.is_empty())
        .collect();
    if text.is_empty() {
        bail!("Claude didn't send back an answer");
    }
    let mut text = text.join("\n\n");
    if reply.stop_reason.as_deref() == Some("max_tokens") {
        text.push_str("\n\n(The answer was cut short.)");
    }
    Ok(text)
}

/// Ask Claude. `history` is the conversation so far, ending with the
/// user's new question. Blocks until the answer arrives.
pub fn ask_claude(api_key: &str, context: &str, history: &[Turn]) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("gamelib/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(180))
        .build()?;
    let response = client
        .post(API_URL)
        .header("x-api-key", api_key.trim())
        .header("anthropic-version", "2026-10-21")
        .header("anthropic-beta", "server-side-fallback-2026-07-01")
        .json(&request_body(context, history))
        .send()
        .context("couldn't reach Claude (check your internet connection)")?;
    let status = response.status();
    let body = response.text()?;
    if !status.is_success() {
        let message = serde_json::from_str::<ApiError>(&body)
            .map(|e| e.error.message)
            .unwrap_or_else(|_| body.chars().take(200).collect());
        match status.as_u16() {
            401 => bail!("Claude didn't accept the API key: {message}"),
            429 => bail!("Too many questions too quickly. Wait a moment and try again."),
            _ => bail!("Claude couldn't answer ({status}): {message}"),
        }
    }
    parse_reply(&body)
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
            last_played: (hours > 0).then_some(1_000_000),
            added_at: 0,
            cover_path: None,
            icon_path: None,
            description: None,
            genres: genres.iter().map(|g| g.to_string()).collect(),
            release_year: None,
        }
    }

    fn library() -> Vec<Game> {
        vec![
            game(1, "Elden Ring", 120, &["Action", "RPG"]),
            game(2, "Dark Souls III", 0, &["Action", "RPG"]),
            game(3, "Tetris Effect", 4, &["Puzzle"]),
            game(4, "Hades", 30, &["Action", "Roguelike"]),
            game(5, "Stardew Valley", 0, &["Simulation"]),
        ]
    }

    fn ask(question: &str) -> Answer {
        let games = library();
        let extras = HashMap::from([(
            3,
            GameExtras {
                status: Some(Status::Playing),
                ..GameExtras::default()
            },
        )]);
        let tags = HashMap::new();
        let lib = Library {
            games: &games,
            extras: &extras,
            tags: &tags,
            taste: &[],
            habits: Habits::default(),
            now: 2_000_000,
        };
        answer(question, &lib)
    }

    #[test]
    fn most_played_respects_the_count() {
        let a = ask("What are my five most played games?");
        assert_eq!(a.games, vec![1, 4, 3]);
        assert!(a.text.starts_with("Your 3 most played"));
        assert_eq!(ask("my top 2 games").games, vec![1, 4]);
    }

    #[test]
    fn similar_finds_the_game_by_title() {
        let a = ask("Recommend something similar to Elden Ring");
        assert_eq!(a.games.first(), Some(&2), "{}", a.text);
        let missing = ask("games like Hollow Knight?");
        assert!(missing.text.contains("hollow knight"), "{}", missing.text);
        assert!(missing.games.is_empty());
    }

    #[test]
    fn neglected_lists_the_untouched_games() {
        let a = ask("Which games have I neglected?");
        assert!(
            a.text.starts_with("2 games have never been launched"),
            "{}",
            a.text
        );
        assert!(a.games.contains(&2) && a.games.contains(&5));
    }

    #[test]
    fn tonight_uses_the_time_given() {
        let a = ask("I have 3 hours, what should I play tonight?");
        assert!(a.text.starts_with("With about 3 h to play"), "{}", a.text);
        // Tetris Effect: 7 h game, 4 h in, being played: finishable tonight.
        assert_eq!(a.games.first(), Some(&3));
    }

    #[test]
    fn complete_next_and_completion_rates() {
        // Tetris Effect is being played; Elden Ring and Hades have outgrown
        // their guessed lengths, so nobody knows how close they are.
        assert_eq!(ask("What should I complete next?").games, vec![3]);
        let rates = ask("Which games in my library have the highest completion rates?");
        assert!(rates.text.contains('%'));
        assert!(!rates.games.is_empty());
    }

    #[test]
    fn unknown_questions_get_help() {
        let a = ask("blorp");
        assert!(a.text.contains("What should I play tonight?"));
    }

    #[test]
    fn free_time_and_numbers_are_read() {
        assert_eq!(free_time_in("i have 2 hours"), Some(120));
        assert_eq!(free_time_in("about 45 minutes"), Some(45));
        assert_eq!(free_time_in("an hour or so"), Some(60));
        assert_eq!(free_time_in("half hour"), Some(30));
        assert_eq!(free_time_in("1.5 hours"), Some(90));
        assert_eq!(number_in("my ten most played"), Some(10));
    }

    #[test]
    fn the_request_caches_the_library_and_falls_back() {
        let history = vec![Turn {
            role: "user".into(),
            text: "hi".into(),
        }];
        let body = request_body("LIBRARY", &history);
        assert_eq!(body["model"], MODEL);
        assert_eq!(body["fallbacks"], "default");
        assert_eq!(body["system"][1]["text"], "LIBRARY");
        assert_eq!(body["system"][1]["cache_control"]["type"], "ephemeral");
        assert_eq!(body["messages"][0]["content"], "hi");
        assert!(body.get("thinking").is_none());
    }

    #[test]
    fn replies_are_parsed() {
        let ok = r#"{"content":[{"type":"thinking","thinking":""},{"type":"text","text":"Play Hades."}],"stop_reason":"end_turn"}"#;
        assert_eq!(parse_reply(ok).unwrap(), "Play Hades.");
        let refused = r#"{"content":[],"stop_reason":"refusal"}"#;
        assert!(parse_reply(refused).unwrap().contains("declined"));
        let cut = r#"{"content":[{"type":"text","text":"Long"}],"stop_reason":"max_tokens"}"#;
        assert!(parse_reply(cut).unwrap().contains("cut short"));
        assert!(parse_reply(r#"{"content":[],"stop_reason":"end_turn"}"#).is_err());
    }

    #[test]
    fn the_context_lists_every_game() {
        let games = library();
        let extras = HashMap::new();
        let tags = HashMap::new();
        let lib = Library {
            games: &games,
            extras: &extras,
            tags: &tags,
            taste: &[],
            habits: Habits::default(),
            now: 2_000_000,
        };
        let context = library_context(&lib);
        assert!(context.starts_with("You own 5 games."));
        assert!(context.contains("Elden Ring | 120.0 | - | - | ~45 h"));
        assert_eq!(context.lines().filter(|l| l.contains(" | ")).count(), 6);
    }
}
