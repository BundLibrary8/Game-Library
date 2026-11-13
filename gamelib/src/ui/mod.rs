//! The window.
//!
//! egui is "immediate mode": every frame, [`GameLibApp::ui`] redraws the
//! whole window from the app's state. There are no callbacks. A click is
//! just `if ui.button("Play").clicked() { … }`.
//!
//! How the files fit together:
//! - `mod.rs` (this file): the app's state, loading data, talking to the
//!   worker, and the frame around every screen (icon rail, status bar).
//! - `theme.rs`: colours and fonts for each look.
//! - `paint.rs`: drawing helpers (gradients, star map, covers, buttons).
//! - `home.rs`, `library.rs`, `details.rs`, `discover.rs`, `assistant.rs`,
//!   `franchises.rs`, `stats.rs`, `profile.rs`, `shop.rs`, `settings.rs`:
//!   one screen each.
//! - `dialogs.rs`: the pop-up windows.
//! - `couch.rs`: Couch Mode, the full-screen controller-friendly view.

mod assistant;
mod couch;
mod details;
mod dialogs;
mod discover;
mod franchises;
mod game_extras;
mod home;
mod library;
mod paint;
mod profile;
mod settings;
mod shop;
mod smart;
mod stats;
mod theme;
mod wrapped;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::Result;
use eframe::egui::{self, Color32, Sense, Stroke, vec2};
use gamelib::backlog::{Goal, Habits};
use gamelib::db::{Db, GameQuery, SortBy, keys};
use gamelib::extras::GameExtras;
use gamelib::friends::Friend;
use gamelib::hardware::PcSpecs;
use gamelib::insights::{self, Recommendation};
use gamelib::model::{Game, Source, Tag};
use gamelib::profile::Profile;
use gamelib::shop::LedgerEntry;
use gamelib::stats::LibraryStats;
use gamelib::worker::{Job, Worker, WorkerConfig, WorkerResult};
use gamelib::{now, paths};

use self::dialogs::Dialog;
use self::paint::{Icon, StarMap};
use self::theme::{Backdrop, Theme};
use crate::text::{short_playtime, thousands};

/// How long a shop preview lasts, in seconds.
const PREVIEW_SECS: f64 = 10.0;
/// Setting: "1" to open straight into Couch Mode.
const COUCH_KEY: &str = "ui.start_couch";
/// Setting: "0" to stop the backgrounds moving.
const ANIMATE_KEY: &str = "ui.animate";

/// This computer's offset from UTC, in seconds, for showing times locally.
pub fn local_offset() -> i32 {
    chrono::Local::now().offset().local_minus_utc()
}

/// Which screen fills the middle of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Library,
    /// One game's page.
    Details(i64),
    /// Backlog Intelligence and suggestions.
    Discover,
    /// Ask questions about the library.
    Assistant,
    /// GameLib Wrapped: a look back at one year.
    Wrapped(i32),
    Franchises,
    Stats,
    Profile,
    Shop,
    Settings,
}

impl Screen {
    fn parse(name: &str) -> Option<Screen> {
        Some(match name {
            "home" => Screen::Home,
            "library" => Screen::Library,
            "discover" => Screen::Discover,
            "assistant" => Screen::Assistant,
            "franchises" => Screen::Franchises,
            "stats" => Screen::Stats,
            "wrapped" => Screen::Wrapped(gamelib::patterns::year_of(now(), local_offset())),
            "profile" => Screen::Profile,
            "shop" => Screen::Shop,
            "settings" => Screen::Settings,
            _ => return None,
        })
    }
}

/// Which games the library is narrowed down to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Installed,
    Favourites,
    RecentlyPlayed,
    Hidden,
    Tag(i64),
    Source(Source),
    /// A smart collection, by its place in the saved list.
    Smart(usize),
}

/// Everything loaded from the database for drawing. Screens read from
/// this instead of asking the database every frame; it is reloaded
/// whenever something changes.
pub struct Data {
    /// Every game that isn't hidden.
    pub visible: Vec<Game>,
    /// The games matching the current filter, search and sort.
    pub games: Vec<Game>,
    pub hidden_count: usize,
    /// Every tag, with how many games carry it.
    pub tags: Vec<(Tag, u64)>,
    pub profile: Profile,
    pub stats: LibraryStats,
    pub stardust: i64,
    pub owned: Vec<String>,
    pub badges: Vec<String>,
    pub showcase: Vec<i64>,
    pub friends: Vec<Friend>,
    pub friends_at: Option<i64>,
    pub name_credits: u32,
    pub ledger: Vec<LedgerEntry>,
    /// The library background in use: the equipped one, or the default.
    pub equipped: &'static str,
    /// Status, rating, franchise and folders for every game.
    pub extras: HashMap<i64, GameExtras>,
    /// What to play next, best first.
    pub recommendations: Vec<Recommendation>,
    /// How well every unplayed game matches your tastes, best first.
    pub taste: Vec<Recommendation>,
    /// Each game's tags, leaving out collections.
    pub taste_tags: HashMap<i64, Vec<String>>,
    /// How you tend to play.
    pub habits: Habits,
    /// Saved smart collections.
    pub smart: Vec<gamelib::smart::SmartCollection>,
    /// Every finished play session.
    pub sessions: Vec<gamelib::patterns::SessionRow>,
    /// When and how you play, over all sessions.
    pub patterns: gamelib::patterns::Patterns,
    pub achievements: Vec<gamelib::achievements::Achievement>,
    /// Each game's tags and collections.
    pub all_tags: HashMap<i64, Vec<String>>,
}

impl Data {
    fn load(db: &Db, filter: Filter, search: &str, sort: SortBy) -> Result<Data> {
        let mut query = GameQuery {
            search: Some(search.to_string()),
            sort,
            ..GameQuery::default()
        };
        match filter {
            Filter::All => {}
            Filter::Installed => query.installed_only = true,
            Filter::Favourites => query.favourites_only = true,
            Filter::RecentlyPlayed => query.sort = SortBy::LastPlayed,
            Filter::Hidden => query.include_hidden = true,
            Filter::Tag(tag_id) => query.tag_id = Some(tag_id),
            Filter::Source(source) => query.source = Some(source),
            Filter::Smart(_) => {} // applied below, once extras are loaded
        }
        let mut games = db.query_games(&query)?;
        match filter {
            Filter::RecentlyPlayed => games.retain(|g| g.last_played.is_some()),
            Filter::Hidden => games.retain(|g| g.hidden),
            _ => {}
        }

        let everything = db.list_games()?;
        let hidden_count = everything.iter().filter(|g| g.hidden).count();
        let visible: Vec<Game> = everything.into_iter().filter(|g| !g.hidden).collect();

        let mut tags = Vec::new();
        for tag in db.list_tags()? {
            let count = db.tag_game_count(tag.id)?;
            tags.push((tag, count));
        }
        let extras = db.all_extras()?;
        // Tags describe a game ("Roguelite"), so they count as tastes.
        // Collections ("Backlog", "Finished") are just piles, so they don't.
        let collections: HashSet<&str> = tags
            .iter()
            .filter(|(tag, _)| tag.is_collection)
            .map(|(tag, _)| tag.name.as_str())
            .collect();
        let all_tags = db.all_game_tags()?;
        let smart = db.smart_collections()?;
        if let Filter::Smart(index) = filter {
            match smart.get(index) {
                Some(collection) => {
                    let none = Vec::new();
                    games.retain(|g| {
                        let e = extras.get(&g.id).cloned().unwrap_or_default();
                        collection
                            .rules
                            .matches(g, &e, all_tags.get(&g.id).unwrap_or(&none), now())
                    });
                }
                None => games.clear(),
            }
        }
        let mut taste_tags = all_tags.clone();
        for names in taste_tags.values_mut() {
            names.retain(|name| !collections.contains(name.as_str()));
        }
        let taste = insights::recommend(&visible, &extras, &taste_tags, usize::MAX);
        let recommendations = taste.iter().take(12).cloned().collect();
        let sessions = db.session_rows(i64::MIN)?;
        let habits = gamelib::backlog::habits(&sessions, now());
        let patterns = gamelib::patterns::patterns(&sessions, now(), local_offset());
        let achievements =
            gamelib::achievements::achievements(&visible, &extras, &sessions, &patterns);
        Ok(Data {
            extras,
            recommendations,
            taste,
            taste_tags,
            habits,
            smart,
            all_tags,
            sessions,
            patterns,
            achievements,
            visible,
            games,
            hidden_count,
            tags,
            profile: db.profile(now())?,
            stats: db.library_stats()?,
            stardust: db.stardust_balance()?,
            owned: db.owned_items()?,
            badges: db.badges()?,
            showcase: db.showcase()?,
            friends: db.cached_friends()?,
            friends_at: db.friends_updated_at()?,
            name_credits: db.name_change_credits()?,
            ledger: db.stardust_ledger()?,
            equipped: db
                .equipped_library_background()?
                .map(|item| item.id)
                .unwrap_or(gamelib::shop::DEFAULT_BACKGROUND_ID),
        })
    }

    /// Look a game up by id among the visible and the filtered games.
    pub fn game(&self, id: i64) -> Option<&Game> {
        self.visible
            .iter()
            .chain(self.games.iter())
            .find(|g| g.id == id)
    }

    pub fn collections(&self) -> impl Iterator<Item = &(Tag, u64)> {
        self.tags.iter().filter(|(tag, _)| tag.is_collection)
    }
}

/// The values being edited on the settings screen.
pub struct SettingsInputs {
    pub folders: Vec<PathBuf>,
    pub steamgriddb_key: String,
    pub steam_key: String,
    pub anthropic_key: String,
}

/// Everything the window is showing right now.
pub struct GameLibApp {
    db: Db,
    worker: Worker,
    theme: Theme,
    /// False until the theme has been handed to egui for the first time.
    theme_applied: bool,
    stars: StarMap,

    screen: Screen,
    filter: Filter,
    search: String,
    sort: SortBy,
    shop_category: shop::Category,

    data: Data,
    /// Set when something changed and `data` needs reloading.
    dirty: bool,

    /// Games that are running (or starting) right now.
    running: HashSet<i64>,
    /// How many background jobs are in progress, for the spinner.
    busy: usize,
    status: String,
    error: Option<String>,
    /// The pop-up window that is open, if any.
    dialog: Dialog,
    settings: SettingsInputs,

    /// A background being tried out from the shop, and when the preview
    /// ends (in egui's clock, seconds).
    preview: Option<(Backdrop, f64)>,
    /// A background forced with the GAMELIB_THEME environment variable.
    forced_backdrop: Option<Backdrop>,
    /// This PC's specs, read once in the background at startup.
    specs: std::sync::Arc<std::sync::Mutex<Option<PcSpecs>>>,
    /// Couch Mode, while it's open.
    couch: Option<couch::Couch>,
    /// Set when Couch Mode was opened at startup and still needs full screen.
    couch_fullscreen_pending: bool,
    /// Game controllers.
    pad: couch::Pad,
    /// Discover: how much free time you have, and what you're after.
    discover_free: u32,
    discover_goal: Goal,
    /// The conversation on the Assistant screen.
    chat: assistant::Chat,
    /// What's being typed in a game's notes panel.
    notes_draft: Option<game_extras::NotesDraft>,
    /// Whether the backgrounds move.
    animate: bool,
    /// The clock the backgrounds are drawn at this frame, or `None` when
    /// they are standing still.
    anim_time: Option<f64>,
}

/// Open the window and run until it is closed.
pub fn run(start_screen: Option<&str>) -> Result<()> {
    // `gamelib ui couch` opens straight into Couch Mode.
    let start_couch = start_screen == Some("couch");
    let screen = match start_screen.filter(|name| *name != "couch") {
        Some(name) => {
            Screen::parse(name).ok_or_else(|| anyhow::anyhow!("unknown screen '{name}'"))?
        }
        None => Screen::Library,
    };
    let db = Db::open(&paths::db_path()?)?;
    let config = WorkerConfig::from_default_paths()?;

    // Normally the window is drawn with the graphics card's modern API.
    // Setting GAMELIB_RENDERER=glow switches to plain OpenGL, which helps
    // on PCs (and virtual machines) where the default fails to start.
    let renderer = match std::env::var("GAMELIB_RENDERER").as_deref() {
        Ok("glow") => eframe::Renderer::Glow,
        _ => eframe::Renderer::default(),
    };
    let options = eframe::NativeOptions {
        renderer,
        viewport: egui::ViewportBuilder::default()
            .with_title("GameLib")
            .with_inner_size([1360.0, 850.0])
            .with_min_inner_size([980.0, 620.0]),
        ..Default::default()
    };
    eframe::run_native(
        "GameLib",
        options,
        Box::new(move |cc| {
            // Lets egui show cover images straight from files.
            egui_extras::install_image_loaders(&cc.egui_ctx);
            theme::install_fonts(&cc.egui_ctx);
            // The worker wakes the window whenever it has a result, so the
            // screen updates even if the mouse isn't moving.
            let ctx = cc.egui_ctx.clone();
            let worker = Worker::spawn(config, move || ctx.request_repaint())?;
            Ok(Box::new(GameLibApp::new(db, worker, screen, start_couch)?))
        }),
    )
    .map_err(|err| anyhow::anyhow!("the window couldn't be opened: {err}"))
}

impl GameLibApp {
    fn new(db: Db, worker: Worker, screen: Screen, start_couch: bool) -> Result<GameLibApp> {
        // Count any playtime left over from a crash, and make sure the
        // profile exists.
        db.recover_unfinished_sessions()?;
        db.profile(now())?;
        db.refresh_exe_installed()?;
        db.grant_free_items(now())?;
        db.grant_earned_items(now())?;
        // Load anything your other PCs saved to the sync folder.
        match db.sync_pull(now()) {
            Ok(Some(summary)) => log::info!(
                "sync: loaded {} new and {} updated games",
                summary.added,
                summary.updated
            ),
            Ok(None) => {}
            Err(err) => log::warn!("couldn't load from the sync folder: {err:#}"),
        }

        // GAMELIB_THEME=constellation (or aurora, synthwave, ember) shows a
        // look without owning it. For trying things out.
        let forced_backdrop = match std::env::var("GAMELIB_THEME").as_deref() {
            Ok("constellation") => Some(Backdrop::Constellation),
            Ok("aurora") => Some(Backdrop::Aurora),
            Ok("synthwave") => Some(Backdrop::Synthwave),
            Ok("ember") => Some(Backdrop::Ember),
            _ => None,
        };
        let settings = SettingsInputs {
            folders: db.scan_folders()?,
            steamgriddb_key: db.get_setting(keys::STEAMGRIDDB_KEY)?.unwrap_or_default(),
            steam_key: db.get_setting(keys::STEAM_API_KEY)?.unwrap_or_default(),
            anthropic_key: db.get_setting(keys::ANTHROPIC_KEY)?.unwrap_or_default(),
        };
        // Reading the PC's specs takes a moment, so it's done on its own
        // thread rather than holding up the window.
        let specs = std::sync::Arc::new(std::sync::Mutex::new(None));
        let slot = std::sync::Arc::clone(&specs);
        std::thread::spawn(move || match gamelib::hardware::detect_specs() {
            Ok(found) => *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(found),
            Err(err) => log::warn!("couldn't read this PC's specs: {err:#}"),
        });
        let mut app = GameLibApp {
            specs,
            data: Data::load(&db, Filter::All, "", SortBy::LastPlayed)?,
            animate: db.get_bool_setting(ANIMATE_KEY, true)?,
            anim_time: None,
            couch: None,
            couch_fullscreen_pending: false,
            pad: couch::Pad::new(),
            discover_free: 120,
            discover_goal: Goal::Anything,
            chat: assistant::Chat::default(),
            notes_draft: None,
            settings,
            db,
            worker,
            theme: Theme::constellation(),
            theme_applied: false,
            stars: StarMap::new(),
            screen,
            filter: Filter::All,
            search: String::new(),
            sort: SortBy::LastPlayed,
            shop_category: shop::Category::All,
            dirty: true,
            running: HashSet::new(),
            busy: 0,
            status: String::new(),
            error: None,
            dialog: Dialog::None,
            preview: None,
            forced_backdrop,
        };
        if start_couch || app.db.get_bool_setting(COUCH_KEY, false)? {
            // Full screen is asked for on the first frame (there's no window yet).
            app.couch = Some(couch::Couch::new());
            app.couch_fullscreen_pending = true;
        }
        if app.db.get_bool_setting(keys::SCAN_ON_STARTUP, false)? {
            // The scan looks for artwork itself when it finishes.
            app.start_job(Job::ScanLibrary, "Scanning for games…");
        } else {
            // Find artwork for anything that lacks it, without being asked.
            app.start_job(
                Job::FetchMissingCovers { retry_all: false },
                "Finding artwork…",
            );
        }
        Ok(app)
    }

    // --- Helpers used by every screen ------------------------------------

    /// Send a job to the worker and show what's happening.
    fn start_job(&mut self, job: Job, status: &str) {
        self.worker.send(job);
        self.busy += 1;
        self.status = status.to_string();
    }

    fn job_done(&mut self) {
        self.busy = self.busy.saturating_sub(1);
    }

    /// Show an error in the red bar instead of crashing. Returns the value
    /// if there was one.
    fn check<T>(&mut self, result: Result<T>) -> Option<T> {
        self.dirty = true;
        match result {
            Ok(value) => Some(value),
            Err(err) => {
                log::error!("{err:#}");
                self.error = Some(format!("{err:#}"));
                None
            }
        }
    }

    fn go(&mut self, screen: Screen) {
        self.screen = screen;
    }

    /// Show the library narrowed to a filter.
    /// Delete a saved smart collection.
    fn delete_smart(&mut self, index: usize) {
        let mut list = self.data.smart.clone();
        if index < list.len() {
            list.remove(index);
            if self.check(self.db.set_smart_collections(&list)).is_some() {
                self.show_filter(Filter::All);
            }
        }
    }

    fn show_filter(&mut self, filter: Filter) {
        self.filter = filter;
        self.screen = Screen::Library;
        self.dirty = true;
    }

    /// Start a game (unless it's already running).
    fn play(&mut self, game_id: i64) {
        if self.running.insert(game_id) {
            self.start_job(Job::Launch { game_id }, "Starting…");
        }
    }

    /// This PC's specs, once they have been read.
    fn pc_specs(&self) -> Option<PcSpecs> {
        self.specs.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn title_of(&self, game_id: i64) -> String {
        match self.data.game(game_id) {
            Some(game) => game.title.clone(),
            None => "The game".to_string(),
        }
    }

    /// The background to draw right now: a shop preview wins, then a
    /// forced one, then whatever is equipped.
    fn current_backdrop(&self) -> Backdrop {
        if let Some((backdrop, _)) = self.preview {
            return backdrop;
        }
        self.forced_backdrop
            .unwrap_or_else(|| Backdrop::from_item(self.data.equipped))
    }

    /// Deal with everything the worker has finished since the last frame.
    /// `try_recv` never waits, so this can't freeze the window.
    fn handle_worker_results(&mut self, ctx: &egui::Context) {
        while let Some(result) = self.worker.try_recv() {
            self.dirty = true;
            match result {
                WorkerResult::ScanFinished(report) => {
                    self.job_done();
                    self.status = format!("Scan finished: {}", report.summary());
                    // Fetch covers and info for anything new.
                    self.start_job(
                        Job::FetchMissingCovers { retry_all: false },
                        "Getting covers and info…",
                    );
                }
                WorkerResult::FolderCandidates(candidates) => {
                    self.job_done();
                    if candidates.is_empty() {
                        self.status = "Folder scan found nothing new.".to_string();
                    } else {
                        self.status =
                            format!("Folder scan found {} games to review.", candidates.len());
                        self.dialog = Dialog::import(candidates);
                    }
                }
                WorkerResult::Imported { added, .. } => {
                    self.job_done();
                    self.status = format!("Added {added} games.");
                    self.start_job(
                        Job::FetchMissingCovers { retry_all: false },
                        "Getting covers and info…",
                    );
                }
                WorkerResult::Launched {
                    game_id,
                    daily_stardust,
                } => {
                    self.job_done();
                    self.running.insert(game_id);
                    self.status = if daily_stardust > 0 {
                        format!("Starting… First game of the day: +{daily_stardust} Stardust")
                    } else {
                        "Starting…".to_string()
                    };
                }
                WorkerResult::SessionStarted { game_id } => {
                    self.running.insert(game_id);
                    self.status = format!(
                        "{} is running. Playtime is being tracked.",
                        self.title_of(game_id)
                    );
                }
                WorkerResult::SessionEnded { summary, unlocked } => {
                    self.running.remove(&summary.game_id);
                    let length = if summary.duration_secs < 60 {
                        "under a minute".to_string()
                    } else {
                        short_playtime(summary.duration_secs)
                    };
                    self.status =
                        format!("Played {} for {length}.", self.title_of(summary.game_id));
                    if summary.stardust > 0 {
                        self.status
                            .push_str(&format!(" +{} Stardust", summary.stardust));
                    }
                    if !unlocked.is_empty() {
                        self.status
                            .push_str(" Something new unlocked on your profile!");
                    }
                }
                WorkerResult::NeverStarted { game_id } => {
                    self.running.remove(&game_id);
                    self.status = format!(
                        "{} never appeared, so no playtime was recorded.",
                        self.title_of(game_id)
                    );
                }
                WorkerResult::CoverReady { path, .. } => {
                    // The file may have replaced an older cover: drop egui's
                    // cached copy so the new one is shown.
                    ctx.forget_image(&paint::file_uri(&path));
                }
                WorkerResult::IconReady { path, .. } => {
                    ctx.forget_image(&paint::file_uri(&path));
                }
                WorkerResult::CoverProgress { done, total } => {
                    self.status = format!("Finding artwork… {} of {total}", done + 1);
                }
                WorkerResult::InfoReady { .. } => {}
                WorkerResult::CoversFinished { downloaded, failed } => {
                    self.job_done();
                    self.status = match (downloaded, failed) {
                        (0, 0) => String::new(),
                        (0, _) => format!(
                            "Couldn't get artwork for {failed} games (no internet?). It will be tried again next time."
                        ),
                        (_, 0) => format!("Found artwork for {downloaded} games."),
                        _ => format!(
                            "Found artwork for {downloaded} games; {failed} will be tried again next time."
                        ),
                    };
                }
                WorkerResult::FriendsUpdated(friends) => {
                    self.job_done();
                    self.status = format!("Friends updated ({}).", friends.len());
                }
                WorkerResult::AssistantReply(reply) => {
                    self.job_done();
                    self.status.clear();
                    self.claude_replied(reply);
                }
                WorkerResult::Error { context, message } => {
                    self.job_done();
                    if context.starts_with("Couldn't ask Claude") {
                        self.chat.waiting = false;
                    }
                    // A launch that failed isn't running.
                    if context.starts_with("Couldn't start") {
                        self.running.clear();
                    }
                    self.error = Some(format!("{context}: {message}"));
                }
            }
        }
    }

    // --- The frame around the screens ------------------------------------

    /// The search box in the library header.
    fn search_box(&mut self, ui: &mut egui::Ui, width: f32) {
        let hint = self.theme.search_hint();
        let edit = egui::TextEdit::singleline(&mut self.search)
            .hint_text(hint)
            .desired_width(width)
            .margin(egui::Margin::symmetric(14, 9));
        if ui.add(edit).changed() {
            self.screen = Screen::Library;
            self.dirty = true;
        }
    }

    fn scan_button(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("⟳ Scan", |ui| {
            if ui.button("Scan Steam, Epic and GOG").clicked() {
                self.start_job(Job::ScanLibrary, "Scanning for games…");
                ui.close();
            }
            if ui.button("Scan my game folders").clicked() {
                if self.settings.folders.is_empty() {
                    self.error = Some("Add a folder to scan in Settings first.".to_string());
                    self.screen = Screen::Settings;
                } else {
                    self.start_job(Job::ScanFolders, "Scanning folders…");
                }
                ui.close();
            }
        });
    }

    fn add_game_button(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        if paint::accent_button(ui, &theme, "+ Add game", 14.5, vec2(18.0, 10.0)).clicked() {
            self.dialog = Dialog::pick_game_to_add();
        }
    }

    /// The Stardust balance as a clickable pill that opens the shop.
    fn stardust_pill(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        if paint::stardust_button(ui, &theme, &thousands(self.data.stardust), 15.0)
            .on_hover_text("Stardust: earned by playing, spent in the Shop")
            .clicked()
        {
            self.go(Screen::Shop);
        }
    }

    /// The icon rail: a glass strip with the profile orb on
    /// top and one icon per screen.
    fn rail(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let area = ui.max_rect().shrink2(vec2(10.0, 16.0));
        ui.painter().rect_filled(area, 22.0, theme.panel);
        ui.painter().rect_stroke(
            area,
            22.0,
            Stroke::new(1.0, theme.border),
            egui::StrokeKind::Inside,
        );

        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.add_space(34.0);

            // The orb: your profile.
            let (rect, response) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::click());
            let [r, g, b, _] = theme.accent.to_array();
            paint::glow(
                ui.painter(),
                rect.center(),
                34.0,
                Color32::from_rgba_unmultiplied(r, g, b, 70),
            );
            paint::avatar(
                ui,
                rect.center(),
                20.0,
                &self.data.profile.display_name,
                self.data.profile.avatar_path.as_deref(),
                &theme,
            );
            if response
                .on_hover_text("Profile")
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                self.go(Screen::Profile);
            }
            ui.add_space(18.0);

            for (screen, icon, tip) in [
                (Screen::Home, Icon::Home, "Home"),
                (Screen::Library, Icon::Library, "Library"),
                (
                    Screen::Discover,
                    Icon::Discover,
                    "Discover: your backlog and what to play",
                ),
                (
                    Screen::Assistant,
                    Icon::Assistant,
                    "Assistant: ask about your library",
                ),
                (Screen::Franchises, Icon::Franchises, "Franchises"),
                (Screen::Stats, Icon::Stats, "Stats"),
                (Screen::Shop, Icon::Shop, "Shop"),
                (Screen::Settings, Icon::Settings, "Settings"),
            ] {
                let selected = self.screen == screen
                    || (screen == Screen::Library && matches!(self.screen, Screen::Details(_)))
                    || (screen == Screen::Stats && matches!(self.screen, Screen::Wrapped(_)));
                let (rect, response) = ui.allocate_exact_size(vec2(50.0, 50.0), Sense::click());
                if selected {
                    ui.painter().rect_filled(rect, 14.0, theme.panel_alt);
                    ui.painter().rect_stroke(
                        rect,
                        14.0,
                        Stroke::new(1.0, theme.accent.gamma_multiply(0.5)),
                        egui::StrokeKind::Inside,
                    );
                } else if response.hovered() {
                    ui.painter()
                        .rect_filled(rect, 14.0, theme.panel_alt.gamma_multiply(0.5));
                }
                let colour = if selected { theme.accent } else { theme.muted };
                paint::icon(ui.painter(), icon, rect.center(), colour);
                if response
                    .on_hover_text(tip)
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    self.go(screen);
                }
            }

            // Couch Mode, at the bottom of the rail.
            ui.add_space(10.0);
            let (rect, response) = ui.allocate_exact_size(vec2(54.0, 54.0), Sense::click());
            if response.hovered() {
                ui.painter()
                    .rect_filled(rect, 14.0, theme.panel_alt.gamma_multiply(0.5));
            }
            paint::icon(ui.painter(), Icon::Couch, rect.center(), theme.muted);
            if response
                .on_hover_text("Couch Mode: full screen, for a TV and controller (F11)")
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                let ctx = ui.ctx().clone();
                self.enter_couch(&ctx);
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        if let Some(error) = self.error.clone() {
            ui.horizontal(|ui| {
                ui.colored_label(theme.danger, format!("⚠ {error}"));
                if ui.small_button("Dismiss").clicked() {
                    self.error = None;
                }
            });
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if self.busy > 0 {
                ui.spinner();
            } else {
                let (dot, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 4.5, theme.success);
            }
            let status = if self.status.is_empty() {
                "Library up to date"
            } else {
                &self.status
            };
            ui.label(paint::muted(status, &theme).size(13.0));
            ui.add_space(16.0);
            let sources: HashSet<Source> = self.data.visible.iter().map(|g| g.source).collect();
            ui.label(
                paint::muted(
                    format!(
                        "{} games · {} sources",
                        self.data.visible.len(),
                        sources.len()
                    ),
                    &theme,
                )
                .size(13.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let with_cover = self
                    .data
                    .visible
                    .iter()
                    .filter(|g| g.cover_path.is_some())
                    .count();
                ui.label(
                    paint::muted(
                        format!("Covers {with_cover}/{}", self.data.visible.len()),
                        &theme,
                    )
                    .size(13.0),
                );
            });
        });
    }
}

impl eframe::App for GameLibApp {
    /// Save the library to the sync folder (if one is set) on the way out.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Err(err) = self.db.sync_push(now()) {
            log::warn!("couldn't save to the sync folder: {err:#}");
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_worker_results(&ctx);
        if self.dirty {
            self.dirty = false;
            match Data::load(&self.db, self.filter, &self.search, self.sort) {
                Ok(data) => self.data = data,
                Err(err) => self.error = Some(format!("{err:#}")),
            }
        }

        // End a shop preview when its time is up.
        let time = ctx.input(|i| i.time);
        if let Some((_, until)) = self.preview {
            if time >= until {
                self.preview = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(until - time));
            }
        }

        // Pick the look, and tell egui when it changes.
        let theme = Theme::for_backdrop(self.current_backdrop());
        if theme != self.theme || !self.theme_applied {
            self.theme = theme;
            self.theme_applied = true;
            theme.apply(&ctx);
        }

        // Controllers, and Couch Mode, which takes over the whole window.
        let pad_input = self.pad.poll(time);
        if std::mem::take(&mut self.couch_fullscreen_pending) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        }
        if let Some(couch) = &self.couch {
            let input = pad_input.merge(couch::keyboard_input(&ctx, couch.typing()));
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| self.couch_screen(ui, input));
            return;
        }
        if pad_input.start || ctx.input(|i| i.key_pressed(egui::Key::F11)) {
            self.enter_couch(&ctx);
        }
        if self.pad.connected() {
            // Look for the controller's Start button now and then.
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Background first, then the panels on top of it. An animated
        // background needs the window redrawn about 30 times a second.
        let animating = self.animate;
        self.anim_time = self.animate.then_some(time);
        paint::backdrop(
            ui.painter(),
            ui.max_rect(),
            &theme,
            &self.stars,
            animating.then_some(time),
        );
        if animating {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }

        let bar = egui::Frame::new()
            .fill(theme.panel)
            .inner_margin(egui::Margin::symmetric(18, 8));
        egui::Panel::bottom("status_bar")
            .frame(bar)
            .show_separator_line(false)
            .show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("rail")
            .exact_size(98.0)
            .resizable(false)
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show(ui, |ui| self.rail(ui));

        // The profile page runs edge to edge (its banner touches the
        // sides); every other screen gets a margin. The margin is inside
        // the scroll area, so the scroll bar sits at the window's edge.
        let margin = if self.screen == Screen::Profile {
            0
        } else {
            28
        };
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        egui::Frame::new()
                            .inner_margin(egui::Margin::symmetric(margin, 0))
                            .show(ui, |ui| {
                                if margin > 0 {
                                    ui.add_space(22.0);
                                }
                                match self.screen {
                                    Screen::Home => self.home_screen(ui),
                                    Screen::Library => self.library_screen(ui),
                                    Screen::Details(game_id) => self.details_screen(ui, game_id),
                                    Screen::Discover => self.discover_screen(ui),
                                    Screen::Assistant => self.assistant_screen(ui),
                                    Screen::Wrapped(year) => self.wrapped_screen(ui, year),
                                    Screen::Franchises => self.franchises_screen(ui),
                                    Screen::Stats => self.stats_screen(ui),
                                    Screen::Profile => self.profile_screen(ui),
                                    Screen::Shop => self.shop_screen(ui),
                                    Screen::Settings => self.settings_screen(ui),
                                }
                                ui.add_space(28.0);
                            });
                    });
            });

        self.show_dialog(&ctx);
    }
}
