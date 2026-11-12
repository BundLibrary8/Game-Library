//! Read-only Steam friends list.
//!
//! Uses two Steam Web API calls: `GetFriendList` (your friends' SteamIDs)
//! and `GetPlayerSummaries` (their names, avatars and what they're playing).
//! Results are cached in the `friends_cache` table so the panel has
//! something to show straight away, even offline.
//!
//! Needs a free Steam Web API key and your SteamID, and your Steam friends
//! list must not be private. The key is stored in Settings, never in code.

use anyhow::{Context, Result};
use rusqlite::params;
use serde::Deserialize;

use crate::db::Db;

const FRIEND_LIST_URL: &str = "https://api.steampowered.com/ISteamUser/GetFriendList/v0001/";
const SUMMARIES_URL: &str = "https://api.steampowered.com/ISteamUser/GetPlayerSummaries/v0002/";
/// `GetPlayerSummaries` accepts at most this many SteamIDs per call.
const SUMMARIES_BATCH: usize = 100;
/// Don't refresh automatically more often than this.
pub const MIN_REFRESH_SECS: i64 = 5 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FriendState {
    Offline,
    Online,
    Busy,
    Away,
    Snooze,
    LookingToTrade,
    LookingToPlay,
}

impl FriendState {
    /// From Steam's `personastate` number.
    pub fn from_steam(code: i64) -> FriendState {
        match code {
            1 => FriendState::Online,
            2 => FriendState::Busy,
            3 => FriendState::Away,
            4 => FriendState::Snooze,
            5 => FriendState::LookingToTrade,
            6 => FriendState::LookingToPlay,
            _ => FriendState::Offline,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            FriendState::Offline => "offline",
            FriendState::Online => "online",
            FriendState::Busy => "busy",
            FriendState::Away => "away",
            FriendState::Snooze => "snooze",
            FriendState::LookingToTrade => "looking_to_trade",
            FriendState::LookingToPlay => "looking_to_play",
        }
    }

    pub fn parse(text: &str) -> FriendState {
        match text {
            "online" => FriendState::Online,
            "busy" => FriendState::Busy,
            "away" => FriendState::Away,
            "snooze" => FriendState::Snooze,
            "looking_to_trade" => FriendState::LookingToTrade,
            "looking_to_play" => FriendState::LookingToPlay,
            _ => FriendState::Offline,
        }
    }

    pub fn is_online(self) -> bool {
        self != FriendState::Offline
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Friend {
    pub steam_id: String,
    pub name: String,
    pub avatar_url: Option<String>,
    pub state: FriendState,
    /// The game they're in right now, if any.
    pub playing: Option<String>,
    /// When this information was fetched.
    pub updated_at: i64,
}

// --- The shapes of Steam's JSON. Only the fields we use are listed; serde
// --- ignores the rest.

#[derive(Deserialize)]
struct FriendListResponse {
    friendslist: FriendList,
}
#[derive(Deserialize)]
struct FriendList {
    friends: Vec<FriendEntry>,
}
#[derive(Deserialize)]
struct FriendEntry {
    steamid: String,
}

#[derive(Deserialize)]
struct SummariesResponse {
    response: SummariesInner,
}
#[derive(Deserialize)]
struct SummariesInner {
    players: Vec<PlayerSummary>,
}
#[derive(Deserialize)]
struct PlayerSummary {
    steamid: String,
    personaname: String,
    avatarmedium: Option<String>,
    #[serde(default)]
    personastate: i64,
    gameextrainfo: Option<String>,
}

/// Pull the SteamIDs out of a `GetFriendList` response.
pub fn parse_friend_list(json: &str) -> Result<Vec<String>> {
    let parsed: FriendListResponse =
        serde_json::from_str(json).context("unexpected reply from Steam's friend list")?;
    Ok(parsed
        .friendslist
        .friends
        .into_iter()
        .map(|f| f.steamid)
        .collect())
}

/// Turn a `GetPlayerSummaries` response into friends.
pub fn parse_player_summaries(json: &str, now: i64) -> Result<Vec<Friend>> {
    let parsed: SummariesResponse =
        serde_json::from_str(json).context("unexpected reply from Steam's player summaries")?;
    Ok(parsed
        .response
        .players
        .into_iter()
        .map(|p| Friend {
            steam_id: p.steamid,
            name: p.personaname,
            avatar_url: p.avatarmedium,
            state: FriendState::from_steam(p.personastate),
            playing: p.gameextrainfo,
            updated_at: now,
        })
        .collect())
}

/// Order for the friends panel: in a game first, then online, then
/// offline; alphabetical within each group.
pub fn sort_friends(friends: &mut [Friend]) {
    fn group(friend: &Friend) -> u8 {
        if friend.playing.is_some() {
            0
        } else if friend.state.is_online() {
            1
        } else {
            2
        }
    }
    friends.sort_by(|a, b| {
        group(a)
            .cmp(&group(b))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// Fetch the friends list from Steam. Slow (network): call it from the
/// worker thread, never from the UI.
pub fn fetch_friends(
    client: &reqwest::blocking::Client,
    api_key: &str,
    steam_id: &str,
    now: i64,
) -> Result<Vec<Friend>> {
    let list_json = client
        .get(FRIEND_LIST_URL)
        .query(&[
            ("key", api_key),
            ("steamid", steam_id),
            ("relationship", "friend"),
        ])
        .send()
        .context("couldn't reach Steam")?
        .error_for_status()
        .context("Steam refused the friends list (is the key right, and the list public?)")?
        .text()?;
    let ids = parse_friend_list(&list_json)?;

    let mut friends = Vec::new();
    for batch in ids.chunks(SUMMARIES_BATCH) {
        let joined = batch.join(",");
        let json = client
            .get(SUMMARIES_URL)
            .query(&[("key", api_key), ("steamids", joined.as_str())])
            .send()
            .context("couldn't reach Steam")?
            .error_for_status()
            .context("Steam refused the player summaries request")?
            .text()?;
        friends.extend(parse_player_summaries(&json, now)?);
    }
    sort_friends(&mut friends);
    Ok(friends)
}

impl Db {
    /// Replace the cached friends list with a freshly fetched one.
    pub fn save_friends(&self, friends: &[Friend]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM friends_cache", [])?;
        for f in friends {
            tx.execute(
                "INSERT OR REPLACE INTO friends_cache(steam_id, name, avatar_url, state, playing, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![f.steam_id, f.name, f.avatar_url, f.state.as_str(), f.playing, f.updated_at],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The friends list as of the last successful refresh, sorted for display.
    pub fn cached_friends(&self) -> Result<Vec<Friend>> {
        let mut stmt = self.conn.prepare(
            "SELECT steam_id, name, avatar_url, state, playing, updated_at FROM friends_cache",
        )?;
        let mut friends: Vec<Friend> = stmt
            .query_map([], |row| {
                Ok(Friend {
                    steam_id: row.get(0)?,
                    name: row.get(1)?,
                    avatar_url: row.get(2)?,
                    state: FriendState::parse(&row.get::<_, String>(3)?),
                    playing: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        sort_friends(&mut friends);
        Ok(friends)
    }

    /// When the cache was last refreshed (for "updated 2 h ago").
    pub fn friends_updated_at(&self) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row("SELECT MAX(updated_at) FROM friends_cache", [], |row| {
                row.get(0)
            })?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRIEND_LIST: &str = include_str!("../tests/fixtures/steam_friend_list.json");
    const SUMMARIES: &str = include_str!("../tests/fixtures/steam_player_summaries.json");

    #[test]
    fn parses_friend_ids() {
        assert_eq!(
            parse_friend_list(FRIEND_LIST).unwrap(),
            [
                "76561198000000001",
                "76561198000000002",
                "76561198000000003"
            ]
        );
    }

    #[test]
    fn parses_player_summaries() {
        let friends = parse_player_summaries(SUMMARIES, 777).unwrap();
        assert_eq!(friends.len(), 3);

        let ada = friends.iter().find(|f| f.name == "Ada").unwrap();
        assert_eq!(ada.steam_id, "76561198000000002");
        assert_eq!(ada.state, FriendState::Online);
        assert_eq!(ada.playing.as_deref(), Some("Ember Road"));
        assert_eq!(
            ada.avatar_url.as_deref(),
            Some("https://avatars.example/ada_medium.jpg")
        );
        assert_eq!(ada.updated_at, 777);

        let zed = friends.iter().find(|f| f.name == "Zed").unwrap();
        assert_eq!(zed.state, FriendState::Offline);
        assert_eq!(zed.playing, None);

        let bob = friends.iter().find(|f| f.name == "bob").unwrap();
        assert_eq!(bob.state, FriendState::Away);
        assert_eq!(bob.avatar_url, None);
    }

    #[test]
    fn bad_json_is_an_error_not_a_crash() {
        assert!(parse_friend_list("{}").is_err());
        assert!(parse_player_summaries("not json", 1).is_err());
    }

    #[test]
    fn friends_sort_playing_then_online_then_offline() {
        let mut friends = parse_player_summaries(SUMMARIES, 1).unwrap();
        sort_friends(&mut friends);
        let names: Vec<&str> = friends.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Ada", "bob", "Zed"]);
    }

    #[test]
    fn state_text_round_trips() {
        for code in 0..=6 {
            let state = FriendState::from_steam(code);
            assert_eq!(FriendState::parse(state.as_str()), state);
        }
        assert_eq!(FriendState::from_steam(99), FriendState::Offline);
    }

    #[test]
    fn cache_is_replaced_on_each_refresh() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.cached_friends().unwrap().is_empty());
        assert_eq!(db.friends_updated_at().unwrap(), None);

        let friends = parse_player_summaries(SUMMARIES, 500).unwrap();
        db.save_friends(&friends).unwrap();
        let cached = db.cached_friends().unwrap();
        assert_eq!(cached.len(), 3);
        assert_eq!(cached[0].name, "Ada", "sorted for display");
        assert_eq!(cached[0].playing.as_deref(), Some("Ember Road"));
        assert_eq!(db.friends_updated_at().unwrap(), Some(500));

        // Someone was unfriended: the next refresh drops them.
        db.save_friends(&friends[..1]).unwrap();
        assert_eq!(db.cached_friends().unwrap().len(), 1);
    }
}
