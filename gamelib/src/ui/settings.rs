//! The settings screen.

use eframe::egui;
use gamelib::db::keys;
use gamelib::model::Source;
use gamelib::paths;
use gamelib::shop::{self, ItemKind};
use gamelib::worker::Job;

use super::theme::{bold, source_name};
use super::{
    ANIMATE_KEY, AUTO_DETECT_KEY, CLOSE_TO_TRAY_KEY, CONTRAST_KEY, COUCH_KEY, GameLibApp,
    NOTIFY_KEY, SPLASH_KEY, TRAY_KEY, paint,
};

impl GameLibApp {
    pub fn settings_screen(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        ui.label(egui::RichText::new("Settings").font(bold(32.0)));
        ui.add_space(10.0);

        theme.panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("Where to look for games", 17.0));
            for source in [
                Source::Steam,
                Source::Epic,
                Source::Gog,
                Source::BattleNet,
                Source::Riot,
                Source::Ea,
            ] {
                let mut enabled = self.db.source_enabled(source).unwrap_or(true);
                if ui.checkbox(&mut enabled, source_name(source)).changed() {
                    self.check(self.db.set_source_enabled(source, enabled));
                }
            }
            let mut on_startup = self
                .db
                .get_bool_setting(keys::SCAN_ON_STARTUP, false)
                .unwrap_or(false);
            if ui
                .checkbox(&mut on_startup, "Scan when GameLib starts")
                .changed()
            {
                self.check(self.db.set_bool_setting(keys::SCAN_ON_STARTUP, on_startup));
            }
            if ui.button("⟳ Rescan now").clicked() {
                self.start_job(Job::ScanLibrary, "Scanning for games…");
            }
        });
        ui.add_space(14.0);

        theme.panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("Folders to scan for loose games", 17.0));
            ui.label(paint::muted(
                "Each subfolder of these counts as one game.",
                &theme,
            ));
            let mut remove = None;
            for (index, folder) in self.settings.folders.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(folder.display().to_string());
                    if ui.small_button("Remove").clicked() {
                        remove = Some(index);
                    }
                });
            }
            let mut folders_changed = false;
            if let Some(index) = remove {
                self.settings.folders.remove(index);
                folders_changed = true;
            }
            ui.horizontal(|ui| {
                if ui.button("Add folder…").clicked()
                    && let Some(folder) = rfd::FileDialog::new()
                        .set_title("Choose a games folder")
                        .pick_folder()
                {
                    let folder = paths::real_path(&folder);
                    if !self.settings.folders.contains(&folder) {
                        self.settings.folders.push(folder);
                        folders_changed = true;
                    }
                }
                if !self.settings.folders.is_empty() && ui.button("Scan these folders").clicked() {
                    self.start_job(Job::ScanFolders, "Scanning folders…");
                }
            });
            if folders_changed {
                self.check(self.db.set_scan_folders(&self.settings.folders));
            }
        });
        ui.add_space(14.0);

        theme.panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("API keys", 17.0));
            ui.label(paint::muted(
                "Stored only in your library database on this PC. Never share them or put them in Git.",
                &theme,
            ));
            egui::Grid::new("api_keys").num_columns(2).spacing([14.0, 10.0]).show(ui, |ui| {
                ui.label("SteamGridDB (cover art)");
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings.steamgriddb_key)
                        .password(true)
                        .desired_width(340.0),
                );
                ui.end_row();
                ui.label("Steam Web API (friends)");
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings.steam_key)
                        .password(true)
                        .desired_width(340.0),
                );
                ui.end_row();
                ui.label("Anthropic (assistant)");
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings.anthropic_key)
                        .password(true)
                        .hint_text("Optional: lets the Assistant ask Claude")
                        .desired_width(340.0),
                );
                ui.end_row();
            });
            let mut use_claude = self
                .db
                .get_bool_setting(keys::ASSISTANT_USE_CLAUDE, true)
                .unwrap_or(true);
            if ui
                .checkbox(
                    &mut use_claude,
                    "Let the Assistant ask Claude (sends a summary of your library: titles, hours, status, ratings and genres)",
                )
                .changed()
            {
                let saved = self.db.set_bool_setting(keys::ASSISTANT_USE_CLAUDE, use_claude);
                self.check(saved);
            }
            ui.horizontal(|ui| {
                if ui.button("Save keys").clicked() {
                    let saved = self
                        .db
                        .set_setting(keys::STEAMGRIDDB_KEY, self.settings.steamgriddb_key.trim())
                        .and_then(|()| self.db.set_setting(keys::STEAM_API_KEY, self.settings.steam_key.trim()))
                        .and_then(|()| self.db.set_setting(keys::ANTHROPIC_KEY, self.settings.anthropic_key.trim()));
                    if self.check(saved).is_some() {
                        self.status = "Keys saved.".to_string();
                    }
                }
                if ui.button("Download missing covers").clicked() {
                    self.start_job(Job::FetchMissingCovers { retry_all: true }, "Getting covers and info…");
                }
            });
        });
        ui.add_space(14.0);

        theme.panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("Appearance", 17.0));
            ui.label(paint::muted(
                "Pick a library background. More can be bought in the Shop.",
                &theme,
            ));
            let mut choice: Option<&'static str> = None;
            ui.horizontal_wrapped(|ui| {
                for item in shop::CATALOGUE
                    .iter()
                    .filter(|i| i.kind == ItemKind::LibraryBackground)
                {
                    if !self.data.owned.iter().any(|id| id == item.id) {
                        continue;
                    }
                    if paint::chip(ui, &theme, item.name, self.data.equipped == item.id).clicked() {
                        choice = Some(item.id);
                    }
                }
            });
            if let Some(choice) = choice {
                self.check(self.db.equip_library_background(Some(choice)));
            }
            let mut couch = self.db.get_bool_setting(COUCH_KEY, false).unwrap_or(false);
            if ui
                .checkbox(&mut couch, "Open in Couch Mode")
                .on_hover_text("Start full screen, ready for a TV and controller. Press F11 or Esc to leave it.")
                .changed()
            {
                self.check(self.db.set_bool_setting(COUCH_KEY, couch));
            }
            let mut animate = self.animate;
            if ui
                .checkbox(&mut animate, "Animated backgrounds")
                .on_hover_text(
                    "Turn this off to save a little processor and battery: the backgrounds stand still.",
                )
                .changed()
            {
                self.animate = animate;
                self.check(self.db.set_bool_setting(ANIMATE_KEY, animate));
            }
            let mut splash = self.db.get_bool_setting(SPLASH_KEY, true).unwrap_or(true);
            if ui
                .checkbox(&mut splash, "Loading animation on startup")
                .changed()
            {
                self.check(self.db.set_bool_setting(SPLASH_KEY, splash));
            }
        });
        ui.add_space(14.0);

        theme.panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("Export and import", 17.0));
            ui.label(paint::muted(
                "Save your whole library (games, playtime, tags, ratings, franchises) as one file, \
                 to keep safe or to move to another PC. Importing merges a file into this library.",
                &theme,
            ));
            ui.horizontal(|ui| {
                if ui.button("Export library…").clicked()
                    && let Some(file) = rfd::FileDialog::new()
                        .set_title("Save your library as")
                        .set_file_name("gamelib-library.json")
                        .add_filter("GameLib library", &["json"])
                        .save_file()
                {
                    let saved = self.db.export_library(gamelib::now()).and_then(|export| {
                        gamelib::backup::write_export(&export, &file).map(|()| export.games.len())
                    });
                    if let Some(count) = self.check(saved) {
                        self.status = format!("Exported {count} games to {}.", file.display());
                    }
                }
                if ui.button("Export CSV…").on_hover_text("A spreadsheet of your games: open it in Excel or Google Sheets").clicked()
                    && let Some(file) = rfd::FileDialog::new()
                        .set_title("Save your library as a spreadsheet")
                        .set_file_name("gamelib-library.csv")
                        .add_filter("Spreadsheet (CSV)", &["csv"])
                        .save_file()
                {
                    let saved = self
                        .db
                        .export_csv()
                        .and_then(|text| gamelib::csvio::write_csv(&text, &file));
                    if self.check(saved).is_some() {
                        self.status = format!("Saved {}.", file.display());
                    }
                }
                if ui
                    .button("Import CSV…")
                    .on_hover_text("Columns are matched by name: title (needed), status, rating, hours_played, hours_to_beat, favourite, tags, notes, exe")
                    .clicked()
                    && let Some(file) = rfd::FileDialog::new()
                        .set_title("Choose a spreadsheet (CSV) to import")
                        .add_filter("Spreadsheet (CSV)", &["csv"])
                        .pick_file()
                {
                    let imported = gamelib::csvio::read_csv(&file)
                        .and_then(|text| self.db.import_csv(&text, gamelib::now()));
                    if let Some(summary) = self.check(imported) {
                        self.status = format!(
                            "CSV imported: {} updated, {} added, {} skipped.",
                            summary.updated, summary.added, summary.skipped
                        );
                        if !summary.not_found.is_empty() {
                            self.status.push_str(&format!(
                                " Not in your library: {}.",
                                summary.not_found.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
                            ));
                        }
                        self.dirty = true;
                    }
                }
                if ui.button("Import library…").clicked()
                    && let Some(file) = rfd::FileDialog::new()
                        .set_title("Choose a GameLib library file")
                        .add_filter("GameLib library", &["json"])
                        .pick_file()
                {
                    let imported = gamelib::backup::read_export(&file)
                        .and_then(|export| self.db.import_library(&export, gamelib::now()));
                    if let Some(summary) = self.check(imported) {
                        self.status = format!(
                            "Imported: {} new games, {} updated.",
                            summary.added, summary.updated
                        );
                    }
                }
            });
        });
        ui.add_space(14.0);

        theme.panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("Accessibility", 17.0));
            ui.horizontal(|ui| {
                ui.label("Text size");
                let mut zoom = ui.ctx().zoom_factor();
                if ui
                    .add(egui::Slider::new(&mut zoom, 0.75..=1.75).step_by(0.05).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)))
                    .changed()
                {
                    ui.ctx().set_zoom_factor(zoom);
                }
                ui.label(paint::muted("(also Ctrl + and Ctrl -, Ctrl 0 to reset)", &theme));
            });
            let mut contrast = self.high_contrast;
            if ui
                .checkbox(&mut contrast, "High contrast (brighter text, solid panels)")
                .changed()
            {
                self.high_contrast = contrast;
                self.check(self.db.set_bool_setting(CONTRAST_KEY, contrast));
            }
            ui.label(paint::muted(
                "To cut down movement, untick Animated backgrounds and Loading animation under Appearance. Press F1 for keyboard shortcuts.",
                &theme,
            ));
        });
        ui.add_space(14.0);

        theme.panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("Playing games", 17.0));
            let mut auto = self.db.get_bool_setting(AUTO_DETECT_KEY, true).unwrap_or(true);
            if ui
                .checkbox(
                    &mut auto,
                    "Track games started outside GameLib (from Steam, a desktop shortcut…)",
                )
                .on_hover_text("GameLib checks the running programs every 15 seconds.")
                .changed()
            {
                self.check(self.db.set_bool_setting(AUTO_DETECT_KEY, auto));
                self.worker.send(gamelib::worker::Job::WatchForGames(auto));
            }
            ui.add_space(10.0);
            ui.label(paint::strong("GameLib account", 17.0));
            match &self.cloud.account {
                Some((profile, email)) => {
                    ui.label(format!("Logged in as @{} ({email})", profile.username));
                }
                None => {
                    ui.label(paint::muted("Not logged in.", &theme));
                }
            }
            ui.horizontal(|ui| {
                if ui.button("Open account").clicked() {
                    self.go(super::Screen::Account);
                }
                ui.label("Supabase anon key");
                ui.add(
                    egui::TextEdit::singleline(&mut self.cloud.anon_key)
                        .password(true)
                        .hint_text("Project Settings → API")
                        .desired_width(260.0),
                );
                if ui.button("Save key").clicked() {
                    let key = self.cloud.anon_key.trim().to_string();
                    if key.len() < 20 {
                        self.error = Some("That key looks too short.".into());
                    } else if self.check(self.db.set_cloud_anon_key(&key)).is_some() {
                        self.status = "Supabase key saved.".into();
                    }
                }
            });
            ui.label(paint::muted(
                format!("Accounts, friends and shared libraries are stored in {}.", gamelib::cloud::DEFAULT_URL),
                &theme,
            ));
            ui.add_space(10.0);
            ui.label(paint::strong("Discord", 17.0));
            let mut presence = self
                .db
                .get_bool_setting(super::notify::DISCORD_PRESENCE_KEY, false)
                .unwrap_or(false);
            let mut restart = false;
            if ui
                .checkbox(&mut presence, "Show what I'm playing on my Discord profile (Rich Presence)")
                .changed()
            {
                self.check(self.db.set_bool_setting(super::notify::DISCORD_PRESENCE_KEY, presence));
                restart = true;
            }
            ui.horizontal(|ui| {
                ui.label("Application ID");
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings.discord_app)
                        .hint_text("from discord.com/developers")
                        .desired_width(220.0),
                );
                if ui.button("Save").clicked() {
                    let id = self.settings.discord_app.trim().to_string();
                    if id.is_empty() || id.chars().all(|c| c.is_ascii_digit()) {
                        self.check(self.db.set_setting(super::notify::DISCORD_APP_KEY, &id));
                        restart = true;
                    } else {
                        self.error = Some("A Discord application ID is a long number.".into());
                    }
                }
            });
            ui.label(paint::muted(
                "Discord needs an application for this: at discord.com/developers/applications press New Application, call it GameLib, and copy its Application ID here. The Discord app must be running.",
                &theme,
            ));
            if let Some(err) = self.discord_error.lock().ok().and_then(|e| e.clone()) {
                ui.label(egui::RichText::new(format!("Last problem: {err}")).color(theme.muted).size(12.5));
            }
            ui.horizontal(|ui| {
                ui.label("Post sessions to a channel");
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings.discord_webhook)
                        .password(true)
                        .hint_text("Webhook URL (optional)")
                        .desired_width(300.0),
                );
                if ui.button("Save").clicked() {
                    let url = self.settings.discord_webhook.trim().to_string();
                    if url.is_empty() || gamelib::discord::is_webhook_url(&url) {
                        self.check(self.db.set_setting(super::notify::DISCORD_WEBHOOK_KEY, &url));
                        self.status = if url.is_empty() {
                            "Discord posts switched off.".into()
                        } else {
                            "Sessions will be posted to Discord.".into()
                        };
                    } else {
                        self.error = Some("That isn't a Discord webhook URL (it starts https://discord.com/api/webhooks/).".into());
                    }
                }
            });
            ui.label(paint::muted(
                "In Discord: Server Settings → Integrations → Webhooks → New Webhook → Copy Webhook URL. Sessions over two minutes are posted.",
                &theme,
            ));
            if restart {
                self.start_discord();
            }
            ui.add_space(10.0);
            ui.label(paint::strong("Notifications and tray", 17.0));
            let mut desktop = self.db.get_bool_setting(NOTIFY_KEY, true).unwrap_or(true);
            if ui
                .checkbox(&mut desktop, "Desktop notifications when GameLib isn't in front (a game closes, an achievement, a goal)")
                .changed()
            {
                self.shared.desktop.store(desktop, std::sync::atomic::Ordering::Relaxed);
                self.check(self.db.set_bool_setting(NOTIFY_KEY, desktop));
            }
            if cfg!(windows) {
                let mut tray = self.tray.is_some();
                if ui.checkbox(&mut tray, "Show GameLib in the system tray").changed() {
                    self.check(self.db.set_bool_setting(TRAY_KEY, tray));
                    if tray {
                        let ctx = ui.ctx().clone();
                        self.start_tray(&ctx);
                    } else {
                        self.stop_tray();
                    }
                }
                let mut close = self.close_to_tray;
                if ui
                    .add_enabled(
                        self.tray.is_some(),
                        egui::Checkbox::new(&mut close, "Closing the window keeps GameLib running in the tray"),
                    )
                    .changed()
                {
                    self.close_to_tray = close;
                    self.check(self.db.set_bool_setting(CLOSE_TO_TRAY_KEY, close));
                }
            } else {
                ui.label(paint::muted("The system tray icon is only on Windows.", &theme));
            }
        });
        ui.add_space(14.0);

        theme.panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("Weekly playtime goal", 17.0));
            let goal = self.db.weekly_goal().ok().flatten();
            let mut hours = goal.map(|g| g.minutes as f32 / 60.0).unwrap_or(0.0);
            let mut is_limit = goal.is_some_and(|g| g.is_limit);
            let mut changed = false;
            ui.horizontal(|ui| {
                changed |= ui
                    .add(
                        egui::Slider::new(&mut hours, 0.0..=60.0)
                            .step_by(1.0)
                            .suffix(" hours a week"),
                    )
                    .on_hover_text("0 means no goal")
                    .changed();
                changed |= ui
                    .checkbox(&mut is_limit, "It's a limit, not a target")
                    .changed();
            });
            if changed {
                let new = (hours >= 1.0).then_some(gamelib::journal::WeeklyGoal {
                    minutes: (hours * 60.0) as u32,
                    is_limit,
                });
                self.check(self.db.set_weekly_goal(new));
            }
            if let Ok(Some((goal, progress))) = self
                .db
                .weekly_progress(gamelib::now(), super::local_offset())
            {
                let colour = match (goal.is_limit, progress.reached()) {
                    (true, true) => theme.danger,
                    (false, true) => theme.success,
                    _ => theme.accent,
                };
                paint::bar(ui, &theme, progress.fraction(), colour, 360.0);
                ui.label(paint::muted(
                    format!(
                        "This week: {:.1} of {} hours",
                        progress.done_minutes as f32 / 60.0,
                        goal.minutes / 60
                    ),
                    &theme,
                ));
            }
        });
        ui.add_space(14.0);

        theme.panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("Updates and sharing", 17.0));
            if gamelib::updates::PUBLISHED {
                ui.label(format!("This is GameLib {}.", gamelib::updates::VERSION));
                let mut auto = self
                    .db
                    .get_bool_setting(gamelib::updates::AUTO_KEY, true)
                    .unwrap_or(true);
                if ui
                    .checkbox(
                        &mut auto,
                        "Update automatically when GameLib opens",
                    )
                    .changed()
                {
                    self.check(self.db.set_bool_setting(gamelib::updates::AUTO_KEY, auto));
                }
            } else {
                ui.label(format!(
                    "This is GameLib {}, built from the source code. It updates when you run git pull and cargo run; the downloadable GameLib.exe updates itself.",
                    gamelib::updates::VERSION
                ));
            }
            ui.add_space(4.0);
            ui.label(paint::muted(
                "Share GameLib: anyone with this link gets the newest GameLib.exe. There's nothing to install: they open it, and it keeps itself up to date.",
                &theme,
            ));
            let link = gamelib::updates::share_link();
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut link.clone())
                        .desired_width(460.0)
                        .interactive(true),
                );
                if ui.button("Copy download link").clicked() {
                    ui.ctx().copy_text(link.clone());
                    self.status = "Download link copied.".into();
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Check for updates").clicked() {
                    self.start_job(gamelib::worker::Job::CheckForUpdates, "Checking for updates…");
                }
                if ui.button("Release notes").clicked() {
                    let url = format!("https://github.com/{}/releases", gamelib::updates::REPOSITORY);
                    self.check(gamelib::launcher::open_link(&url));
                }
            });
            if let Some(release) = self.update.clone() {
                ui.add_space(6.0);
                ui.label(paint::strong(format!("GameLib {} is available", release.version), 15.0));
                if !release.notes.trim().is_empty() {
                    ui.label(paint::muted(release.notes.chars().take(600).collect::<String>(), &theme));
                }
                ui.horizontal(|ui| {
                    if gamelib::updates::PUBLISHED
                        && release.exe.is_some()
                        && ui.button("Update now").clicked()
                    {
                        self.worker.send(gamelib::worker::Job::UpdateItself);
                        self.status = format!("Downloading GameLib {}…", release.version);
                    }
                    if let Some(download) = &release.download
                        && ui.button("Download").clicked()
                    {
                        self.check(gamelib::launcher::open_link(download));
                    }
                    if ui.button("Open the release page").clicked() {
                        self.check(gamelib::launcher::open_link(&release.url));
                    }
                });
                ui.label(paint::muted(
                    "To update from source instead, run the commands under \"Get the latest version\" in the README.",
                    &theme,
                ));
            }
        });
        ui.add_space(14.0);

        theme.panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("Admin panel", 17.0));
            ui.label(paint::muted(
                "Give Stardust and set any picture as the background. Locked with the password in admin_password.txt in GameLib's data folder (shortcut: Ctrl+Shift+A).",
                &theme,
            ));
            if ui.button("Open admin panel").clicked() {
                self.go(super::Screen::Admin);
            }
        });
        ui.add_space(14.0);

        theme.panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("Sync between PCs", 17.0));
            ui.label(paint::muted(
                "Pick a folder that OneDrive, Dropbox, Google Drive or iCloud keeps in step on all your PCs. \
                 GameLib saves your library there when it closes and loads changes from your other PCs when it starts. \
                 Playtime only ever goes up; for everything else, the PC that saved last wins.",
                &theme,
            ));
            let folder = self.db.sync_folder().ok().flatten();
            ui.horizontal(|ui| {
                match &folder {
                    Some(folder) => {
                        ui.label(paint::strong(folder.display().to_string(), 14.0));
                    }
                    None => {
                        ui.label(paint::muted("No sync folder set.", &theme));
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Choose folder…").clicked()
                    && let Some(chosen) = rfd::FileDialog::new()
                        .set_title("Choose a folder your cloud storage syncs")
                        .pick_folder()
                {
                    let set = self
                        .db
                        .set_sync_folder(Some(&chosen))
                        .and_then(|()| self.db.sync_now(gamelib::now()));
                    if let Some(report) = self.check(set) {
                        self.status = sync_message(&report);
                        self.dirty = true;
                    }
                }
                if folder.is_some() {
                    if ui.button("Sync now").clicked()
                        && let Some(report) = self.check(self.db.sync_now(gamelib::now()))
                    {
                        self.status = sync_message(&report);
                        self.dirty = true;
                    }
                    if ui.button("Stop syncing").clicked() {
                        self.check(self.db.set_sync_folder(None));
                    }
                }
            });
        });
        ui.add_space(14.0);

        theme.panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("This PC", 17.0));
            match self.pc_specs() {
                Some(specs) => {
                    egui::Grid::new("pc_specs")
                        .num_columns(2)
                        .spacing([18.0, 6.0])
                        .show(ui, |ui| {
                            for (label, value) in [
                                ("System", specs.os.clone()),
                                (
                                    "Processor",
                                    format!("{} ({} threads)", specs.cpu, specs.cores),
                                ),
                                ("Memory", format!("{:.0} GB RAM", specs.ram_gb.round())),
                                (
                                    "Graphics",
                                    specs
                                        .gpu
                                        .clone()
                                        .unwrap_or_else(|| "Not detected".to_string()),
                                ),
                            ] {
                                ui.label(paint::muted(label, &theme));
                                ui.label(value);
                                ui.end_row();
                            }
                        });
                    ui.label(
                        paint::muted(
                            "Used for the \"Can I run it?\" check on each game's page.",
                            &theme,
                        )
                        .size(13.5),
                    );
                }
                None => {
                    ui.label(paint::muted("Reading this PC's specs…", &theme));
                }
            }
        });
        ui.add_space(14.0);

        if let Ok(dir) = paths::data_dir() {
            ui.label(paint::muted(
                format!("GameLib keeps its data in {}", dir.display()),
                &theme,
            ));
        }
    }
}

/// What a sync did, for the status bar.
fn sync_message(report: &gamelib::backup::SyncReport) -> String {
    match report.pulled {
        Some(summary) => format!(
            "Synced: loaded {} new and {} updated games from your other PCs.",
            summary.added, summary.updated
        ),
        None if report.pushed => "Synced: your library is saved to the sync folder.".to_string(),
        None => "Nothing to sync.".to_string(),
    }
}
