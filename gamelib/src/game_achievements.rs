//! The achievement tracker: each game's own achievements.
//!
//! - **Steam games** are read from the Steam Web API (with the key and
//!   Steam account from Settings): which you've unlocked and when
//!   (`GetPlayerAchievements`), their names, descriptions and icons
//!   (`GetSchemaForGame`), and how rare each is (`GetGlobalAchievement
//!   PercentagesForApp`, which needs no key). Your Steam profile's "game
//!   details" must be public for Steam to share them.
//! - **Any other game** can have a checklist you write yourself ("Beat the
//!   final boss", "Find every relic") and tick off.
//!
//! (GameLib's own library-wide achievements, like "Play 100 hours", are in
//! [`crate::achievements`].)

use std::collections::HashMap;

use anyhow::{Context, Result, ensure};
use rusqlite::params;
use serde::Deserialize;

use crate::db::Db;

#[derive(Debug, Clone, PartialEq)]
pub struct GameAchievement {
    pub game_id: i64,
    pub api_name: String,
    pub name: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub achieved: bool,
    pub unlocked_at: Option<i64>,
    /// The share of players who have it, 0–100.
    pub global_percent: Option<f32>,
    /// Written by hand rather than read from Steam.
    pub manual: bool,
}

/// How far through a game's achievements you are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Completion {
    pub unlocked: usize,
    pub total: usize,
}

impl Completion {
    pub fn percent(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.unlocked as f32 * 100.0 / self.total as f32
        }
    }
}

// --- Steam replies ---------------------------------------------------------

#[derive(Deserialize)]
struct PlayerReply {
    playerstats: PlayerStats,
}

#[derive(Deserialize)]
struct PlayerStats {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    achievements: Vec<PlayerAchievement>,
}

#[derive(Deserialize)]
struct PlayerAchievement {
    apiname: String,
    achieved: u8,
    #[serde(default)]
    unlocktime: i64,
}

#[derive(Deserialize)]
struct SchemaReply {
    #[serde(default)]
    game: SchemaGame,
}

#[derive(Deserialize, Default)]
struct SchemaGame {
    #[serde(default, rename = "availableGameStats")]
    stats: SchemaStats,
}

#[derive(Deserialize, Default)]
struct SchemaStats {
    #[serde(default)]
    achievements: Vec<SchemaAchievement>,
}

#[derive(Deserialize)]
struct SchemaAchievement {
    name: String,
    #[serde(default, rename = "displayName")]
    display_name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    icongray: Option<String>,
}

#[derive(Deserialize)]
struct GlobalReply {
    achievementpercentages: GlobalList,
}

#[derive(Deserialize)]
struct GlobalList {
    #[serde(default)]
    achievements: Vec<GlobalAchievement>,
}

#[derive(Deserialize)]
struct GlobalAchievement {
    name: String,
    // Steam sends this as a number in some replies and a string in others.
    percent: serde_json::Value,
}

/// Put Steam's three replies together into one list.
pub fn combine_steam(
    game_id: i64,
    player_json: &str,
    schema_json: &str,
    global_json: Option<&str>,
) -> Result<Vec<GameAchievement>> {
    let player: PlayerReply =
        serde_json::from_str(player_json).context("Steam sent back something unexpected")?;
    ensure!(
        player.playerstats.success,
        "Steam didn't share the achievements ({}). Set your Steam profile's game details to public.",
        player
            .playerstats
            .error
            .unwrap_or_else(|| "no reason given".into())
    );
    let schema: SchemaReply = serde_json::from_str(schema_json).unwrap_or(SchemaReply {
        game: SchemaGame::default(),
    });
    let schema: HashMap<String, SchemaAchievement> = schema
        .game
        .stats
        .achievements
        .into_iter()
        .map(|a| (a.name.clone(), a))
        .collect();
    let global: HashMap<String, f32> = global_json
        .and_then(|json| serde_json::from_str::<GlobalReply>(json).ok())
        .map(|reply| {
            reply
                .achievementpercentages
                .achievements
                .into_iter()
                .filter_map(|a| {
                    let percent = match &a.percent {
                        serde_json::Value::Number(n) => n.as_f64(),
                        serde_json::Value::String(s) => s.parse().ok(),
                        _ => None,
                    }?;
                    Some((a.name, percent as f32))
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(player
        .playerstats
        .achievements
        .into_iter()
        .map(|a| {
            let info = schema.get(&a.apiname);
            let achieved = a.achieved == 1;
            GameAchievement {
                game_id,
                name: info
                    .map(|i| i.display_name.clone())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| a.apiname.clone()),
                description: info
                    .and_then(|i| i.description.clone())
                    .filter(|d| !d.is_empty()),
                icon_url: info.and_then(|i| {
                    if achieved {
                        i.icon.clone()
                    } else {
                        i.icongray.clone().or(i.icon.clone())
                    }
                }),
                achieved,
                unlocked_at: (achieved && a.unlocktime > 0).then_some(a.unlocktime),
                global_percent: global.get(&a.apiname).copied(),
                manual: false,
                api_name: a.apiname,
            }
        })
        .collect())
}

/// Read a Steam game's achievements from the Steam Web API.
pub fn fetch_steam(
    client: &reqwest::blocking::Client,
    api_key: &str,
    steam_id: &str,
    app_id: &str,
    game_id: i64,
) -> Result<Vec<GameAchievement>> {
    let get = |url: &str, query: &[(&str, &str)]| -> Result<String> {
        Ok(client
            .get(url)
            .query(query)
            .send()
            .context("couldn't reach Steam")?
            .text()?)
    };
    let player = get(
        "https://api.steampowered.com/ISteamUserStats/GetPlayerAchievements/v1/",
        &[
            ("key", api_key),
            ("steamid", steam_id),
            ("appid", app_id),
            ("l", "english"),
        ],
    )?;
    let schema = get(
        "https://api.steampowered.com/ISteamUserStats/GetSchemaForGame/v2/",
        &[("key", api_key), ("appid", app_id), ("l", "english")],
    )
    .unwrap_or_default();
    let global = get(
        "https://api.steampowered.com/ISteamUserStats/GetGlobalAchievementPercentagesForApp/v2/",
        &[("gameid", app_id)],
    )
    .ok();
    combine_steam(game_id, &player, &schema, global.as_deref())
}

// --- Storage ---------------------------------------------------------------

fn from_row(row: &rusqlite::Row) -> rusqlite::Result<GameAchievement> {
    Ok(GameAchievement {
        game_id: row.get(0)?,
        api_name: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        icon_url: row.get(4)?,
        achieved: row.get(5)?,
        unlocked_at: row.get(6)?,
        global_percent: row.get::<_, Option<f64>>(7)?.map(|p| p as f32),
        manual: row.get(8)?,
    })
}

const COLUMNS: &str =
    "game_id, api_name, name, description, icon_url, achieved, unlocked_at, global_percent, manual";

impl Db {
    /// Replace a game's Steam achievements (hand-written ones are kept).
    pub fn save_steam_achievements(&self, game_id: i64, list: &[GameAchievement]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM game_achievements WHERE game_id = ?1 AND manual = 0",
            [game_id],
        )?;
        for a in list {
            tx.execute(
                &format!("INSERT OR REPLACE INTO game_achievements({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)"),
                params![
                    game_id,
                    a.api_name,
                    a.name,
                    a.description,
                    a.icon_url,
                    a.achieved,
                    a.unlocked_at,
                    a.global_percent.map(|p| p as f64)
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// A game's achievements: unlocked first (newest first), then the rest
    /// (most common first, so the next easy one is at the top).
    pub fn game_achievements(&self, game_id: i64) -> Result<Vec<GameAchievement>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM game_achievements WHERE game_id = ?1
             ORDER BY achieved DESC, unlocked_at DESC, global_percent DESC, name COLLATE NOCASE"
        ))?;
        let rows = stmt.query_map([game_id], from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Unlocked and total for every game that has achievements.
    pub fn achievement_completion(&self) -> Result<HashMap<i64, Completion>> {
        let mut stmt = self.conn.prepare(
            "SELECT game_id, SUM(achieved), COUNT(*) FROM game_achievements GROUP BY game_id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                Completion {
                    unlocked: row.get::<_, i64>(1)?.max(0) as usize,
                    total: row.get::<_, i64>(2)?.max(0) as usize,
                },
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The most recently unlocked achievements across all games.
    pub fn recent_achievements(&self, limit: usize) -> Result<Vec<GameAchievement>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM game_achievements WHERE achieved = 1
             ORDER BY unlocked_at DESC NULLS LAST LIMIT ?1"
        ))?;
        let rows = stmt.query_map([limit as i64], from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The rarest achievements you have.
    pub fn rarest_achievements(&self, limit: usize) -> Result<Vec<GameAchievement>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM game_achievements
             WHERE achieved = 1 AND global_percent IS NOT NULL
             ORDER BY global_percent ASC LIMIT ?1"
        ))?;
        let rows = stmt.query_map([limit as i64], from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Add a hand-written achievement to a game's checklist.
    pub fn add_manual_achievement(
        &self,
        game_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> Result<()> {
        let name = name.trim();
        ensure!(!name.is_empty(), "give the achievement a name");
        let next: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM game_achievements WHERE game_id = ?1 AND manual = 1",
            [game_id],
            |row| row.get(0),
        )?;
        // Find a free id even if some were deleted.
        let mut n = next + 1;
        loop {
            let taken: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM game_achievements WHERE game_id = ?1 AND api_name = ?2",
                params![game_id, format!("manual-{n}")],
                |row| row.get(0),
            )?;
            if taken == 0 {
                break;
            }
            n += 1;
        }
        self.conn.execute(
            &format!("INSERT INTO game_achievements({COLUMNS}) VALUES (?1, ?2, ?3, ?4, NULL, 0, NULL, NULL, 1)"),
            params![
                game_id,
                format!("manual-{n}"),
                name,
                description.map(str::trim).filter(|d| !d.is_empty())
            ],
        )?;
        Ok(())
    }

    /// Tick (or untick) a hand-written achievement.
    pub fn set_manual_achieved(
        &self,
        game_id: i64,
        api_name: &str,
        achieved: bool,
        now: i64,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE game_achievements SET achieved = ?3, unlocked_at = CASE WHEN ?3 THEN ?4 ELSE NULL END
             WHERE game_id = ?1 AND api_name = ?2 AND manual = 1",
            params![game_id, api_name, achieved, now],
        )?;
        Ok(())
    }

    pub fn delete_manual_achievement(&self, game_id: i64, api_name: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM game_achievements WHERE game_id = ?1 AND api_name = ?2 AND manual = 1",
            params![game_id, api_name],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DetectedGame;

    const PLAYER: &str = include_str!("../tests/fixtures/steam_player_achievements.json");
    const SCHEMA: &str = include_str!("../tests/fixtures/steam_achievement_schema.json");
    const GLOBAL: &str = include_str!("../tests/fixtures/steam_global_achievements.json");

    #[test]
    fn steam_replies_are_combined() {
        let list = combine_steam(7, PLAYER, SCHEMA, Some(GLOBAL)).unwrap();
        assert_eq!(list.len(), 3);
        let first = list
            .iter()
            .find(|a| a.api_name == "ACH_WIN_ONE_GAME")
            .unwrap();
        assert!(first.achieved);
        assert_eq!(first.name, "Winner");
        assert_eq!(first.unlocked_at, Some(1_700_000_000));
        assert_eq!(first.global_percent, Some(67.5));
        assert!(first.icon_url.as_deref().unwrap().ends_with("win.jpg"));
        let locked = list
            .iter()
            .find(|a| a.api_name == "ACH_WIN_100_GAMES")
            .unwrap();
        assert!(!locked.achieved);
        assert!(
            locked
                .icon_url
                .as_deref()
                .unwrap()
                .ends_with("win100_gray.jpg")
        );
        assert_eq!(
            locked.global_percent,
            Some(2.1),
            "string percentages are read too"
        );
        let unknown = list.iter().find(|a| a.api_name == "ACH_SECRET").unwrap();
        assert_eq!(
            unknown.name, "ACH_SECRET",
            "no schema entry: the id is the name"
        );
    }

    #[test]
    fn a_private_profile_gives_a_helpful_error() {
        let private = r#"{"playerstats":{"error":"Profile is not public","success":false}}"#;
        let err = combine_steam(1, private, "{}", None).unwrap_err();
        assert!(format!("{err}").contains("public"));
    }

    #[test]
    fn achievements_are_stored_and_counted() {
        let db = Db::open_in_memory().unwrap();
        let game = db
            .upsert_detected(&DetectedGame::manual("A", "/g/a.exe"), 1)
            .unwrap()
            .game_id;
        let list = combine_steam(game, PLAYER, SCHEMA, Some(GLOBAL)).unwrap();
        db.save_steam_achievements(game, &list).unwrap();
        db.add_manual_achievement(game, "Find every relic", Some("All 40"))
            .unwrap();
        db.add_manual_achievement(game, "Beat it on hard", None)
            .unwrap();
        assert!(db.add_manual_achievement(game, "  ", None).is_err());

        let all = db.game_achievements(game).unwrap();
        assert_eq!(all.len(), 5);
        assert!(all[0].achieved, "unlocked ones first");
        let completion = db.achievement_completion().unwrap()[&game];
        assert_eq!((completion.unlocked, completion.total), (1, 5));

        db.set_manual_achieved(game, "manual-1", true, 99).unwrap();
        assert_eq!(db.achievement_completion().unwrap()[&game].unlocked, 2);
        assert_eq!(
            db.recent_achievements(1).unwrap()[0].name,
            "Winner",
            "newest unlock time first"
        );
        assert_eq!(db.rarest_achievements(5).unwrap()[0].name, "Winner");

        // Refreshing from Steam keeps the hand-written ones.
        db.save_steam_achievements(game, &list[..1]).unwrap();
        assert_eq!(db.game_achievements(game).unwrap().len(), 3);
        db.delete_manual_achievement(game, "manual-2").unwrap();
        db.add_manual_achievement(game, "Another", None).unwrap();
        assert_eq!(db.game_achievements(game).unwrap().len(), 3);
    }
}
