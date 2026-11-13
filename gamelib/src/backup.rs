//! Keeping things safe: save-game backups, a list of installed mods, and
//! exporting or importing the whole library as one file.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::db::Db;
use crate::extras::Status;
use crate::model::{DetectedGame, LaunchTarget, Source};

// --- Save-game backups ---------------------------------------------------

/// One backup of a game's save folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveBackup {
    /// When it was made (unix timestamp).
    pub made_at: i64,
    pub path: PathBuf,
    pub files: usize,
    pub bytes: u64,
}

fn backups_dir(backups_root: &Path, game_id: i64) -> PathBuf {
    backups_root.join(game_id.to_string())
}

/// Copy everything under `from` into `to`. Returns (files, bytes).
/// Links are skipped rather than followed, so a backup can't wander off
/// into the rest of the disk.
fn copy_tree(from: &Path, to: &Path) -> Result<(usize, u64)> {
    let mut files = 0;
    let mut bytes = 0;
    for entry in WalkDir::new(from).follow_links(false) {
        let entry = entry.with_context(|| format!("couldn't read {}", from.display()))?;
        let relative = entry.path().strip_prefix(from).unwrap_or(entry.path());
        let dest = to.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest)
                .with_context(|| format!("couldn't create {}", dest.display()))?;
        } else if entry.file_type().is_file() {
            std::fs::copy(entry.path(), &dest)
                .with_context(|| format!("couldn't copy {}", entry.path().display()))?;
            // Count the size of the copy rather than trusting what `copy`
            // returns: on Windows that comes from a progress report some
            // systems don't give.
            bytes += std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
            files += 1;
        }
    }
    Ok((files, bytes))
}

/// Back up a game's save folder into `backups_root/<game id>/<time>/`.
pub fn backup_saves(
    save_dir: &Path,
    backups_root: &Path,
    game_id: i64,
    now: i64,
) -> Result<SaveBackup> {
    anyhow::ensure!(
        save_dir.is_dir(),
        "the save folder {} doesn't exist",
        save_dir.display()
    );
    let path = backups_dir(backups_root, game_id).join(now.to_string());
    anyhow::ensure!(!path.exists(), "a backup was already made this second");
    let (files, bytes) = copy_tree(save_dir, &path)?;
    anyhow::ensure!(
        files > 0,
        "the save folder is empty, so there was nothing to back up"
    );
    Ok(SaveBackup {
        made_at: now,
        path,
        files,
        bytes,
    })
}

/// A game's backups, newest first.
pub fn list_backups(backups_root: &Path, game_id: i64) -> Vec<SaveBackup> {
    let Ok(entries) = std::fs::read_dir(backups_dir(backups_root, game_id)) else {
        return Vec::new();
    };
    let mut backups: Vec<SaveBackup> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let made_at: i64 = entry.file_name().to_str()?.parse().ok()?;
            let path = entry.path();
            let (mut files, mut bytes) = (0, 0);
            for file in WalkDir::new(&path).into_iter().filter_map(|e| e.ok()) {
                if file.file_type().is_file() {
                    files += 1;
                    bytes += file.metadata().map(|m| m.len()).unwrap_or(0);
                }
            }
            Some(SaveBackup {
                made_at,
                path,
                files,
                bytes,
            })
        })
        .collect();
    backups.sort_by_key(|b| std::cmp::Reverse(b.made_at));
    backups
}

/// Put a backup back: the current saves are themselves backed up first
/// (so restoring can be undone), then replaced with the backup's files.
pub fn restore_saves(
    backup: &SaveBackup,
    save_dir: &Path,
    backups_root: &Path,
    game_id: i64,
    now: i64,
) -> Result<()> {
    if save_dir.is_dir()
        && WalkDir::new(save_dir)
            .into_iter()
            .flatten()
            .any(|e| e.file_type().is_file())
    {
        backup_saves(save_dir, backups_root, game_id, now)?;
    }
    copy_tree(&backup.path, save_dir)?;
    Ok(())
}

// --- Mods ----------------------------------------------------------------

/// One thing in a game's mods folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModEntry {
    pub name: String,
    pub is_folder: bool,
    pub bytes: u64,
    /// When it was last changed (unix timestamp), if known.
    pub modified: Option<i64>,
}

/// What is installed in a mods folder: each file or folder directly
/// inside it counts as one mod. Sorted by name.
pub fn list_mods(mods_dir: &Path) -> Result<Vec<ModEntry>> {
    let entries = std::fs::read_dir(mods_dir)
        .with_context(|| format!("couldn't open the mods folder {}", mods_dir.display()))?;
    let mut mods: Vec<ModEntry> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            let path = entry.path();
            let is_folder = path.is_dir();
            let bytes = WalkDir::new(&path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
                .sum();
            let modified = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);
            Some(ModEntry {
                name,
                is_folder,
                bytes,
                modified,
            })
        })
        .collect();
    mods.sort_by_key(|m| m.name.to_lowercase());
    Ok(mods)
}

// --- Export and import ---------------------------------------------------

/// The newest export format this version writes and understands.
pub const EXPORT_VERSION: u32 = 1;

/// A whole library in one file, for moving to another PC or keeping safe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryExport {
    pub version: u32,
    pub exported_at: i64,
    pub games: Vec<ExportedGame>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportedGame {
    pub title: String,
    pub source: String,
    pub source_id: String,
    pub install_dir: Option<PathBuf>,
    /// An exe path, or a launcher link.
    pub launch: String,
    pub launch_is_link: bool,
    #[serde(default)]
    pub launch_args: Vec<String>,
    pub launch_working_dir: Option<PathBuf>,
    #[serde(default)]
    pub favourite: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub playtime_secs: u64,
    pub last_played: Option<i64>,
    pub description: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    pub release_year: Option<u16>,
    /// Tag names, with whether each is a collection.
    #[serde(default)]
    pub tags: Vec<(String, bool)>,
    pub status: Option<String>,
    pub rating: Option<u8>,
    pub franchise: Option<String>,
    pub franchise_order: Option<i64>,
    pub save_dir: Option<PathBuf>,
    pub mods_dir: Option<PathBuf>,
    #[serde(default)]
    pub minutes_to_beat: Option<u32>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub review: Option<String>,
    #[serde(default)]
    pub process_name: Option<String>,
}

/// What an import did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportSummary {
    pub added: usize,
    pub updated: usize,
}

impl Db {
    /// Everything about every game, ready to save as a file.
    pub fn export_library(&self, now: i64) -> Result<LibraryExport> {
        let extras = self.all_extras()?;
        let mut games = Vec::new();
        for game in self.list_games()? {
            let extra = extras.get(&game.id).cloned().unwrap_or_default();
            let (launch, launch_is_link, launch_args, launch_working_dir) = match &game.launch {
                LaunchTarget::Uri(uri) => (uri.clone(), true, Vec::new(), None),
                LaunchTarget::Exe {
                    path,
                    args,
                    working_dir,
                } => (
                    path.to_string_lossy().into_owned(),
                    false,
                    args.clone(),
                    working_dir.clone(),
                ),
            };
            games.push(ExportedGame {
                title: game.title,
                source: game.source.as_str().to_string(),
                source_id: game.source_id,
                install_dir: game.install_dir,
                launch,
                launch_is_link,
                launch_args,
                launch_working_dir,
                favourite: game.favourite,
                hidden: game.hidden,
                playtime_secs: game.playtime_secs,
                last_played: game.last_played,
                description: game.description,
                genres: game.genres,
                release_year: game.release_year,
                tags: self
                    .tags_for_game(game.id)?
                    .into_iter()
                    .map(|tag| (tag.name, tag.is_collection))
                    .collect(),
                status: extra.status.map(|s| s.as_str().to_string()),
                rating: extra.rating,
                franchise: extra.franchise,
                franchise_order: extra.franchise_order,
                save_dir: extra.save_dir,
                mods_dir: extra.mods_dir,
                minutes_to_beat: extra.minutes_to_beat,
                notes: extra.notes,
                review: extra.review,
                process_name: extra.process_name,
            });
        }
        Ok(LibraryExport {
            version: EXPORT_VERSION,
            exported_at: now,
            games,
        })
    }

    /// Merge an exported library into this one. Games are matched by where
    /// they came from, so importing twice changes nothing the second
    /// time. Playtime is never lowered: the larger of the two is kept.
    /// All or nothing: if anything fails, the library is left as it was.
    pub fn import_library(&self, export: &LibraryExport, now: i64) -> Result<ImportSummary> {
        anyhow::ensure!(
            export.version <= EXPORT_VERSION,
            "this file was made by a newer version of GameLib"
        );
        let tx = self.conn.unchecked_transaction()?;
        let mut summary = ImportSummary {
            added: 0,
            updated: 0,
        };
        for game in &export.games {
            let source = Source::parse(&game.source)?;
            let launch = if game.launch_is_link {
                LaunchTarget::Uri(game.launch.clone())
            } else {
                LaunchTarget::Exe {
                    path: PathBuf::from(&game.launch),
                    args: game.launch_args.clone(),
                    working_dir: game.launch_working_dir.clone(),
                }
            };
            let outcome = self.upsert_detected(
                &DetectedGame {
                    title: game.title.clone(),
                    source,
                    source_id: game.source_id.clone(),
                    install_dir: game.install_dir.clone(),
                    launch,
                },
                now,
            )?;
            if outcome.inserted {
                summary.added += 1;
            } else {
                summary.updated += 1;
            }
            let id = outcome.game_id;
            self.set_favourite(id, game.favourite)?;
            self.set_hidden(id, game.hidden)?;
            tx.execute(
                "UPDATE games SET playtime_secs = MAX(playtime_secs, ?2),
                    last_played = CASE WHEN ?3 IS NULL THEN last_played
                                       ELSE MAX(COALESCE(last_played, 0), ?3) END
                 WHERE id = ?1",
                rusqlite::params![id, game.playtime_secs as i64, game.last_played],
            )?;
            if game.description.is_some() || !game.genres.is_empty() || game.release_year.is_some()
            {
                self.set_game_info(
                    id,
                    game.description.as_deref(),
                    &game.genres,
                    game.release_year,
                )?;
            }
            for (name, is_collection) in &game.tags {
                let tag = self.ensure_tag(name, *is_collection)?;
                self.tag_game(id, tag.id)?;
            }
            self.set_status(id, game.status.as_deref().and_then(Status::parse))?;
            self.set_rating(id, game.rating.filter(|r| (1..=5).contains(r)))?;
            self.set_franchise(id, game.franchise.as_deref(), game.franchise_order)?;
            self.set_save_dir(id, game.save_dir.as_deref())?;
            self.set_mods_dir(id, game.mods_dir.as_deref())?;
            self.set_minutes_to_beat(id, game.minutes_to_beat)?;
            self.set_notes(id, game.notes.as_deref())?;
            self.set_review(id, game.review.as_deref())?;
            self.set_process_name(id, game.process_name.as_deref())?;
        }
        tx.commit()?;
        self.refresh_exe_installed()?;
        Ok(summary)
    }
}

/// Write a library export to a file as readable JSON.
pub fn write_export(export: &LibraryExport, file: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(export)?;
    std::fs::write(file, json).with_context(|| format!("couldn't write {}", file.display()))
}

/// Read a library export back from a file.
pub fn read_export(file: &Path) -> Result<LibraryExport> {
    let json = std::fs::read_to_string(file)
        .with_context(|| format!("couldn't read {}", file.display()))?;
    serde_json::from_str(&json).context("that file isn't a GameLib library export")
}

// --- Sync through a cloud folder ---------------------------------------------

/// The file GameLib keeps in the sync folder.
pub const SYNC_FILE: &str = "gamelib-sync.json";
const SYNC_FOLDER_KEY: &str = "sync.folder";
/// The `exported_at` of the newest sync file this PC has seen (written or
/// loaded), so the same file isn't loaded twice.
const SYNC_SEEN_KEY: &str = "sync.seen";

/// What a sync did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncReport {
    /// Changes loaded from the folder (another PC saved newer ones).
    pub pulled: Option<ImportSummary>,
    /// This library was saved to the folder.
    pub pushed: bool,
}

impl Db {
    /// The folder used for sync: one that OneDrive, Dropbox, Google Drive
    /// or similar keeps the same on every PC.
    pub fn sync_folder(&self) -> Result<Option<PathBuf>> {
        Ok(self
            .get_setting(SYNC_FOLDER_KEY)?
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from))
    }

    pub fn set_sync_folder(&self, folder: Option<&Path>) -> Result<()> {
        match folder {
            Some(folder) => self.set_setting(SYNC_FOLDER_KEY, &folder.to_string_lossy()),
            None => self.delete_setting(SYNC_FOLDER_KEY),
        }
    }

    /// Load the sync file if another PC has saved a newer one since this
    /// PC last looked. Playtime only ever goes up; other details take the
    /// other PC's values.
    pub fn sync_pull(&self, now: i64) -> Result<Option<ImportSummary>> {
        let Some(folder) = self.sync_folder()? else {
            return Ok(None);
        };
        let file = folder.join(SYNC_FILE);
        if !file.exists() {
            return Ok(None);
        }
        let export = read_export(&file)?;
        let seen: i64 = self
            .get_setting(SYNC_SEEN_KEY)?
            .and_then(|s| s.parse().ok())
            .unwrap_or(i64::MIN);
        if export.exported_at <= seen {
            return Ok(None);
        }
        let summary = self.import_library(&export, now)?;
        self.set_setting(SYNC_SEEN_KEY, &export.exported_at.to_string())?;
        Ok(Some(summary))
    }

    /// Save this library to the sync folder.
    pub fn sync_push(&self, now: i64) -> Result<bool> {
        let Some(folder) = self.sync_folder()? else {
            return Ok(false);
        };
        anyhow::ensure!(
            folder.is_dir(),
            "the sync folder {} isn't there",
            folder.display()
        );
        let export = self.export_library(now)?;
        // Write next to it, then swap, so a half-written file is never
        // picked up by the cloud app.
        let temp = folder.join(format!("{SYNC_FILE}.tmp"));
        write_export(&export, &temp)?;
        std::fs::rename(&temp, folder.join(SYNC_FILE))
            .with_context(|| format!("couldn't save to {}", folder.display()))?;
        self.set_setting(SYNC_SEEN_KEY, &export.exported_at.to_string())?;
        Ok(true)
    }

    /// Load anything newer from the folder, then save the result back.
    pub fn sync_now(&self, now: i64) -> Result<SyncReport> {
        let pulled = self.sync_pull(now)?;
        let pushed = self.sync_push(now)?;
        Ok(SyncReport { pulled, pushed })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, contents: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn saves_are_backed_up_listed_and_restored() {
        let dir = tempfile::tempdir().unwrap();
        let saves = dir.path().join("saves");
        let backups = dir.path().join("backups");
        write(&saves.join("slot1.sav"), b"level 3");
        write(&saves.join("profiles/me.cfg"), b"name=me");

        let first = backup_saves(&saves, &backups, 7, 1000).unwrap();
        assert_eq!((first.files, first.bytes), (2, 14));
        assert_eq!(
            fs::read(first.path.join("profiles/me.cfg")).unwrap(),
            b"name=me"
        );

        // Play on, back up again.
        write(&saves.join("slot1.sav"), b"level 9!");
        backup_saves(&saves, &backups, 7, 2000).unwrap();
        let listed = list_backups(&backups, 7);
        assert_eq!(
            listed.iter().map(|b| b.made_at).collect::<Vec<_>>(),
            [2000, 1000]
        );
        assert!(
            list_backups(&backups, 8).is_empty(),
            "other games have none"
        );

        // Restore the first: saves go back, and the current ones are kept
        // as a new backup so the restore can be undone.
        restore_saves(&listed[1], &saves, &backups, 7, 3000).unwrap();
        assert_eq!(fs::read(saves.join("slot1.sav")).unwrap(), b"level 3");
        let after = list_backups(&backups, 7);
        assert_eq!(after.len(), 3);
        assert_eq!(
            fs::read(after[0].path.join("slot1.sav")).unwrap(),
            b"level 9!"
        );
    }

    #[test]
    fn backing_up_nothing_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let backups = dir.path().join("backups");
        assert!(backup_saves(&dir.path().join("missing"), &backups, 1, 1).is_err());
        let empty = dir.path().join("empty");
        fs::create_dir(&empty).unwrap();
        assert!(backup_saves(&empty, &backups, 1, 2).is_err());
    }

    #[test]
    fn mods_are_the_things_directly_inside_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("BetterGrass/textures/grass.png"),
            &[0; 300],
        );
        write(&dir.path().join("BetterGrass/readme.txt"), &[0; 50]);
        write(&dir.path().join("fast_travel.pak"), &[0; 1200]);

        let mods = list_mods(dir.path()).unwrap();
        assert_eq!(mods.len(), 2);
        assert_eq!(
            (mods[0].name.as_str(), mods[0].is_folder, mods[0].bytes),
            ("BetterGrass", true, 350)
        );
        assert_eq!(
            (mods[1].name.as_str(), mods[1].is_folder, mods[1].bytes),
            ("fast_travel.pak", false, 1200)
        );
        assert!(list_mods(&dir.path().join("missing")).is_err());
    }

    fn library() -> Db {
        let db = Db::open_in_memory().unwrap();
        let steam = DetectedGame {
            title: "Dota 2".into(),
            source: Source::Steam,
            source_id: "570".into(),
            install_dir: Some(PathBuf::from("/steam/dota")),
            launch: LaunchTarget::Uri("steam://rungameid/570".into()),
        };
        let a = db.upsert_detected(&steam, 10).unwrap().game_id;
        let exe = DetectedGame {
            title: "Old Game".into(),
            source: Source::Gog,
            source_id: "12".into(),
            install_dir: None,
            launch: LaunchTarget::Exe {
                path: PathBuf::from("/gog/old/old.exe"),
                args: vec!["-windowed".into()],
                working_dir: Some(PathBuf::from("/gog/old")),
            },
        };
        let b = db.upsert_detected(&exe, 20).unwrap().game_id;
        db.set_favourite(a, true).unwrap();
        let session = db.start_session(a, 100).unwrap();
        db.finish_session(session, 4000).unwrap();
        let tag = db.ensure_tag("Backlog", true).unwrap();
        db.tag_game(b, tag.id).unwrap();
        db.set_status(b, Some(Status::Backlog)).unwrap();
        db.set_rating(a, Some(5)).unwrap();
        db.set_franchise(a, Some("Dota"), Some(2)).unwrap();
        db.set_game_info(a, Some("A MOBA."), &["Strategy".to_string()], Some(2013))
            .unwrap();
        db
    }

    #[test]
    fn a_library_survives_export_to_a_file_and_import_elsewhere() {
        let original = library();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("library.json");
        write_export(&original.export_library(500).unwrap(), &file).unwrap();

        let other = Db::open_in_memory().unwrap();
        let summary = other
            .import_library(&read_export(&file).unwrap(), 900)
            .unwrap();
        assert_eq!(
            summary,
            ImportSummary {
                added: 2,
                updated: 0
            }
        );

        // The copy exports to exactly the same thing.
        let mut a = original.export_library(1).unwrap();
        let mut b = other.export_library(1).unwrap();
        a.games.sort_by(|x, y| x.title.cmp(&y.title));
        b.games.sort_by(|x, y| x.title.cmp(&y.title));
        assert_eq!(a, b);
        let dota = &b.games[0];
        assert_eq!(
            (dota.playtime_secs, dota.rating, dota.favourite),
            (3900, Some(5), true)
        );
        assert_eq!(dota.franchise.as_deref(), Some("Dota"));
    }

    #[test]
    fn importing_twice_adds_nothing_and_never_lowers_playtime() {
        let db = library();
        let mut export = db.export_library(1).unwrap();
        export.games[0].playtime_secs = 10; // an older export with less playtime
        let summary = db.import_library(&export, 2).unwrap();
        assert_eq!(
            summary,
            ImportSummary {
                added: 0,
                updated: 2
            }
        );
        assert_eq!(db.list_games().unwrap().len(), 2);
        let dota = db
            .list_games()
            .unwrap()
            .into_iter()
            .find(|g| g.title == "Dota 2")
            .unwrap();
        assert_eq!(dota.playtime_secs, 3900);
    }

    #[test]
    fn bad_files_are_refused_and_change_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes.json");
        fs::write(&file, "{\"hello\": 1}").unwrap();
        assert!(read_export(&file).is_err());

        let db = library();
        let mut export = db.export_library(1).unwrap();
        export.version = EXPORT_VERSION + 1;
        assert!(db.import_library(&export, 2).is_err());

        // One bad game in the middle: nothing from the file is kept.
        let empty = Db::open_in_memory().unwrap();
        export.version = EXPORT_VERSION;
        export.games[1].source = "playstation".into();
        assert!(empty.import_library(&export, 2).is_err());
        assert!(empty.list_games().unwrap().is_empty());
    }

    #[test]
    fn two_pcs_sync_through_a_folder() {
        let cloud = tempfile::tempdir().unwrap();
        let pc1 = Db::open_in_memory().unwrap();
        let pc2 = Db::open_in_memory().unwrap();
        for db in [&pc1, &pc2] {
            db.set_sync_folder(Some(cloud.path())).unwrap();
        }
        let id = pc1
            .upsert_detected(&DetectedGame::manual("Ember Road", "/g/ember.exe"), 1)
            .unwrap()
            .game_id;
        pc1.set_notes(id, Some("chapter 3")).unwrap();
        assert!(pc1.sync_now(100).unwrap().pushed);

        // The second PC picks it up once, not again.
        let report = pc2.sync_now(200).unwrap();
        assert_eq!(report.pulled.map(|s| s.added), Some(1));
        let game = &pc2.list_games().unwrap()[0];
        assert_eq!(
            pc2.extras(game.id).unwrap().notes.as_deref(),
            Some("chapter 3")
        );
        assert_eq!(pc2.sync_pull(300).unwrap(), None);

        // Nothing set up: nothing happens.
        let alone = Db::open_in_memory().unwrap();
        assert_eq!(alone.sync_now(1).unwrap(), SyncReport::default());
    }
}
