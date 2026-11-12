//! The settings screen.

use std::path::PathBuf;

use anyhow::Result;
use eframe::egui;
use gamelib::db::{Db, keys};
use gamelib::model::Source;
use gamelib::paths;
use gamelib::worker::Job;

use super::{GameLibApp, source_name};

/// The values being edited on the settings screen.
pub struct SettingsInputs {
    pub folders: Vec<PathBuf>,
    steamgriddb_key: String,
    steam_key: String,
}

impl SettingsInputs {
    pub fn load(db: &Db) -> Result<SettingsInputs> {
        Ok(SettingsInputs {
            folders: db.scan_folders()?,
            steamgriddb_key: db.get_setting(keys::STEAMGRIDDB_KEY)?.unwrap_or_default(),
            steam_key: db.get_setting(keys::STEAM_API_KEY)?.unwrap_or_default(),
        })
    }
}

impl GameLibApp {
    pub fn settings_screen(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.heading("Settings");

            ui.add_space(8.0);
            ui.strong("Where to look for games");
            for source in [Source::Steam, Source::Epic, Source::Gog] {
                let mut enabled = self.db.source_enabled(source).unwrap_or(true);
                if ui.checkbox(&mut enabled, source_name(source)).changed() {
                    self.check(self.db.set_source_enabled(source, enabled));
                }
            }
            let mut on_startup = self.db.get_bool_setting(keys::SCAN_ON_STARTUP, false).unwrap_or(false);
            if ui.checkbox(&mut on_startup, "Scan when GameLib starts").changed() {
                self.check(self.db.set_bool_setting(keys::SCAN_ON_STARTUP, on_startup));
            }
            if ui.button("Rescan now").clicked() {
                self.start_job(Job::ScanLibrary, "Scanning for games…");
            }

            ui.add_space(12.0);
            ui.strong("Folders to scan for loose games");
            ui.weak("Each subfolder of these counts as one game. Use \"Scan folders\" in the top bar.");
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
            if ui.button("Add folder…").clicked()
                && let Some(folder) = rfd::FileDialog::new().set_title("Choose a games folder").pick_folder()
            {
                let folder = paths::real_path(&folder);
                if !self.settings.folders.contains(&folder) {
                    self.settings.folders.push(folder);
                    folders_changed = true;
                }
            }
            if folders_changed {
                self.check(self.db.set_scan_folders(&self.settings.folders));
            }

            ui.add_space(12.0);
            ui.strong("API keys");
            ui.weak("Stored only in your library database on this PC. Never share them or put them in Git.");
            let mut save_keys = false;
            egui::Grid::new("api_keys").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("SteamGridDB (cover art)");
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings.steamgriddb_key)
                        .password(true)
                        .desired_width(300.0),
                );
                ui.end_row();
                ui.label("Steam Web API (friends)");
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings.steam_key)
                        .password(true)
                        .desired_width(300.0),
                );
                ui.end_row();
            });
            ui.horizontal(|ui| {
                save_keys = ui.button("Save keys").clicked();
                if ui.button("Download missing covers").clicked() {
                    self.start_job(Job::FetchMissingCovers, "Getting covers and info…");
                }
            });
            if save_keys {
                let saved = self
                    .db
                    .set_setting(keys::STEAMGRIDDB_KEY, self.settings.steamgriddb_key.trim())
                    .and_then(|()| self.db.set_setting(keys::STEAM_API_KEY, self.settings.steam_key.trim()));
                if self.check(saved).is_some() {
                    self.status = "Keys saved.".to_string();
                }
            }

            ui.add_space(12.0);
            ui.strong("Files");
            if let Ok(dir) = paths::data_dir() {
                ui.weak(format!("GameLib keeps its data in {}", dir.display()));
            }
        });
    }
}
