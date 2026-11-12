//! The temporary window.
//!
//! This is a plain, working front end so the backend can be used with a
//! mouse. It is meant to be replaced by the designed UI in
//! `plan/gamelib plan/14 UI Designs.md`; nothing in the backend depends
//! on it.
//!
//! egui is "immediate mode": every frame, [`GameLibApp::ui`] redraws the
//! whole window from the app's state. There are no callbacks. A click is
//! just `if ui.button("Play").clicked() { … }`.

mod library;
mod profile;
mod settings;
mod shop;
mod stats;

use std::collections::HashSet;

use anyhow::Result;
use eframe::egui;
use gamelib::db::{Db, GameQuery, SortBy, keys};
use gamelib::model::{DetectedGame, Game, Source, Tag};
use gamelib::worker::{Job, Worker, WorkerConfig, WorkerResult};
use gamelib::{now, paths};

use crate::text::format_duration;

/// Which screen fills the middle of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Library,
    Stats,
    Profile,
    Shop,
    Settings,
}

impl Screen {
    fn parse(name: &str) -> Option<Screen> {
        Some(match name {
            "library" => Screen::Library,
            "stats" => Screen::Stats,
            "profile" => Screen::Profile,
            "shop" => Screen::Shop,
            "settings" => Screen::Settings,
            _ => return None,
        })
    }
}

/// Which games the sidebar has narrowed the library down to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Favourites,
    RecentlyPlayed,
    Hidden,
    Tag(i64),
    Source(Source),
}

/// One row of the import review window.
pub struct ImportRow {
    pub ticked: bool,
    pub game: DetectedGame,
}

/// Everything the window is showing right now.
pub struct GameLibApp {
    db: Db,
    worker: Worker,

    screen: Screen,
    filter: Filter,
    search: String,
    sort: SortBy,
    selected: Option<i64>,

    /// The games matching the current filter, loaded from the database.
    /// The grid draws from this list, not the database, so it stays fast.
    games: Vec<Game>,
    /// Every tag, with how many games carry it.
    tags: Vec<(Tag, u64)>,
    /// How many games each source has.
    source_counts: Vec<(Source, usize)>,
    stardust: i64,
    display_name: String,
    /// Set when something changed and the lists above need reloading.
    dirty: bool,

    /// Games that are running (or starting) right now.
    running: HashSet<i64>,
    /// How many background jobs are in progress, for the spinner.
    busy: usize,
    status: String,
    error: Option<String>,

    /// The folder-scan results being reviewed, if that window is open.
    import: Option<Vec<ImportRow>>,
    library: library::LibraryInputs,
    profile: profile::ProfileInputs,
    settings: settings::SettingsInputs,
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
            .with_inner_size([1200.0, 760.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "GameLib",
        options,
        Box::new(move |cc| {
            // Lets egui show cover images straight from files.
            egui_extras::install_image_loaders(&cc.egui_ctx);
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

        let mut app = GameLibApp {
            settings: settings::SettingsInputs::load(&db)?,
            profile: profile::ProfileInputs::load(&db)?,
            library: library::LibraryInputs::default(),
            db,
            worker,
            screen,
            filter: Filter::All,
            search: String::new(),
            sort: SortBy::Title,
            selected: None,
            games: Vec::new(),
            tags: Vec::new(),
            source_counts: Vec::new(),
            stardust: 0,
            display_name: String::new(),
            dirty: true,
            running: HashSet::new(),
            busy: 0,
            status: "Ready".to_string(),
            error: None,
            import: None,
        };
        if app.db.get_bool_setting(keys::SCAN_ON_STARTUP, false)? {
            app.start_job(Job::ScanLibrary, "Scanning for games…");
        }
        Ok(app)
    }

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
        match result {
            Ok(value) => {
                self.dirty = true;
                Some(value)
            }
            Err(err) => {
                log::error!("{err:#}");
                self.error = Some(format!("{err:#}"));
                None
            }
        }
    }

    /// Reload the in-memory lists from the database.
    fn reload(&mut self) -> Result<()> {
        let mut query = GameQuery {
            search: Some(self.search.clone()),
            sort: self.sort,
            ..GameQuery::default()
        };
        match self.filter {
            Filter::All => {}
            Filter::Favourites => query.favourites_only = true,
            Filter::RecentlyPlayed => query.sort = SortBy::LastPlayed,
            Filter::Hidden => query.include_hidden = true,
            Filter::Tag(tag_id) => query.tag_id = Some(tag_id),
            Filter::Source(source) => query.source = Some(source),
        }
        let mut games = self.db.query_games(&query)?;
        match self.filter {
            Filter::RecentlyPlayed => games.retain(|g| g.last_played.is_some()),
            Filter::Hidden => games.retain(|g| g.hidden),
            _ => {}
        }
        self.games = games;

        self.tags.clear();
        for tag in self.db.list_tags()? {
            let count = self.db.tag_game_count(tag.id)?;
            self.tags.push((tag, count));
        }
        let all = self.db.query_games(&GameQuery::default())?;
        self.source_counts = Source::ALL
            .iter()
            .map(|&source| (source, all.iter().filter(|g| g.source == source).count()))
            .filter(|(_, count)| *count > 0)
            .collect();
        self.stardust = self.db.stardust_balance()?;
        self.display_name = self.db.profile(now())?.display_name;
        Ok(())
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
                        self.import = Some(
                            candidates
                                .into_iter()
                                .map(|game| ImportRow { ticked: true, game })
                                .collect(),
                        );
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
                WorkerResult::SessionEnded {
                    summary,
                    new_banners,
                } => {
                    self.running.remove(&summary.game_id);
                    self.status = format!(
                        "Played {} for {}.",
                        self.title_of(summary.game_id),
                        format_duration(summary.duration_secs)
                    );
                    if summary.stardust > 0 {
                        self.status.push_str(&format!(" +✦{}", summary.stardust));
                    }
                    if !new_banners.is_empty() {
                        self.status.push_str(" New profile banner unlocked!");
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
                    ctx.forget_image(&library::file_uri(&path));
                }
                WorkerResult::InfoReady { .. } => {}
                WorkerResult::CoversFinished { downloaded, failed } => {
                    self.job_done();
                    if downloaded > 0 || failed > 0 {
                        self.status = format!("Downloaded {downloaded} covers ({failed} failed).");
                    }
                }
                WorkerResult::FriendsUpdated(friends) => {
                    self.job_done();
                    self.status = format!("Friends updated ({}).", friends.len());
                }
                WorkerResult::Error { context, message } => {
                    self.job_done();
                    self.error = Some(format!("{context}: {message}"));
                }
            }
        }
    }

    fn title_of(&self, game_id: i64) -> String {
        match self.db.get_game(game_id) {
            Ok(Some(game)) => game.title,
            _ => "The game".to_string(),
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("GameLib");
            ui.separator();

            let search = egui::TextEdit::singleline(&mut self.search)
                .hint_text("Search games…")
                .desired_width(220.0);
            if ui.add(search).changed() {
                self.screen = Screen::Library;
                self.dirty = true;
            }

            let before = self.sort;
            egui::ComboBox::from_id_salt("sort")
                .selected_text(sort_name(self.sort))
                .show_ui(ui, |ui| {
                    for sort in [
                        SortBy::Title,
                        SortBy::LastPlayed,
                        SortBy::Playtime,
                        SortBy::RecentlyAdded,
                    ] {
                        ui.selectable_value(&mut self.sort, sort, sort_name(sort));
                    }
                });
            self.dirty |= self.sort != before;

            ui.separator();
            if ui
                .button("Scan")
                .on_hover_text("Find Steam, Epic and GOG games")
                .clicked()
            {
                self.start_job(Job::ScanLibrary, "Scanning for games…");
            }
            if ui
                .button("Scan folders")
                .on_hover_text("Look for loose games in the folders set in Settings")
                .clicked()
            {
                if self.settings.folders.is_empty() {
                    self.error = Some("Add a folder to scan in Settings first.".to_string());
                } else {
                    self.start_job(Job::ScanFolders, "Scanning folders…");
                }
            }
            if ui.button("+ Add game").clicked() {
                self.library.add_game_from_dialog();
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(&self.display_name)
                    .on_hover_text("Profile")
                    .clicked()
                {
                    self.screen = Screen::Profile;
                }
                if ui
                    .button(format!("✦ {}", self.stardust))
                    .on_hover_text("Stardust: earned by playing, spent in the Shop")
                    .clicked()
                {
                    self.screen = Screen::Shop;
                }
            });
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let pick = |ui: &mut egui::Ui, app: &mut GameLibApp, filter: Filter, label: String| {
            let active = app.screen == Screen::Library && app.filter == filter;
            if ui.selectable_label(active, label).clicked() {
                app.screen = Screen::Library;
                app.filter = filter;
                app.dirty = true;
            }
        };

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(4.0);
            ui.label(egui::RichText::new("LIBRARY").small().weak());
            pick(ui, self, Filter::All, "All games".to_string());
            pick(ui, self, Filter::Favourites, "★ Favourites".to_string());
            pick(
                ui,
                self,
                Filter::RecentlyPlayed,
                "Recently played".to_string(),
            );
            pick(ui, self, Filter::Hidden, "Hidden".to_string());

            let tags = self.tags.clone();
            for (heading, want_collections) in [("COLLECTIONS", true), ("TAGS", false)] {
                let group: Vec<_> = tags
                    .iter()
                    .filter(|(t, _)| t.is_collection == want_collections)
                    .collect();
                if group.is_empty() {
                    continue;
                }
                ui.add_space(8.0);
                ui.label(egui::RichText::new(heading).small().weak());
                for (tag, count) in group {
                    pick(
                        ui,
                        self,
                        Filter::Tag(tag.id),
                        format!("{} ({count})", tag.name),
                    );
                }
            }

            if !self.source_counts.is_empty() {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("SOURCES").small().weak());
                for (source, count) in self.source_counts.clone() {
                    pick(
                        ui,
                        self,
                        Filter::Source(source),
                        format!("{} ({count})", source_name(source)),
                    );
                }
            }

            ui.add_space(8.0);
            ui.separator();
            for (screen, label) in [
                (Screen::Stats, "Stats"),
                (Screen::Profile, "Profile"),
                (Screen::Shop, "Shop"),
                (Screen::Settings, "Settings"),
            ] {
                if ui.selectable_label(self.screen == screen, label).clicked() {
                    self.screen = screen;
                }
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        if let Some(error) = self.error.clone() {
            ui.horizontal(|ui| {
                ui.colored_label(ui.visuals().error_fg_color, format!("⚠ {error}"));
                if ui.small_button("Dismiss").clicked() {
                    self.error = None;
                }
            });
        }
        ui.horizontal(|ui| {
            if self.busy > 0 {
                ui.spinner();
            }
            ui.label(&self.status);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(format!("{} games shown", self.games.len()));
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
            if let Err(err) = self.reload() {
                self.error = Some(format!("{err:#}"));
            }
        }

        egui::Panel::top("top_bar").show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui);
            ui.add_space(4.0);
        });
        egui::Panel::bottom("status_bar").show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("sidebar")
            .default_size(180.0)
            .show(ui, |ui| self.sidebar(ui));

        match self.screen {
            Screen::Library => self.library_screen(ui),
            Screen::Stats => {
                egui::CentralPanel::default().show(ui, |ui| self.stats_screen(ui));
            }
            Screen::Profile => {
                egui::CentralPanel::default().show(ui, |ui| self.profile_screen(ui));
            }
            Screen::Shop => {
                egui::CentralPanel::default().show(ui, |ui| self.shop_screen(ui));
            }
            Screen::Settings => {
                egui::CentralPanel::default().show(ui, |ui| self.settings_screen(ui));
            }
        }
        self.import_window(&ctx);
    }
}

fn sort_name(sort: SortBy) -> &'static str {
    match sort {
        SortBy::Title => "Sort: Title",
        SortBy::LastPlayed => "Sort: Last played",
        SortBy::Playtime => "Sort: Most played",
        SortBy::RecentlyAdded => "Sort: Recently added",
    }
}

pub fn source_name(source: Source) -> &'static str {
    match source {
        Source::Steam => "Steam",
        Source::Epic => "Epic",
        Source::Gog => "GOG",
        Source::Folder => "Folder",
        Source::Manual => "Manual",
    }
}

/// A big number with a caption under it, for the stats and profile pages.
pub fn stat_tile(ui: &mut egui::Ui, value: &str, caption: &str) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_width(130.0);
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(value).size(22.0).strong());
            ui.weak(caption);
        });
    });
}
