//! The background worker.
//!
//! egui redraws the window about 60 times a second, so the UI must never
//! wait on anything slow. Instead it **sends a [`Job`] down a channel**; a
//! worker thread does it and **sends a [`WorkerResult`] back**. Each frame
//! the UI calls [`Worker::try_recv`], which never blocks.
//!
//! ```no_run
//! # use gamelib::worker::{Job, Worker, WorkerConfig};
//! # fn demo(config: WorkerConfig) -> anyhow::Result<()> {
//! let worker = Worker::spawn(config, || { /* ctx.request_repaint() */ })?;
//! worker.send(Job::ScanLibrary);
//! // …later, once per frame:
//! while let Some(result) = worker.try_recv() {
//!     println!("{result:?}");
//! }
//! # Ok(()) }
//! ```
//!
//! The worker opens its own database connection (SQLite is happy with
//! several), so no locks are shared with the UI.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result};

use crate::db::{Db, SessionSummary, keys};
use crate::friends::{self, Friend};
use crate::model::{DetectedGame, Game, Source};
use crate::playtime::{self, SystemEnvironment, Timings, TrackEvent};
use crate::sources::{self, GameSource, ScanReport, folder::FolderSource};
use crate::{artwork, launcher, metadata};

/// How long to wait before looking online again for a game whose cover
/// couldn't be found anywhere.
const ART_RETRY_SECS: i64 = 7 * 86_400;

/// Work the UI can ask for.
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    /// Scan Steam, Epic and GOG (whichever are enabled) and save the results.
    ScanLibrary,
    /// Scan the folders set in Settings. Nothing is saved: the candidates
    /// come back for the import review screen.
    ScanFolders,
    /// Save the games ticked on the import review screen.
    Import(Vec<DetectedGame>),
    /// Start a game and track its playtime.
    Launch {
        game_id: i64,
        /// Which launch options to use.
        profile: ProfileChoice,
    },
    /// Measure how much disk space each installed game takes.
    MeasureDisk,
    /// Ask GitHub whether a newer GameLib has been released.
    CheckForUpdates,
    /// List the connected peripherals and paired Bluetooth devices.
    ScanDevices,
    /// Connect or disconnect a paired Bluetooth device.
    SetBluetooth { address: u64, connect: bool },
    /// Read Steam achievements: for one game, or every Steam game.
    FetchAchievements { game_id: Option<i64> },
    /// Read which games each Steam friend owns.
    FetchFriendGames,
    /// Switch on (or off) noticing games started outside GameLib (from
    /// Steam, a desktop shortcut…) and tracking their playtime too.
    WatchForGames(bool),
    /// Find covers, icons and store info for games that lack them. Games
    /// looked up recently without luck are skipped unless `retry_all`.
    FetchMissingCovers { retry_all: bool },
    /// Download one game's cover again, optionally searching for a
    /// different title ("Search again").
    FetchCover {
        game_id: i64,
        search_title: Option<String>,
    },
    /// Fetch the Steam friends list.
    RefreshFriends,
    /// Ask Claude a question about the library. `context` describes the
    /// library; `history` is the conversation, ending with the question.
    AskClaude {
        api_key: String,
        context: String,
        history: Vec<crate::assistant::Turn>,
    },
    /// Finish the current job and stop the worker.
    Shutdown,
}

/// Which launch profile to start a game with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProfileChoice {
    /// The game's default profile, if it has one.
    #[default]
    Default,
    /// No extra options.
    Plain,
    /// This profile.
    Profile(i64),
}

/// What the worker reports back.
#[derive(Debug, Clone, PartialEq)]
pub enum WorkerResult {
    ScanFinished(ScanReport),
    FolderCandidates(Vec<DetectedGame>),
    Imported {
        added: usize,
        updated: usize,
    },
    /// The game was asked to start. `daily_stardust` is the first-launch-
    /// of-the-day reward (0 if already claimed today).
    Launched {
        game_id: i64,
        daily_stardust: u32,
    },
    /// The game's process appeared: it's really running now.
    SessionStarted {
        game_id: i64,
    },
    /// The game closed and its playtime was saved. `unlocked` lists any
    /// earned banners or badges this session unlocked.
    SessionEnded {
        summary: SessionSummary,
        unlocked: Vec<&'static str>,
    },
    /// The game never appeared after launching; nothing was recorded.
    NeverStarted {
        game_id: i64,
    },
    CoverReady {
        game_id: i64,
        path: PathBuf,
    },
    /// The game's own icon was read out of its exe and saved.
    IconReady {
        game_id: i64,
        path: PathBuf,
    },
    /// Description/genres/year were saved for this game.
    InfoReady {
        game_id: i64,
    },
    /// How far through looking for covers online the worker is.
    CoverProgress {
        done: usize,
        total: usize,
    },
    /// A batch of cover downloads finished.
    CoversFinished {
        downloaded: usize,
        failed: usize,
    },
    FriendsUpdated(Vec<Friend>),
    /// Claude's answer, or why there isn't one.
    AssistantReply(Result<String, String>),
    /// How far through measuring install folders the worker is.
    DiskProgress {
        done: usize,
        total: usize,
    },
    DiskMeasured {
        games: usize,
        bytes: u64,
    },
    /// The result of checking for a newer version.
    UpdateChecked(Result<crate::updates::UpdateCheck, String>),
    /// What's plugged in, and the paired Bluetooth devices (or why they
    /// couldn't be listed).
    DevicesFound {
        devices: Result<Vec<crate::devices::Device>, String>,
        bluetooth: Result<Vec<crate::devices::BtDevice>, String>,
    },
    /// Achievements were read: how many games, and any that failed.
    AchievementsUpdated {
        games: usize,
        failed: Vec<String>,
    },
    /// Friends' game lists were read: how many friends, and how many
    /// keep theirs private.
    FriendGamesUpdated {
        friends: usize,
        private: usize,
    },
    /// A Bluetooth device was connected or disconnected (or wasn't).
    BluetoothChanged {
        address: u64,
        connect: bool,
        result: Result<(), String>,
    },
    /// Something went wrong. Show `message` in the status bar; the app
    /// carries on.
    Error {
        context: String,
        message: String,
    },
}

/// What the worker needs to know to start.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub db_path: PathBuf,
    pub covers_dir: PathBuf,
    /// Where icons read out of games' exes are saved.
    pub icons_dir: PathBuf,
    pub timings: Timings,
}

impl WorkerConfig {
    /// The real locations under `%APPDATA%\gamelib\`.
    pub fn from_default_paths() -> Result<WorkerConfig> {
        Ok(WorkerConfig {
            db_path: crate::paths::db_path()?,
            covers_dir: crate::paths::covers_dir()?,
            icons_dir: crate::paths::icons_dir()?,
            timings: Timings::default(),
        })
    }
}

/// The UI's handle on the worker thread.
pub struct Worker {
    jobs: Sender<Job>,
    /// The fast lane: quick jobs that shouldn't wait behind slow ones.
    fast: Sender<Job>,
    stop_watcher: Arc<WatcherSwitches>,
    results: Receiver<WorkerResult>,
    thread: Option<JoinHandle<()>>,
}

impl Job {
    /// Jobs that are quick, or that someone is waiting on (Play), go in the
    /// fast lane so they never queue behind a scan or a long artwork batch.
    fn is_fast(&self) -> bool {
        matches!(
            self,
            Job::Launch { .. }
                | Job::SetBluetooth { .. }
                | Job::ScanDevices
                | Job::AskClaude { .. }
                | Job::CheckForUpdates
                | Job::WatchForGames(_)
        )
    }
}

/// Run jobs from a channel until it closes or says stop. A job that panics
/// is reported as an error; the thread carries on with the next one.
fn job_loop(state: WorkerState, jobs: Receiver<Job>) {
    for job in jobs {
        if job == Job::Shutdown {
            break;
        }
        let reporter = state.reporter.clone();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state.run(job)));
        if let Err(panic) = outcome {
            let message = panic_text(&panic);
            log::error!("a background job crashed: {message}");
            reporter.send(WorkerResult::Error {
                context: "Something went wrong".to_string(),
                message,
            });
        }
    }
}

/// The message inside a panic, if it has one.
pub fn panic_text(panic: &Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "an unexpected error".to_string())
}

impl Worker {
    /// Start the worker thread. `wake` is called after every result is
    /// sent; the UI passes something that calls `ctx.request_repaint()`,
    /// so the window redraws even if the mouse isn't moving.
    pub fn spawn(config: WorkerConfig, wake: impl Fn() + Send + Sync + 'static) -> Result<Worker> {
        Worker::spawn_observed(config, wake, |_| {})
    }

    /// Like [`Worker::spawn`], and `observe` also sees every result as it
    /// is sent, on the thread that sends it. The UI uses this to show a
    /// desktop notification while its window is hidden and not drawing.
    pub fn spawn_observed(
        config: WorkerConfig,
        wake: impl Fn() + Send + Sync + 'static,
        observe: impl Fn(&WorkerResult) + Send + Sync + 'static,
    ) -> Result<Worker> {
        // Open the database here, so a failure is reported to the caller
        // straight away rather than lost inside the thread.
        let db = Db::open(&config.db_path)?;
        for dir in [&config.covers_dir, &config.icons_dir] {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("couldn't create {}", dir.display()))?;
        }
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (fast_tx, fast_rx) = mpsc::channel::<Job>();
        // The fast lane has its own database connection.
        let fast_db = Db::open(&config.db_path)?;
        let (result_tx, result_rx) = mpsc::channel::<WorkerResult>();

        let reporter = Reporter {
            results: result_tx,
            wake: Arc::new(wake),
            observe: Arc::new(observe),
        };
        let tracking = Arc::new(Mutex::new(HashSet::new()));
        let watcher = Arc::new(WatcherSwitches::default());
        let fast_state = WorkerState {
            db: fast_db,
            config: config.clone(),
            reporter: reporter.clone(),
            tracking: Arc::clone(&tracking),
            watcher: Arc::clone(&watcher),
        };
        let stop_watcher = Arc::clone(&watcher);
        thread::Builder::new()
            .name("gamelib-fast".into())
            .spawn(move || job_loop(fast_state, fast_rx))
            .context("couldn't start the worker thread")?;
        let thread = thread::Builder::new()
            .name("gamelib-worker".into())
            .spawn(move || {
                let state = WorkerState {
                    db,
                    config,
                    reporter,
                    tracking,
                    watcher,
                };
                job_loop(state, job_rx);
            })
            .context("couldn't start the worker thread")?;

        Ok(Worker {
            stop_watcher,
            jobs: job_tx,
            fast: fast_tx,
            results: result_rx,
            thread: Some(thread),
        })
    }

    /// Queue a job. Returns immediately.
    pub fn send(&self, job: Job) {
        // The only way this fails is if the worker has stopped, in which
        // case there is nobody to do the job anyway.
        let lane = if job.is_fast() {
            &self.fast
        } else {
            &self.jobs
        };
        if lane.send(job).is_err() {
            log::warn!("job sent after the worker stopped");
        }
    }

    /// The next finished result, if there is one. Never blocks: call it
    /// every frame.
    pub fn try_recv(&self) -> Option<WorkerResult> {
        self.results.try_recv().ok()
    }

    /// Wait for the next result. For tests and command-line use; the UI
    /// should use [`Worker::try_recv`].
    pub fn recv_timeout(&self, timeout: std::time::Duration) -> Option<WorkerResult> {
        self.results.recv_timeout(timeout).ok()
    }

    /// Stop the worker after its current job and wait for it to finish.
    /// (Playtime trackers for games still running carry on until the
    /// program exits; their progress is checkpointed every minute.)
    pub fn shutdown(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        self.stop_watcher.stopping.store(true, Ordering::Relaxed);
        let _ = self.fast.send(Job::Shutdown);
        let _ = self.jobs.send(Job::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Sends results to the UI and wakes it up.
#[derive(Clone)]
struct Reporter {
    results: Sender<WorkerResult>,
    wake: Arc<dyn Fn() + Send + Sync>,
    observe: Arc<dyn Fn(&WorkerResult) + Send + Sync>,
}

impl Reporter {
    fn send(&self, result: WorkerResult) {
        (self.observe)(&result);
        // If the UI has gone away there is nobody to tell; that's fine.
        let _ = self.results.send(result);
        (self.wake)();
    }

    fn error(&self, context: &str, err: &anyhow::Error) {
        log::error!("{context}: {err:#}");
        self.send(WorkerResult::Error {
            context: context.to_string(),
            message: format!("{err:#}"),
        });
    }
}

struct WorkerState {
    db: Db,
    config: WorkerConfig,
    reporter: Reporter,
    /// Games whose playtime is being tracked right now, so clicking Play
    /// twice doesn't count the same session twice.
    tracking: Arc<Mutex<HashSet<i64>>>,
    /// Shared switches for the "games started elsewhere" watcher.
    watcher: Arc<WatcherSwitches>,
}

/// Switches for the watcher thread.
#[derive(Default)]
struct WatcherSwitches {
    /// Noticing games started elsewhere is switched on.
    enabled: AtomicBool,
    /// The watcher thread has been started (it's only started once).
    started: AtomicBool,
    /// GameLib is closing: the watcher stops.
    stopping: AtomicBool,
}

/// How often the watcher looks at the running programs, in seconds.
const WATCH_EVERY: u64 = 15;

impl WorkerState {
    /// Do one job. Errors become an `Error` result; they never stop the
    /// worker.
    fn run(&self, job: Job) {
        let context = match &job {
            Job::ScanLibrary => "Scan failed".to_string(),
            Job::ScanFolders => "Folder scan failed".to_string(),
            Job::Import(_) => "Import failed".to_string(),
            Job::Launch { game_id, .. } => match self.db.get_game(*game_id) {
                Ok(Some(game)) => format!("Couldn't start {}", game.title),
                _ => "Couldn't start the game".to_string(),
            },
            Job::FetchMissingCovers { .. } | Job::FetchCover { .. } => {
                "Cover download failed".to_string()
            }
            Job::RefreshFriends => "Couldn't update friends".to_string(),
            Job::AskClaude { .. } => "Couldn't ask Claude".to_string(),
            Job::MeasureDisk => "Couldn't measure disk use".to_string(),
            Job::CheckForUpdates => "Couldn't check for updates".to_string(),
            Job::ScanDevices => "Couldn't list devices".to_string(),
            Job::SetBluetooth { .. } => "Bluetooth".to_string(),
            Job::WatchForGames(_) => "Couldn't watch for games".to_string(),
            Job::FetchAchievements { .. } => "Couldn't read achievements".to_string(),
            Job::FetchFriendGames => "Couldn't read your friends' games".to_string(),
            Job::Shutdown => return,
        };
        let outcome = match job {
            Job::ScanLibrary => self.scan_library(),
            Job::ScanFolders => self.scan_folders(),
            Job::Import(games) => self.import(&games),
            Job::Launch { game_id, profile } => self.launch(game_id, profile),
            Job::MeasureDisk => self.measure_disk(),
            Job::WatchForGames(on) => self.watch_for_games(on),
            Job::FetchAchievements { game_id } => self.fetch_achievements(game_id),
            Job::FetchFriendGames => self.fetch_friend_games(),
            Job::ScanDevices => {
                let devices = crate::devices::list_devices().map_err(|e| format!("{e:#}"));
                let bluetooth = crate::devices::bluetooth_devices().map_err(|e| format!("{e:#}"));
                self.reporter
                    .send(WorkerResult::DevicesFound { devices, bluetooth });
                Ok(())
            }
            Job::SetBluetooth { address, connect } => {
                // Connecting can take several seconds; don't hold up other jobs.
                let reporter = self.reporter.clone();
                (|| -> Result<()> {
                    thread::Builder::new()
                        .name("gamelib-bluetooth".into())
                        .spawn(move || {
                            let result = crate::devices::set_bluetooth_connected(address, connect)
                                .map_err(|e| format!("{e:#}"));
                            reporter.send(WorkerResult::BluetoothChanged {
                                address,
                                connect,
                                result,
                            });
                        })?;
                    Ok(())
                })()
            }
            Job::CheckForUpdates => {
                let result = crate::updates::check().map_err(|err| format!("{err:#}"));
                self.reporter.send(WorkerResult::UpdateChecked(result));
                Ok(())
            }
            Job::FetchMissingCovers { retry_all } => self.fetch_missing_covers(retry_all),
            Job::FetchCover {
                game_id,
                search_title,
            } => self.fetch_cover(game_id, search_title.as_deref()),
            Job::RefreshFriends => self.refresh_friends(),
            Job::AskClaude {
                api_key,
                context,
                history,
            } => self.ask_claude(api_key, context, history),
            Job::Shutdown => Ok(()),
        };
        if let Err(err) = outcome {
            self.reporter.error(&context, &err);
        }
    }

    /// Claude can take a while to answer, so the question gets its own
    /// thread and the worker stays free for other jobs.
    fn ask_claude(
        &self,
        api_key: String,
        context: String,
        history: Vec<crate::assistant::Turn>,
    ) -> Result<()> {
        let reporter = self.reporter.clone();
        thread::Builder::new()
            .name("gamelib-assistant".into())
            .spawn(move || {
                let reply = crate::assistant::ask_claude(&api_key, &context, &history)
                    .map_err(|err| format!("{err:#}"));
                if let Err(err) = &reply {
                    log::error!("Claude: {err}");
                }
                reporter.send(WorkerResult::AssistantReply(reply));
            })?;
        Ok(())
    }

    fn scan_library(&self) -> Result<()> {
        let sources = sources::enabled_sources(&self.db)?;
        let report = sources::scan_into(&self.db, &sources, crate::now())?;
        self.db.refresh_exe_installed()?;
        self.reporter.send(WorkerResult::ScanFinished(report));
        Ok(())
    }

    fn scan_folders(&self) -> Result<()> {
        let source = FolderSource {
            roots: self.db.scan_folders()?,
            // Don't offer games that a store or launcher already manages…
            skip_dirs: self.db.install_dirs(&[
                Source::Steam,
                Source::Epic,
                Source::Gog,
                Source::BattleNet,
                Source::Riot,
                Source::Ea,
            ])?,
        };
        // …or ones that were already imported from a folder or by hand.
        let known: HashSet<String> = self
            .db
            .list_games()?
            .into_iter()
            .filter(|g| matches!(g.source, Source::Folder | Source::Manual))
            .map(|g| g.source_id)
            .collect();
        let candidates = source
            .detect()?
            .into_iter()
            .filter(|g| !known.contains(&g.source_id))
            .collect();
        self.reporter
            .send(WorkerResult::FolderCandidates(candidates));
        Ok(())
    }

    fn import(&self, games: &[DetectedGame]) -> Result<()> {
        let (added, updated) = sources::import_games(&self.db, games, crate::now())?;
        self.reporter
            .send(WorkerResult::Imported { added, updated });
        Ok(())
    }

    fn measure_disk(&self) -> Result<()> {
        let games: Vec<_> = self
            .db
            .list_games()?
            .into_iter()
            .filter(|g| g.installed && g.install_dir.is_some())
            .collect();
        let total = games.len();
        let mut bytes = 0;
        let mut measured = 0;
        for (done, game) in games.iter().enumerate() {
            if let Some(size) = crate::health::measure_install(&self.db, game, crate::now())? {
                bytes += size;
                measured += 1;
            }
            self.reporter.send(WorkerResult::DiskProgress {
                done: done + 1,
                total,
            });
        }
        self.reporter.send(WorkerResult::DiskMeasured {
            games: measured,
            bytes,
        });
        Ok(())
    }

    fn launch(&self, game_id: i64, choice: ProfileChoice) -> Result<()> {
        let mut game = self.db.require_game(game_id)?;
        let profile = match choice {
            ProfileChoice::Default => self.db.default_launch_profile(game_id)?,
            ProfileChoice::Plain => None,
            ProfileChoice::Profile(id) => self.db.launch_profile(id)?,
        };
        game.launch = crate::profiles::apply(&game, profile.as_ref());
        launcher::launch(&game)?;
        let daily_stardust = self.db.award_daily_launch(game_id, crate::now())?;
        self.reporter.send(WorkerResult::Launched {
            game_id,
            daily_stardust,
        });

        let watch = playtime::Watch {
            dir: game.watch_dir(),
            process_name: self.db.extras(game_id)?.process_name,
        };
        if watch.is_empty() {
            log::warn!(
                "{} has no install folder or process name, so its playtime can't be tracked",
                game.title
            );
            return Ok(());
        }
        self.start_tracking(game_id, watch)?;
        Ok(())
    }

    /// Switch the watcher on or off, starting its thread the first time.
    fn watch_for_games(&self, on: bool) -> Result<()> {
        self.watcher.enabled.store(on, Ordering::Relaxed);
        if !on || self.watcher.started.swap(true, Ordering::Relaxed) {
            return Ok(());
        }
        let state = WorkerState {
            db: Db::open(&self.config.db_path)?,
            config: self.config.clone(),
            reporter: self.reporter.clone(),
            tracking: Arc::clone(&self.tracking),
            watcher: Arc::clone(&self.watcher),
        };
        thread::Builder::new()
            .name("gamelib-watcher".into())
            .spawn(move || {
                let mut system = match sysinfo::System::new() {
                    Ok(system) => system,
                    Err(err) => {
                        log::warn!("can't watch for games: {err}");
                        return;
                    }
                };
                while !state.watcher.stopping.load(Ordering::Relaxed) {
                    if state.watcher.enabled.load(Ordering::Relaxed)
                        && let Err(err) = state.look_for_running_games(&mut system)
                    {
                        log::warn!("watching for games: {err:#}");
                    }
                    // Sleep in short steps so closing GameLib isn't held up.
                    for _ in 0..WATCH_EVERY * 4 {
                        if state.watcher.stopping.load(Ordering::Relaxed) {
                            return;
                        }
                        thread::sleep(std::time::Duration::from_millis(250));
                    }
                }
            })
            .context("couldn't start watching for games")?;
        Ok(())
    }

    /// One look at the running programs: start tracking any library game
    /// that's running but isn't being tracked.
    fn look_for_running_games(&self, system: &mut sysinfo::System) -> Result<()> {
        let running = playtime::running_exes(system);
        if running.is_empty() {
            return Ok(());
        }
        let extras = self.db.all_extras()?;
        let tracked = self
            .tracking
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        for game in self.db.list_games()? {
            if !game.installed || tracked.contains(&game.id) {
                continue;
            }
            let watch = playtime::Watch {
                // Only folders that clearly belong to this one game.
                dir: game
                    .watch_dir()
                    .filter(|dir| playtime::is_specific_folder(dir))
                    .map(|dir| crate::paths::real_path(&dir)),
                process_name: extras.get(&game.id).and_then(|e| e.process_name.clone()),
            };
            if !watch.is_empty() && watch.matches(&running) {
                log::info!("{} was started outside GameLib; tracking it", game.title);
                self.start_tracking(game.id, watch)?;
            }
        }
        Ok(())
    }

    /// Track a game's playtime on its own thread until it closes. Returns
    /// false if it was already being tracked.
    fn start_tracking(&self, game_id: i64, watch: playtime::Watch) -> Result<bool> {
        if !self
            .tracking
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(game_id)
        {
            return Ok(false); // already being tracked
        }

        // Tracking lasts as long as the game runs, so it gets its own
        // thread (and its own database connection) and the worker stays
        // free for other jobs.
        let db_path = self.config.db_path.clone();
        let timings = self.config.timings;
        let reporter = self.reporter.clone();
        let tracking = Arc::clone(&self.tracking);
        thread::Builder::new()
            .name(format!("gamelib-track-{game_id}"))
            .spawn(move || {
                let outcome = (|| -> Result<()> {
                    let db = Db::open(&db_path)?;
                    let mut env = SystemEnvironment::new()?;
                    playtime::track(&db, game_id, &watch, &timings, &mut env, |event| {
                        let result = match event {
                            TrackEvent::SessionStarted { game_id, .. } => {
                                WorkerResult::SessionStarted { game_id }
                            }
                            TrackEvent::SessionEnded(summary) => {
                                let unlocked =
                                    db.grant_earned_items(crate::now()).unwrap_or_else(|err| {
                                        log::error!("couldn't check earned banners: {err:#}");
                                        Vec::new()
                                    });
                                WorkerResult::SessionEnded { summary, unlocked }
                            }
                            TrackEvent::GaveUp { game_id } => {
                                WorkerResult::NeverStarted { game_id }
                            }
                        };
                        reporter.send(result);
                    })
                })();
                tracking
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&game_id);
                if let Err(err) = outcome {
                    reporter.error("Playtime tracking failed", &err);
                }
            })
            .context("couldn't start the playtime tracker")?;
        Ok(true)
    }

    fn api_key(&self, key: &str, what: &str) -> Result<String> {
        self.db
            .get_setting(key)?
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .with_context(|| format!("no {what} has been entered in Settings"))
    }

    fn fetch_cover(&self, game_id: i64, search_title: Option<&str>) -> Result<()> {
        let api_key = self.api_key(keys::STEAMGRIDDB_KEY, "SteamGridDB API key")?;
        let client = metadata::http_client()?;
        let game = self.db.require_game(game_id)?;
        let url = metadata::find_cover_url(&client, &api_key, &game, search_title)?
            .with_context(|| format!("no cover found for {}", game.title))?;
        let path = metadata::download_cover(&client, &url, &self.config.covers_dir, game_id)?;
        if search_title.is_some() {
            // "Search again" is the user's choice: later scans keep it.
            self.db.set_cover_manual(game_id, &path)?;
        } else if !self.db.set_cover_auto(game_id, &path)? {
            return Ok(()); // a hand-picked cover is already there
        }
        self.reporter
            .send(WorkerResult::CoverReady { game_id, path });
        Ok(())
    }

    /// Artwork that is already on this PC: covers from Steam's own cache,
    /// and icons read out of games' exes. No internet or key needed, so
    /// this is done first. One failure doesn't stop the rest.
    fn find_local_artwork(&self) -> Result<usize> {
        let mut covers = 0;
        // Steam not being installed just means there is no cache to look in.
        let steam_root = steamlocate::SteamDir::locate()
            .ok()
            .map(|steam| steam.path().to_path_buf());
        if let Some(steam_root) = &steam_root {
            for game in self.db.games_without_cover()? {
                match artwork::copy_steam_cache_cover(steam_root, &game, &self.config.covers_dir) {
                    Ok(Some(path)) => {
                        if self.db.set_cover_auto(game.id, &path)? {
                            covers += 1;
                            self.reporter.send(WorkerResult::CoverReady {
                                game_id: game.id,
                                path,
                            });
                        }
                    }
                    Ok(None) => {}
                    Err(err) => log::warn!("cached cover for {} failed: {err:#}", game.title),
                }
            }
        }

        for game in self.db.games_without_icon()? {
            let Some(exe) = artwork::icon_source_exe(&game) else {
                continue;
            };
            let dest = self.config.icons_dir.join(format!("{}.png", game.id));
            match artwork::save_exe_icon(&exe, &dest) {
                Ok(true) => {
                    self.db.set_icon(game.id, &dest)?;
                    self.reporter.send(WorkerResult::IconReady {
                        game_id: game.id,
                        path: dest,
                    });
                }
                Ok(false) => log::info!("{} has no icon in {}", game.title, exe.display()),
                Err(err) => log::warn!("icon for {} failed: {err:#}", game.title),
            }
        }
        Ok(covers)
    }

    /// Download one game's cover, trying each place it might be:
    /// 1. Steam's public image servers (no key needed). Games from other
    ///    launchers are matched to Steam by their exact title.
    /// 2. SteamGridDB (any game; needs the key from Settings)
    /// 3. the game's wide Steam Store banner, made into a cover
    ///
    /// A miss at one step just moves on to the next.
    fn download_cover_for(
        &self,
        client: &reqwest::blocking::Client,
        api_key: Option<&str>,
        game: &Game,
    ) -> Result<Option<PathBuf>> {
        let covers_dir = &self.config.covers_dir;
        // Set if any server couldn't be reached. Then "nothing found" isn't
        // trustworthy, and the game must be tried again another time.
        let mut unreachable = false;
        let mut note = |what: &str, err: &anyhow::Error| {
            unreachable |= metadata::is_unreachable(err);
            log::info!("{what} for {}: {err:#}", game.title);
        };

        // A Steam game's app id is known. For anything else, see whether
        // Steam sells a game with exactly the same name.
        let steam_app = if game.source == Source::Steam {
            Some(game.source_id.clone())
        } else {
            metadata::find_steam_app(client, &game.title).unwrap_or_else(|err| {
                note("Steam search failed", &err);
                None
            })
        };
        if let Some(app_id) = &steam_app {
            // Ask Steam for the exact address first, then try the usual ones.
            let mut urls = metadata::steam_cover_urls(client, app_id).unwrap_or_else(|err| {
                note("Steam store items lookup failed", &err);
                Vec::new()
            });
            urls.extend(artwork::steam_cdn_cover_urls(app_id));
            for url in urls {
                match metadata::download_cover(client, &url, covers_dir, game.id) {
                    Ok(path) => return Ok(Some(path)),
                    Err(err) => note("no cover on Steam's image server", &err),
                }
            }
        }
        // Epic games: the official box art, from the catalogue the Epic
        // launcher keeps on this PC.
        if game.source == Source::Epic
            && let Some(catalog) = crate::covers_extra::epic_catalog()
        {
            let item_id = match &game.launch {
                crate::model::LaunchTarget::Uri(uri) => crate::covers_extra::epic_item_id(uri),
                _ => None,
            };
            match crate::covers_extra::epic_cover_url(
                catalog,
                &game.source_id,
                item_id.as_deref(),
                &game.title,
            ) {
                Ok(Some(url)) => {
                    match metadata::download_cover(client, &url, covers_dir, game.id) {
                        Ok(path) => return Ok(Some(path)),
                        Err(err) => note("Epic box art download failed", &err),
                    }
                }
                Ok(None) => {}
                Err(err) => note("Epic catalogue unreadable", &err),
            }
        }
        if let Some(api_key) = api_key {
            match metadata::find_cover_url(client, api_key, game, None) {
                Ok(Some(url)) => {
                    match metadata::download_cover(client, &url, covers_dir, game.id) {
                        Ok(path) => return Ok(Some(path)),
                        Err(err) => note("SteamGridDB download failed", &err),
                    }
                }
                Ok(None) => {}
                Err(err) => note("SteamGridDB had nothing", &err),
            }
        }
        if let Some(app_id) = &steam_app {
            match metadata::fetch_store_info(client, app_id) {
                Ok(Some(info)) => {
                    // The store reply also has the description and requirements.
                    self.save_store_info(game, &info)?;
                    if let Some(url) = info.header_image {
                        match metadata::download_cover(client, &url, covers_dir, game.id) {
                            Ok(path) => return Ok(Some(path)),
                            Err(err) => note("store banner download failed", &err),
                        }
                    }
                }
                Ok(None) => {}
                Err(err) => note("Steam Store lookup failed", &err),
            }
        }
        // Last of all: the picture at the top of the game's Wikipedia
        // article (its cover or logo). This is how games that are on no
        // store GameLib can read, like Valorant, get a cover.
        match crate::covers_extra::wikipedia_cover_url(client, &game.title) {
            Ok(Some(url)) => match metadata::download_cover(client, &url, covers_dir, game.id) {
                Ok(path) => return Ok(Some(path)),
                Err(err) => note("Wikipedia picture download failed", &err),
            },
            Ok(None) => {}
            Err(err) => note("Wikipedia lookup failed", &err),
        }
        if unreachable {
            anyhow::bail!("the artwork servers couldn't be reached");
        }
        Ok(None)
    }

    fn fetch_missing_covers(&self, retry_all: bool) -> Result<()> {
        let mut downloaded = self.find_local_artwork()?;
        let mut failed = 0;

        let client = metadata::http_client()?;
        // No key just means fewer covers; everything else still works.
        let api_key = self
            .api_key(keys::STEAMGRIDDB_KEY, "SteamGridDB API key")
            .ok();

        // Games with no art anywhere would otherwise be looked up again on
        // every start, so each is only retried after a while.
        let retry_after = if retry_all { 0 } else { ART_RETRY_SECS };
        let due = self.db.games_due_cover_lookup(crate::now(), retry_after)?;
        let total = due.len();
        let mut offline = false;

        // One at a time, with a pause, to be polite to the servers. One
        // failure doesn't stop the rest.
        for (index, game) in due.into_iter().enumerate() {
            self.reporter
                .send(WorkerResult::CoverProgress { done: index, total });
            match self.download_cover_for(&client, api_key.as_deref(), &game) {
                Ok(Some(path)) => {
                    if self.db.set_cover_auto(game.id, &path)? {
                        downloaded += 1;
                        self.reporter.send(WorkerResult::CoverReady {
                            game_id: game.id,
                            path,
                        });
                    }
                    self.db.mark_art_checked(game.id, crate::now())?;
                }
                Ok(None) => {
                    log::info!("no cover found for {}", game.title);
                    self.db.mark_art_checked(game.id, crate::now())?;
                }
                // A failure (no internet, say) isn't recorded as checked,
                // so the game is tried again next time.
                Err(err) => {
                    failed += 1;
                    log::warn!("cover for {} failed: {err:#}", game.title);
                    // If the very first game can't reach any server, the
                    // rest won't either: stop rather than wait through a
                    // timeout for every game in the library.
                    if index == 0 {
                        failed = total;
                        offline = true;
                        break;
                    }
                }
            }
            thread::sleep(metadata::REQUEST_PAUSE);
        }

        // Description, genres, year and system requirements for Steam
        // games that lack them.
        let extras = self.db.all_extras()?;
        let needs_info = self.db.list_games()?.into_iter().filter(|g| {
            // No point asking if the servers couldn't be reached just now.
            !offline
                && g.source == Source::Steam
                && (g.description.is_none()
                    || extras
                        .get(&g.id)
                        .is_none_or(|e| e.requirements_min.is_none()))
        });
        for game in needs_info {
            match metadata::fetch_store_info(&client, &game.source_id) {
                Ok(Some(info)) => {
                    self.save_store_info(&game, &info)?;
                    // Remember that we asked, so a game with no requirements
                    // listed isn't fetched again on every scan.
                    if info.requirements_min.is_none() {
                        self.db.set_requirements(game.id, Some(""), None)?;
                    }
                    self.reporter
                        .send(WorkerResult::InfoReady { game_id: game.id });
                }
                Ok(None) => {
                    self.db.set_requirements(game.id, Some(""), None)?;
                }
                Err(err) => {
                    log::warn!("store info for {} failed: {err:#}", game.title);
                    if metadata::is_unreachable(&err) {
                        break; // offline: the rest would fail the same way
                    }
                }
            }
            thread::sleep(metadata::REQUEST_PAUSE);
        }
        self.reporter
            .send(WorkerResult::CoversFinished { downloaded, failed });
        Ok(())
    }

    /// Save what the Steam Store said about a game.
    fn save_store_info(&self, game: &Game, info: &metadata::StoreInfo) -> Result<()> {
        if game.description.is_none() {
            self.db.set_game_info(
                game.id,
                info.description.as_deref(),
                &info.genres,
                info.release_year,
            )?;
        }
        if info.requirements_min.is_some() || info.requirements_rec.is_some() {
            self.db.set_requirements(
                game.id,
                info.requirements_min.as_deref(),
                info.requirements_rec.as_deref(),
            )?;
        }
        Ok(())
    }

    /// The Steam Web API key and Steam account, or a clear message about
    /// which is missing.
    fn steam_account(&self) -> Result<(String, String)> {
        let api_key = self.api_key(keys::STEAM_API_KEY, "Steam Web API key")?;
        let steam_id = self
            .db
            .profile(crate::now())?
            .steam_id
            .context("no Steam account is linked in your profile")?;
        Ok((api_key, steam_id))
    }

    fn fetch_achievements(&self, game_id: Option<i64>) -> Result<()> {
        let (api_key, steam_id) = self.steam_account()?;
        let client = metadata::http_client()?;
        let games: Vec<Game> = match game_id {
            Some(id) => vec![self.db.require_game(id)?],
            None => self
                .db
                .list_games()?
                .into_iter()
                .filter(|g| g.source == Source::Steam && g.playtime_secs > 0)
                .collect(),
        };
        let mut done = 0;
        let mut failed = Vec::new();
        for game in games.iter().filter(|g| g.source == Source::Steam) {
            match crate::game_achievements::fetch_steam(
                &client,
                &api_key,
                &steam_id,
                &game.source_id,
                game.id,
            ) {
                // Games without achievements come back empty; that's fine.
                Ok(list) => {
                    self.db.save_steam_achievements(game.id, &list)?;
                    done += 1;
                }
                Err(err) => {
                    log::info!("achievements for {}: {err:#}", game.title);
                    failed.push(format!("{}: {err:#}", game.title));
                    if metadata::is_unreachable(&err) {
                        break; // offline: no point trying the rest
                    }
                }
            }
        }
        self.reporter.send(WorkerResult::AchievementsUpdated {
            games: done,
            failed,
        });
        Ok(())
    }

    fn fetch_friend_games(&self) -> Result<()> {
        let (api_key, _) = self.steam_account()?;
        let client = metadata::http_client()?;
        let friends = self.db.cached_friends()?;
        anyhow::ensure!(
            !friends.is_empty(),
            "no friends loaded yet: refresh your friends list on your profile first"
        );
        let mut private = 0;
        for friend in &friends {
            let games =
                crate::friend_games::fetch_owned_games(&client, &api_key, &friend.steam_id)?;
            if games.is_none() {
                private += 1;
            }
            self.db
                .save_friend_games(&friend.steam_id, games.as_deref(), crate::now())?;
        }
        self.reporter.send(WorkerResult::FriendGamesUpdated {
            friends: friends.len(),
            private,
        });
        Ok(())
    }

    fn refresh_friends(&self) -> Result<()> {
        let api_key = self.api_key(keys::STEAM_API_KEY, "Steam Web API key")?;
        let now = crate::now();
        let steam_id = self
            .db
            .profile(now)?
            .steam_id
            .context("no Steam account is linked in your profile")?;
        let client = metadata::http_client()?;
        let friends = friends::fetch_friends(&client, &api_key, &steam_id, now)?;
        self.db.save_friends(&friends)?;
        self.reporter.send(WorkerResult::FriendsUpdated(friends));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    const WAIT: Duration = Duration::from_secs(10);

    struct Fixture {
        dir: tempfile::TempDir,
        worker: Worker,
        wakes: Arc<AtomicUsize>,
    }

    impl Fixture {
        fn new() -> Fixture {
            let dir = tempfile::tempdir().unwrap();
            let config = WorkerConfig {
                db_path: dir.path().join("library.db"),
                covers_dir: dir.path().join("covers"),
                icons_dir: dir.path().join("icons"),
                timings: Timings::default(),
            };
            let wakes = Arc::new(AtomicUsize::new(0));
            let counter = Arc::clone(&wakes);
            let worker = Worker::spawn(config, move || {
                counter.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
            Fixture { dir, worker, wakes }
        }

        /// A second connection to the same database, like the UI has.
        fn db(&self) -> Db {
            Db::open(&self.dir.path().join("library.db")).unwrap()
        }

        fn next(&self) -> WorkerResult {
            self.worker
                .recv_timeout(WAIT)
                .expect("the worker should reply")
        }
    }

    #[test]
    fn import_job_saves_games_and_wakes_the_ui() {
        let f = Fixture::new();
        assert_eq!(f.worker.try_recv(), None, "nothing to report yet");

        let games = vec![
            DetectedGame::manual("One", "/g/one/one.exe"),
            DetectedGame::manual("Two", "/g/two/two.exe"),
        ];
        f.worker.send(Job::Import(games.clone()));
        assert_eq!(
            f.next(),
            WorkerResult::Imported {
                added: 2,
                updated: 0
            }
        );
        assert_eq!(f.wakes.load(Ordering::SeqCst), 1);

        f.worker.send(Job::Import(games));
        assert_eq!(
            f.next(),
            WorkerResult::Imported {
                added: 0,
                updated: 2
            }
        );

        // The UI's own connection sees what the worker saved.
        assert_eq!(f.db().list_games().unwrap().len(), 2);
    }

    #[test]
    fn folder_scan_returns_candidates_without_saving_them() {
        let f = Fixture::new();
        let games_dir = f.dir.path().join("games");
        for name in ["Alpha", "Beta"] {
            std::fs::create_dir_all(games_dir.join(name)).unwrap();
            std::fs::write(games_dir.join(name).join("game.exe"), b"x").unwrap();
        }
        let db = f.db();
        db.set_scan_folders(std::slice::from_ref(&games_dir))
            .unwrap();

        f.worker.send(Job::ScanFolders);
        let WorkerResult::FolderCandidates(candidates) = f.next() else {
            panic!("expected candidates");
        };
        assert_eq!(candidates.len(), 2);
        assert!(
            db.list_games().unwrap().is_empty(),
            "nothing saved until imported"
        );

        // Import one; the next scan only offers the other.
        f.worker.send(Job::Import(vec![candidates[0].clone()]));
        f.next();
        f.worker.send(Job::ScanFolders);
        let WorkerResult::FolderCandidates(remaining) = f.next() else {
            panic!("expected candidates");
        };
        assert_eq!(remaining, [candidates[1].clone()]);
    }

    #[test]
    fn errors_come_back_as_results_and_the_worker_keeps_going() {
        let f = Fixture::new();
        f.worker.send(Job::Launch {
            game_id: 404,
            profile: ProfileChoice::Default,
        });
        let WorkerResult::Error { context, message } = f.next() else {
            panic!("expected an error");
        };
        assert_eq!(context, "Couldn't start the game");
        assert!(message.contains("404"), "{message}");

        // A game whose exe has gone names the game in the message.
        let id = f
            .db()
            .add_manual_game("Gone", std::path::Path::new("/nowhere/gone.exe"), 1)
            .unwrap();
        f.worker.send(Job::Launch {
            game_id: id,
            profile: ProfileChoice::Default,
        });
        let WorkerResult::Error { context, .. } = f.next() else {
            panic!("expected an error");
        };
        assert_eq!(context, "Couldn't start Gone");

        // Still alive and working.
        f.worker.send(Job::Import(vec![]));
        assert_eq!(
            f.next(),
            WorkerResult::Imported {
                added: 0,
                updated: 0
            }
        );
    }

    #[test]
    fn jobs_needing_a_key_explain_what_is_missing() {
        let f = Fixture::new();
        f.worker.send(Job::RefreshFriends);
        let WorkerResult::Error { message, .. } = f.next() else {
            panic!("expected an error");
        };
        assert!(message.contains("Steam Web API key"), "{message}");

        let id = f
            .db()
            .add_manual_game("G", std::path::Path::new("/g/g.exe"), 1)
            .unwrap();
        f.worker.send(Job::FetchCover {
            game_id: id,
            search_title: None,
        });
        let WorkerResult::Error { message, .. } = f.next() else {
            panic!("expected an error");
        };
        assert!(message.contains("SteamGridDB API key"), "{message}");
    }

    #[test]
    fn shutdown_stops_the_thread() {
        let f = Fixture::new();
        f.worker.send(Job::Import(vec![]));
        f.worker.shutdown(); // returns only once the thread has finished
    }
}
