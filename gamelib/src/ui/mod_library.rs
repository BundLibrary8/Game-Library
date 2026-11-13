//! The Mod library: every game's mods in one place. Switch mods on and
//! off, install new ones from a zip, file or folder, and remove them.

use std::path::PathBuf;
use std::rc::Rc;

use eframe::egui;
use gamelib::mods::{self, ModItem};

use super::theme::{bold, regular};
use super::{GameLibApp, Screen, paint};
use gamelib::health::format_bytes;

/// What's on disk, read at most every few seconds.
pub struct ModsNow {
    /// Games with a mods folder: (game id, folder, the mods or why not).
    pub games: Vec<(i64, PathBuf, Result<Vec<ModItem>, String>)>,
    /// Games without one but with a likely folder: (game id, suggestion).
    pub suggestions: Vec<(i64, PathBuf)>,
}

#[derive(Default)]
pub struct ModLibraryState {
    pub search: String,
    /// A mod waiting for "Delete it?" to be confirmed: (game, name, on).
    pub confirm_delete: Option<(i64, String, bool)>,
}

enum Action {
    Toggle(PathBuf, String, bool),
    Install(i64, PathBuf),
    AskDelete(i64, String, bool),
    Delete(PathBuf, String, bool),
    UseFolder(i64, PathBuf),
    Open(PathBuf),
    Details(i64),
}

impl GameLibApp {
    fn mods_now(&mut self, time: f64) -> Rc<ModsNow> {
        let (games, extras) = (&self.data.visible, &self.data.extras);
        super::memo(&mut self.memos.mods, self.generation, time, 5.0, || {
            let mut with = Vec::new();
            let mut suggestions = Vec::new();
            for game in games {
                match extras.get(&game.id).and_then(|e| e.mods_dir.clone()) {
                    Some(dir) => {
                        let list = mods::list(&dir).map_err(|e| format!("{e:#}"));
                        with.push((game.id, dir, list));
                    }
                    None => {
                        if let Some(dir) =
                            game.install_dir.as_deref().and_then(mods::suggest_mods_dir)
                        {
                            suggestions.push((game.id, dir));
                        }
                    }
                }
            }
            Rc::new(ModsNow {
                games: with,
                suggestions,
            })
        })
    }

    pub fn mod_library_screen(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let time = ui.input(|i| i.time);
        let now = self.mods_now(time);
        let total: usize = now
            .games
            .iter()
            .filter_map(|g| g.2.as_ref().ok())
            .map(|l| l.len())
            .sum();
        let on: usize = now
            .games
            .iter()
            .filter_map(|g| g.2.as_ref().ok())
            .flatten()
            .filter(|m| m.enabled)
            .count();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Mod library").font(bold(32.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.mod_library.search)
                        .hint_text("Find a mod or game…")
                        .desired_width(240.0),
                );
            });
        });
        ui.label(paint::muted(
            "Switching a mod off moves it into a hidden .gamelib-disabled folder the game doesn't load; switching it on moves it back. Set a game's mods folder on its page.",
            &theme,
        ));
        ui.add_space(10.0);
        paint::stat_row(
            ui,
            &theme,
            &[
                ("Games with mods", now.games.len().to_string()),
                ("Mods", total.to_string()),
                ("Switched on", on.to_string()),
            ],
        );
        ui.add_space(14.0);

        let mut action: Option<Action> = None;
        let search = self.mod_library.search.trim().to_lowercase();

        if !now.suggestions.is_empty() {
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(ui.available_width());
                ui.label(paint::strong("Mods folders found", 16.0));
                ui.label(paint::muted(
                    "These games have a folder that looks like where their mods go.",
                    &theme,
                ));
                for (id, dir) in &now.suggestions {
                    ui.horizontal(|ui| {
                        ui.label(paint::strong(self.title_of(*id), 14.0));
                        ui.label(paint::muted(dir.display().to_string(), &theme).size(12.5));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Use this folder").clicked() {
                                action = Some(Action::UseFolder(*id, dir.clone()));
                            }
                        });
                    });
                }
            });
            ui.add_space(14.0);
        }

        if now.games.is_empty() {
            ui.label(paint::muted(
                "No mods folders yet. Open a game's page and set its mods folder under \"Saves and mods\", or use one found above.",
                &theme,
            ));
        }

        for (game_id, dir, list) in &now.games {
            let title = self.title_of(*game_id);
            let matches_game = search.is_empty() || title.to_lowercase().contains(&search);
            let mods: Vec<&ModItem> = match list {
                Ok(list) => list
                    .iter()
                    .filter(|m| matches_game || m.name.to_lowercase().contains(&search))
                    .collect(),
                Err(_) => Vec::new(),
            };
            if !matches_game && mods.is_empty() {
                continue;
            }
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let name = ui.add(
                        egui::Label::new(egui::RichText::new(&title).font(bold(17.0))).sense(egui::Sense::click()),
                    );
                    if name.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        action = Some(Action::Details(*game_id));
                    }
                    if let Ok(list) = list {
                        let on = list.iter().filter(|m| m.enabled).count();
                        ui.label(paint::muted(format!("{on} of {} on", list.len()), &theme));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Open folder").clicked() {
                            action = Some(Action::Open(dir.clone()));
                        }
                        if ui.button("Install folder…").clicked()
                            && let Some(source) = rfd::FileDialog::new().set_title("Choose a mod folder").pick_folder()
                        {
                            action = Some(Action::Install(*game_id, source));
                        }
                        if ui
                            .button("Install from file…")
                            .on_hover_text("A .zip is unpacked into the mods folder; anything else is copied in")
                            .clicked()
                            && let Some(source) = rfd::FileDialog::new()
                                .set_title("Choose a mod (.zip or file)")
                                .add_filter("Mods", &["zip", "pak", "dll", "esp", "esm", "jar", "package"])
                                .add_filter("All files", &["*"])
                                .pick_file()
                        {
                            action = Some(Action::Install(*game_id, source));
                        }
                    });
                });
                ui.label(paint::muted(dir.display().to_string(), &theme).size(12.0));
                if let Err(err) = list {
                    ui.colored_label(theme.danger, err);
                    return;
                }
                if mods.is_empty() {
                    ui.label(paint::muted("No mods installed.", &theme));
                }
                for m in mods {
                    ui.horizontal(|ui| {
                        let mut enabled = m.enabled;
                        if ui
                            .checkbox(&mut enabled, "")
                            .on_hover_text(if m.enabled {
                                "On: the game loads it"
                            } else {
                                "Off: the game ignores it"
                            })
                            .changed()
                        {
                            action = Some(Action::Toggle(dir.clone(), m.name.clone(), enabled));
                        }
                        let colour = if m.enabled { theme.text } else { theme.muted };
                        ui.label(egui::RichText::new(&m.name).font(bold(14.0)).color(colour));
                        ui.label(
                            egui::RichText::new(format!(
                                "{} · {}",
                                if m.is_folder { "folder" } else { "file" },
                                format_bytes(m.bytes)
                            ))
                            .font(regular(12.0))
                            .color(theme.muted),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let asking = self
                                .mod_library
                                .confirm_delete
                                .as_ref()
                                .is_some_and(|c| c.0 == *game_id && c.1 == m.name);
                            if asking {
                                if ui.button("Cancel").clicked() {
                                    self.mod_library.confirm_delete = None;
                                }
                                if ui
                                    .button(
                                        egui::RichText::new("Delete for good").color(theme.danger),
                                    )
                                    .clicked()
                                {
                                    action = Some(Action::Delete(
                                        dir.clone(),
                                        m.name.clone(),
                                        m.enabled,
                                    ));
                                }
                            } else if ui.small_button("Remove").clicked() {
                                action =
                                    Some(Action::AskDelete(*game_id, m.name.clone(), m.enabled));
                            }
                        });
                    });
                }
            });
            ui.add_space(12.0);
        }

        let Some(action) = action else { return };
        match action {
            Action::Toggle(dir, name, on) => {
                if self.check(mods::set_enabled(&dir, &name, on)).is_some() {
                    self.status = format!("{name} switched {}.", if on { "on" } else { "off" });
                }
            }
            Action::Install(game_id, source) => {
                let dir = now
                    .games
                    .iter()
                    .find(|g| g.0 == game_id)
                    .map(|g| g.1.clone());
                if let Some(dir) = dir
                    && let Some(added) = self.check(mods::install(&dir, &source))
                {
                    self.status = if added.is_empty() {
                        "Installed (it went into folders that were already there).".to_string()
                    } else {
                        format!("Installed {}.", added.join(", "))
                    };
                }
            }
            Action::AskDelete(game_id, name, on) => {
                self.mod_library.confirm_delete = Some((game_id, name, on));
            }
            Action::Delete(dir, name, on) => {
                self.mod_library.confirm_delete = None;
                if self.check(mods::remove(&dir, &name, on)).is_some() {
                    self.status = format!("Removed {name}.");
                }
            }
            Action::UseFolder(game_id, dir) => {
                self.check(self.db.set_mods_dir(game_id, Some(&dir)));
                self.dirty = true;
            }
            Action::Open(dir) => {
                self.check(gamelib::launcher::open_folder(&dir));
            }
            Action::Details(id) => self.go(Screen::Details(id)),
        }
        // Read the folders again.
        self.memos.mods = None;
    }
}
