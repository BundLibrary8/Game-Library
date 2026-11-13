//! CSV export and import, for spreadsheets (Excel, Google Sheets…).
//!
//! **Export** writes one row per game: title, source, hours played, status,
//! rating and so on.
//!
//! **Import** reads a CSV with a header row. Columns are matched by name
//! (any order, any case; unknown columns are ignored). Each row updates
//! the game with the same title (or the same source and source id). A row
//! for a game that isn't in the library is added only if it has an `exe`
//! column with a path; otherwise it's skipped and counted. Playtime only
//! ever goes up.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::db::Db;
use crate::extras::Status;
use crate::model::Game;

pub const COLUMNS: [&str; 13] = [
    "title",
    "source",
    "source_id",
    "hours_played",
    "last_played",
    "status",
    "rating",
    "hours_to_beat",
    "favourite",
    "genres",
    "tags",
    "notes",
    "exe",
];

/// Quote a field if it needs it.
fn field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) || text.starts_with(' ') || text.ends_with(' ') {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// Parse CSV text into rows of fields. Handles quotes, doubled quotes,
/// commas and line breaks inside quotes, and `\r\n` line ends.
pub fn parse(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    current.push('"');
                    chars.next();
                }
                '"' => quoted = false,
                c => current.push(c),
            }
        } else {
            match c {
                '"' => quoted = true,
                ',' => row.push(std::mem::take(&mut current)),
                '\r' => {}
                '\n' => {
                    row.push(std::mem::take(&mut current));
                    rows.push(std::mem::take(&mut row));
                }
                c => current.push(c),
            }
        }
    }
    if !current.is_empty() || !row.is_empty() {
        row.push(current);
        rows.push(row);
    }
    rows.retain(|r| !(r.len() == 1 && r[0].trim().is_empty()));
    rows
}

fn date(at: i64) -> String {
    chrono::DateTime::from_timestamp(at, 0)
        .map(|t| t.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

impl Db {
    /// The whole library as CSV text.
    pub fn export_csv(&self) -> Result<String> {
        let extras = self.all_extras()?;
        let tags = self.all_game_tags()?;
        let mut out = format!("{}\r\n", COLUMNS.join(","));
        for game in self.list_games()? {
            let e = extras.get(&game.id).cloned().unwrap_or_default();
            let exe = match &game.launch {
                crate::model::LaunchTarget::Exe { path, .. } => path.to_string_lossy().into_owned(),
                crate::model::LaunchTarget::Uri(_) => String::new(),
            };
            let row = [
                game.title.clone(),
                game.source.as_str().to_string(),
                game.source_id.clone(),
                format!("{:.1}", game.playtime_secs as f64 / 3600.0),
                game.last_played.map(date).unwrap_or_default(),
                e.status.map(|s| s.label().to_string()).unwrap_or_default(),
                e.rating.map(|r| r.to_string()).unwrap_or_default(),
                e.minutes_to_beat
                    .map(|m| format!("{:.1}", m as f64 / 60.0))
                    .unwrap_or_default(),
                if game.favourite { "yes" } else { "" }.to_string(),
                game.genres.join("; "),
                tags.get(&game.id).map(|t| t.join("; ")).unwrap_or_default(),
                e.notes.unwrap_or_default(),
                exe,
            ];
            let fields: Vec<String> = row.iter().map(|f| field(f)).collect();
            out.push_str(&fields.join(","));
            out.push_str("\r\n");
        }
        Ok(out)
    }

    /// Import CSV text. See the module notes for how rows are matched.
    pub fn import_csv(&self, text: &str, now: i64) -> Result<CsvImport> {
        let rows = parse(text);
        let Some((header, rows)) = rows.split_first() else {
            bail!("the file is empty");
        };
        let index: HashMap<String, usize> = header
            .iter()
            .enumerate()
            .map(|(i, name)| (name.trim().to_lowercase().replace(' ', "_"), i))
            .collect();
        if !index.contains_key("title") {
            bail!("the first row needs a \"title\" column");
        }
        let games = self.list_games()?;
        let by_title: HashMap<String, &Game> =
            games.iter().map(|g| (g.title.to_lowercase(), g)).collect();
        let mut summary = CsvImport::default();
        let tx = self.conn.unchecked_transaction()?;
        for (line, row) in rows.iter().enumerate() {
            let get = |name: &str| {
                index
                    .get(name)
                    .and_then(|i| row.get(*i))
                    .map(|v| v.trim())
                    .filter(|v| !v.is_empty())
            };
            let Some(title) = get("title") else {
                summary.skipped += 1;
                continue;
            };
            let existing = match (get("source"), get("source_id")) {
                (Some(source), Some(id)) => games
                    .iter()
                    .find(|g| g.source.as_str() == source && g.source_id == id),
                _ => None,
            }
            .or_else(|| by_title.get(&title.to_lowercase()).copied());
            let id = match (existing, get("exe")) {
                (Some(game), _) => {
                    summary.updated += 1;
                    game.id
                }
                (None, Some(exe)) => {
                    summary.added += 1;
                    self.add_manual_game(title, &PathBuf::from(exe), now)?
                }
                (None, None) => {
                    summary.skipped += 1;
                    summary.not_found.push(title.to_string());
                    continue;
                }
            };
            let row_error = |what: &str, value: &str| {
                anyhow::anyhow!("row {}: {what} \"{value}\" isn't understood", line + 2)
            };
            if let Some(hours) = get("hours_played") {
                let hours: f64 = hours
                    .parse()
                    .map_err(|_| row_error("hours_played", hours))?;
                self.conn.execute(
                    "UPDATE games SET playtime_secs = MAX(playtime_secs, ?2) WHERE id = ?1",
                    rusqlite::params![id, (hours.max(0.0) * 3600.0) as i64],
                )?;
            }
            if let Some(status) = get("status") {
                let parsed = Status::ALL.into_iter().find(|s| {
                    s.label().eq_ignore_ascii_case(status)
                        || s.as_str().eq_ignore_ascii_case(status)
                });
                self.set_status(id, Some(parsed.ok_or_else(|| row_error("status", status))?))?;
            }
            if let Some(rating) = get("rating") {
                let stars: u8 = rating.parse().map_err(|_| row_error("rating", rating))?;
                self.set_rating(id, Some(stars))?;
            }
            if let Some(hours) = get("hours_to_beat") {
                let hours: f64 = hours
                    .parse()
                    .map_err(|_| row_error("hours_to_beat", hours))?;
                self.set_minutes_to_beat(id, Some((hours * 60.0).round() as u32))?;
            }
            if let Some(favourite) = get("favourite") {
                let yes = matches!(
                    favourite.to_lowercase().as_str(),
                    "yes" | "y" | "true" | "1" | "x"
                );
                self.set_favourite(id, yes)?;
            }
            if let Some(notes) = get("notes") {
                self.set_notes(id, Some(notes))?;
            }
            if let Some(tags) = get("tags") {
                for name in tags.split(';').map(str::trim).filter(|t| !t.is_empty()) {
                    let tag = self.ensure_tag(name, false)?;
                    self.tag_game(id, tag.id)?;
                }
            }
        }
        tx.commit()?;
        self.refresh_exe_installed()?;
        Ok(summary)
    }
}

/// What a CSV import did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CsvImport {
    pub added: usize,
    pub updated: usize,
    pub skipped: usize,
    /// Titles that weren't in the library and had no exe to add them with.
    pub not_found: Vec<String>,
}

pub fn write_csv(text: &str, file: &Path) -> Result<()> {
    // A byte-order mark makes Excel read the file as UTF-8.
    std::fs::write(file, format!("\u{feff}{text}"))
        .with_context(|| format!("couldn't write {}", file.display()))
}

pub fn read_csv(file: &Path) -> Result<String> {
    std::fs::read_to_string(file).with_context(|| format!("couldn't read {}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DetectedGame;

    #[test]
    fn csv_text_is_parsed_with_quotes() {
        let rows = parse("a,b,c\r\n\"x, y\",\"say \"\"hi\"\"\",\"two\nlines\"\n\n1,,3");
        assert_eq!(rows[0], vec!["a", "b", "c"]);
        assert_eq!(rows[1], vec!["x, y", "say \"hi\"", "two\nlines"]);
        assert_eq!(rows[2], vec!["1", "", "3"]);
        assert_eq!(rows.len(), 3);
        assert_eq!(field("plain"), "plain");
        assert_eq!(field("a,b"), "\"a,b\"");
    }

    #[test]
    fn export_then_import_round_trips() {
        let db = Db::open_in_memory().unwrap();
        let id = db
            .upsert_detected(&DetectedGame::manual("Ember Road", "/g/ember.exe"), 1)
            .unwrap()
            .game_id;
        db.set_rating(id, Some(4)).unwrap();
        db.set_notes(id, Some("chapter 3, \"the bridge\"")).unwrap();
        let csv = db.export_csv().unwrap();
        assert!(csv.starts_with("title,source,"));
        assert!(csv.contains("\"chapter 3, \"\"the bridge\"\"\""));

        let other = Db::open_in_memory().unwrap();
        let summary = other.import_csv(&csv, 5).unwrap();
        assert_eq!((summary.added, summary.updated), (1, 0));
        let game = &other.list_games().unwrap()[0];
        let extras = other.extras(game.id).unwrap();
        assert_eq!(extras.rating, Some(4));
        assert_eq!(extras.notes.as_deref(), Some("chapter 3, \"the bridge\""));
    }

    #[test]
    fn a_spreadsheet_updates_games_by_title() {
        let db = Db::open_in_memory().unwrap();
        let id = db
            .upsert_detected(&DetectedGame::manual("Hades", "/g/hades.exe"), 1)
            .unwrap()
            .game_id;
        let csv = "Title,Status,Hours Played,Hours to beat,Favourite,Tags\n\
                   hades,Finished,12.5,22,yes,Roguelike; Co-op\n\
                   Not Owned,Playing,,,,\n";
        let summary = db.import_csv(csv, 9).unwrap();
        assert_eq!(summary.updated, 1);
        assert_eq!(summary.not_found, vec!["Not Owned"]);
        let game = db.require_game(id).unwrap();
        assert_eq!(game.playtime_secs, 45_000);
        assert!(game.favourite);
        let extras = db.extras(id).unwrap();
        assert_eq!(extras.status, Some(Status::Finished));
        assert_eq!(extras.minutes_to_beat, Some(22 * 60));
        assert_eq!(db.tags_for_game(id).unwrap().len(), 2);

        assert!(
            db.import_csv("Status\nPlaying\n", 1).is_err(),
            "no title column"
        );
        let bad = db.import_csv("title,rating\nhades,lots\n", 1).unwrap_err();
        assert!(format!("{bad}").contains("row 2"));
    }
}
