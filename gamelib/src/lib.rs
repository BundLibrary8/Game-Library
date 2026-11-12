//! GameLib backend: everything the app does that isn't drawing the window.
//!
//! The UI (egui) will sit on top of this crate. Nothing in here knows the UI
//! exists, which is what makes it testable with plain `cargo test`.
//!
//! Module map (see `plan/gamelib plan/02 Architecture.md`):
//! - [`model`]    core types: `Game`, `Source`, `LaunchTarget`…
//! - [`db`]       SQLite storage: games, tags, play sessions, settings
//! - [`sources`]  finding installed games (Steam, Epic, GOG, folder scan)
//! - [`launcher`] starting a game
//! - [`playtime`] working out when a game is running, and for how long
//! - [`metadata`] cover art and store info from the internet
//! - [`artwork`]  covers and icons found on this PC
//! - [`stats`]    totals and "most played" queries
//! - [`profile`]  the local profile, showcase, badges and earned banners
//! - [`friends`]  read-only Steam friends list
//! - [`shop`]     Stardust ledger, rewards and the cosmetics shop
//! - [`worker`]   the background thread the UI talks to through channels

pub mod artwork;
pub mod db;
pub mod friends;
pub mod launcher;
pub mod metadata;
pub mod model;
pub mod paths;
pub mod playtime;
pub mod profile;
pub mod shop;
pub mod sources;
pub mod stats;
pub mod worker;

pub use db::Db;
pub use model::{DetectedGame, Game, LaunchTarget, Source};

/// The current time as a unix timestamp (seconds since 1970, UTC).
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
