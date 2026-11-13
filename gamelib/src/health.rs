//! Library health: install checks, disk space, and a report of anything
//! that needs attention (missing exes, missing covers, duplicates…).
//!
//! The checks are split in two. [`check_library`] is quick (it looks at
//! the database and whether a few files exist), so the report can be built
//! whenever the page opens. Measuring how big each install folder is means
//! reading every file in it, so that runs in the background
//! ([`measure_install`]) and the result is saved on the game.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use rusqlite::params;
use walkdir::WalkDir;

use crate::db::Db;
use crate::extras::GameExtras;
use crate::model::{Game, LaunchTarget};

/// Something wrong (or worth a look) with one game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issue {
    /// The exe the game starts from isn't there.
    ExeMissing(PathBuf),
    /// The install folder isn't there.
    InstallDirMissing(PathBuf),
    /// A rescan couldn't find the game; it's kept as "not installed".
    NotInstalled,
    /// No cover picture.
    NoCover,
    /// No genres, so recommendations and length guesses can't use it.
    NoGenres,
    /// The saves folder set for backups isn't there.
    SaveDirMissing(PathBuf),
    /// Another game has the same title (the id of the other one).
    Duplicate(i64),
}

impl Issue {
    /// Problems stop a game working; the rest are just tidying up.
    pub fn is_problem(&self) -> bool {
        matches!(
            self,
            Issue::ExeMissing(_) | Issue::InstallDirMissing(_) | Issue::SaveDirMissing(_)
        )
    }

    pub fn describe(&self) -> String {
        match self {
            Issue::ExeMissing(path) => format!("The exe is missing: {}", path.display()),
            Issue::InstallDirMissing(path) => {
                format!("The install folder is missing: {}", path.display())
            }
            Issue::NotInstalled => "Not installed (a scan couldn't find it)".into(),
            Issue::NoCover => "No cover picture".into(),
            Issue::NoGenres => "No genres".into(),
            Issue::SaveDirMissing(path) => {
                format!("The saves folder is missing: {}", path.display())
            }
            Issue::Duplicate(_) => "Another game has the same title".into(),
        }
    }
}

/// The whole report.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Report {
    /// Games with something to look at, worst first.
    pub games: Vec<(i64, Vec<Issue>)>,
    pub problems: usize,
    pub suggestions: usize,
    /// Space used by the games measured so far, in bytes.
    pub measured_bytes: u64,
    pub measured_games: usize,
    /// Each drive games are installed on: its folder, free space (GB) and
    /// how many games are on it.
    pub drives: Vec<(PathBuf, f64, usize)>,
    /// A score out of 100: 100 is nothing to fix.
    pub score: u8,
}

fn normal_title(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Check every game. `sizes` is each game's measured size in bytes;
/// `free_space` looks up free space (GB) for a folder.
pub fn check_library(
    games: &[Game],
    extras: &HashMap<i64, GameExtras>,
    sizes: &HashMap<i64, u64>,
    mut free_space: impl FnMut(&Path) -> Option<(PathBuf, f64)>,
) -> Report {
    let mut by_title: HashMap<String, Vec<i64>> = HashMap::new();
    for game in games {
        by_title
            .entry(normal_title(&game.title))
            .or_default()
            .push(game.id);
    }
    let mut report = Report::default();
    let mut drives: HashMap<PathBuf, (f64, usize)> = HashMap::new();
    for game in games.iter().filter(|g| !g.hidden) {
        let mut issues = Vec::new();
        if !game.installed {
            issues.push(Issue::NotInstalled);
        } else {
            if let LaunchTarget::Exe { path, .. } = &game.launch
                && !path.is_file()
            {
                issues.push(Issue::ExeMissing(path.clone()));
            }
            if let Some(dir) = &game.install_dir
                && !dir.is_dir()
            {
                issues.push(Issue::InstallDirMissing(dir.clone()));
            }
        }
        if game.cover_path.as_deref().is_none_or(|p| !p.is_file()) {
            issues.push(Issue::NoCover);
        }
        if game.genres.is_empty() {
            issues.push(Issue::NoGenres);
        }
        if let Some(dir) = extras.get(&game.id).and_then(|e| e.save_dir.clone())
            && !dir.is_dir()
        {
            issues.push(Issue::SaveDirMissing(dir));
        }
        if let Some(other) = by_title[&normal_title(&game.title)]
            .iter()
            .find(|id| **id != game.id)
        {
            issues.push(Issue::Duplicate(*other));
        }
        if let Some(bytes) = sizes.get(&game.id) {
            report.measured_bytes += bytes;
            report.measured_games += 1;
        }
        if game.installed
            && let Some(dir) = &game.install_dir
            && dir.is_dir()
            && let Some((mount, free)) = free_space(dir)
        {
            let entry = drives.entry(mount).or_insert((free, 0));
            entry.1 += 1;
        }
        if !issues.is_empty() {
            report.problems += issues.iter().filter(|i| i.is_problem()).count();
            report.suggestions += issues.iter().filter(|i| !i.is_problem()).count();
            report.games.push((game.id, issues));
        }
    }
    report.games.sort_by_key(|(id, issues)| {
        (
            std::cmp::Reverse(issues.iter().filter(|i| i.is_problem()).count()),
            std::cmp::Reverse(issues.len()),
            *id,
        )
    });
    report.drives = drives
        .into_iter()
        .map(|(path, (free, count))| (path, free, count))
        .collect();
    report.drives.sort_by(|a, b| a.0.cmp(&b.0));
    let total = games.iter().filter(|g| !g.hidden).count().max(1) as f32;
    let penalty = (report.problems as f32 * 3.0 + report.suggestions as f32) / (total * 4.0);
    report.score = (100.0 * (1.0 - penalty.min(1.0))).round() as u8;
    report
}

/// How many bytes the files in a folder take up.
pub fn folder_size(dir: &Path) -> u64 {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.metadata().ok())
        .map(|meta| meta.len())
        .sum()
}

/// "12.3 GB", "450 MB".
pub fn format_bytes(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.0} MB", b / MB)
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

/// The drive a folder is on, and its free space in GB.
pub fn drive_of(path: &Path) -> Option<(PathBuf, f64)> {
    let disks = sysinfo::Disks::new_with_refreshed_list().ok()?;
    let path = crate::paths::real_path(path);
    disks
        .iter()
        .filter(|disk| crate::paths::is_inside(&path, disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
        .map(|disk| {
            (
                disk.mount_point().to_path_buf(),
                disk.available_space() as f64 / (1024.0 * 1024.0 * 1024.0),
            )
        })
}

impl Db {
    /// Each measured game's install size, in bytes.
    pub fn install_sizes(&self) -> Result<HashMap<i64, u64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, size_bytes FROM games WHERE size_bytes IS NOT NULL")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?.max(0) as u64))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn set_install_size(&self, game_id: i64, bytes: u64, now: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET size_bytes = ?2, size_checked_at = ?3 WHERE id = ?1",
            params![game_id, bytes as i64, now],
        )?;
        Ok(())
    }
}

/// Measure one game's install folder and save the size. Returns the size,
/// or `None` if it has no folder to measure.
pub fn measure_install(db: &Db, game: &Game, now: i64) -> Result<Option<u64>> {
    let Some(dir) = game.install_dir.as_deref().filter(|d| d.is_dir()) else {
        return Ok(None);
    };
    let bytes = folder_size(dir);
    db.set_install_size(game.id, bytes, now)?;
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DetectedGame, Source};

    fn game(id: i64, title: &str, dir: Option<PathBuf>) -> Game {
        Game {
            id,
            title: title.into(),
            source: Source::Folder,
            source_id: id.to_string(),
            install_dir: dir.clone(),
            launch: LaunchTarget::Exe {
                path: dir.unwrap_or_default().join("game.exe"),
                args: vec![],
                working_dir: None,
            },
            favourite: false,
            hidden: false,
            installed: true,
            playtime_secs: 0,
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
    fn problems_and_suggestions_are_found() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good");
        std::fs::create_dir_all(&good).unwrap();
        std::fs::write(good.join("game.exe"), vec![0u8; 3000]).unwrap();
        let cover = dir.path().join("c.jpg");
        std::fs::write(&cover, "x").unwrap();
        let mut ok = game(1, "Good Game", Some(good.clone()));
        ok.cover_path = Some(cover);
        let broken = game(2, "Broken", Some(dir.path().join("gone")));
        let twin = game(3, "good game!", Some(good.clone()));
        let games = vec![ok, broken, twin];
        let sizes = HashMap::from([(1, 3000)]);
        let report = check_library(&games, &HashMap::new(), &sizes, |_| {
            Some((PathBuf::from("/"), 50.0))
        });
        // Broken has two problems, so it comes first.
        assert_eq!(report.games[0].0, 2);
        assert_eq!(report.problems, 2);
        assert!(
            report
                .games
                .iter()
                .any(|(id, issues)| *id == 3 && issues.contains(&Issue::Duplicate(1)))
        );
        assert_eq!(report.measured_bytes, 3000);
        assert_eq!(report.drives, vec![(PathBuf::from("/"), 50.0, 2)]);
        assert!(report.score < 100);

        let db = Db::open_in_memory().unwrap();
        let id = db
            .upsert_detected(&DetectedGame::manual("Good", good.join("game.exe")), 1)
            .unwrap()
            .game_id;
        let stored = db.require_game(id).unwrap();
        let mut with_dir = stored.clone();
        with_dir.install_dir = Some(good.clone());
        assert_eq!(measure_install(&db, &with_dir, 5).unwrap(), Some(3000));
        assert_eq!(db.install_sizes().unwrap()[&id], 3000);
    }

    #[test]
    fn a_tidy_library_scores_100() {
        let report = check_library(&[], &HashMap::new(), &HashMap::new(), |_| None);
        assert_eq!(report.score, 100);
        assert_eq!(format_bytes(5 * 1024 * 1024 * 1024 + 1), "5.0 GB");
        assert_eq!(format_bytes(300 * 1024 * 1024), "300 MB");
        assert_eq!(format_bytes(10), "1 KB");
    }
}
