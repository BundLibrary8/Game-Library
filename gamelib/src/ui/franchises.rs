//! The franchises screen: your series, in playing order, with how far
//! through each you are; and suggestions for series GameLib has spotted.

use eframe::egui::{self, Rect, Sense, Stroke, vec2};
use gamelib::extras::Status;
use gamelib::insights;

use super::theme::{bold, regular};
use super::{GameLibApp, Screen, paint};

impl GameLibApp {
    pub fn franchises_screen(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        ui.label(egui::RichText::new("Franchises").font(bold(32.0)));
        ui.label(paint::muted(
            "Series you own, in playing order. Add a game to one from its ⋯ menu.",
            &theme,
        ));
        ui.add_space(10.0);

        let franchises = insights::franchises(&self.data.visible, &self.data.extras);
        let guesses = insights::guess_franchises(&self.data.visible, &self.data.extras);
        let mut open: Option<i64> = None;
        let mut create: Option<insights::FranchiseGuess> = None;

        if franchises.is_empty() && guesses.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("No series yet").font(bold(22.0)));
                ui.label(paint::muted(
                    "When you own two games from the same series, it will be suggested here.",
                    &theme,
                ));
            });
            return;
        }

        let slot = vec2(118.0, 177.0);
        for franchise in &franchises {
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(ui.available_width());
                let total = franchise.games.len();
                ui.horizontal(|ui| {
                    ui.label(paint::strong(&franchise.name, 20.0));
                    let done = franchise.finished == total;
                    let text = if done {
                        "Series finished".to_string()
                    } else {
                        format!("{} of {total} finished", franchise.finished)
                    };
                    ui.label(egui::RichText::new(text).color(if done {
                        theme.success
                    } else {
                        theme.muted
                    }));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        paint::bar(
                            ui,
                            &theme,
                            franchise.finished as f32 / total.max(1) as f32,
                            theme.accent,
                            220.0,
                        );
                    });
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(14.0, 14.0);
                    for (index, game_id) in franchise.games.iter().enumerate() {
                        let Some(game) = self.data.game(*game_id) else {
                            continue;
                        };
                        let (rect, response) = ui.allocate_exact_size(slot, Sense::click());
                        paint::cover(ui, rect, game, 10.0, 13.0);
                        // The place in the playing order, top-left.
                        let badge =
                            Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(26.0, 26.0));
                        let finished = self
                            .data
                            .extras
                            .get(game_id)
                            .is_some_and(|e| e.status == Some(Status::Finished));
                        let fill = if finished {
                            theme.success
                        } else {
                            theme.opaque_panel()
                        };
                        ui.painter().circle_filled(badge.center(), 13.0, fill);
                        ui.painter().circle_stroke(
                            badge.center(),
                            13.0,
                            Stroke::new(1.0, theme.border),
                        );
                        ui.painter().text(
                            badge.center(),
                            egui::Align2::CENTER_CENTER,
                            if finished {
                                "✓".to_string()
                            } else {
                                (index + 1).to_string()
                            },
                            bold(13.0),
                            if finished {
                                theme.on_accent
                            } else {
                                theme.text
                            },
                        );
                        if response
                            .on_hover_text(&game.title)
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            open = Some(*game_id);
                        }
                    }
                });
            });
            ui.add_space(14.0);
        }

        if !guesses.is_empty() {
            ui.add_space(6.0);
            ui.label(paint::strong("Suggested", 19.0));
            ui.label(paint::muted(
                "These look like series, going by their names. Check the order, then create the ones that are right.",
                &theme,
            ));
            ui.add_space(4.0);
            for guess in &guesses {
                paint::panel(ui, theme.panel_frame(), |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(paint::strong(&guess.name, 17.0));
                        ui.label(paint::muted(format!("{} games", guess.games.len()), &theme));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Create franchise").clicked() {
                                create = Some(guess.clone());
                            }
                        });
                    });
                    let names: Vec<String> = guess
                        .games
                        .iter()
                        .enumerate()
                        .map(|(i, (id, _))| format!("{}. {}", i + 1, self.title_of(*id)))
                        .collect();
                    ui.label(
                        egui::RichText::new(names.join("   "))
                            .font(regular(14.5))
                            .color(theme.muted),
                    );
                });
                ui.add_space(10.0);
            }
        }

        if let Some(guess) = create {
            let result = guess
                .games
                .iter()
                .enumerate()
                .try_for_each(|(index, (id, _))| {
                    self.db
                        .set_franchise(*id, Some(&guess.name), Some(index as i64 + 1))
                });
            if self.check(result).is_some() {
                self.status = format!("Created the {} franchise.", guess.name);
            }
        }
        if let Some(game_id) = open {
            self.go(Screen::Details(game_id));
        }
    }
}
