//! End to end, with a real process: launch a "game" through the worker,
//! let the tracker spot it in the real process list, and check the
//! playtime that gets saved.
//!
//! The pretend game is a copy of the system's `sleep` program, so this
//! test only runs on Unix-like systems. On Windows, use the manual test
//! checklist with a real game.
#![cfg(unix)]

use std::path::PathBuf;
use std::time::Duration;

use gamelib::db::Db;
use gamelib::model::{DetectedGame, LaunchTarget, Source};
use gamelib::playtime::Timings;
use gamelib::worker::{Job, Worker, WorkerConfig, WorkerResult};

fn find_sleep() -> Option<PathBuf> {
    ["/bin/sleep", "/usr/bin/sleep"]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
}

#[test]
fn launching_a_real_process_records_its_playtime() {
    let Some(sleep) = find_sleep() else {
        eprintln!("no sleep program found; skipping");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let game_dir = dir.path().join("Pretend Game");
    std::fs::create_dir(&game_dir).unwrap();
    let exe = game_dir.join("pretend.exe");
    std::fs::copy(&sleep, &exe).unwrap();

    let db_path = dir.path().join("library.db");
    let db = Db::open(&db_path).unwrap();
    let game = DetectedGame {
        title: "Pretend Game".into(),
        source: Source::Manual,
        source_id: exe.to_string_lossy().into_owned(),
        install_dir: Some(game_dir.clone()),
        launch: LaunchTarget::Exe {
            path: exe,
            args: vec!["4".into()], // "play" for four seconds
            working_dir: None,
        },
    };
    let game_id = db.upsert_detected(&game, gamelib::now()).unwrap().game_id;

    // Short timings so the test takes seconds, not minutes.
    let config = WorkerConfig {
        db_path,
        covers_dir: dir.path().join("covers"),
        timings: Timings {
            poll_waiting: 1,
            poll_running: 1,
            checkpoint_every: 2,
            exit_grace: 2,
            start_timeout: 20,
        },
    };
    let worker = Worker::spawn(config, || {}).unwrap();
    worker.send(Job::Launch { game_id });

    let mut started = false;
    let summary = loop {
        match worker
            .recv_timeout(Duration::from_secs(30))
            .expect("tracker went quiet")
        {
            WorkerResult::Launched {
                game_id: id,
                daily_stardust,
            } => {
                assert_eq!(id, game_id);
                assert_eq!(daily_stardust, 5, "first launch of the day");
            }
            WorkerResult::SessionStarted { .. } => started = true,
            WorkerResult::SessionEnded { summary, .. } => break summary,
            other => panic!("unexpected result: {other:?}"),
        }
    };
    assert!(started, "SessionStarted should come before SessionEnded");
    assert_eq!(summary.game_id, game_id);
    assert!(
        (2..=5).contains(&summary.duration_secs),
        "a 4 s game measured as {} s",
        summary.duration_secs
    );

    let saved = db.require_game(game_id).unwrap();
    assert_eq!(saved.playtime_secs, summary.duration_secs);
    assert!(saved.last_played.is_some());
    assert_eq!(db.sessions_for_game(game_id).unwrap().len(), 1);
    assert_eq!(db.stardust_balance().unwrap(), 5);
}
