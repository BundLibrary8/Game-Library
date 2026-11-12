---
tags: [gamelib, design]
status: draft-needs-review
section: 2
---
# 03 Data Model (Section 2, draft)

Back to [[00 Home]] · Prev: [[02 Architecture]] · Next: [[04 Game Sources]]

> [!warning] Draft
> Not agreed yet. Review it and note any changes in [[12 Decisions Log]].

## In Rust (`model.rs`)
```rust
pub enum Source { Steam, Epic, Gog, Folder, Manual }

/// How to start the game.
pub enum LaunchTarget {
    /// Run an .exe directly (GOG, folder scan, manual)
    Exe { path: PathBuf, args: Vec<String>, working_dir: Option<PathBuf> },
    /// Ask a launcher to start it (Steam, Epic), e.g. "steam://rungameid/570"
    Uri(String),
}

pub struct Game {
    pub id: i64,                    // database id
    pub title: String,
    pub source: Source,
    pub source_id: String,          // Steam app id, Epic AppName, GOG id, or exe path
    pub install_dir: Option<PathBuf>,
    pub launch: LaunchTarget,
    pub favourite: bool,
    pub hidden: bool,
    pub playtime_secs: u64,
    pub last_played: Option<i64>,   // unix timestamp
    pub added_at: i64,
    pub cover_path: Option<PathBuf>,
    pub description: Option<String>,
    pub genres: Vec<String>,
    pub release_year: Option<u16>,
}
```
`enum` plus `match` is the Rust feature that makes this tidy: the compiler forces you to handle every source and launch type.

## In SQLite (`db.rs`)
```sql
games(
  id INTEGER PRIMARY KEY,
  title TEXT NOT NULL,
  source TEXT NOT NULL,            -- 'steam' | 'epic' | 'gog' | 'folder' | 'manual'
  source_id TEXT NOT NULL,
  install_dir TEXT,
  launch_kind TEXT NOT NULL,       -- 'exe' | 'uri'
  launch_value TEXT NOT NULL,      -- exe path or URI
  launch_args TEXT,                -- JSON array
  favourite INTEGER NOT NULL DEFAULT 0,
  hidden INTEGER NOT NULL DEFAULT 0,
  playtime_secs INTEGER NOT NULL DEFAULT 0,
  last_played INTEGER,
  added_at INTEGER NOT NULL,
  cover_path TEXT,
  description TEXT,
  genres TEXT,                     -- JSON array
  release_year INTEGER,
  UNIQUE(source, source_id)        -- stops duplicates on rescan
)

tags(id INTEGER PRIMARY KEY, name TEXT UNIQUE NOT NULL, is_collection INTEGER NOT NULL DEFAULT 0)
game_tags(game_id INTEGER, tag_id INTEGER, PRIMARY KEY(game_id, tag_id))

play_sessions(id INTEGER PRIMARY KEY, game_id INTEGER NOT NULL,
              started_at INTEGER NOT NULL, ended_at INTEGER, duration_secs INTEGER)

settings(key TEXT PRIMARY KEY, value TEXT)   -- API key, scan folders, enabled sources
```

### Design choices to check
- **Collections are tags with `is_collection = 1`.** "Backlog" is a tag that also appears in the sidebar. That gives one system instead of two and less code. *Open question in [[12 Decisions Log]].*
- **Rescans never delete your data.** The `UNIQUE(source, source_id)` rule means a rescan updates the title, path and launch info, but keeps favourites, tags, playtime and covers.
- **Uninstalled games** aren't deleted automatically. They're shown greyed out ("not installed"), so you keep their playtime history.
- **Playtime is stored twice on purpose**: per session in `play_sessions`, for stats, and as a running total in `games.playtime_secs`, so the grid loads fast.
- **Schema versioning**: `PRAGMA user_version` stores the schema version. On startup the app runs any upgrade steps it hasn't run yet.

## Where files live
```
%APPDATA%\gamelib\
├─ library.db
└─ covers\<game id>.jpg
```
