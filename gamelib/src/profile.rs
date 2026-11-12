//! The local profile: name, bio, picture, banner, badges and showcase.
//!
//! There is always exactly one profile. It lives on this PC only.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{Db, keys};
use crate::shop::{self, Acquired, ItemKind};

pub const DEFAULT_NAME: &str = "Player";
pub const MAX_NAME_CHARS: usize = 32;
pub const MAX_BIO_CHARS: usize = 160;
pub const SHOWCASE_SLOTS: usize = 4;
pub const BADGE_SLOTS: usize = 3;

/// What the profile banner shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Banner {
    /// An image the user uploaded.
    Upload(PathBuf),
    /// A game's art.
    Game(i64),
    /// An earned or bought banner (a shop item id).
    Item(String),
}

impl Banner {
    /// The text stored in the database: `upload:<path>`, `game:<id>` or `item:<id>`.
    pub fn to_text(&self) -> String {
        match self {
            Banner::Upload(path) => format!("upload:{}", path.to_string_lossy()),
            Banner::Game(id) => format!("game:{id}"),
            Banner::Item(id) => format!("item:{id}"),
        }
    }

    pub fn parse(text: &str) -> Result<Banner> {
        let Some((kind, value)) = text.split_once(':') else {
            bail!("bad banner value '{text}'");
        };
        Ok(match kind {
            "upload" => Banner::Upload(PathBuf::from(value)),
            "game" => Banner::Game(value.parse().context("bad game id in banner")?),
            "item" => Banner::Item(value.to_string()),
            _ => bail!("bad banner value '{text}'"),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub display_name: String,
    /// False until the free first name has been chosen.
    pub name_chosen: bool,
    pub bio: Option<String>,
    pub avatar_path: Option<PathBuf>,
    pub banner: Option<Banner>,
    /// The linked Steam account (a 17-digit SteamID64), for the friends list.
    pub steam_id: Option<String>,
    pub created_at: i64,
}

/// A banner unlocked automatically by a milestone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EarnedBanner {
    /// The shop catalogue id of the banner.
    pub item_id: &'static str,
    pub goal: Goal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Goal {
    HoursPlayed(u64),
    GamesFinished(u64),
}

impl Goal {
    pub fn is_met(self, total_playtime_secs: u64, finished_games: u64) -> bool {
        match self {
            Goal::HoursPlayed(hours) => total_playtime_secs >= hours * 3600,
            Goal::GamesFinished(count) => finished_games >= count,
        }
    }

    /// Text for a locked banner, e.g. "Play for 100 hours".
    pub fn describe(self) -> String {
        match self {
            Goal::HoursPlayed(hours) => format!("Play for {hours} hours"),
            Goal::GamesFinished(count) => format!("Finish {count} games"),
        }
    }
}

pub const EARNED_BANNERS: &[EarnedBanner] = &[
    EarnedBanner {
        item_id: "banner_100h",
        goal: Goal::HoursPlayed(100),
    },
    EarnedBanner {
        item_id: "banner_250h",
        goal: Goal::HoursPlayed(250),
    },
    EarnedBanner {
        item_id: "banner_500h",
        goal: Goal::HoursPlayed(500),
    },
    EarnedBanner {
        item_id: "banner_10_finished",
        goal: Goal::GamesFinished(10),
    },
];

/// Which earned banners these totals unlock. A pure function, so the rules
/// can be tested without a database.
pub fn earned_banners(total_playtime_secs: u64, finished_games: u64) -> Vec<&'static str> {
    EARNED_BANNERS
        .iter()
        .filter(|b| b.goal.is_met(total_playtime_secs, finished_games))
        .map(|b| b.item_id)
        .collect()
}

/// A SteamID64 is 17 digits.
pub fn is_valid_steam_id(steam_id: &str) -> bool {
    steam_id.len() == 17 && steam_id.chars().all(|c| c.is_ascii_digit())
}

/// Copy a chosen picture into GameLib's own `profile` folder, so it keeps
/// working if the original is moved or deleted. `name` is the file name to
/// give it, without the extension (e.g. "avatar").
pub fn import_image(source: &Path, profile_dir: &Path, name: &str) -> Result<PathBuf> {
    let extension = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    let dest = profile_dir.join(format!("{name}.{extension}"));
    std::fs::copy(source, &dest)
        .with_context(|| format!("couldn't copy {} to {}", source.display(), dest.display()))?;
    Ok(dest)
}

impl Db {
    /// The profile. The first call creates it, named "Player", and hands
    /// out the Founder badge.
    pub fn profile(&self, now: i64) -> Result<Profile> {
        let created = self.conn.execute(
            "INSERT OR IGNORE INTO profile(id, display_name, created_at) VALUES (1, ?1, ?2)",
            params![DEFAULT_NAME, now],
        )?;
        if created > 0 {
            self.grant_item(shop::FOUNDER_BADGE_ID, Acquired::Gift, now)?;
        }

        let profile = self.conn.query_row(
            "SELECT display_name, name_chosen, bio, avatar_path, banner, steam_id, created_at
             FROM profile WHERE id = 1",
            [],
            |r| {
                Ok(Profile {
                    display_name: r.get(0)?,
                    name_chosen: r.get(1)?,
                    bio: r.get(2)?,
                    avatar_path: r.get::<_, Option<String>>(3)?.map(PathBuf::from),
                    // Parsed just below, where a bad value can be reported.
                    banner: None,
                    steam_id: r.get(5)?,
                    created_at: r.get(6)?,
                })
            },
        )?;
        let banner_text: Option<String> =
            self.conn
                .query_row("SELECT banner FROM profile WHERE id = 1", [], |r| r.get(0))?;
        Ok(Profile {
            banner: banner_text.as_deref().map(Banner::parse).transpose()?,
            ..profile
        })
    }

    /// Change the display name. **The first change is free; later ones use
    /// up a "Name change" bought in the shop.** Returns true if a credit
    /// was spent.
    pub fn set_display_name(&self, name: &str, now: i64) -> Result<bool> {
        let name = name.trim();
        anyhow::ensure!(!name.is_empty(), "the name can't be empty");
        anyhow::ensure!(
            name.chars().count() <= MAX_NAME_CHARS,
            "the name can be at most {MAX_NAME_CHARS} characters"
        );
        let profile = self.profile(now)?;
        if profile.name_chosen && profile.display_name == name {
            return Ok(false); // nothing to change, nothing to charge
        }

        let tx = self.conn.unchecked_transaction()?;
        let mut spent_credit = false;
        if profile.name_chosen {
            let credits = shop::name_change_credits(&tx)?;
            anyhow::ensure!(
                credits > 0,
                "changing your name again needs a Name change from the shop"
            );
            tx.execute(
                "UPDATE settings SET value = ?2 WHERE key = ?1",
                params![keys::NAME_CHANGE_CREDITS, (credits - 1).to_string()],
            )?;
            spent_credit = true;
        }
        tx.execute(
            "UPDATE profile SET display_name = ?1, name_chosen = 1 WHERE id = 1",
            [name],
        )?;
        tx.commit()?;
        Ok(spent_credit)
    }

    /// Set the bio (up to 160 characters). An empty bio clears it.
    pub fn set_bio(&self, bio: &str, now: i64) -> Result<()> {
        let bio = bio.trim();
        anyhow::ensure!(
            bio.chars().count() <= MAX_BIO_CHARS,
            "the bio can be at most {MAX_BIO_CHARS} characters"
        );
        self.profile(now)?;
        let value = if bio.is_empty() { None } else { Some(bio) };
        self.conn
            .execute("UPDATE profile SET bio = ?1 WHERE id = 1", [value])?;
        Ok(())
    }

    pub fn set_avatar_path(&self, path: Option<&Path>, now: i64) -> Result<()> {
        self.profile(now)?;
        let value = path.map(|p| p.to_string_lossy().into_owned());
        self.conn
            .execute("UPDATE profile SET avatar_path = ?1 WHERE id = 1", [value])?;
        Ok(())
    }

    /// Set (or clear) the banner. An item banner must be an owned profile
    /// banner; a game banner must be a game in the library.
    pub fn set_banner(&self, banner: Option<&Banner>, now: i64) -> Result<()> {
        self.profile(now)?;
        match banner {
            Some(Banner::Item(item_id)) => {
                self.require_owned(item_id, ItemKind::ProfileBanner)?;
            }
            Some(Banner::Game(game_id)) => {
                self.require_game(*game_id)?;
            }
            Some(Banner::Upload(_)) | None => {}
        }
        self.conn.execute(
            "UPDATE profile SET banner = ?1 WHERE id = 1",
            [banner.map(Banner::to_text)],
        )?;
        Ok(())
    }

    /// Link (or unlink) the Steam account used for the friends list.
    pub fn set_steam_id(&self, steam_id: Option<&str>, now: i64) -> Result<()> {
        self.profile(now)?;
        let steam_id = steam_id.map(str::trim).filter(|s| !s.is_empty());
        if let Some(id) = steam_id {
            anyhow::ensure!(
                is_valid_steam_id(id),
                "a SteamID is 17 digits, like 76561198000000000"
            );
        }
        self.conn
            .execute("UPDATE profile SET steam_id = ?1 WHERE id = 1", [steam_id])?;
        Ok(())
    }

    /// The showcased games, in slot order.
    pub fn showcase(&self) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT game_id FROM profile_showcase ORDER BY slot")?;
        let ids = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(ids)
    }

    /// Choose the showcase: up to 4 different games.
    pub fn set_showcase(&self, game_ids: &[i64]) -> Result<()> {
        anyhow::ensure!(
            game_ids.len() <= SHOWCASE_SLOTS,
            "the showcase holds at most {SHOWCASE_SLOTS} games"
        );
        anyhow::ensure!(
            !has_duplicates(game_ids),
            "a game can only be showcased once"
        );
        for id in game_ids {
            self.require_game(*id)?;
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM profile_showcase", [])?;
        for (index, game_id) in game_ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO profile_showcase(slot, game_id) VALUES (?1, ?2)",
                params![index as i64 + 1, game_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The badges shown next to the name, in slot order.
    pub fn badges(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT item_id FROM profile_badges ORDER BY slot")?;
        let ids = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(ids)
    }

    /// Choose which badges to show: up to 3 different, owned badges.
    pub fn set_badges(&self, item_ids: &[&str]) -> Result<()> {
        anyhow::ensure!(
            item_ids.len() <= BADGE_SLOTS,
            "you can show at most {BADGE_SLOTS} badges"
        );
        anyhow::ensure!(!has_duplicates(item_ids), "a badge can only be shown once");
        for id in item_ids {
            self.require_owned(id, ItemKind::Badge)?;
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM profile_badges", [])?;
        for (index, item_id) in item_ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO profile_badges(slot, item_id) VALUES (?1, ?2)",
                params![index as i64 + 1, item_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Hand out any earned banners the current totals have unlocked.
    /// Returns the ids of the ones that are new this time.
    pub fn grant_earned_banners(&self, now: i64) -> Result<Vec<&'static str>> {
        let total_playtime: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(playtime_secs), 0) FROM games",
                [],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        let finished = self.finished_count()?;

        let mut newly_granted = Vec::new();
        for item_id in earned_banners(total_playtime.max(0) as u64, finished) {
            if self.grant_item(item_id, Acquired::Earned, now)? {
                newly_granted.push(item_id);
            }
        }
        Ok(newly_granted)
    }
}

fn has_duplicates<T: PartialEq>(items: &[T]) -> bool {
    items
        .iter()
        .enumerate()
        .any(|(i, item)| items[..i].contains(item))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DetectedGame;

    const HOUR: u64 = 3600;

    fn add_game(db: &Db, name: &str) -> i64 {
        db.upsert_detected(
            &DetectedGame::manual(name, format!("/g/{name}/{name}.exe")),
            1,
        )
        .unwrap()
        .game_id
    }

    #[test]
    fn earned_banner_rules() {
        assert!(earned_banners(0, 0).is_empty());
        assert!(earned_banners(100 * HOUR - 1, 9).is_empty());
        assert_eq!(earned_banners(100 * HOUR, 0), ["banner_100h"]);
        assert_eq!(
            earned_banners(260 * HOUR, 0),
            ["banner_100h", "banner_250h"]
        );
        assert_eq!(earned_banners(0, 10), ["banner_10_finished"]);
        assert_eq!(earned_banners(500 * HOUR, 12).len(), 4);
    }

    #[test]
    fn every_earned_banner_is_in_the_catalogue_and_not_for_sale() {
        for banner in EARNED_BANNERS {
            let item = shop::find_item(banner.item_id).expect("in catalogue");
            assert_eq!(item.kind, ItemKind::ProfileBanner);
            assert_eq!(item.price, None);
        }
    }

    #[test]
    fn banner_text_round_trips() {
        for banner in [
            Banner::Upload(PathBuf::from("C:\\pics\\me.png")),
            Banner::Game(42),
            Banner::Item("banner_nebula".into()),
        ] {
            assert_eq!(Banner::parse(&banner.to_text()).unwrap(), banner);
        }
        assert!(Banner::parse("nonsense").is_err());
        assert!(Banner::parse("game:abc").is_err());
    }

    #[test]
    fn there_is_only_ever_one_profile_row() {
        let db = Db::open_in_memory().unwrap();
        let first = db.profile(100).unwrap();
        let second = db.profile(999).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.display_name, DEFAULT_NAME);
        assert_eq!(first.created_at, 100);

        let rows: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM profile", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
        // The table itself refuses a second row.
        assert!(
            db.conn
                .execute(
                    "INSERT INTO profile(id, display_name, created_at) VALUES (2, 'x', 1)",
                    []
                )
                .is_err()
        );
    }

    #[test]
    fn creating_the_profile_gives_the_founder_badge() {
        let db = Db::open_in_memory().unwrap();
        db.profile(1).unwrap();
        assert!(db.owns_item(shop::FOUNDER_BADGE_ID).unwrap());
    }

    #[test]
    fn profile_fields_save_and_load() {
        let db = Db::open_in_memory().unwrap();
        db.set_bio("  I like games.  ", 1).unwrap();
        db.set_avatar_path(Some(Path::new("/p/avatar.png")), 1)
            .unwrap();
        db.set_steam_id(Some("76561198000000000"), 1).unwrap();
        db.set_banner(Some(&Banner::Upload(PathBuf::from("/p/banner.jpg"))), 1)
            .unwrap();

        let p = db.profile(1).unwrap();
        assert_eq!(p.bio.as_deref(), Some("I like games."));
        assert_eq!(p.avatar_path, Some(PathBuf::from("/p/avatar.png")));
        assert_eq!(p.steam_id.as_deref(), Some("76561198000000000"));
        assert_eq!(
            p.banner,
            Some(Banner::Upload(PathBuf::from("/p/banner.jpg")))
        );

        db.set_bio("", 1).unwrap();
        db.set_steam_id(None, 1).unwrap();
        let p = db.profile(1).unwrap();
        assert_eq!(p.bio, None);
        assert_eq!(p.steam_id, None);
    }

    #[test]
    fn bio_and_steam_id_are_validated() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.set_bio(&"x".repeat(160), 1).is_ok());
        assert!(db.set_bio(&"x".repeat(161), 1).is_err());
        assert!(db.set_steam_id(Some("12345"), 1).is_err());
        assert!(db.set_steam_id(Some("7656119800000000x"), 1).is_err());
    }

    #[test]
    fn first_name_is_free_then_needs_a_name_change() {
        let db = Db::open_in_memory().unwrap();
        assert!(
            !db.set_display_name("Morgan", 1).unwrap(),
            "first one is free"
        );
        assert_eq!(db.profile(1).unwrap().display_name, "Morgan");

        // Setting the same name again costs nothing.
        assert!(!db.set_display_name("Morgan", 1).unwrap());

        // A different name needs a credit…
        assert!(db.set_display_name("Nova", 1).is_err());
        assert_eq!(db.profile(1).unwrap().display_name, "Morgan");

        // …which comes from the shop.
        db.conn
            .execute(
                "INSERT INTO stardust_ledger(amount, reason, created_at) VALUES (500, 'playtime', 0)",
                [],
            )
            .unwrap();
        db.purchase(shop::NAME_CHANGE_ID, 2).unwrap();
        assert!(db.set_display_name("Nova", 3).unwrap());
        assert_eq!(db.profile(3).unwrap().display_name, "Nova");
        assert_eq!(db.name_change_credits().unwrap(), 0);
        assert!(db.set_display_name("Third", 4).is_err());
    }

    #[test]
    fn names_are_validated() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.set_display_name("   ", 1).is_err());
        assert!(
            db.set_display_name(&"n".repeat(MAX_NAME_CHARS + 1), 1)
                .is_err()
        );
        assert!(!db.profile(1).unwrap().name_chosen);
    }

    #[test]
    fn showcase_holds_up_to_four_different_games() {
        let db = Db::open_in_memory().unwrap();
        let ids: Vec<i64> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|n| add_game(&db, n))
            .collect();

        db.set_showcase(&[ids[2], ids[0]]).unwrap();
        assert_eq!(db.showcase().unwrap(), [ids[2], ids[0]]);

        assert!(db.set_showcase(&ids).is_err(), "five is too many");
        assert!(db.set_showcase(&[ids[0], ids[0]]).is_err(), "no duplicates");
        assert!(db.set_showcase(&[9999]).is_err(), "must be a real game");
        assert_eq!(
            db.showcase().unwrap(),
            [ids[2], ids[0]],
            "failed changes leave it alone"
        );

        // A deleted game drops out of the showcase.
        db.delete_game(ids[2]).unwrap();
        assert_eq!(db.showcase().unwrap(), [ids[0]]);
    }

    #[test]
    fn only_owned_badges_can_be_shown() {
        let db = Db::open_in_memory().unwrap();
        db.profile(1).unwrap();
        db.set_badges(&[shop::FOUNDER_BADGE_ID]).unwrap();
        assert_eq!(db.badges().unwrap(), [shop::FOUNDER_BADGE_ID]);

        assert!(db.set_badges(&["badge_night_owl"]).is_err(), "not owned");
        assert!(db.set_badges(&["bg_aurora"]).is_err(), "not a badge");
        assert!(
            db.set_badges(&[shop::FOUNDER_BADGE_ID, shop::FOUNDER_BADGE_ID])
                .is_err()
        );
        assert_eq!(db.badges().unwrap(), [shop::FOUNDER_BADGE_ID]);

        db.set_badges(&[]).unwrap();
        assert!(db.badges().unwrap().is_empty());
    }

    #[test]
    fn item_banners_must_be_owned() {
        let db = Db::open_in_memory().unwrap();
        let nebula = Banner::Item("banner_nebula".into());
        assert!(db.set_banner(Some(&nebula), 1).is_err());
        db.grant_item("banner_nebula", Acquired::Gift, 1).unwrap();
        db.set_banner(Some(&nebula), 1).unwrap();
        assert_eq!(db.profile(1).unwrap().banner, Some(nebula));

        assert!(db.set_banner(Some(&Banner::Game(404)), 1).is_err());
        db.set_banner(None, 1).unwrap();
        assert_eq!(db.profile(1).unwrap().banner, None);
    }

    #[test]
    fn earned_banners_are_granted_once_when_the_goal_is_reached() {
        let db = Db::open_in_memory().unwrap();
        let game = add_game(&db, "long");
        assert!(db.grant_earned_banners(1).unwrap().is_empty());

        let s = db.start_session(game, 0).unwrap();
        db.finish_session(s, 100 * HOUR as i64).unwrap();
        assert_eq!(db.grant_earned_banners(2).unwrap(), ["banner_100h"]);
        assert!(
            db.grant_earned_banners(3).unwrap().is_empty(),
            "not granted twice"
        );
        assert!(db.owns_item("banner_100h").unwrap());
    }

    #[test]
    fn imported_images_are_copied_into_the_profile_folder() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("Me.PNG");
        std::fs::write(&source, b"picture").unwrap();
        let profile_dir = dir.path().join("profile");
        std::fs::create_dir(&profile_dir).unwrap();

        let dest = import_image(&source, &profile_dir, "avatar").unwrap();
        assert_eq!(dest, profile_dir.join("avatar.png"));
        assert_eq!(std::fs::read(dest).unwrap(), b"picture");
    }
}
