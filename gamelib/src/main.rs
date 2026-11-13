//! GameLib's program. With no arguments it opens the window; with
//! arguments it runs a command (see `gamelib help`).

// On Windows, GameLib is a window program: no black console window opens
// with it, and closing a console can't close GameLib. Commands typed in a
// terminal still print there (see `attach_console`).
#![cfg_attr(windows, windows_subsystem = "windows")]

mod cli;
mod text;
mod ui;

/// Print to the terminal GameLib was started from, if there is one. A
/// window program has no console of its own on Windows.
fn attach_console() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

fn main() {
    attach_console();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome = cli::init_logging().and_then(|()| match args.first().map(String::as_str) {
        None => ui::run(None),
        // `gamelib ui stats` opens the window on a particular screen.
        Some("ui") => ui::run(args.get(1).map(String::as_str)),
        Some(_) => cli::run(&args),
    });
    if let Err(err) = outcome {
        eprintln!("Error: {err:#}");
        std::process::exit(1);
    }
}
