//! The window.
//!
//! egui is "immediate mode": every frame, [`GameLibApp::ui`] redraws the
//! whole window from the app's state. There are no callbacks. A click is
//! just `if ui.button("Play").clicked() { … }`.
//!
//! How the files fit together:
//! - `mod.rs` (this file): the app's state, loading data, talking to the
//!   worker, and the frame around every screen (top bar, sidebar or icon
//!   rail, status bar).
//! - `theme.rs`: colours and fonts for the two looks.
//! - `paint.rs`: drawing helpers (gradients, star map, covers, buttons).
//! - `home.rs`, `library.rs`, `details.rs`, `stats.rs`, `profile.rs`,
//!   `shop.rs`, `settings.rs`: one screen each.
//! - `dialogs.rs`: the pop-up windows.

mod details;
mod dialogs;
mod home;
mod library;
mod paint;
mod profile;
mod settings;
mod shop;
mod stats;
mod theme;

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::Result;
use eframe::egui::{self, Color32, Rect, Sense, Stroke, vec2};
use gamelib::db::{Db, GameQuery, SortBy, keys};
use gamelib::friends::Friend;
use gamelib::model::{Game, Source, Tag};
use gamelib::profile::Profile;
use gamelib::shop::LedgerEntry;
use gamelib::stats::LibraryStats;
use gamelib::worker::{Job, Worker, WorkerConfig, WorkerResult};
use gamelib::{now, paths};

use self::dialogs::Dialog;
use self::paint::{Icon, StarMap};
use self::theme::{Backdrop, Theme, bold, regular, semibold};
use crate::text::{short_playtime, thousands};

/// How long a shop preview lasts, in seconds.
const PREVIEW_SECS: f64 = 10.0;
/// Setting: "1" to make the Constellation stars twinkle.
const TWINKLE_KEY: &str = "ui.twinkle";

/// Which screen fills the middle of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Library,
    /// One game's page.
    Details(i64),
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
            "stats" => Screen::Stats,
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
    /// The equipped library background, if any.
    pub equipped: Option<&'static str>,
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
        Ok(Data {
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
            equipped: db.equipped_library_background()?.map(|item| item.id),
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
    twinkle: bool,
}

/// Open the window and run until it is closed.
pub fn run(start_screen: Option<&str>) -> Result<()> {
    let screen = match start_screen {
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
            Ok(Box::new(GameLibApp::new(db, worker, screen)?))
        }),
    )
    .map_err(|err| anyhow::anyhow!("the window couldn't be opened: {err}"))
}

impl GameLibApp {
    fn new(db: Db, worker: Worker, screen: Screen) -> Result<GameLibApp> {
        // Count any playtime left over from a crash, and make sure the
        // profile exists.
        db.recover_unfinished_sessions()?;
        db.profile(now())?;
        db.refresh_exe_installed()?;
        db.grant_earned_items(now())?;

        // GAMELIB_THEME=constellation (or aurora, synthwave, ember) shows a
        // look without owning it. For trying things out.
        let forced_backdrop = match std::env::var("GAMELIB_THEME").as_deref() {
            Ok("constellation") => Some(Backdrop::Constellation),
            Ok("aurora") => Some(Backdrop::Aurora),
            Ok("synthwave") => Some(Backdrop::Synthwave),
            Ok("ember") => Some(Backdrop::Plain),
            _ => None,
        };
        let settings = SettingsInputs {
            folders: db.scan_folders()?,
            steamgriddb_key: db.get_setting(keys::STEAMGRIDDB_KEY)?.unwrap_or_default(),
            steam_key: db.get_setting(keys::STEAM_API_KEY)?.unwrap_or_default(),
        };
        let mut app = GameLibApp {
            data: Data::load(&db, Filter::All, "", SortBy::LastPlayed)?,
            twinkle: db.get_bool_setting(TWINKLE_KEY, false)?,
            settings,
            db,
            worker,
            theme: Theme::ember(),
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
        if app.db.get_bool_setting(keys::SCAN_ON_STARTUP, false)? {
            app.start_job(Job::ScanLibrary, "Scanning for games…");
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
                    self.start_job(Job::FetchMissingCovers, "Getting covers and info…");
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
                    self.start_job(Job::FetchMissingCovers, "Getting covers and info…");
                }
                WorkerResult::Launched {
                    game_id,
                    daily_stardust,
                } => {
                    self.job_done();
                    self.running.insert(game_id);
                    self.status = if daily_stardust > 0 {
                        format!("Starting… First game of the day: +✦{daily_stardust}")
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
                        self.status.push_str(&format!(" +✦{}", summary.stardust));
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
                WorkerResult::InfoReady { .. } => {}
                WorkerResult::CoversFinished { downloaded, failed } => {
                    self.job_done();
                    self.status = if downloaded > 0 || failed > 0 {
                        format!("Downloaded {downloaded} covers ({failed} failed).")
                    } else {
                        String::new()
                    };
                }
                WorkerResult::FriendsUpdated(friends) => {
                    self.job_done();
                    self.status = format!("Friends updated ({}).", friends.len());
                }
                WorkerResult::Error { context, message } => {
                    self.job_done();
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

    /// The search box, shared by the top bar and the Constellation header.
    fn search_box(&mut self, ui: &mut egui::Ui, width: f32) {
        let hint = if self.theme.rail {
            "⌕  Search the stars…"
        } else {
            "⌕  Search your games…"
        };
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
        let text = egui::RichText::new(format!("✦ {}", thousands(self.data.stardust)))
            .font(semibold(15.0))
            .color(theme.stardust);
        if ui
            .button(text)
            .on_hover_text("Stardust: earned by playing, spent in the Shop")
            .clicked()
        {
            self.go(Screen::Shop);
        }
    }

    /// Ember's top bar: logo, navigation, search, buttons, avatar.
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        ui.horizontal_centered(|ui| {
            // Logo.
            let (logo, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
            paint::gradient(
                ui.painter(),
                logo,
                7.0,
                theme.accent,
                Color32::from_rgb(0xe0, 0x6a, 0x1a),
                vec2(0.4, 1.0),
            );
            ui.label(egui::RichText::new("GameLib").font(bold(19.0)));
            ui.add_space(60.0);

            for (screen, label) in [
                (Screen::Home, "Home"),
                (Screen::Library, "Library"),
                (Screen::Stats, "Stats"),
                (Screen::Shop, "Shop"),
            ] {
                let selected = self.screen == screen
                    || (screen == Screen::Library && matches!(self.screen, Screen::Details(_)));
                let colour = if selected { theme.text } else { theme.muted };
                let text = egui::RichText::new(label)
                    .font(semibold(15.0))
                    .color(colour);
                let button = egui::Button::new(text)
                    .fill(if selected {
                        theme.panel_alt
                    } else {
                        Color32::TRANSPARENT
                    })
                    .stroke(Stroke::NONE);
                if ui.add(button).clicked() {
                    self.go(screen);
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Avatar, with a ring in the accent colour.
                let (rect, response) = ui.allocate_exact_size(vec2(46.0, 46.0), Sense::click());
                ui.painter()
                    .circle_stroke(rect.center(), 22.0, Stroke::new(2.0, theme.accent));
                paint::avatar(
                    ui,
                    rect.center(),
                    19.0,
                    &self.data.profile.display_name,
                    self.data.profile.avatar_path.as_deref(),
                    &theme,
                );
                if response.on_hover_text("Profile").clicked() {
                    self.go(Screen::Profile);
                }
                self.stardust_pill(ui);
                self.add_game_button(ui);
                self.scan_button(ui);
                let width = (ui.available_width() - 24.0).clamp(140.0, 420.0);
                self.search_box(ui, width);
            });
        });
    }

    /// One row of Ember's sidebar: a label, an optional colour dot, and a
    /// count on the right.
    fn side_row(
        ui: &mut egui::Ui,
        theme: &Theme,
        label: &str,
        count: Option<usize>,
        selected: bool,
        dot: Option<Color32>,
    ) -> egui::Response {
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
        let painter = ui.painter();
        if selected || response.hovered() {
            let fill = if selected {
                theme.panel_alt
            } else {
                theme.panel_alt.gamma_multiply(0.5)
            };
            painter.rect_filled(rect.shrink2(vec2(8.0, 1.0)), 8.0, fill);
        }
        let mut x = rect.left() + 22.0;
        if let Some(colour) = dot {
            painter.rect_filled(
                Rect::from_center_size(egui::pos2(x + 4.0, rect.center().y), vec2(9.0, 9.0)),
                2.5,
                colour,
            );
            x += 18.0;
        }
        let font = if selected {
            semibold(15.0)
        } else {
            regular(15.0)
        };
        painter.text(
            egui::pos2(x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            font,
            theme.text,
        );
        if let Some(count) = count {
            painter.text(
                egui::pos2(rect.right() - 22.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                count.to_string(),
                regular(14.0),
                theme.muted,
            );
        }
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let in_library = matches!(self.screen, Screen::Library);
        let heading = |ui: &mut egui::Ui, text: &str| {
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.add_space(22.0);
                ui.label(paint::caps(text, 11.5, theme.muted));
            });
            ui.add_space(2.0);
        };
        let visible = &self.data.visible;
        let mut chosen: Option<Filter> = None;
        let mut new_collection = false;
        let settings_height = 52.0;

        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical()
            .max_height(ui.available_height() - settings_height)
            .auto_shrink(false)
            .show(ui, |ui| {
                heading(ui, "Library");
                let rows = [
                    (Filter::All, "All games", visible.len()),
                    (
                        Filter::Favourites,
                        "Favourites",
                        visible.iter().filter(|g| g.favourite).count(),
                    ),
                    (
                        Filter::RecentlyPlayed,
                        "Recently played",
                        visible.iter().filter(|g| g.last_played.is_some()).count(),
                    ),
                    (
                        Filter::Installed,
                        "Installed",
                        visible.iter().filter(|g| g.installed).count(),
                    ),
                    (Filter::Hidden, "Hidden", self.data.hidden_count),
                ];
                for (filter, label, count) in rows {
                    if filter == Filter::Hidden && count == 0 {
                        continue;
                    }
                    let selected = in_library && self.filter == filter;
                    if Self::side_row(ui, &theme, label, Some(count), selected, None).clicked() {
                        chosen = Some(filter);
                    }
                }

                heading(ui, "Collections");
                for (tag, count) in self.data.collections() {
                    let selected = in_library && self.filter == Filter::Tag(tag.id);
                    if Self::side_row(ui, &theme, &tag.name, Some(*count as usize), selected, None)
                        .clicked()
                    {
                        chosen = Some(Filter::Tag(tag.id));
                    }
                }
                let (rect, response) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
                ui.painter().text(
                    egui::pos2(rect.left() + 22.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    "+ New collection",
                    regular(15.0),
                    if response.hovered() {
                        theme.text
                    } else {
                        theme.muted
                    },
                );
                new_collection = response
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked();

                let plain_tags: Vec<_> = self
                    .data
                    .tags
                    .iter()
                    .filter(|(t, _)| !t.is_collection)
                    .collect();
                if !plain_tags.is_empty() {
                    heading(ui, "Tags");
                    for (tag, count) in plain_tags {
                        let selected = in_library && self.filter == Filter::Tag(tag.id);
                        if Self::side_row(
                            ui,
                            &theme,
                            &tag.name,
                            Some(*count as usize),
                            selected,
                            None,
                        )
                        .clicked()
                        {
                            chosen = Some(Filter::Tag(tag.id));
                        }
                    }
                }

                heading(ui, "Sources");
                for source in Source::ALL {
                    let count = visible.iter().filter(|g| g.source == source).count();
                    if count == 0 {
                        continue;
                    }
                    let selected = in_library && self.filter == Filter::Source(source);
                    let dot = Some(theme::source_colour(source));
                    if Self::side_row(
                        ui,
                        &theme,
                        theme::source_name(source),
                        Some(count),
                        selected,
                        dot,
                    )
                    .clicked()
                    {
                        chosen = Some(Filter::Source(source));
                    }
                }
                ui.add_space(12.0);
            });

        ui.add_space(8.0);
        if Self::side_row(
            ui,
            &theme,
            "⚙  Settings",
            None,
            self.screen == Screen::Settings,
            None,
        )
        .clicked()
        {
            self.go(Screen::Settings);
        }
        if let Some(filter) = chosen {
            self.show_filter(filter);
        }
        if new_collection {
            self.dialog = Dialog::NewCollection {
                name: String::new(),
            };
        }
    }

    /// Constellation's icon rail: a glass strip with the profile orb on
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
            ui.spacing_mut().item_spacing.y = 14.0;
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
                (Screen::Stats, Icon::Stats, "Stats"),
                (Screen::Shop, Icon::Shop, "Shop"),
                (Screen::Settings, Icon::Settings, "Settings"),
            ] {
                let selected = self.screen == screen
                    || (screen == Screen::Library && matches!(self.screen, Screen::Details(_)));
                let (rect, response) = ui.allocate_exact_size(vec2(54.0, 54.0), Sense::click());
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

        // Background first, then the panels on top of it.
        let twinkling = self.twinkle && theme.backdrop == Backdrop::Constellation;
        paint::backdrop(
            ui.painter(),
            ui.max_rect(),
            &theme,
            &self.stars,
            twinkling.then_some(time),
        );
        if twinkling {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }

        let bar = egui::Frame::new()
            .fill(theme.panel)
            .inner_margin(egui::Margin::symmetric(18, 8));
        egui::Panel::bottom("status_bar")
            .frame(bar)
            .show_separator_line(!theme.rail)
            .show(ui, |ui| self.status_bar(ui));
        if theme.rail {
            egui::Panel::left("rail")
                .exact_size(98.0)
                .resizable(false)
                .frame(egui::Frame::NONE)
                .show_separator_line(false)
                .show(ui, |ui| self.rail(ui));
        } else {
            egui::Panel::top("top_bar")
                .exact_size(64.0)
                .frame(
                    egui::Frame::new()
                        .fill(theme.panel)
                        .inner_margin(egui::Margin::symmetric(20, 0)),
                )
                .show(ui, |ui| self.top_bar(ui));
            egui::Panel::left("sidebar")
                .exact_size(264.0)
                .resizable(false)
                .frame(egui::Frame::new().fill(theme.panel))
                .show(ui, |ui| self.sidebar(ui));
        }

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
