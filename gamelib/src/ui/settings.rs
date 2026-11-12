//! The settings screen.

use eframe::egui;
use gamelib::db::keys;
use gamelib::model::Source;
use gamelib::paths;
use gamelib::shop::{self, ItemKind};
use gamelib::worker::Job;

use super::theme::{bold, source_name};
use super::{ANIMATE_KEY, GameLibApp, paint};

impl GameLibApp {
    pub fn settings_screen(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        ui.label(egui::RichText::new("Settings").font(bold(32.0)));
        ui.add_space(10.0);

        theme.panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("Where to look for games", 17.0));
            for source in [Source::Steam, Source::Epic, Source::Gog] {
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
            });
            ui.horizontal(|ui| {
                if ui.button("Save keys").clicked() {
                    let saved = self
                        .db
                        .set_setting(keys::STEAMGRIDDB_KEY, self.settings.steamgriddb_key.trim())
                        .and_then(|()| self.db.set_setting(keys::STEAM_API_KEY, self.settings.steam_key.trim()));
                    if self.check(saved).is_some() {
                        self.status = "Keys saved.".to_string();
                    }
                }
                if ui.button("Download missing covers").clicked() {
                    self.start_job(Job::FetchMissingCovers, "Getting covers and info…");
                }
            });
        });
        ui.add_space(14.0);

        theme.panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("Appearance", 17.0));
            ui.label(paint::muted(
                "Library backgrounds come from the Shop.",
                &theme,
            ));
            let mut choice: Option<Option<&'static str>> = None;
            ui.horizontal_wrapped(|ui| {
                if paint::chip(ui, &theme, "None (Ember)", self.data.equipped.is_none())
                    .clicked()
                {
                    choice = Some(None);
                }
                for item in shop::CATALOGUE
                    .iter()
                    .filter(|i| i.kind == ItemKind::LibraryBackground)
                {
                    if !self.data.owned.iter().any(|id| id == item.id) {
                        continue;
                    }
                    if paint::chip(ui, &theme, item.name, self.data.equipped == Some(item.id))
                        .clicked()
                    {
                        choice = Some(Some(item.id));
                    }
                }
            });
            if let Some(choice) = choice {
                self.check(self.db.equip_library_background(choice));
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
