//! GameLib's program. With no arguments it opens the window; with
//! arguments it runs a command (see `gamelib help`).

mod cli;
mod text;
mod ui;

fn main() {
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
