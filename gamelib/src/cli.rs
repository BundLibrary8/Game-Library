//! Command-line commands, for trying the backend from a terminal.
//! Run `gamelib help` for the list.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use gamelib::db::{Db, GameQuery, keys};
use gamelib::model::Game;
use gamelib::worker::{Job, Worker, WorkerConfig, WorkerResult};
use gamelib::{now, paths, shop};

use crate::text::format_duration;

const HELP: &str = "\
GameLib backend

Usage: gamelib              open the window
       gamelib <command>    run one of the commands below

Library
  list [search text]         show games (search is optional)
  scan                       find Steam, Epic and GOG games
  scan-folder <folder>       find loose games in a folder and add them
  add <exe> <title>          add a game by hand
  launch <game id>           start a game and track its playtime
  favourite <game id>        toggle favourite
  tag <game id> <tag>        add a tag
  collection <game id> <name>  add to a collection
  finish <game id>           mark a game finished
  covers                     download missing covers and store info

Stats, profile and shop
  stats                      totals and most-played games
  profile                    show the profile
  name <new name>            change the display name
  shop                       show the shop and your Stardust
  buy <item id>              buy a shop item
  friends                    refresh and show Steam friends

Settings
  set-key steamgriddb <key>  cover art API key
  set-key steam <key>        Steam Web API key (friends)
  set-steam-id <SteamID64>   link your Steam account
  where                      show where GameLib keeps its files
";

pub fn init_logging() -> Result<()> {
    use simplelog::{Config, LevelFilter, WriteLogger};
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths::log_path()?)
        .context("couldn't open the log file")?;
    // A second logger can't be installed; that isn't worth stopping for.
    let _ = WriteLogger::init(LevelFilter::Info, Config::default(), file);
    Ok(())
}

pub fn run(args: &[String]) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("help");
    let rest = args.get(1..).unwrap_or(&[]);
    if matches!(command, "help" | "--help" | "-h") {
        print!("{HELP}");
        return Ok(());
    }

    let db = Db::open(&paths::db_path()?)?;
    // Count any playtime left over from a crash.
    db.recover_unfinished_sessions()?;

    match command {
        "list" => {
            let search = (!rest.is_empty()).then(|| rest.join(" "));
            let games = db.query_games(&GameQuery {
                search,
                ..GameQuery::default()
            })?;
            if games.is_empty() {
                println!("No games yet. Try `gamelib scan` or `gamelib add <exe> <title>`.");
            }
            for game in &games {
                print_game(game);
            }
        }
        "scan" => {
            let worker = start_worker()?;
            worker.send(Job::ScanLibrary);
            match wait(&worker, 300)? {
                WorkerResult::ScanFinished(report) => {
                    println!("Scan finished: {}", report.summary());
                    for (source, error) in &report.errors {
                        println!("  {source}: {error}");
                    }
                }
                other => bail!("unexpected reply: {other:?}"),
            }
        }
        "scan-folder" => {
            let folder = paths::real_path(&PathBuf::from(one_arg(rest, "a folder")?));
            let mut folders = db.scan_folders()?;
            if !folders.contains(&folder) {
                folders.push(folder);
                db.set_scan_folders(&folders)?;
            }
            let worker = start_worker()?;
            worker.send(Job::ScanFolders);
            let WorkerResult::FolderCandidates(candidates) = wait(&worker, 300)? else {
                bail!("unexpected reply from the folder scan");
            };
            for game in &candidates {
                println!("  found {} ({})", game.title, game.source_id);
            }
            worker.send(Job::Import(candidates));
            if let WorkerResult::Imported { added, .. } = wait(&worker, 60)? {
                println!("Added {added} games.");
            }
        }
        "add" => {
            let [exe, title @ ..] = rest else {
                bail!("usage: gamelib add <exe> <title>");
            };
            let exe = Path::new(exe);
            if !gamelib::launcher::is_launchable_exe(exe) {
                bail!("{} isn't a file", exe.display());
            }
            let exe = paths::real_path(exe);
            let id = db.add_manual_game(&title.join(" "), &exe, now())?;
            println!("Added as game {id}.");
        }
        "launch" => {
            let game = db.require_game(game_id(rest)?)?;
            let worker = start_worker()?;
            worker.send(Job::Launch { game_id: game.id });
            println!("Starting {}…", game.title);
            loop {
                // A play session has no time limit, so wait as long as it takes.
                match wait(&worker, u64::MAX / 2000)? {
                    WorkerResult::Launched { daily_stardust, .. } if daily_stardust > 0 => {
                        println!("First game of the day: +✦{daily_stardust}");
                    }
                    WorkerResult::Launched { .. } => {}
                    WorkerResult::SessionStarted { .. } => {
                        println!("Running. Playtime is being tracked.")
                    }
                    WorkerResult::SessionEnded {
                        summary,
                        new_banners,
                    } => {
                        println!(
                            "Played for {}. +✦{}",
                            format_duration(summary.duration_secs),
                            summary.stardust
                        );
                        for banner in new_banners {
                            println!("New banner unlocked: {banner}");
                        }
                        break;
                    }
                    WorkerResult::NeverStarted { .. } => {
                        println!("The game never appeared, so nothing was recorded.");
                        break;
                    }
                    WorkerResult::Error { context, message } => bail!("{context}: {message}"),
                    _ => {}
                }
            }
        }
        "favourite" => {
            let game = db.require_game(game_id(rest)?)?;
            db.set_favourite(game.id, !game.favourite)?;
            println!(
                "{} is {} a favourite.",
                game.title,
                if game.favourite { "no longer" } else { "now" }
            );
        }
        "tag" | "collection" => {
            let [id, name @ ..] = rest else {
                bail!("usage: gamelib {command} <game id> <name>");
            };
            let game = db.require_game(id.parse().context("the game id must be a number")?)?;
            let tag = db.ensure_tag(&name.join(" "), command == "collection")?;
            db.tag_game(game.id, tag.id)?;
            println!("{} → {}", game.title, tag.name);
        }
        "finish" => {
            let game = db.require_game(game_id(rest)?)?;
            let reward = db.finish_game(game.id, now())?;
            println!("{} marked as finished. +✦{reward}", game.title);
            for banner in db.grant_earned_banners(now())? {
                println!("New banner unlocked: {banner}");
            }
        }
        "covers" => {
            let worker = start_worker()?;
            worker.send(Job::FetchMissingCovers);
            loop {
                match wait(&worker, 600)? {
                    WorkerResult::CoverReady { game_id, path } => {
                        println!("  cover for game {game_id}: {}", path.display());
                    }
                    WorkerResult::CoversFinished { downloaded, failed } => {
                        println!("Downloaded {downloaded} covers ({failed} failed).");
                        break;
                    }
                    WorkerResult::Error { context, message } => bail!("{context}: {message}"),
                    _ => {}
                }
            }
        }
        "stats" => {
            let stats = db.library_stats()?;
            println!(
                "Games:          {} ({} installed)",
                stats.game_count, stats.installed_count
            );
            println!(
                "Total playtime: {}",
                format_duration(stats.total_playtime_secs)
            );
            println!("Sessions:       {}", stats.session_count);
            println!("Finished:       {}", stats.finished_count);
            println!("\nMost played");
            for (rank, game) in db.top_played(10)?.iter().enumerate() {
                println!(
                    "  {:>2}. {:<40} {}",
                    rank + 1,
                    game.title,
                    format_duration(game.playtime_secs)
                );
            }
        }
        "profile" => {
            let profile = db.profile(now())?;
            println!("{}", profile.display_name);
            if let Some(bio) = &profile.bio {
                println!("{bio}");
            }
            println!("Badges:   {}", db.badges()?.join(", "));
            println!("Stardust: ✦{}", db.stardust_balance()?);
            println!(
                "Steam:    {}",
                profile.steam_id.as_deref().unwrap_or("not linked")
            );
        }
        "name" => {
            let spent = db.set_display_name(&rest.join(" "), now())?;
            println!(
                "Name changed{}.",
                if spent {
                    " (used a Name change)"
                } else {
                    " (free)"
                }
            );
        }
        "shop" => {
            db.profile(now())?;
            println!("Stardust: ✦{}\n", db.stardust_balance()?);
            for item in shop::items_for_sale() {
                let price = match item.price {
                    Some(_) if db.owns_item(item.id)? => "✓ owned".to_string(),
                    Some(price) => format!("✦{price}"),
                    None => String::new(),
                };
                println!(
                    "  {:<20} {:<18} {:<18} {price}",
                    item.id,
                    item.name,
                    format!("{:?}", item.kind)
                );
            }
        }
        "buy" => {
            let balance = db.purchase(one_arg(rest, "an item id")?, now())?;
            println!("Bought. Stardust left: ✦{balance}");
        }
        "friends" => {
            let worker = start_worker()?;
            worker.send(Job::RefreshFriends);
            match wait(&worker, 60)? {
                WorkerResult::FriendsUpdated(friends) => {
                    for friend in friends {
                        let status = friend
                            .playing
                            .map(|game| format!("playing {game}"))
                            .unwrap_or_else(|| friend.state.as_str().to_string());
                        println!("  {:<30} {status}", friend.name);
                    }
                }
                WorkerResult::Error { context, message } => bail!("{context}: {message}"),
                other => bail!("unexpected reply: {other:?}"),
            }
        }
        "set-key" => {
            let [which, key] = rest else {
                bail!("usage: gamelib set-key <steamgriddb|steam> <key>");
            };
            let setting = match which.as_str() {
                "steamgriddb" => keys::STEAMGRIDDB_KEY,
                "steam" => keys::STEAM_API_KEY,
                other => bail!("unknown key '{other}' (use steamgriddb or steam)"),
            };
            db.set_setting(setting, key.trim())?;
            println!("Saved.");
        }
        "set-steam-id" => {
            db.set_steam_id(Some(one_arg(rest, "a SteamID")?), now())?;
            println!("Saved.");
        }
        "where" => {
            println!("Data folder: {}", paths::data_dir()?.display());
            println!("Database:    {}", paths::db_path()?.display());
            println!("Log:         {}", paths::log_path()?.display());
        }
        other => bail!("unknown command '{other}'. Run `gamelib help`."),
    }
    Ok(())
}

fn start_worker() -> Result<Worker> {
    Worker::spawn(WorkerConfig::from_default_paths()?, || {})
}

/// Wait for the worker's next result.
fn wait(worker: &Worker, timeout_secs: u64) -> Result<WorkerResult> {
    worker
        .recv_timeout(Duration::from_secs(timeout_secs))
        .context("the worker didn't reply in time")
}

fn one_arg<'a>(rest: &'a [String], what: &str) -> Result<&'a str> {
    match rest {
        [value] => Ok(value),
        _ => bail!("expected {what}"),
    }
}

fn game_id(rest: &[String]) -> Result<i64> {
    one_arg(rest, "a game id")?
        .parse()
        .context("the game id must be a number")
}

fn print_game(game: &Game) {
    let mut notes = Vec::new();
    if game.favourite {
        notes.push("★".to_string());
    }
    if !game.installed {
        notes.push("not installed".to_string());
    }
    if game.playtime_secs > 0 {
        notes.push(format_duration(game.playtime_secs));
    }
    println!(
        "{:>4}  {:<40} {:<7} {}",
        game.id,
        game.title,
        game.source.as_str(),
        notes.join(" · ")
    );
}
