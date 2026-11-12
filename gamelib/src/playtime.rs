//! Playtime tracking.
//!
//! The obvious approach ("time the process we started") doesn't work for
//! Steam and Epic: we only open a link, the launcher starts the game itself.
//! So instead **a game counts as running while any process's exe lives
//! inside the game's install folder**. That works for every source.
//!
//! The decision logic is the pure function [`step`]: given the state, "is
//! it running right now?" and the time, it returns the new state and what
//! to do. Tests feed it fake process lists and times; no game is launched.
//! [`track`] is the thin loop around it that does the real waiting.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use sysinfo::{ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System, UpdateKind};

use crate::db::{Db, SessionSummary};
use crate::paths::{is_inside, real_path};

/// The timings from the design, in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timings {
    /// How often to look for the game while waiting for it to appear.
    pub poll_waiting: i64,
    /// How often to check a running game is still running.
    pub poll_running: i64,
    /// How often to save progress, so a crash loses at most this much.
    pub checkpoint_every: i64,
    /// No matching process for this long means the game has closed.
    pub exit_grace: i64,
    /// Give up if the game never appears within this long of launching.
    pub start_timeout: i64,
}

impl Default for Timings {
    fn default() -> Self {
        Timings {
            poll_waiting: 2,
            poll_running: 5,
            checkpoint_every: 60,
            exit_grace: 15,
            start_timeout: 180,
        }
    }
}

/// Where the tracker is up to for one launched game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackState {
    /// Play was clicked; no process seen yet.
    WaitingToStart { launched_at: i64 },
    Running {
        started_at: i64,
        /// The last time a process was seen in the install folder.
        last_seen: i64,
        last_checkpoint: i64,
    },
    /// The game closed (or never started). Nothing more to do.
    Finished,
}

/// What the tracker should do after a [`step`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Nothing; check again later.
    Nothing,
    /// The game has appeared: open a session.
    StartSession { started_at: i64 },
    /// Save progress on the session.
    Checkpoint { at: i64 },
    /// The game has closed: finish the session.
    EndSession { ended_at: i64 },
    /// The game never appeared: record nothing.
    GiveUp,
}

/// The whole decision logic. Pure: no clock, no processes, no database.
pub fn step(
    state: TrackState,
    is_running: bool,
    now: i64,
    timings: &Timings,
) -> (TrackState, Action) {
    match state {
        TrackState::WaitingToStart { launched_at } => {
            if is_running {
                let running = TrackState::Running {
                    started_at: now,
                    last_seen: now,
                    last_checkpoint: now,
                };
                (running, Action::StartSession { started_at: now })
            } else if now - launched_at >= timings.start_timeout {
                (TrackState::Finished, Action::GiveUp)
            } else {
                (state, Action::Nothing)
            }
        }
        TrackState::Running {
            started_at,
            last_seen,
            last_checkpoint,
        } => {
            if is_running {
                let due = now - last_checkpoint >= timings.checkpoint_every;
                let next = TrackState::Running {
                    started_at,
                    last_seen: now,
                    last_checkpoint: if due { now } else { last_checkpoint },
                };
                let action = if due {
                    Action::Checkpoint { at: now }
                } else {
                    Action::Nothing
                };
                (next, action)
            } else if now - last_seen >= timings.exit_grace {
                // The session ends when the game was last seen, not now: the
                // grace period isn't playtime.
                (
                    TrackState::Finished,
                    Action::EndSession {
                        ended_at: last_seen,
                    },
                )
            } else {
                // Not seen, but not for long enough to be sure. Games often
                // restart themselves (launcher → game), so wait.
                (state, Action::Nothing)
            }
        }
        TrackState::Finished => (TrackState::Finished, Action::Nothing),
    }
}

/// Is any of these running exes inside the game's folder?
pub fn is_running_in(watch_dir: &Path, running_exes: &[PathBuf]) -> bool {
    running_exes.iter().any(|exe| is_inside(exe, watch_dir))
}

/// The exe path of every running process we're allowed to see.
pub fn running_exes(system: &mut System) -> Vec<PathBuf> {
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    system
        .processes()
        .values()
        // A zombie has exited but not been cleaned up yet: not running.
        .filter(|process| process.status() != ProcessStatus::Zombie)
        .filter_map(|process| process.exe().map(Path::to_path_buf))
        .collect()
}

/// What happened while tracking, reported back to whoever started it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackEvent {
    SessionStarted {
        game_id: i64,
        session_id: i64,
    },
    SessionEnded(SessionSummary),
    /// The game never appeared within the start timeout.
    GaveUp {
        game_id: i64,
    },
}

/// The outside world, as the tracker sees it. The real one is
/// [`SystemEnvironment`]; tests use a fake with a pretend clock.
pub trait Environment {
    fn now(&self) -> i64;
    fn sleep(&mut self, duration: Duration);
    fn running_exes(&mut self) -> Vec<PathBuf>;
}

/// The real clock and the real process list.
pub struct SystemEnvironment {
    system: System,
}

impl SystemEnvironment {
    pub fn new() -> Result<SystemEnvironment> {
        let system = System::new().context("couldn't get access to the process list")?;
        Ok(SystemEnvironment { system })
    }
}

impl Environment for SystemEnvironment {
    fn now(&self) -> i64 {
        crate::now()
    }
    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
    fn running_exes(&mut self) -> Vec<PathBuf> {
        running_exes(&mut self.system)
    }
}

/// Track one launched game from "Play clicked" until it closes (or never
/// starts). Blocks for as long as the game runs, so call it on a
/// background thread.
pub fn track(
    db: &Db,
    game_id: i64,
    watch_dir: &Path,
    timings: &Timings,
    env: &mut dyn Environment,
    mut on_event: impl FnMut(TrackEvent),
) -> Result<()> {
    // Compare like with like: the process list reports real, full paths.
    let watch_dir = real_path(watch_dir);
    let mut state = TrackState::WaitingToStart {
        launched_at: env.now(),
    };
    let mut session_id: Option<i64> = None;

    loop {
        let poll = match state {
            TrackState::WaitingToStart { .. } => timings.poll_waiting,
            TrackState::Running { .. } => timings.poll_running,
            TrackState::Finished => return Ok(()),
        };
        env.sleep(Duration::from_secs(poll.max(1) as u64));

        let running = is_running_in(&watch_dir, &env.running_exes());
        let (next, action) = step(state, running, env.now(), timings);
        state = next;

        match action {
            Action::Nothing => {}
            Action::StartSession { started_at } => {
                let id = db.start_session(game_id, started_at)?;
                session_id = Some(id);
                on_event(TrackEvent::SessionStarted {
                    game_id,
                    session_id: id,
                });
            }
            Action::Checkpoint { at } => {
                if let Some(id) = session_id {
                    db.checkpoint_session(id, at)?;
                }
            }
            Action::EndSession { ended_at } => {
                if let Some(id) = session_id {
                    on_event(TrackEvent::SessionEnded(db.finish_session(id, ended_at)?));
                }
            }
            Action::GiveUp => on_event(TrackEvent::GaveUp { game_id }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DetectedGame;

    const T: Timings = Timings {
        poll_waiting: 2,
        poll_running: 5,
        checkpoint_every: 60,
        exit_grace: 15,
        start_timeout: 180,
    };

    fn waiting(launched_at: i64) -> TrackState {
        TrackState::WaitingToStart { launched_at }
    }

    fn running(started_at: i64, last_seen: i64, last_checkpoint: i64) -> TrackState {
        TrackState::Running {
            started_at,
            last_seen,
            last_checkpoint,
        }
    }

    #[test]
    fn default_timings_match_the_design() {
        assert_eq!(Timings::default(), T);
    }

    #[test]
    fn waiting_keeps_waiting_until_the_process_appears() {
        assert_eq!(
            step(waiting(100), false, 110, &T),
            (waiting(100), Action::Nothing)
        );
        assert_eq!(
            step(waiting(100), true, 130, &T),
            (
                running(130, 130, 130),
                Action::StartSession { started_at: 130 }
            )
        );
    }

    #[test]
    fn gives_up_if_the_game_never_appears_within_three_minutes() {
        assert_eq!(step(waiting(100), false, 279, &T).1, Action::Nothing);
        assert_eq!(
            step(waiting(100), false, 280, &T),
            (TrackState::Finished, Action::GiveUp)
        );
        // …but a game that appears right at the deadline still counts.
        assert!(matches!(
            step(waiting(100), true, 280, &T).1,
            Action::StartSession { .. }
        ));
    }

    #[test]
    fn running_checkpoints_every_minute() {
        let (state, action) = step(running(1000, 1050, 1000), true, 1055, &T);
        assert_eq!(action, Action::Nothing);
        assert_eq!(state, running(1000, 1055, 1000));

        let (state, action) = step(state, true, 1060, &T);
        assert_eq!(action, Action::Checkpoint { at: 1060 });
        assert_eq!(state, running(1000, 1060, 1060));
    }

    #[test]
    fn a_short_gap_does_not_end_the_session() {
        // Seen at 1050, missing at 1055 and 1060 (launcher handing over)…
        let state = running(1000, 1050, 1000);
        let (state, action) = step(state, false, 1055, &T);
        assert_eq!(action, Action::Nothing);
        let (state, action) = step(state, false, 1060, &T);
        assert_eq!(action, Action::Nothing);
        // …then back again: still the same session.
        let (state, action) = step(state, true, 1064, &T);
        assert_eq!(state, running(1000, 1064, 1064));
        assert_eq!(action, Action::Checkpoint { at: 1064 });
    }

    #[test]
    fn fifteen_seconds_without_a_process_ends_the_session_at_last_seen() {
        let state = running(1000, 2000, 1980);
        assert_eq!(step(state, false, 2014, &T).1, Action::Nothing);
        assert_eq!(
            step(state, false, 2015, &T),
            (TrackState::Finished, Action::EndSession { ended_at: 2000 })
        );
    }

    #[test]
    fn finished_stays_finished() {
        assert_eq!(
            step(TrackState::Finished, true, 5, &T),
            (TrackState::Finished, Action::Nothing)
        );
    }

    #[test]
    fn running_means_an_exe_inside_the_install_folder() {
        let exes = vec![
            PathBuf::from("/usr/bin/bash"),
            PathBuf::from("/games/Ember Road/bin/ember.exe"),
        ];
        assert!(is_running_in(Path::new("/games/Ember Road"), &exes));
        assert!(
            !is_running_in(Path::new("/games/Ember"), &exes),
            "similar name, different folder"
        );
        assert!(!is_running_in(Path::new("/games/Other"), &exes));
        assert!(!is_running_in(Path::new("/games/Ember Road"), &[]));
    }

    #[test]
    fn the_real_process_list_includes_this_test() {
        let mut system = System::new().unwrap();
        let exes = running_exes(&mut system);
        let me = std::env::current_exe().unwrap();
        assert!(exes.contains(&me), "{} not in process list", me.display());
    }

    /// A pretend world: sleeping moves the clock, and the game "runs"
    /// between two chosen times.
    struct FakeEnvironment {
        now: i64,
        game_runs: std::ops::Range<i64>,
    }

    impl Environment for FakeEnvironment {
        fn now(&self) -> i64 {
            self.now
        }
        fn sleep(&mut self, duration: Duration) {
            self.now += duration.as_secs() as i64;
        }
        fn running_exes(&mut self) -> Vec<PathBuf> {
            if self.game_runs.contains(&self.now) {
                vec![PathBuf::from("/games/ember/ember.exe")]
            } else {
                vec![]
            }
        }
    }

    fn db_with_game() -> (Db, i64) {
        let db = Db::open_in_memory().unwrap();
        let id = db
            .upsert_detected(
                &DetectedGame::manual("Ember Road", "/games/ember/ember.exe"),
                1,
            )
            .unwrap()
            .game_id;
        (db, id)
    }

    #[test]
    fn tracking_a_ten_minute_session_records_about_ten_minutes() {
        let (db, game) = db_with_game();
        // Launched at 10 000; the game shows up 20 s later and runs 10 minutes.
        let mut env = FakeEnvironment {
            now: 10_000,
            game_runs: 10_020..10_620,
        };
        let mut events = Vec::new();
        track(&db, game, Path::new("/games/ember"), &T, &mut env, |e| {
            events.push(e)
        })
        .unwrap();

        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], TrackEvent::SessionStarted { .. }));
        let TrackEvent::SessionEnded(summary) = events[1] else {
            panic!("expected SessionEnded, got {:?}", events[1]);
        };
        // Polling every 5 s means the measurement can be a few seconds short.
        assert!(
            (590..=600).contains(&summary.duration_secs),
            "{}",
            summary.duration_secs
        );
        assert_eq!(summary.stardust, 1, "10 minutes → ✦1");

        let saved = db.require_game(game).unwrap();
        assert_eq!(saved.playtime_secs, summary.duration_secs);
        assert!(saved.last_played.is_some());
        assert_eq!(db.sessions_for_game(game).unwrap().len(), 1);
    }

    #[test]
    fn tracking_a_game_that_never_starts_records_nothing() {
        let (db, game) = db_with_game();
        let mut env = FakeEnvironment {
            now: 10_000,
            game_runs: 0..0,
        };
        let mut events = Vec::new();
        track(&db, game, Path::new("/games/ember"), &T, &mut env, |e| {
            events.push(e)
        })
        .unwrap();

        assert_eq!(events, [TrackEvent::GaveUp { game_id: game }]);
        assert!(env.now >= 10_180, "waited the full three minutes");
        assert!(db.sessions_for_game(game).unwrap().is_empty());
        assert_eq!(db.require_game(game).unwrap().playtime_secs, 0);
    }
}
