//! Core types shared by every other module.

use std::path::PathBuf;

use anyhow::{Result, bail};

/// Where a game came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Steam,
    Epic,
    Gog,
    BattleNet,
    Riot,
    Ea,
    Folder,
    Manual,
}

impl Source {
    pub const ALL: [Source; 8] = [
        Source::Steam,
        Source::Epic,
        Source::Gog,
        Source::BattleNet,
        Source::Riot,
        Source::Ea,
        Source::Folder,
        Source::Manual,
    ];

    /// The text stored in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Steam => "steam",
            Source::Epic => "epic",
            Source::Gog => "gog",
            Source::BattleNet => "battlenet",
            Source::Riot => "riot",
            Source::Ea => "ea",
            Source::Folder => "folder",
            Source::Manual => "manual",
        }
    }

    pub fn parse(s: &str) -> Result<Source> {
        Ok(match s {
            "steam" => Source::Steam,
            "epic" => Source::Epic,
            "gog" => Source::Gog,
            "battlenet" => Source::BattleNet,
            "riot" => Source::Riot,
            "ea" => Source::Ea,
            "folder" => Source::Folder,
            "manual" => Source::Manual,
            other => bail!("unknown game source '{other}'"),
        })
    }
}

/// How to start the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchTarget {
    /// Run an .exe directly (GOG, folder scan, manual).
    Exe {
        path: PathBuf,
        args: Vec<String>,
        working_dir: Option<PathBuf>,
    },
    /// Ask a launcher to start it (Steam, Epic), e.g. `steam://rungameid/570`.
    Uri(String),
}

impl LaunchTarget {
    /// A plain "run this exe" target with no arguments.
    pub fn exe(path: impl Into<PathBuf>) -> LaunchTarget {
        LaunchTarget::Exe {
            path: path.into(),
            args: Vec::new(),
            working_dir: None,
        }
    }
}

/// A game as stored in the library.
#[derive(Debug, Clone, PartialEq)]
pub struct Game {
    /// Database id.
    pub id: i64,
    pub title: String,
    pub source: Source,
    /// Steam app id, Epic AppName, GOG id, or exe path.
    pub source_id: String,
    pub install_dir: Option<PathBuf>,
    pub launch: LaunchTarget,
    pub favourite: bool,
    pub hidden: bool,
    /// False when a rescan could no longer find the game. It keeps its
    /// history and is shown greyed out instead of being deleted.
    pub installed: bool,
    pub playtime_secs: u64,
    /// Unix timestamp.
    pub last_played: Option<i64>,
    pub added_at: i64,
    pub cover_path: Option<PathBuf>,
    /// The game's own icon, read out of its exe and saved as a PNG.
    pub icon_path: Option<PathBuf>,
    pub description: Option<String>,
    pub genres: Vec<String>,
    pub release_year: Option<u16>,
}

impl Game {
    /// The folder to watch for running processes while tracking playtime:
    /// the install folder, or failing that the folder the exe lives in.
    pub fn watch_dir(&self) -> Option<PathBuf> {
        if let Some(dir) = &self.install_dir {
            return Some(dir.clone());
        }
        match &self.launch {
            LaunchTarget::Exe { path, .. } => path.parent().map(|p| p.to_path_buf()),
            LaunchTarget::Uri(_) => None,
        }
    }
}

/// A game found by a scan, before it has been saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedGame {
    pub title: String,
    pub source: Source,
    pub source_id: String,
    pub install_dir: Option<PathBuf>,
    pub launch: LaunchTarget,
}

impl DetectedGame {
    /// A game added by hand: the exe path doubles as its unique id.
    pub fn manual(title: impl Into<String>, exe: impl Into<PathBuf>) -> DetectedGame {
        let exe: PathBuf = exe.into();
        DetectedGame {
            title: title.into(),
            source: Source::Manual,
            source_id: exe.to_string_lossy().into_owned(),
            install_dir: exe.parent().map(|p| p.to_path_buf()),
            launch: LaunchTarget::exe(exe),
        }
    }
}

/// A tag. Collections are tags with `is_collection` set, so they also show
/// in the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub is_collection: bool,
}

/// One stretch of playing a game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaySession {
    pub id: i64,
    pub game_id: i64,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub duration_secs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_text_round_trips() {
        for source in Source::ALL {
            assert_eq!(Source::parse(source.as_str()).unwrap(), source);
        }
        assert!(Source::parse("origin").is_err());
    }

    #[test]
    fn manual_game_uses_exe_folder_as_install_dir() {
        let g = DetectedGame::manual("Ember Road", "/games/ember/ember.exe");
        assert_eq!(g.source, Source::Manual);
        assert_eq!(g.source_id, "/games/ember/ember.exe");
        assert_eq!(g.install_dir, Some(PathBuf::from("/games/ember")));
    }
}
