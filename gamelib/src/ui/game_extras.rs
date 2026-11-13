//! The extra parts of a game's page: completion status and rating, the
//! "can I run it" check, and save backups and mods.

use eframe::egui::{self, Color32, Sense, vec2};
use gamelib::extras::{GameExtras, Status};
use gamelib::hardware::{self, Verdict};
use gamelib::model::Game;
use gamelib::{backup, now, paths};

use super::theme::{Theme, bold, regular};
use super::{GameLibApp, paint};
use crate::text::format_ago;

fn megabytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

/// Five stars you can click. Returns the new rating if one was clicked
/// (clicking the current rating clears it).
fn star_rating(ui: &mut egui::Ui, theme: &Theme, rating: Option<u8>) -> Option<Option<u8>> {
    let mut chosen = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for star in 1..=5u8 {
            let (rect, response) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
            // Light up to the star under the mouse, or to the saved rating.
            let hovered_up_to = ui
                .input(|i| i.pointer.hover_pos())
                .filter(|_| response.hovered())
                .map(|_| star);
            let lit = hovered_up_to.or(rating).is_some_and(|r| star <= r);
            let colour = if lit {
                Color32::from_rgb(0xff, 0xcf, 0x4a)
            } else {
                theme.muted.gamma_multiply(0.5)
            };
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "★",
                regular(22.0),
                colour,
            );
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                chosen = Some(if rating == Some(star) {
                    None
                } else {
                    Some(star)
                });
            }
        }
    });
    chosen
}

impl GameLibApp {
    /// The row under the Launch button: where you are with the game, and
    /// what you'd give it out of five.
    pub fn status_and_rating(&mut self, ui: &mut egui::Ui, game: &Game, extras: &GameExtras) {
        let theme = self.theme;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.label(paint::caps("Status", 11.5, theme.muted));
            for status in Status::ALL {
                let selected = extras.status == Some(status);
                if paint::chip(ui, &theme, status.label(), selected).clicked() {
                    let new = if selected { None } else { Some(status) };
                    let result = self.db.set_status(game.id, new).and_then(|()| {
                        // Finishing a game also files it under Finished and
                        // pays the Stardust reward (once per game).
                        if new == Some(Status::Finished) {
                            let reward = self.db.finish_game(game.id, now())?;
                            self.db.grant_earned_items(now())?;
                            if reward > 0 {
                                self.status = format!("Finished! +{reward} Stardust");
                            }
                        }
                        Ok(())
                    });
                    self.check(result);
                }
            }
            ui.add_space(14.0);
            ui.label(paint::caps("Your rating", 11.5, theme.muted));
            if let Some(rating) = star_rating(ui, &theme, extras.rating) {
                self.check(self.db.set_rating(game.id, rating));
            }
        });
    }

    /// "Can I run it?": the game's requirements beside this PC's specs.
    pub fn requirements_panel(&mut self, ui: &mut egui::Ui, game: &Game, extras: &GameExtras) {
        let theme = self.theme;
        let minimum = extras
            .requirements_min
            .as_deref()
            .filter(|text| !text.is_empty());
        let recommended = extras
            .requirements_rec
            .as_deref()
            .filter(|text| !text.is_empty());
        paint::panel(ui, theme.panel_frame().inner_margin(24.0), |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("Can I run it?", 16.0));
            let Some(specs) = self.pc_specs() else {
                ui.label(paint::muted("Reading this PC's specs…", &theme));
                return;
            };
            let Some(minimum) = minimum else {
                ui.label(paint::muted(
                    "No system requirements are known for this game. They are fetched from the Steam Store for Steam games.",
                    &theme,
                ));
                return;
            };
            let drive = game.install_dir.as_deref();
            let free = drive.and_then(hardware::free_space_gb);
            for (heading, text) in [("Minimum", Some(minimum)), ("Recommended", recommended)] {
                let Some(text) = text else { continue };
                let checks = hardware::check_requirements(text, &specs, free, game.installed);
                if checks.is_empty() {
                    continue;
                }
                ui.add_space(6.0);
                let failed = checks.iter().any(|c| c.verdict == Verdict::Fail);
                ui.horizontal(|ui| {
                    ui.label(paint::caps(heading, 11.5, theme.muted));
                    if failed {
                        ui.label(
                            egui::RichText::new("below what's asked for")
                                .color(theme.danger)
                                .size(13.0),
                        );
                    }
                });
                // Laid out by hand with fixed column widths, so long
                // requirement text wraps within its own column.
                let full = ui.available_width();
                let needed_width = ((full - 150.0) * 0.5).max(160.0);
                for check in &checks {
                    let (mark, colour) = match check.verdict {
                        Verdict::Pass => ("✓", theme.success),
                        Verdict::Fail => ("✗", theme.danger),
                        Verdict::Compare => ("?", theme.muted),
                    };
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        ui.allocate_ui(vec2(18.0, 20.0), |ui| {
                            ui.label(egui::RichText::new(mark).font(bold(15.0)).color(colour));
                        });
                        ui.allocate_ui(vec2(96.0, 20.0), |ui| {
                            ui.set_min_width(96.0);
                            ui.label(paint::muted(check.part, &theme));
                        });
                        ui.allocate_ui(vec2(needed_width, 20.0), |ui| {
                            ui.set_min_width(needed_width);
                            ui.add(egui::Label::new(&check.needed).wrap());
                        });
                        ui.add(
                            egui::Label::new(paint::muted(
                                format!("Yours: {}", check.yours),
                                &theme,
                            ))
                            .wrap(),
                        );
                    });
                }
            }
            ui.add_space(4.0);
            ui.label(
                paint::muted(
                    "Memory and storage are checked for you. Rows marked ? can't be judged automatically: compare them yourself.",
                    &theme,
                )
                .size(13.0),
            );
        });
    }

    /// Save backups and installed mods.
    pub fn saves_and_mods_panel(&mut self, ui: &mut egui::Ui, game: &Game, extras: &GameExtras) {
        let theme = self.theme;
        let backups_root = paths::backups_dir().ok();
        paint::panel(ui, theme.panel_frame().inner_margin(24.0), |ui| {
            ui.set_width(ui.available_width());
            ui.columns(2, |columns| {
                // Saves.
                let ui = &mut columns[0];
                ui.label(paint::strong("Save backups", 16.0));
                match (&extras.save_dir, &backups_root) {
                    (Some(save_dir), Some(root)) => {
                        ui.add(egui::Label::new(paint::muted(save_dir.display().to_string(), &theme).size(13.0)).truncate());
                        let backups = backup::list_backups(root, game.id);
                        ui.horizontal(|ui| {
                            if paint::accent_button(ui, &theme, "Back up now", 14.0, vec2(16.0, 9.0)).clicked() {
                                match backup::backup_saves(save_dir, root, game.id, now()) {
                                    Ok(made) => {
                                        self.status = format!(
                                            "Backed up {} files ({}).",
                                            made.files,
                                            megabytes(made.bytes)
                                        );
                                    }
                                    Err(err) => self.error = Some(format!("{err:#}")),
                                }
                            }
                            if ui.button("Change folder…").clicked()
                                && let Some(dir) = rfd::FileDialog::new().set_title("Where does this game keep its saves?").pick_folder()
                            {
                                self.check(self.db.set_save_dir(game.id, Some(&dir)));
                            }
                        });
                        if backups.is_empty() {
                            ui.label(paint::muted("No backups yet.", &theme));
                        }
                        for made in backups.iter().take(5) {
                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "{} · {} files · {}",
                                    format_ago(made.made_at, now()),
                                    made.files,
                                    megabytes(made.bytes)
                                ));
                                if ui
                                    .small_button("Restore")
                                    .on_hover_text("Puts these saves back. Your current saves are backed up first, so this can be undone.")
                                    .clicked()
                                {
                                    match backup::restore_saves(made, save_dir, root, game.id, now()) {
                                        Ok(()) => self.status = "Saves restored.".to_string(),
                                        Err(err) => self.error = Some(format!("{err:#}")),
                                    }
                                }
                            });
                        }
                        if backups.len() > 5 {
                            ui.label(paint::muted(format!("…and {} older", backups.len() - 5), &theme).size(13.0));
                        }
                    }
                    _ => {
                        ui.label(paint::muted(
                            "Tell GameLib where this game keeps its saves, and it can back them up.",
                            &theme,
                        ));
                        if ui.button("Choose save folder…").clicked()
                            && let Some(dir) = rfd::FileDialog::new().set_title("Where does this game keep its saves?").pick_folder()
                        {
                            self.check(self.db.set_save_dir(game.id, Some(&dir)));
                        }
                    }
                }

                // Mods.
                let ui = &mut columns[1];
                ui.label(paint::strong("Mods", 16.0));
                match &extras.mods_dir {
                    Some(mods_dir) => {
                        ui.add(egui::Label::new(paint::muted(mods_dir.display().to_string(), &theme).size(13.0)).truncate());
                        match backup::list_mods(mods_dir) {
                            Ok(mods) if mods.is_empty() => {
                                ui.label(paint::muted("No mods installed.", &theme));
                            }
                            Ok(mods) => {
                                let total: u64 = mods.iter().map(|m| m.bytes).sum();
                                ui.label(format!("{} installed · {}", mods.len(), megabytes(total)));
                                egui::ScrollArea::vertical()
                                    .id_salt("mods")
                                    .max_height(150.0)
                                    .show(ui, |ui| {
                                        for entry in &mods {
                                            ui.horizontal(|ui| {
                                                ui.label(&entry.name);
                                                ui.label(paint::muted(megabytes(entry.bytes), &theme).size(13.0));
                                            });
                                        }
                                    });
                            }
                            Err(err) => {
                                ui.label(egui::RichText::new(format!("{err:#}")).color(theme.danger));
                            }
                        }
                        if ui.button("Change folder…").clicked()
                            && let Some(dir) = rfd::FileDialog::new().set_title("Where are this game's mods?").pick_folder()
                        {
                            self.check(self.db.set_mods_dir(game.id, Some(&dir)));
                        }
                    }
                    None => {
                        ui.label(paint::muted(
                            "Point GameLib at this game's mods folder to see what's installed.",
                            &theme,
                        ));
                        if ui.button("Choose mods folder…").clicked()
                            && let Some(dir) = rfd::FileDialog::new().set_title("Where are this game's mods?").pick_folder()
                        {
                            self.check(self.db.set_mods_dir(game.id, Some(&dir)));
                        }
                    }
                }
            });
        });
    }
}
