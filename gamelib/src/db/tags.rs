//! Tags and collections. A collection is a tag with `is_collection = 1`.

use anyhow::Result;
use rusqlite::{OptionalExtension, params};

use super::Db;
use crate::model::Tag;

fn tag_from_row(row: &rusqlite::Row) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: row.get(0)?,
        name: row.get(1)?,
        is_collection: row.get(2)?,
    })
}

impl Db {
    /// Find the tag with this name (ignoring case), creating it if needed.
    /// Asking for a collection upgrades an existing plain tag to one.
    pub fn ensure_tag(&self, name: &str, is_collection: bool) -> Result<Tag> {
        let name = name.trim();
        anyhow::ensure!(!name.is_empty(), "a tag needs a name");
        self.conn.execute(
            "INSERT INTO tags(name, is_collection) VALUES (?1, ?2)
             ON CONFLICT(name) DO UPDATE SET is_collection = MAX(is_collection, excluded.is_collection)",
            params![name, is_collection],
        )?;
        Ok(self.conn.query_row(
            "SELECT id, name, is_collection FROM tags WHERE name = ?1",
            [name],
            tag_from_row,
        )?)
    }

    pub fn find_tag(&self, name: &str) -> Result<Option<Tag>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, name, is_collection FROM tags WHERE name = ?1",
                [name.trim()],
                tag_from_row,
            )
            .optional()?)
    }

    /// Every tag, collections included, in alphabetical order.
    pub fn list_tags(&self) -> Result<Vec<Tag>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, is_collection FROM tags ORDER BY name")?;
        let tags = stmt
            .query_map([], tag_from_row)?
            .collect::<Result<_, _>>()?;
        Ok(tags)
    }

    /// Just the collections, for the sidebar.
    pub fn list_collections(&self) -> Result<Vec<Tag>> {
        Ok(self
            .list_tags()?
            .into_iter()
            .filter(|t| t.is_collection)
            .collect())
    }

    pub fn rename_tag(&self, tag_id: i64, new_name: &str) -> Result<()> {
        let new_name = new_name.trim();
        anyhow::ensure!(!new_name.is_empty(), "a tag needs a name");
        self.conn.execute(
            "UPDATE tags SET name = ?2 WHERE id = ?1",
            params![tag_id, new_name],
        )?;
        Ok(())
    }

    pub fn set_tag_is_collection(&self, tag_id: i64, is_collection: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE tags SET is_collection = ?2 WHERE id = ?1",
            params![tag_id, is_collection],
        )?;
        Ok(())
    }

    /// Delete a tag. The games that had it are untouched.
    pub fn delete_tag(&self, tag_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM tags WHERE id = ?1", [tag_id])?;
        Ok(())
    }

    /// Put a tag on a game. Doing it twice is harmless.
    pub fn tag_game(&self, game_id: i64, tag_id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO game_tags(game_id, tag_id) VALUES (?1, ?2)",
            [game_id, tag_id],
        )?;
        Ok(())
    }

    pub fn untag_game(&self, game_id: i64, tag_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM game_tags WHERE game_id = ?1 AND tag_id = ?2",
            [game_id, tag_id],
        )?;
        Ok(())
    }

    pub fn tags_for_game(&self, game_id: i64) -> Result<Vec<Tag>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.name, t.is_collection FROM tags t
             JOIN game_tags gt ON gt.tag_id = t.id
             WHERE gt.game_id = ?1 ORDER BY t.name",
        )?;
        let tags = stmt
            .query_map([game_id], tag_from_row)?
            .collect::<Result<_, _>>()?;
        Ok(tags)
    }

    /// Every game's tag names, by game id (games with no tags are absent).
    pub fn all_game_tags(&self) -> Result<std::collections::HashMap<i64, Vec<String>>> {
        let mut stmt = self.conn.prepare(
            "SELECT gt.game_id, t.name FROM game_tags gt JOIN tags t ON t.id = gt.tag_id
             ORDER BY t.name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut map: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
        for row in rows {
            let (game_id, name) = row?;
            map.entry(game_id).or_default().push(name);
        }
        Ok(map)
    }

    /// How many games carry this tag (for the sidebar counts).
    pub fn tag_game_count(&self, tag_id: i64) -> Result<u64> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM game_tags WHERE tag_id = ?1",
            [tag_id],
            |row| row.get(0),
        )?;
        Ok(n as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DetectedGame;

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

    #[test]
    fn ensure_tag_does_not_duplicate_and_ignores_case() {
        let db = Db::open_in_memory().unwrap();
        let a = db.ensure_tag("RPG", false).unwrap();
        let b = db.ensure_tag(" rpg ", false).unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(db.list_tags().unwrap().len(), 1);
    }

    #[test]
    fn collections_are_tags_with_a_flag() {
        let db = Db::open_in_memory().unwrap();
        db.ensure_tag("RPG", false).unwrap();
        db.ensure_tag("Backlog", true).unwrap();
        let collections = db.list_collections().unwrap();
        assert_eq!(collections.len(), 1);
        assert_eq!(collections[0].name, "Backlog");

        // Asking for an existing plain tag as a collection upgrades it…
        assert!(db.ensure_tag("RPG", true).unwrap().is_collection);
        // …and asking for a collection as a plain tag doesn't downgrade it.
        assert!(db.ensure_tag("Backlog", false).unwrap().is_collection);
    }

    #[test]
    fn tagging_and_untagging_a_game() {
        let (db, game) = db_with_game();
        let tag = db.ensure_tag("Backlog", true).unwrap();
        db.tag_game(game, tag.id).unwrap();
        db.tag_game(game, tag.id).unwrap(); // twice is fine
        assert_eq!(db.tags_for_game(game).unwrap(), vec![tag.clone()]);
        assert_eq!(db.tag_game_count(tag.id).unwrap(), 1);
        assert_eq!(db.all_game_tags().unwrap()[&game], ["Backlog"]);

        db.untag_game(game, tag.id).unwrap();
        assert!(db.tags_for_game(game).unwrap().is_empty());
    }

    #[test]
    fn deleting_a_tag_keeps_the_game() {
        let (db, game) = db_with_game();
        let tag = db.ensure_tag("Old", false).unwrap();
        db.tag_game(game, tag.id).unwrap();
        db.delete_tag(tag.id).unwrap();
        assert!(db.get_game(game).unwrap().is_some());
        assert!(db.tags_for_game(game).unwrap().is_empty());
    }
}
