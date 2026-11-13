//! Launch profiles: named sets of extra launch options for a game, like
//! "Windowed" (`-windowed -w 1280 -h 720`) or "Mods" (`--mod-dir mods`).
//! One can be the default; Play uses it unless you pick another.
//!
//! The options are added after the game's own. For a Steam game the
//! profile's options are passed through Steam (`steam://run/<id>//<options>/`);
//! other launchers (Epic, GOG Galaxy links, Battle.net, Riot, EA) don't
//! take options from outside, so a profile does nothing for them.

use anyhow::{Result, ensure};
use rusqlite::params;

use crate::db::Db;
use crate::model::{Game, LaunchTarget, Source};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchProfile {
    pub id: i64,
    pub game_id: i64,
    pub name: String,
    /// The options as typed, e.g. `-windowed "-config=my settings.cfg"`.
    pub args: String,
    pub is_default: bool,
}

/// Split options the way a command line would: on spaces, except inside
/// double quotes.
pub fn split_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        args.push(current);
    }
    args
}

/// Can this game take launch options at all?
pub fn supports_options(game: &Game) -> bool {
    matches!(game.launch, LaunchTarget::Exe { .. }) || game.source == Source::Steam
}

/// The launch target with the profile's options added.
pub fn apply(game: &Game, profile: Option<&LaunchProfile>) -> LaunchTarget {
    let extra = profile.map(|p| split_args(&p.args)).unwrap_or_default();
    if extra.is_empty() {
        return game.launch.clone();
    }
    match &game.launch {
        LaunchTarget::Exe {
            path,
            args,
            working_dir,
        } => LaunchTarget::Exe {
            path: path.clone(),
            args: args.iter().cloned().chain(extra).collect(),
            working_dir: working_dir.clone(),
        },
        LaunchTarget::Uri(_) if game.source == Source::Steam => {
            // Steam reads the options between the two slashes. Spaces
            // and quotes are written as %-codes, which Steam undoes.
            let encode = |arg: &str| {
                let arg = if arg.contains(' ') {
                    format!("\"{arg}\"")
                } else {
                    arg.to_string()
                };
                arg.replace('%', "%25")
                    .replace(' ', "%20")
                    .replace('"', "%22")
                    .replace('/', "%2F")
            };
            let options: Vec<String> = extra.iter().map(|a| encode(a)).collect();
            LaunchTarget::Uri(format!(
                "steam://run/{}//{}/",
                game.source_id,
                options.join("%20")
            ))
        }
        LaunchTarget::Uri(uri) => LaunchTarget::Uri(uri.clone()),
    }
}

fn from_row(row: &rusqlite::Row) -> rusqlite::Result<LaunchProfile> {
    Ok(LaunchProfile {
        id: row.get(0)?,
        game_id: row.get(1)?,
        name: row.get(2)?,
        args: row.get(3)?,
        is_default: row.get(4)?,
    })
}

impl Db {
    pub fn launch_profiles(&self, game_id: i64) -> Result<Vec<LaunchProfile>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, game_id, name, args, is_default FROM launch_profiles
             WHERE game_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map([game_id], from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn launch_profile(&self, profile_id: i64) -> Result<Option<LaunchProfile>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, game_id, name, args, is_default FROM launch_profiles WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map([profile_id], from_row)?;
        Ok(rows.next().transpose()?)
    }

    /// The profile Play uses when none is picked.
    pub fn default_launch_profile(&self, game_id: i64) -> Result<Option<LaunchProfile>> {
        Ok(self
            .launch_profiles(game_id)?
            .into_iter()
            .find(|p| p.is_default))
    }

    /// Add a profile, or change one (`id` set). Returns its id.
    pub fn save_launch_profile(
        &self,
        game_id: i64,
        id: Option<i64>,
        name: &str,
        args: &str,
    ) -> Result<i64> {
        let name = name.trim();
        ensure!(!name.is_empty(), "give the profile a name");
        let args = args.trim();
        Ok(match id {
            Some(id) => {
                self.conn.execute(
                    "UPDATE launch_profiles SET name = ?2, args = ?3 WHERE id = ?1",
                    params![id, name, args],
                )?;
                id
            }
            None => {
                self.conn.execute(
                    "INSERT INTO launch_profiles(game_id, name, args) VALUES (?1, ?2, ?3)",
                    params![game_id, name, args],
                )?;
                self.conn.last_insert_rowid()
            }
        })
    }

    pub fn delete_launch_profile(&self, profile_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM launch_profiles WHERE id = ?1", [profile_id])?;
        Ok(())
    }

    /// Make one profile the default (or none, with `None`).
    pub fn set_default_launch_profile(&self, game_id: i64, profile_id: Option<i64>) -> Result<()> {
        self.conn.execute(
            "UPDATE launch_profiles SET is_default = (id IS ?2) WHERE game_id = ?1",
            params![game_id, profile_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DetectedGame;

    fn steam_game() -> Game {
        Game {
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
        }
    }

    fn profile(args: &str) -> LaunchProfile {
        LaunchProfile {
            id: 1,
            game_id: 1,
            name: "Test".into(),
            args: args.into(),
            is_default: false,
        }
    }

    #[test]
    fn options_split_like_a_command_line() {
        assert_eq!(
            split_args(r#"-windowed  -w 1280 "-config=my file.cfg" """#),
            vec!["-windowed", "-w", "1280", "-config=my file.cfg", ""]
        );
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn exe_games_get_the_options_added() {
        let mut game = steam_game();
        game.source = Source::Manual;
        game.launch = LaunchTarget::Exe {
            path: "/g/a.exe".into(),
            args: vec!["-own".into()],
            working_dir: None,
        };
        match apply(&game, Some(&profile("-windowed -fps 60"))) {
            LaunchTarget::Exe { args, .. } => {
                assert_eq!(args, vec!["-own", "-windowed", "-fps", "60"])
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(apply(&game, None), game.launch);
    }

    #[test]
    fn steam_games_pass_options_through_steam() {
        let game = steam_game();
        assert_eq!(
            apply(&game, Some(&profile(r#"-novid "+exec my.cfg""#))),
            LaunchTarget::Uri("steam://run/400//-novid%20%22+exec%20my.cfg%22/".into())
        );
        assert!(supports_options(&game));
        let mut epic = steam_game();
        epic.source = Source::Epic;
        epic.launch = LaunchTarget::Uri("com.epicgames.launcher://apps/x?action=launch".into());
        assert!(!supports_options(&epic));
        assert_eq!(apply(&epic, Some(&profile("-x"))), epic.launch);
    }

    #[test]
    fn profiles_are_saved_and_one_is_the_default() {
        let db = Db::open_in_memory().unwrap();
        let game = db
            .upsert_detected(&DetectedGame::manual("A", "/g/a.exe"), 1)
            .unwrap()
            .game_id;
        let windowed = db
            .save_launch_profile(game, None, "Windowed", " -windowed ")
            .unwrap();
        let mods = db
            .save_launch_profile(game, None, "Mods", "--mods")
            .unwrap();
        assert!(db.save_launch_profile(game, None, "  ", "").is_err());
        db.set_default_launch_profile(game, Some(mods)).unwrap();
        assert_eq!(db.default_launch_profile(game).unwrap().unwrap().id, mods);
        db.set_default_launch_profile(game, Some(windowed)).unwrap();
        let all = db.launch_profiles(game).unwrap();
        assert_eq!(all.iter().filter(|p| p.is_default).count(), 1);
        assert_eq!(all[0].args, "-windowed");
        db.save_launch_profile(game, Some(windowed), "Window", "-w")
            .unwrap();
        assert_eq!(db.launch_profile(windowed).unwrap().unwrap().name, "Window");
        db.set_default_launch_profile(game, None).unwrap();
        assert_eq!(db.default_launch_profile(game).unwrap(), None);
        db.delete_launch_profile(mods).unwrap();
        assert_eq!(db.launch_profiles(game).unwrap().len(), 1);
    }
}
