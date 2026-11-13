//! What games your Steam friends have.
//!
//! For each friend, Steam's `GetOwnedGames` lists the games they own, with
//! their playtime (overall and in the last two weeks). Steam only shares
//! this for friends whose profile's "game details" are public; for the
//! rest GameLib notes that the list is private.
//!
//! The lists are kept in the database, so the page works offline and
//! Steam isn't asked again until you press Refresh.

use std::collections::HashMap;

use anyhow::{Context, Result};
use rusqlite::params;
use serde::Deserialize;

use crate::db::Db;
use crate::model::{Game, Source};

/// One game a friend owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FriendGame {
    pub steam_id: String,
    pub app_id: String,
    pub name: String,
    pub playtime_minutes: u64,
    /// Played in the last two weeks.
    pub recent_minutes: u64,
}

#[derive(Deserialize)]
struct OwnedReply {
    #[serde(default)]
    response: OwnedResponse,
}

#[derive(Deserialize, Default)]
struct OwnedResponse {
    /// Missing altogether when the profile is private.
    games: Option<Vec<OwnedGame>>,
}

#[derive(Deserialize)]
struct OwnedGame {
    appid: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    playtime_forever: u64,
    #[serde(default)]
    playtime_2weeks: u64,
}

/// Read Steam's reply. `None` means the friend's game list is private.
pub fn parse_owned_games(json: &str, steam_id: &str) -> Result<Option<Vec<FriendGame>>> {
    let reply: OwnedReply =
        serde_json::from_str(json).context("Steam sent back something unexpected")?;
    Ok(reply.response.games.map(|games| {
        games
            .into_iter()
            .filter(|g| !g.name.is_empty())
            .map(|g| FriendGame {
                steam_id: steam_id.to_string(),
                app_id: g.appid.to_string(),
                name: g.name,
                playtime_minutes: g.playtime_forever,
                recent_minutes: g.playtime_2weeks,
            })
            .collect()
    }))
}

/// Ask Steam for one friend's games.
pub fn fetch_owned_games(
    client: &reqwest::blocking::Client,
    api_key: &str,
    steam_id: &str,
) -> Result<Option<Vec<FriendGame>>> {
    let json = client
        .get("https://api.steampowered.com/IPlayerService/GetOwnedGames/v1/")
        .query(&[
            ("key", api_key),
            ("steamid", steam_id),
            ("include_appinfo", "1"),
            ("include_played_free_games", "1"),
        ])
        .send()
        .context("couldn't reach Steam")?
        .error_for_status()
        .context("Steam refused the request (is the Steam Web API key right?)")?
        .text()?;
    parse_owned_games(&json, steam_id)
}

fn simplify(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Which of your games a friend's game is: the same Steam app, or (for
/// games you have from elsewhere) the same title.
pub fn match_library<'g>(friend_game: &FriendGame, library: &'g [Game]) -> Option<&'g Game> {
    library
        .iter()
        .find(|g| g.source == Source::Steam && g.source_id == friend_game.app_id)
        .or_else(|| {
            let wanted = simplify(&friend_game.name);
            library.iter().find(|g| simplify(&g.title) == wanted)
        })
}

impl Db {
    /// Replace a friend's game list. `None` records that it's private.
    pub fn save_friend_games(
        &self,
        steam_id: &str,
        games: Option<&[FriendGame]>,
        now: i64,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM friend_games WHERE steam_id = ?1", [steam_id])?;
        for g in games.unwrap_or_default() {
            tx.execute(
                "INSERT OR REPLACE INTO friend_games(steam_id, app_id, name, playtime_minutes, recent_minutes)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![g.steam_id, g.app_id, g.name, g.playtime_minutes as i64, g.recent_minutes as i64],
            )?;
        }
        tx.execute(
            "INSERT OR REPLACE INTO friend_games_checked(steam_id, checked_at, visible) VALUES (?1, ?2, ?3)",
            params![steam_id, now, games.is_some()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Every friend's games, by friend.
    pub fn all_friend_games(&self) -> Result<HashMap<String, Vec<FriendGame>>> {
        let mut stmt = self.conn.prepare(
            "SELECT steam_id, app_id, name, playtime_minutes, recent_minutes FROM friend_games
             ORDER BY recent_minutes DESC, playtime_minutes DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(FriendGame {
                steam_id: row.get(0)?,
                app_id: row.get(1)?,
                name: row.get(2)?,
                playtime_minutes: row.get::<_, i64>(3)?.max(0) as u64,
                recent_minutes: row.get::<_, i64>(4)?.max(0) as u64,
            })
        })?;
        let mut out: HashMap<String, Vec<FriendGame>> = HashMap::new();
        for row in rows {
            let game = row?;
            out.entry(game.steam_id.clone()).or_default().push(game);
        }
        Ok(out)
    }

    /// Whose game lists are private, and when each friend was last checked.
    pub fn friend_games_checked(&self) -> Result<HashMap<String, (i64, bool)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT steam_id, checked_at, visible FROM friend_games_checked")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, (row.get(1)?, row.get(2)?))))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DetectedGame, LaunchTarget};

    const OWNED: &str = include_str!("../tests/fixtures/steam_owned_games.json");

    #[test]
    fn owned_games_are_read_and_private_lists_noticed() {
        let games = parse_owned_games(OWNED, "42").unwrap().unwrap();
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].name, "Hades");
        assert_eq!(games[0].recent_minutes, 95);
        assert_eq!(parse_owned_games(r#"{"response":{}}"#, "42").unwrap(), None);
    }

    #[test]
    fn friend_games_match_your_library_and_are_stored() {
        let db = Db::open_in_memory().unwrap();
        let mut steam = DetectedGame::manual("Hades", "/g/h.exe");
        steam.source = Source::Steam;
        steam.source_id = "1145360".into();
        steam.launch = LaunchTarget::Uri("steam://rungameid/1145360".into());
        db.upsert_detected(&steam, 1).unwrap();
        db.upsert_detected(&DetectedGame::manual("Stardew Valley", "/g/s.exe"), 1)
            .unwrap();
        let library = db.list_games().unwrap();

        let games = parse_owned_games(OWNED, "42").unwrap().unwrap();
        assert_eq!(match_library(&games[0], &library).unwrap().title, "Hades");
        assert_eq!(
            match_library(&games[1], &library).unwrap().title,
            "Stardew Valley",
            "matched by title"
        );

        db.save_friend_games("42", Some(&games), 5).unwrap();
        db.save_friend_games("43", None, 5).unwrap();
        assert_eq!(db.all_friend_games().unwrap()["42"].len(), 2);
        let checked = db.friend_games_checked().unwrap();
        assert_eq!(checked["42"], (5, true));
        assert_eq!(checked["43"], (5, false));
    }
}
