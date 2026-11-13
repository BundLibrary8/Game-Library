//! The settings screen.

use eframe::egui;
use gamelib::db::keys;
use gamelib::model::Source;
use gamelib::paths;
use gamelib::shop::{self, ItemKind};
use gamelib::worker::Job;

use super::theme::{bold, source_name};
use super::{ANIMATE_KEY, COUCH_KEY, GameLibApp, paint};

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
