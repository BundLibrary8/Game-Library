//! Starting games.

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::model::{Game, LaunchTarget};

/// Start a game. Returns as soon as it has been asked to start; working out
/// when it is actually running is [`crate::playtime`]'s job.
pub fn launch(game: &Game) -> Result<()> {
    if let LaunchTarget::Exe { path, .. } = &game.launch
        && !path.is_file()
    {
        bail!(
            "{} is missing (moved or deleted?). Use \"Locate exe…\" to point at it again",
            path.display()
        );
    }
    let mut command = build_command(&game.launch)?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("couldn't start {}", game.title))?;
    // Wait for the process on a spare thread. Nothing is done with the
    // result, but an exited process that nobody waits for lingers in the
    // process list (a "zombie" on Linux/macOS), and the playtime tracker
    // would think the game was still running.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Work out the command that starts a launch target, without running it.
/// Kept separate from [`launch`] so it can be tested.
pub fn build_command(target: &LaunchTarget) -> Result<Command> {
    match target {
        LaunchTarget::Exe {
            path,
            args,
            working_dir,
        } => {
            let mut command = Command::new(path);
            command.args(args);
            // Many games look for their data files relative to the current
            // folder, so default to the folder the exe is in.
            let dir = working_dir.as_deref().or_else(|| path.parent());
            if let Some(dir) = dir.filter(|d| !d.as_os_str().is_empty()) {
                command.current_dir(dir);
            }
            Ok(command)
        }
        LaunchTarget::Uri(uri) => {
            check_uri(uri)?;
            Ok(uri_command(uri))
        }
    }
}

/// A launcher URI must look like `scheme://something` with no spaces or
/// control characters. It is handed to the operating system, so anything
/// odd is refused rather than passed on.
fn check_uri(uri: &str) -> Result<()> {
    let Some((scheme, rest)) = uri.split_once("://") else {
        bail!("'{uri}' is not a launcher link");
    };
    let scheme_ok = !scheme.is_empty()
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    let rest_ok = !rest.is_empty()
        && !rest
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '"');
    if !scheme_ok || !rest_ok {
        bail!("'{uri}' is not a launcher link");
    }
    Ok(())
}

/// The command that asks the operating system to open a URI with whatever
/// program is registered for it (Steam for `steam://`, Epic for
/// `com.epicgames.launcher://`).
fn uri_command(uri: &str) -> Command {
    if cfg!(windows) {
        // rundll32 rather than `cmd /C start`: cmd treats the `&` in Epic's
        // links as "run another command", which breaks them.
        let mut command = Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler").arg(uri);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        command.arg(uri);
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(uri);
        command
    }
}

/// True if `exe` looks like something that can be launched (it exists and
/// is a file). Used by "Locate exe…" and manual add.
pub fn is_launchable_exe(exe: &Path) -> bool {
    exe.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Source;
    use std::ffi::OsStr;
    use std::path::PathBuf;

    #[test]
    fn exe_command_has_args_and_runs_in_the_exe_folder() {
        let target = LaunchTarget::Exe {
            path: PathBuf::from("/games/ember/ember.exe"),
            args: vec!["-windowed".into(), "--save=slot 1".into()],
            working_dir: None,
        };
        let command = build_command(&target).unwrap();
        assert_eq!(command.get_program(), OsStr::new("/games/ember/ember.exe"));
        let args: Vec<&OsStr> = command.get_args().collect();
        assert_eq!(args, [OsStr::new("-windowed"), OsStr::new("--save=slot 1")]);
        assert_eq!(command.get_current_dir(), Some(Path::new("/games/ember")));
    }

    #[test]
    fn exe_command_uses_the_given_working_dir() {
        let target = LaunchTarget::Exe {
            path: PathBuf::from("/games/ember/bin/ember.exe"),
            args: vec![],
            working_dir: Some(PathBuf::from("/games/ember")),
        };
        let command = build_command(&target).unwrap();
        assert_eq!(command.get_current_dir(), Some(Path::new("/games/ember")));
    }

    #[test]
    fn uri_is_passed_whole_as_the_last_argument() {
        let uri = "com.epicgames.launcher://apps/ns%3Aitem%3Aapp?action=launch&silent=true";
        let command = build_command(&LaunchTarget::Uri(uri.into())).unwrap();
        assert_eq!(command.get_args().last(), Some(OsStr::new(uri)));
    }

    #[test]
    fn odd_uris_are_refused() {
        for bad in [
            "",
            "notepad.exe",
            "://nothing",
            "steam://",
            "steam://run game",
            "steam://run\"&calc",
            "1steam://rungameid/1",
        ] {
            assert!(
                build_command(&LaunchTarget::Uri(bad.into())).is_err(),
                "{bad:?}"
            );
        }
        assert!(build_command(&LaunchTarget::Uri("steam://rungameid/570".into())).is_ok());
    }

    #[test]
    fn launching_a_missing_exe_is_a_clear_error() {
        let game = Game {
            id: 1,
            title: "Gone".into(),
            source: Source::Manual,
            source_id: "/nowhere/gone.exe".into(),
            install_dir: None,
            launch: LaunchTarget::exe("/nowhere/gone.exe"),
            favourite: false,
            hidden: false,
            installed: true,
            playtime_secs: 0,
            last_played: None,
            added_at: 0,
            cover_path: None,
            icon_path: None,
            description: None,
            genres: vec![],
            release_year: None,
        };
        let err = launch(&game).unwrap_err().to_string();
        assert!(err.contains("missing"), "{err}");
    }
}
