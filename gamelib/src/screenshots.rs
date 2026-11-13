//! Screenshot galleries: finding the pictures you took of each game.
//!
//! Looked for in:
//! - the folder you set on the game's page,
//! - Steam's screenshot folder for the game
//!   (`Steam\userdata\<account>\760\remote\<app id>\screenshots`),
//! - the Xbox Game Bar's `Videos\Captures` folder, whose files are named
//!   after the game ("Elden Ring 2026-01-05 21-14-03.png").

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Result;
use rusqlite::params;

use crate::db::Db;
use crate::model::{Game, Source};

const PICTURES: [&str; 5] = ["png", "jpg", "jpeg", "webp", "bmp"];

/// One screenshot, and when it was taken (file time).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shot {
    pub path: PathBuf,
    pub taken: Option<SystemTime>,
}

fn is_picture(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| PICTURES.contains(&e.as_str()))
}

fn pictures_in(dir: &Path, keep: impl Fn(&Path) -> bool) -> Vec<Shot> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_picture(p) && keep(p))
        .map(|path| Shot {
            taken: path.metadata().and_then(|m| m.modified()).ok(),
            path,
        })
        .collect()
}

/// Steam's screenshot folders for an app, across every account on this PC.
pub fn steam_folders(steam_root: &Path, app_id: &str) -> Vec<PathBuf> {
    let Ok(accounts) = std::fs::read_dir(steam_root.join("userdata")) else {
        return Vec::new();
    };
    accounts
        .filter_map(Result::ok)
        .map(|account| {
            account
                .path()
                .join("760")
                .join("remote")
                .join(app_id)
                .join("screenshots")
        })
        .filter(|dir| dir.is_dir())
        .collect()
}

/// Where the Xbox Game Bar saves captures.
pub fn captures_folder() -> Option<PathBuf> {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.video_dir().map(|v| v.join("Captures")))
        .filter(|dir| dir.is_dir())
}

/// Lower case, with anything that isn't a letter or digit turned into a
/// single space, so names compare word by word.
fn simplify(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Does a file name begin with these whole words?
fn starts_with_words(name: &str, words: &str) -> bool {
    name == words || name.starts_with(&format!("{words} "))
}

/// Every screenshot of a game, newest first. `steam_root` and `captures`
/// are passed in so tests can use fake folders. `all_titles` is every
/// title in the library, so a capture of "Portal 2" isn't given to
/// "Portal" too.
pub fn find(
    game: &Game,
    own_folder: Option<&Path>,
    steam_root: Option<&Path>,
    captures: Option<&Path>,
    all_titles: &[String],
) -> Vec<Shot> {
    let mut shots = Vec::new();
    if let Some(dir) = own_folder {
        shots.extend(pictures_in(dir, |_| true));
    }
    if game.source == Source::Steam
        && let Some(root) = steam_root
    {
        for dir in steam_folders(root, &game.source_id) {
            shots.extend(pictures_in(&dir, |_| true));
        }
    }
    if let Some(dir) = captures {
        let title = simplify(&game.title);
        let longer: Vec<String> = all_titles
            .iter()
            .map(|t| simplify(t))
            .filter(|t| t.len() > title.len() && starts_with_words(t, &title))
            .collect();
        if title.len() >= 3 {
            shots.extend(pictures_in(dir, |path| {
                path.file_name()
                    .map(|n| simplify(&n.to_string_lossy()))
                    .is_some_and(|name| {
                        starts_with_words(&name, &title)
                            && !longer.iter().any(|l| starts_with_words(&name, l))
                    })
            }));
        }
    }
    shots.sort_by(|a, b| b.taken.cmp(&a.taken).then(a.path.cmp(&b.path)));
    shots.dedup_by(|a, b| a.path == b.path);
    shots
}

/// The Steam folder on this PC, if Steam is installed.
pub fn steam_root() -> Option<PathBuf> {
    steamlocate::SteamDir::locate()
        .ok()
        .map(|dir| dir.path().to_path_buf())
}

impl Db {
    pub fn screenshots_dir(&self, game_id: i64) -> Result<Option<PathBuf>> {
        Ok(self
            .conn
            .query_row(
                "SELECT screenshots_dir FROM games WHERE id = ?1",
                [game_id],
                |row| row.get::<_, Option<String>>(0),
            )?
            .map(PathBuf::from))
    }

    pub fn set_screenshots_dir(&self, game_id: i64, dir: Option<&Path>) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET screenshots_dir = ?2 WHERE id = ?1",
            params![game_id, dir.map(|d| d.to_string_lossy().into_owned())],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LaunchTarget;

    #[test]
    fn screenshots_are_found_in_every_place() {
        let root = tempfile::tempdir().unwrap();
        let steam = root.path().join("Steam");
        let shots = steam.join("userdata/123/760/remote/400/screenshots");
        std::fs::create_dir_all(shots.join("thumbnails")).unwrap();
        std::fs::write(shots.join("a.jpg"), "x").unwrap();
        std::fs::write(shots.join("notes.txt"), "x").unwrap();
        let own = root.path().join("mine");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(own.join("b.PNG"), "x").unwrap();
        let captures = root.path().join("Captures");
        std::fs::create_dir_all(&captures).unwrap();
        std::fs::write(captures.join("Portal 2026-01-05 21-14-03.png"), "x").unwrap();
        std::fs::write(captures.join("Portal 2 2026-01-05.png"), "x").unwrap();
        std::fs::write(captures.join("Tetris 2026.png"), "x").unwrap();

        let game = Game {
            id: 1,
            title: "Portal".into(),
            source: Source::Steam,
            source_id: "400".into(),
            install_dir: None,
            launch: LaunchTarget::Uri("steam://rungameid/400".into()),
            favourite: false,
            hidden: false,
            installed: true,
            playtime_secs: 0,
            last_played: None,
            added_at: 0,
            cover_path: None,
            icon_path: None,
            description: None,
            genres: vec![],
            release_year: None,
        };
        let titles = vec!["Portal".to_string(), "Portal 2".to_string()];
        let found = find(&game, Some(&own), Some(&steam), Some(&captures), &titles);
        let names: Vec<String> = found
            .iter()
            .map(|s| s.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 3, "{names:?}");
        assert!(names.contains(&"a.jpg".to_string()));
        assert!(names.contains(&"b.PNG".to_string()));
        assert!(!names.iter().any(|n| n.starts_with("Tetris")));
        assert!(
            !names.iter().any(|n| n.starts_with("Portal 2 ")),
            "belongs to Portal 2"
        );
    }

    #[test]
    fn the_folder_is_saved() {
        let db = Db::open_in_memory().unwrap();
        let id = db
            .upsert_detected(&crate::model::DetectedGame::manual("A", "/g/a.exe"), 1)
            .unwrap()
            .game_id;
        db.set_screenshots_dir(id, Some(Path::new("/pics")))
            .unwrap();
        assert_eq!(
            db.screenshots_dir(id).unwrap(),
            Some(PathBuf::from("/pics"))
        );
    }
}
