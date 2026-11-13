//! Finding installed games.
//!
//! Every store is a [`GameSource`]: something that can look around the PC
//! and return the games it finds. "Scan" loops over the enabled sources.
//! **If one source fails (say Epic isn't installed), the others still
//! run**, and the failure is reported instead of stopping the scan.

pub mod battlenet;
pub mod ea;
pub mod epic;
pub mod folder;
pub mod gog;
pub mod riot;
pub mod steam;
pub mod uninstall;

use anyhow::Result;

use crate::db::Db;
use crate::model::{DetectedGame, Source};

pub trait GameSource {
    /// A name for messages, e.g. "Steam".
    fn name(&self) -> &str;
    /// Which [`Source`] the games it finds belong to.
    fn source(&self) -> Source;
    /// Look for installed games. Slow: run it on the worker thread.
    fn detect(&self) -> Result<Vec<DetectedGame>>;
}

/// What a scan did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanReport {
    /// New games added to the library.
    pub added: usize,
    /// Games that were already there and were refreshed.
    pub updated: usize,
    /// Games no longer found, now marked "not installed".
    pub missing: usize,
    /// Sources that failed: (source name, what went wrong).
    pub errors: Vec<(String, String)>,
}

impl ScanReport {
    /// A one-line summary for the status bar.
    pub fn summary(&self) -> String {
        let mut text = format!(
            "{} new, {} updated, {} no longer installed",
            self.added, self.updated, self.missing
        );
        for (source, _) in &self.errors {
            text.push_str(&format!("; {source} not found"));
        }
        text
    }
}

/// The launcher sources that are switched on in Settings. (Folder scan is
/// separate: its results go to the import review screen first.)
pub fn enabled_sources(db: &Db) -> Result<Vec<Box<dyn GameSource>>> {
    let mut sources: Vec<Box<dyn GameSource>> = Vec::new();
    if db.source_enabled(Source::Steam)? {
        sources.push(Box::new(steam::SteamSource::default()));
    }
    if db.source_enabled(Source::Epic)? {
        sources.push(Box::new(epic::EpicSource::default()));
    }
    if db.source_enabled(Source::Gog)? {
        sources.push(Box::new(gog::GogSource));
    }
    // These come after the stores above, so a game they also know about
    // (an EA game bought on Steam, say) stays with the store it came from.
    if db.source_enabled(Source::BattleNet)? {
        sources.push(Box::new(battlenet::BattleNetSource));
    }
    if db.source_enabled(Source::Riot)? {
        sources.push(Box::new(riot::RiotSource));
    }
    if db.source_enabled(Source::Ea)? {
        sources.push(Box::new(ea::EaSource));
    }
    Ok(sources)
}

/// Run each source and save what it finds. Games that a source used to
/// find but no longer does are marked "not installed" (only when that
/// source's scan succeeded, so a broken scan never greys out a library).
pub fn scan_into(db: &Db, sources: &[Box<dyn GameSource>], now: i64) -> Result<ScanReport> {
    let mut report = ScanReport::default();
    for source in sources {
        match source.detect() {
            Ok(mut games) => {
                // A launcher can list a game that a store already has (an EA
                // game bought on Steam opens through the EA app too). Keep it
                // with the store, so it doesn't appear twice.
                if is_launcher(source.source()) {
                    let store_dirs =
                        db.install_dirs(&[Source::Steam, Source::Epic, Source::Gog])?;
                    games.retain(|game| {
                        game.install_dir.as_deref().is_none_or(|dir| {
                            !store_dirs.iter().any(|known| {
                                crate::paths::is_inside(dir, known)
                                    || crate::paths::is_inside(known, dir)
                            })
                        })
                    });
                }
                let mut found_ids = Vec::with_capacity(games.len());
                for game in &games {
                    let outcome = db.upsert_detected(game, now)?;
                    if outcome.inserted {
                        report.added += 1;
                    } else {
                        report.updated += 1;
                    }
                    found_ids.push(game.source_id.clone());
                }
                report.missing += db.mark_missing(source.source(), &found_ids)?;
                log::info!("{}: found {} games", source.name(), games.len());
            }
            Err(err) => {
                log::warn!("{} scan failed: {err:#}", source.name());
                report
                    .errors
                    .push((source.name().to_string(), format!("{err:#}")));
            }
        }
    }
    Ok(report)
}

/// Sources that are launchers for games sold elsewhere too, rather than
/// stores. Their games give way to a store's copy of the same game.
fn is_launcher(source: Source) -> bool {
    matches!(source, Source::BattleNet | Source::Riot | Source::Ea)
}

/// Save games the user ticked on the import review screen. Returns
/// (added, updated).
pub fn import_games(db: &Db, games: &[DetectedGame], now: i64) -> Result<(usize, usize)> {
    let mut added = 0;
    let mut updated = 0;
    for game in games {
        if db.upsert_detected(game, now)?.inserted {
            added += 1;
        } else {
            updated += 1;
        }
    }
    Ok((added, updated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LaunchTarget;

    /// A pretend store for tests.
    struct FakeSource {
        source: Source,
        result: Result<Vec<&'static str>, &'static str>,
    }

    impl GameSource for FakeSource {
        fn name(&self) -> &str {
            "Fake"
        }
        fn source(&self) -> Source {
            self.source
        }
        fn detect(&self) -> Result<Vec<DetectedGame>> {
            match &self.result {
                Ok(ids) => Ok(ids
                    .iter()
                    .map(|id| DetectedGame {
                        title: format!("Game {id}"),
                        source: self.source,
                        source_id: id.to_string(),
                        install_dir: None,
                        launch: LaunchTarget::Uri(format!("fake://{id}")),
                    })
                    .collect()),
                Err(message) => anyhow::bail!("{message}"),
            }
        }
    }

    fn fake(
        source: Source,
        result: Result<Vec<&'static str>, &'static str>,
    ) -> Box<dyn GameSource> {
        Box::new(FakeSource { source, result })
    }

    #[test]
    fn scanning_twice_adds_nothing_the_second_time() {
        let db = Db::open_in_memory().unwrap();
        let sources = [fake(Source::Steam, Ok(vec!["1", "2"]))];
        let first = scan_into(&db, &sources, 10).unwrap();
        assert_eq!((first.added, first.updated, first.missing), (2, 0, 0));
        let second = scan_into(&db, &sources, 20).unwrap();
        assert_eq!((second.added, second.updated, second.missing), (0, 2, 0));
        assert_eq!(db.list_games().unwrap().len(), 2);
    }

    #[test]
    fn a_launcher_game_already_owned_through_a_store_is_not_added_twice() {
        let db = Db::open_in_memory().unwrap();
        let store_copy = DetectedGame {
            title: "Apex Legends".into(),
            source: Source::Steam,
            source_id: "1172470".into(),
            install_dir: Some(std::path::PathBuf::from("/steam/common/Apex Legends")),
            launch: LaunchTarget::Uri("steam://rungameid/1172470".into()),
        };
        db.upsert_detected(&store_copy, 1).unwrap();

        struct Ea;
        impl GameSource for Ea {
            fn name(&self) -> &str {
                "EA app"
            }
            fn source(&self) -> Source {
                Source::Ea
            }
            fn detect(&self) -> Result<Vec<DetectedGame>> {
                let game = |title: &str, dir: &str| DetectedGame {
                    title: title.into(),
                    source: Source::Ea,
                    source_id: title.into(),
                    install_dir: Some(std::path::PathBuf::from(dir)),
                    launch: LaunchTarget::Uri("origin2://game/launch?offerIds=X".into()),
                };
                Ok(vec![
                    game("Apex Legends", "/steam/common/Apex Legends"),
                    game("The Sims 4", "/ea/The Sims 4"),
                ])
            }
        }
        let sources: Vec<Box<dyn GameSource>> = vec![Box::new(Ea)];
        let report = scan_into(&db, &sources, 2).unwrap();
        assert_eq!(report.added, 1);
        let titles: Vec<String> = db
            .list_games()
            .unwrap()
            .into_iter()
            .map(|g| g.title)
            .collect();
        assert_eq!(
            titles,
            ["Apex Legends", "The Sims 4"],
            "Apex stays a Steam game"
        );
        assert_eq!(db.list_games().unwrap()[0].source, Source::Steam);
    }

    #[test]
    fn one_failing_source_does_not_stop_the_others() {
        let db = Db::open_in_memory().unwrap();
        let sources = [
            fake(Source::Epic, Err("Epic isn't installed")),
            fake(Source::Steam, Ok(vec!["1"])),
        ];
        let report = scan_into(&db, &sources, 10).unwrap();
        assert_eq!(report.added, 1);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].1.contains("Epic isn't installed"));
        assert!(report.summary().contains("Fake not found"));
    }

    #[test]
    fn uninstalled_games_are_marked_missing_but_a_failed_scan_marks_nothing() {
        let db = Db::open_in_memory().unwrap();
        scan_into(&db, &[fake(Source::Steam, Ok(vec!["1", "2"]))], 10).unwrap();

        // Steam can't be read this time: nothing should be greyed out.
        let report = scan_into(&db, &[fake(Source::Steam, Err("locked"))], 20).unwrap();
        assert_eq!(report.missing, 0);
        assert!(db.list_games().unwrap().iter().all(|g| g.installed));

        // Game 2 was uninstalled.
        let report = scan_into(&db, &[fake(Source::Steam, Ok(vec!["1"]))], 30).unwrap();
        assert_eq!(report.missing, 1);
        let games = db.list_games().unwrap();
        assert_eq!(games.len(), 2, "nothing is deleted");
        assert_eq!(games.iter().filter(|g| g.installed).count(), 1);
    }
}
