//! Stardust ✦ and the cosmetics shop.
//!
//! Stardust is earned by playing and spent on cosmetics. Every change is a
//! row in the `stardust_ledger` table (+ earned, − spent) and **the balance
//! is simply the sum of the rows**, so it's always possible to see why the
//! balance is what it is. No real money is involved anywhere.

use std::fmt;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::{Db, keys};

/// ✦ per full hour of tracked playtime.
pub const STARDUST_PER_HOUR: u64 = 10;
/// ✦ for finishing a game (once per game).
pub const FINISH_REWARD: u32 = 50;
/// A game must have this much playtime before finishing it pays out.
pub const FINISH_MIN_PLAYTIME_SECS: u64 = 3600;
/// ✦ for the first game launched each day.
pub const DAILY_REWARD: u32 = 5;
/// The collection that "finishing" a game puts it in.
pub const FINISHED_COLLECTION: &str = "Finished";

const SECS_PER_DAY: i64 = 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    LibraryBackground,
    ProfileBanner,
    Badge,
    /// Used up when spent, so it can be bought more than once.
    NameChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShopItem {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: ItemKind,
    /// `None` means it can't be bought: it is earned or given.
    pub price: Option<u32>,
}

const fn item(
    id: &'static str,
    name: &'static str,
    kind: ItemKind,
    price: Option<u32>,
) -> ShopItem {
    ShopItem {
        id,
        name,
        kind,
        price,
    }
}

pub const NAME_CHANGE_ID: &str = "name_change";
pub const FOUNDER_BADGE_ID: &str = "badge_founder";

/// Everything that exists. Built into the app: adding items means releasing
/// a new version. **Never change an `id`**, because saved data refers to it.
pub const CATALOGUE: &[ShopItem] = &[
    item(
        "bg_constellation",
        "Constellation",
        ItemKind::LibraryBackground,
        Some(800),
    ),
    item(
        "bg_aurora",
        "Aurora",
        ItemKind::LibraryBackground,
        Some(600),
    ),
    item(
        "bg_synthwave_grid",
        "Synthwave Grid",
        ItemKind::LibraryBackground,
        Some(600),
    ),
    item(
        "banner_nebula",
        "Nebula",
        ItemKind::ProfileBanner,
        Some(300),
    ),
    item(
        "banner_pixel_forest",
        "Pixel Forest",
        ItemKind::ProfileBanner,
        Some(300),
    ),
    item("badge_night_owl", "Night Owl", ItemKind::Badge, Some(150)),
    item(
        "badge_speedrunner",
        "Speedrunner",
        ItemKind::Badge,
        Some(150),
    ),
    item(
        "badge_coop_captain",
        "Co-op Captain",
        ItemKind::Badge,
        Some(150),
    ),
    item(
        NAME_CHANGE_ID,
        "Name change",
        ItemKind::NameChange,
        Some(500),
    ),
    // Given to everyone who sets up GameLib in its first version.
    item(FOUNDER_BADGE_ID, "Founder", ItemKind::Badge, None),
    // Earned banners: unlocked by milestones, see `profile::EARNED_ITEMS`.
    item(
        "banner_100h",
        "Century (100 h played)",
        ItemKind::ProfileBanner,
        None,
    ),
    item(
        "banner_250h",
        "Devoted (250 h played)",
        ItemKind::ProfileBanner,
        None,
    ),
    item(
        "banner_500h",
        "Legend (500 h played)",
        ItemKind::ProfileBanner,
        None,
    ),
    item(
        "banner_10_finished",
        "Finisher (10 games finished)",
        ItemKind::ProfileBanner,
        None,
    ),
    // Earned badge: unlocked by finishing 5 games.
    item(
        "badge_backlog_slayer",
        "Backlog Slayer",
        ItemKind::Badge,
        None,
    ),
];

pub fn find_item(item_id: &str) -> Option<&'static ShopItem> {
    CATALOGUE.iter().find(|item| item.id == item_id)
}

/// The items that are actually for sale.
pub fn items_for_sale() -> impl Iterator<Item = &'static ShopItem> {
    CATALOGUE.iter().filter(|item| item.price.is_some())
}

/// Why a purchase (or equipping an item) was refused. Get it back out of an
/// `anyhow::Error` with `err.downcast_ref::<ShopError>()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShopError {
    UnknownItem(String),
    NotForSale(&'static str),
    AlreadyOwned(&'static str),
    NotEnoughStardust {
        balance: i64,
        price: u32,
    },
    NotOwned(&'static str),
    WrongKind {
        item: &'static str,
        expected: ItemKind,
    },
}

impl fmt::Display for ShopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShopError::UnknownItem(id) => write!(f, "there is no shop item called '{id}'"),
            ShopError::NotForSale(name) => write!(f, "{name} can't be bought, it has to be earned"),
            ShopError::AlreadyOwned(name) => write!(f, "you already own {name}"),
            ShopError::NotEnoughStardust { balance, price } => {
                write!(
                    f,
                    "not enough Stardust: this costs ✦{price} and you have ✦{balance}"
                )
            }
            ShopError::NotOwned(name) => write!(f, "you don't own {name} yet"),
            ShopError::WrongKind { item, expected } => {
                write!(f, "{item} is not a {expected:?} item")
            }
        }
    }
}

impl std::error::Error for ShopError {}

/// How an item was acquired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acquired {
    Bought,
    Earned,
    Gift,
}

impl Acquired {
    fn as_str(self) -> &'static str {
        match self {
            Acquired::Bought => "bought",
            Acquired::Earned => "earned",
            Acquired::Gift => "gift",
        }
    }
}

/// One row of the Stardust ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEntry {
    pub id: i64,
    /// Positive when earned, negative when spent.
    pub amount: i64,
    /// `playtime`, `finished`, `daily` or `purchase`.
    pub reason: String,
    pub game_id: Option<i64>,
    pub item_id: Option<String>,
    pub created_at: i64,
}

/// Stardust for one play session: ✦10 per hour, counted in whole minutes
/// and rounded down. 90 minutes → ✦15; 5 minutes → ✦0.
pub fn playtime_reward(duration_secs: u64) -> u32 {
    let whole_minutes = duration_secs / 60;
    (whole_minutes * STARDUST_PER_HOUR / 60) as u32
}

fn insert_ledger(
    conn: &Connection,
    amount: i64,
    reason: &str,
    game_id: Option<i64>,
    item_id: Option<&str>,
    at: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO stardust_ledger(amount, reason, game_id, item_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![amount, reason, game_id, item_id, at],
    )?;
    Ok(())
}

fn balance(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(amount), 0) FROM stardust_ledger",
        [],
        |row| row.get(0),
    )?)
}

fn owns(conn: &Connection, item_id: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM owned_items WHERE item_id = ?1",
            [item_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// Pay the playtime reward for a finished session. Called by
/// `Db::finish_session` inside its transaction.
pub(crate) fn award_playtime(
    conn: &Connection,
    game_id: i64,
    duration_secs: u64,
    at: i64,
) -> Result<u32> {
    let reward = playtime_reward(duration_secs);
    if reward > 0 {
        insert_ledger(conn, reward as i64, "playtime", Some(game_id), None, at)?;
    }
    Ok(reward)
}

impl Db {
    /// Current Stardust: the sum of every ledger row.
    pub fn stardust_balance(&self) -> Result<i64> {
        balance(&self.conn)
    }

    /// The ledger, newest first: the history behind the balance.
    pub fn stardust_ledger(&self) -> Result<Vec<LedgerEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, amount, reason, game_id, item_id, created_at
             FROM stardust_ledger ORDER BY id DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(LedgerEntry {
                    id: row.get(0)?,
                    amount: row.get(1)?,
                    reason: row.get(2)?,
                    game_id: row.get(3)?,
                    item_id: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    pub fn owns_item(&self, item_id: &str) -> Result<bool> {
        owns(&self.conn, item_id)
    }

    /// Ids of every owned item, oldest first.
    pub fn owned_items(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT item_id FROM owned_items ORDER BY acquired_at, item_id")?;
        let ids = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(ids)
    }

    /// Give an item without charging for it (earned banners, the Founder
    /// badge). Returns false if it was already owned.
    pub fn grant_item(&self, item_id: &str, how: Acquired, now: i64) -> Result<bool> {
        let item = find_item(item_id).ok_or_else(|| ShopError::UnknownItem(item_id.to_string()))?;
        let added = self.conn.execute(
            "INSERT OR IGNORE INTO owned_items(item_id, acquired_at, how) VALUES (?1, ?2, ?3)",
            params![item.id, now, how.as_str()],
        )?;
        Ok(added > 0)
    }

    /// Buy an item with Stardust. The check, the ledger row and the
    /// ownership row all happen in **one transaction**: if anything fails,
    /// nothing is saved. Returns the new balance.
    pub fn purchase(&self, item_id: &str, now: i64) -> Result<i64> {
        let item = find_item(item_id).ok_or_else(|| ShopError::UnknownItem(item_id.to_string()))?;
        let price = item.price.ok_or(ShopError::NotForSale(item.name))?;

        let tx = self.conn.unchecked_transaction()?;
        let consumable = item.kind == ItemKind::NameChange;
        if !consumable && owns(&tx, item.id)? {
            return Err(ShopError::AlreadyOwned(item.name).into());
        }
        let current = balance(&tx)?;
        if current < price as i64 {
            return Err(ShopError::NotEnoughStardust {
                balance: current,
                price,
            }
            .into());
        }

        insert_ledger(&tx, -(price as i64), "purchase", None, Some(item.id), now)?;
        if consumable {
            let credits = name_change_credits(&tx)?;
            tx.execute(
                "INSERT INTO settings(key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![keys::NAME_CHANGE_CREDITS, (credits + 1).to_string()],
            )?;
        } else {
            tx.execute(
                "INSERT INTO owned_items(item_id, acquired_at, how) VALUES (?1, ?2, 'bought')",
                params![item.id, now],
            )?;
        }
        tx.commit()?;
        Ok(current - price as i64)
    }

    /// Unused "Name change" purchases.
    pub fn name_change_credits(&self) -> Result<u32> {
        name_change_credits(&self.conn)
    }

    /// ✦5 for the first game launched each day (days are counted in UTC).
    /// Returns what was paid: 5 the first time today, 0 after that.
    pub fn award_daily_launch(&self, game_id: i64, now: i64) -> Result<u32> {
        let today = now.div_euclid(SECS_PER_DAY);
        let tx = self.conn.unchecked_transaction()?;
        let already: bool = tx
            .query_row(
                "SELECT 1 FROM stardust_ledger WHERE reason = 'daily' AND created_at / ?1 = ?2",
                [SECS_PER_DAY, today],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if already {
            return Ok(0);
        }
        insert_ledger(&tx, DAILY_REWARD as i64, "daily", Some(game_id), None, now)?;
        tx.commit()?;
        Ok(DAILY_REWARD)
    }

    /// Mark a game finished: it joins the "Finished" collection, and pays
    /// ✦50 **once per game**, provided it has at least an hour of playtime.
    /// Returns the Stardust paid by this call (0 or 50).
    pub fn finish_game(&self, game_id: i64, now: i64) -> Result<u32> {
        let game = self.require_game(game_id)?;
        let tx = self.conn.unchecked_transaction()?;
        let finished = self.ensure_tag(FINISHED_COLLECTION, true)?;
        self.tag_game(game_id, finished.id)?;

        let already_paid: bool = tx
            .query_row(
                "SELECT 1 FROM stardust_ledger WHERE reason = 'finished' AND game_id = ?1",
                [game_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        let reward = if !already_paid && game.playtime_secs >= FINISH_MIN_PLAYTIME_SECS {
            insert_ledger(
                &tx,
                FINISH_REWARD as i64,
                "finished",
                Some(game_id),
                None,
                now,
            )?;
            FINISH_REWARD
        } else {
            0
        };
        tx.commit()?;
        Ok(reward)
    }

    /// How many games are in the "Finished" collection.
    pub fn finished_count(&self) -> Result<u64> {
        match self.find_tag(FINISHED_COLLECTION)? {
            Some(tag) => self.tag_game_count(tag.id),
            None => Ok(0),
        }
    }

    /// The equipped library background, if any.
    pub fn equipped_library_background(&self) -> Result<Option<&'static ShopItem>> {
        Ok(self
            .get_setting(keys::EQUIPPED_LIBRARY_BG)?
            .and_then(|id| find_item(&id)))
    }

    /// Equip an owned library background, or pass `None` for the default.
    pub fn equip_library_background(&self, item_id: Option<&str>) -> Result<()> {
        match item_id {
            None => self.delete_setting(keys::EQUIPPED_LIBRARY_BG),
            Some(id) => {
                let item = self.require_owned(id, ItemKind::LibraryBackground)?;
                self.set_setting(keys::EQUIPPED_LIBRARY_BG, item.id)
            }
        }
    }

    /// Check that an item exists, is of the expected kind, and is owned.
    pub(crate) fn require_owned(
        &self,
        item_id: &str,
        expected: ItemKind,
    ) -> Result<&'static ShopItem> {
        let item = find_item(item_id).ok_or_else(|| ShopError::UnknownItem(item_id.to_string()))?;
        if item.kind != expected {
            return Err(ShopError::WrongKind {
                item: item.name,
                expected,
            }
            .into());
        }
        if !self.owns_item(item.id)? {
            return Err(ShopError::NotOwned(item.name).into());
        }
        Ok(item)
    }
}

pub(crate) fn name_change_credits(conn: &Connection) -> Result<u32> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [keys::NAME_CHANGE_CREDITS],
            |row| row.get(0),
        )
        .optional()?;
    Ok(value.and_then(|v| v.parse().ok()).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DetectedGame;

    const HOUR: i64 = 3600;

    fn shop_error(err: &anyhow::Error) -> &ShopError {
        err.downcast_ref::<ShopError>()
            .expect("expected a ShopError")
    }

    fn db_with_game() -> (Db, i64) {
        let db = Db::open_in_memory().unwrap();
        let id = db
            .upsert_detected(
                &DetectedGame::manual("Ember Road", "/g/ember/ember.exe"),
                100,
            )
            .unwrap()
            .game_id;
        (db, id)
    }

    /// Play `secs` of a game in one session.
    fn play(db: &Db, game: i64, start: i64, secs: i64) -> u32 {
        let s = db.start_session(game, start).unwrap();
        db.finish_session(s, start + secs).unwrap().stardust
    }

    fn give(db: &Db, amount: i64) {
        insert_ledger(&db.conn, amount, "playtime", None, None, 0).unwrap();
    }

    fn row_counts(db: &Db) -> (i64, i64) {
        let count = |table: &str| -> i64 {
            db.conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap()
        };
        (count("stardust_ledger"), count("owned_items"))
    }

    #[test]
    fn playtime_reward_rule() {
        assert_eq!(playtime_reward(90 * 60), 15);
        assert_eq!(playtime_reward(60 * 60), 10);
        assert_eq!(playtime_reward(5 * 60), 0);
        assert_eq!(playtime_reward(6 * 60), 1);
        assert_eq!(
            playtime_reward(6 * 60 + 59),
            1,
            "seconds past the minute don't count"
        );
        assert_eq!(playtime_reward(0), 0);
    }

    #[test]
    fn catalogue_ids_are_unique() {
        let mut ids: Vec<&str> = CATALOGUE.iter().map(|i| i.id).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before);
    }

    #[test]
    fn playing_earns_stardust_and_balance_is_the_ledger_sum() {
        let (db, game) = db_with_game();
        assert_eq!(db.stardust_balance().unwrap(), 0);
        assert_eq!(play(&db, game, 1000, 90 * 60), 15);
        assert_eq!(play(&db, game, 50_000, 2 * HOUR), 20);
        assert_eq!(play(&db, game, 90_000, 60), 0, "too short to earn anything");

        assert_eq!(db.stardust_balance().unwrap(), 35);
        let ledger = db.stardust_ledger().unwrap();
        assert_eq!(ledger.len(), 2, "no row for a zero reward");
        assert_eq!(ledger.iter().map(|e| e.amount).sum::<i64>(), 35);
        assert!(
            ledger
                .iter()
                .all(|e| e.reason == "playtime" && e.game_id == Some(game))
        );
    }

    #[test]
    fn buying_reduces_the_balance_by_exactly_the_price() {
        let db = Db::open_in_memory().unwrap();
        give(&db, 1000);
        let new_balance = db.purchase("bg_constellation", 5).unwrap();
        assert_eq!(new_balance, 200);
        assert_eq!(db.stardust_balance().unwrap(), 200);
        assert!(db.owns_item("bg_constellation").unwrap());

        let newest = &db.stardust_ledger().unwrap()[0];
        assert_eq!(newest.amount, -800);
        assert_eq!(newest.reason, "purchase");
        assert_eq!(newest.item_id.as_deref(), Some("bg_constellation"));
    }

    #[test]
    fn cannot_buy_with_too_little_stardust_and_nothing_changes() {
        let db = Db::open_in_memory().unwrap();
        give(&db, 799);
        let before = row_counts(&db);

        let err = db.purchase("bg_constellation", 5).unwrap_err();
        assert_eq!(
            shop_error(&err),
            &ShopError::NotEnoughStardust {
                balance: 799,
                price: 800
            }
        );
        assert_eq!(row_counts(&db), before, "both tables unchanged");
        assert_eq!(db.stardust_balance().unwrap(), 799);
        assert!(!db.owns_item("bg_constellation").unwrap());
    }

    #[test]
    fn cannot_buy_the_same_item_twice() {
        let db = Db::open_in_memory().unwrap();
        give(&db, 1000);
        db.purchase("badge_night_owl", 5).unwrap();
        let before = row_counts(&db);

        let err = db.purchase("badge_night_owl", 6).unwrap_err();
        assert_eq!(shop_error(&err), &ShopError::AlreadyOwned("Night Owl"));
        assert_eq!(row_counts(&db), before);
        assert_eq!(db.stardust_balance().unwrap(), 850);
    }

    #[test]
    fn a_failed_purchase_rolls_back_the_ledger_row() {
        // Force the second insert of the transaction to fail: the ownership
        // row can't be written, so the ledger row must not survive either.
        let db = Db::open_in_memory().unwrap();
        give(&db, 1000);
        db.conn
            .execute_batch(
                "CREATE TRIGGER block_owned BEFORE INSERT ON owned_items
                 BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
            )
            .unwrap();
        let before = row_counts(&db);

        assert!(db.purchase("bg_aurora", 5).is_err());
        assert_eq!(row_counts(&db), before, "both tables unchanged");
        assert_eq!(db.stardust_balance().unwrap(), 1000);
    }

    #[test]
    fn unknown_and_earned_items_cannot_be_bought() {
        let db = Db::open_in_memory().unwrap();
        give(&db, 10_000);
        let err = db.purchase("bg_does_not_exist", 1).unwrap_err();
        assert!(matches!(shop_error(&err), ShopError::UnknownItem(_)));
        let err = db.purchase("banner_100h", 1).unwrap_err();
        assert!(matches!(shop_error(&err), ShopError::NotForSale(_)));
        assert_eq!(db.stardust_balance().unwrap(), 10_000);
    }

    #[test]
    fn name_changes_are_consumable_and_can_be_bought_repeatedly() {
        let db = Db::open_in_memory().unwrap();
        give(&db, 1200);
        db.purchase(NAME_CHANGE_ID, 1).unwrap();
        db.purchase(NAME_CHANGE_ID, 2).unwrap();
        assert_eq!(db.name_change_credits().unwrap(), 2);
        assert_eq!(db.stardust_balance().unwrap(), 200);
        assert!(
            !db.owns_item(NAME_CHANGE_ID).unwrap(),
            "credits, not an owned item"
        );
    }

    #[test]
    fn finishing_pays_once_per_game() {
        let (db, game) = db_with_game();
        play(&db, game, 1000, 2 * HOUR); // ✦20
        assert_eq!(db.finish_game(game, 9000).unwrap(), 50);
        assert_eq!(
            db.finish_game(game, 9001).unwrap(),
            0,
            "second finish pays nothing"
        );
        assert_eq!(db.stardust_balance().unwrap(), 70);
        assert_eq!(db.finished_count().unwrap(), 1);
    }

    #[test]
    fn finishing_a_barely_played_game_pays_nothing_until_it_has_an_hour() {
        let (db, game) = db_with_game();
        play(&db, game, 1000, 20 * 60); // ✦3
        assert_eq!(db.finish_game(game, 5000).unwrap(), 0);
        assert_eq!(db.finished_count().unwrap(), 1, "still counts as finished");
        assert_eq!(db.stardust_balance().unwrap(), 3);

        play(&db, game, 10_000, HOUR); // ✦10, now 80 minutes in total
        assert_eq!(db.finish_game(game, 20_000).unwrap(), 50);
        assert_eq!(db.stardust_balance().unwrap(), 63);
    }

    #[test]
    fn daily_reward_is_paid_once_per_day() {
        let (db, game) = db_with_game();
        let day = 20_000 * SECS_PER_DAY;
        assert_eq!(db.award_daily_launch(game, day + 100).unwrap(), 5);
        assert_eq!(db.award_daily_launch(game, day + 80_000).unwrap(), 0);
        assert_eq!(
            db.award_daily_launch(game, day + SECS_PER_DAY + 1).unwrap(),
            5
        );
        assert_eq!(db.stardust_balance().unwrap(), 10);
    }

    #[test]
    fn only_owned_backgrounds_can_be_equipped() {
        let db = Db::open_in_memory().unwrap();
        give(&db, 2000);
        assert!(db.equipped_library_background().unwrap().is_none());

        let err = db
            .equip_library_background(Some("bg_constellation"))
            .unwrap_err();
        assert!(matches!(shop_error(&err), ShopError::NotOwned(_)));

        db.purchase("bg_constellation", 1).unwrap();
        db.equip_library_background(Some("bg_constellation"))
            .unwrap();
        assert_eq!(
            db.equipped_library_background().unwrap().unwrap().name,
            "Constellation"
        );

        db.purchase("badge_night_owl", 1).unwrap();
        let err = db
            .equip_library_background(Some("badge_night_owl"))
            .unwrap_err();
        assert!(matches!(shop_error(&err), ShopError::WrongKind { .. }));

        db.equip_library_background(None).unwrap();
        assert!(db.equipped_library_background().unwrap().is_none());
    }

    #[test]
    fn granting_is_free_and_only_happens_once() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.grant_item("banner_100h", Acquired::Earned, 1).unwrap());
        assert!(!db.grant_item("banner_100h", Acquired::Earned, 2).unwrap());
        assert_eq!(db.owned_items().unwrap(), ["banner_100h"]);
        assert_eq!(db.stardust_balance().unwrap(), 0);
    }
}
