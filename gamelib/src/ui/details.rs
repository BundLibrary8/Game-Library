//! One game's page: cover, Launch, the numbers, a chart of weekly
//! playtime, and tags.

use eframe::egui::{self, Color32, Rect, Sense, Stroke, pos2, vec2};
use gamelib::model::LaunchTarget;
use gamelib::now;
use gamelib::shop::FINISHED_COLLECTION;

use super::dialogs::Dialog;
use super::theme::{Backdrop, Theme, bold, regular, source_name};
use super::{GameLibApp, Screen, paint};
use crate::text::{format_last_played, long_playtime, short_playtime};

/// How many weeks the playtime chart shows.
const CHART_WEEKS: usize = 12;

impl GameLibApp {
    pub fn details_screen(&mut self, ui: &mut egui::Ui, game_id: i64) {
        let theme = self.theme;
        if ui
            .add(egui::Button::new(paint::muted("←  Library", &theme)).frame(false))
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            self.go(Screen::Library);
            return;
        }
        let Some(game) = self
            .data
            .game(game_id)
            .cloned()
            .or_else(|| self.db.get_game(game_id).ok().flatten())
        else {
            ui.label("This game is no longer in the library.");
            return;
        };
        let tags = self.db.tags_for_game(game_id).unwrap_or_default();
        let sessions = self.db.session_count(game_id).unwrap_or(0);
        let weekly = self
            .db
            .weekly_playtime(Some(game_id), now(), CHART_WEEKS)
            .unwrap_or_else(|_| vec![0; CHART_WEEKS]);
        let is_finished = tags.iter().any(|t| t.name == FINISHED_COLLECTION);
        let extras = self.data.extras.get(&game_id).cloned().unwrap_or_default();
        let running = self.running.contains(&game_id);
        ui.add_space(6.0);

        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 36.0;
            // The cover, big.
            let cover_width = (ui.available_width() * 0.24).clamp(220.0, 340.0);
            let (cover_rect, _) =
                ui.allocate_exact_size(vec2(cover_width, cover_width * 1.5), Sense::hover());
            paint::cover(ui, cover_rect, &game, 18.0, 20.0);
            paint::source_badge(ui, cover_rect, &game);

            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 10.0;
                // "SPACE · EXPLORATION · 2022"
                let mut facts: Vec<String> = game.genres.iter().take(3).cloned().collect();
                if let Some(year) = game.release_year {
                    facts.push(year.to_string());
                }
                if facts.is_empty() {
                    facts.push(source_name(game.source).to_string());
                }
                ui.label(
                    paint::caps(&facts.join(" · "), 13.0, theme.accent).extra_letter_spacing(3.0),
                );
                ui.add(egui::Label::new(egui::RichText::new(&game.title).font(bold(46.0))).wrap());
                if let Some(description) = &game.description {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(description)
                                .size(16.5)
                                .color(theme.text.gamma_multiply(0.85)),
                        )
                        .wrap(),
                    );
                }
                ui.add_space(8.0);

                // Launch, favourite, and the ⋯ menu.
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    let label = if running {
                        "RUNNING…"
                    } else if game.installed {
                        "▶  LAUNCH"
                    } else {
                        "NOT INSTALLED"
                    };
                    let launch = ui
                        .add_enabled_ui(game.installed && !running, |ui| {
                            paint::accent_button(ui, &theme, label, 19.0, vec2(38.0, 18.0))
                        })
                        .inner;
                    if launch.clicked() {
                        self.play(game_id);
                    }

                    let star_colour = if game.favourite {
                        Color32::from_rgb(0xff, 0xcf, 0x4a)
                    } else {
                        theme.muted
                    };
                    let star =
                        egui::Button::new(egui::RichText::new("★").size(22.0).color(star_colour))
                            .min_size(vec2(56.0, 56.0))
                            .corner_radius(14.0);
                    if ui
                        .add(star)
                        .on_hover_text(if game.favourite {
                            "Remove from favourites"
                        } else {
                            "Add to favourites"
                        })
                        .clicked()
                    {
                        self.check(self.db.set_favourite(game_id, !game.favourite));
                    }

                    let more = egui::Button::new(egui::RichText::new("⋯").size(22.0))
                        .min_size(vec2(56.0, 56.0))
                        .corner_radius(14.0);
                    egui::containers::menu::MenuButton::from_button(more).ui(ui, |ui| {
                        self.more_menu(ui, &game, is_finished);
                    });
                });
                ui.add_space(6.0);
                self.status_and_rating(ui, &game, &extras);
                ui.add_space(8.0);

                paint::stat_row(
                    ui,
                    &theme,
                    &[
                        ("Playtime", long_playtime(game.playtime_secs)),
                        ("Last played", format_last_played(game.last_played, now())),
                        ("Sessions", sessions.to_string()),
                        ("Source", source_name(game.source).to_string()),
                    ],
                );
            });
        });
        ui.add_space(22.0);

        // The chart and the tags, side by side in one panel.
        theme.panel_frame().inner_margin(24.0).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let tags_width = 330.0_f32.min(ui.available_width() * 0.35);
                let chart_width = ui.available_width() - tags_width - 30.0;
                ui.vertical(|ui| {
                    ui.set_width(chart_width);
                    ui.horizontal(|ui| {
                        ui.label(paint::strong("Weekly playtime", 16.0));
                        let note = match theme.backdrop {
                            Backdrop::Constellation => "· plotted as a constellation",
                            _ => "· last 12 weeks",
                        };
                        ui.label(paint::muted(note, &theme));
                    });
                    weekly_chart(ui, &theme, &weekly, chart_width, 190.0);
                });
                ui.add_space(30.0);
                ui.vertical(|ui| {
                    ui.label(paint::strong("Tags", 16.0));
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                        for tag in &tags {
                            if paint::chip(ui, &theme, &tag.name, false)
                                .on_hover_text("Click to remove from this game")
                                .clicked()
                            {
                                self.check(self.db.untag_game(game_id, tag.id));
                            }
                        }
                        if paint::chip(ui, &theme, "+ Tag", false).clicked() {
                            self.dialog = Dialog::NewTag {
                                game_id,
                                name: String::new(),
                                as_collection: false,
                            };
                        }
                    });
                });
            });
        });
        ui.add_space(18.0);
        self.notes_panel(ui, &game, &extras);
        ui.add_space(14.0);
        self.requirements_panel(ui, &game, &extras);
        ui.add_space(18.0);
        self.saves_and_mods_panel(ui, &game, &extras);
    }

    /// The ⋯ menu on a game's page.
    fn more_menu(&mut self, ui: &mut egui::Ui, game: &gamelib::model::Game, is_finished: bool) {
        let id = game.id;
        ui.set_min_width(220.0);
        if ui.button("Rename…").clicked() {
            self.dialog = Dialog::Rename {
                game_id: id,
                title: game.title.clone(),
            };
            ui.close();
        }
        if ui.button("Change cover…").clicked() {
            let picked = rfd::FileDialog::new()
                .set_title("Choose a cover image")
                .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
                .pick_file();
            if let Some(image) = picked {
                self.check(self.db.set_cover_manual(id, &image));
            }
            ui.close();
        }
        if ui.button("Search for a cover…").clicked() {
            self.dialog = Dialog::CoverSearch {
                game_id: id,
                title: game.title.clone(),
            };
            ui.close();
        }
        if matches!(game.launch, LaunchTarget::Exe { .. }) && ui.button("Locate exe…").clicked() {
            if let Some(exe) = rfd::FileDialog::new()
                .set_title("Where is the game's exe?")
                .pick_file()
            {
                self.check(self.db.set_launch(id, &LaunchTarget::exe(exe)));
            }
            ui.close();
        }
        ui.separator();
        if !is_finished && ui.button("Mark as finished").clicked() {
            let result = self.db.finish_game(id, now()).and_then(|reward| {
                let unlocked = self.db.grant_earned_items(now())?;
                Ok((reward, unlocked))
            });
            if let Some((reward, unlocked)) = self.check(result) {
                self.status = format!("Marked as finished. +{reward} Stardust");
                if !unlocked.is_empty() {
                    self.status
                        .push_str(" Something new unlocked on your profile!");
                }
            }
            ui.close();
        }
        if ui.button("Set franchise…").clicked() {
            let extras = self.data.extras.get(&id).cloned().unwrap_or_default();
            self.dialog = Dialog::Franchise {
                game_id: id,
                name: extras
                    .franchise
                    .unwrap_or_else(|| gamelib::insights::series_of(&game.title).0),
                order: extras
                    .franchise_order
                    .map(|n| n.to_string())
                    .unwrap_or_default(),
            };
            ui.close();
        }
        let collections: Vec<_> = self
            .data
            .collections()
            .map(|(tag, _)| tag.clone())
            .collect();
        ui.menu_button("Add to collection", |ui| {
            for tag in &collections {
                if ui.button(&tag.name).clicked() {
                    self.check(self.db.tag_game(id, tag.id));
                    ui.close();
                }
            }
            if ui.button("New collection…").clicked() {
                self.dialog = Dialog::NewTag {
                    game_id: id,
                    name: String::new(),
                    as_collection: true,
                };
                ui.close();
            }
        });
        let showcased = self.data.showcase.contains(&id);
        if ui
            .button(if showcased {
                "Remove from profile showcase"
            } else {
                "Show on profile"
            })
            .clicked()
        {
            let mut games = self.data.showcase.clone();
            match games.iter().position(|g| *g == id) {
                Some(index) => {
                    games.remove(index);
                }
                None => games.push(id),
            }
            self.check(self.db.set_showcase(&games));
            ui.close();
        }
        ui.separator();
        if ui
            .button(if game.hidden {
                "Unhide"
            } else {
                "Hide from library"
            })
            .clicked()
        {
            self.check(self.db.set_hidden(id, !game.hidden));
            ui.close();
        }
        if ui
            .button(egui::RichText::new("Remove from GameLib…").color(self.theme.danger))
            .clicked()
        {
            self.dialog = Dialog::ConfirmRemove { game_id: id };
            ui.close();
        }
    }
}

/// The weekly playtime chart: one glowing point per week, joined by lines.
pub fn weekly_chart(ui: &mut egui::Ui, theme: &Theme, weekly: &[u64], width: f32, height: f32) {
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let painter = ui.painter();
    let plot = Rect::from_min_max(rect.min + vec2(18.0, 24.0), rect.max - vec2(18.0, 30.0));
    let max = weekly.iter().copied().max().unwrap_or(0);

    if max == 0 {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "No playtime in the last 12 weeks.",
            regular(14.5),
            theme.muted,
        );
        return;
    }

    let count = weekly.len().max(2);
    let points: Vec<egui::Pos2> = weekly
        .iter()
        .enumerate()
        .map(|(i, secs)| {
            let x = plot.left() + plot.width() * i as f32 / (count - 1) as f32;
            let y = plot.bottom() - plot.height() * (*secs as f32 / max as f32);
            pos2(x, y)
        })
        .collect();

    let line = Stroke::new(1.6, theme.accent.gamma_multiply(0.55));
    painter.line(points.clone(), line);
    let hover = response.hover_pos();
    for (i, point) in points.iter().enumerate() {
        let [r, g, b, _] = theme.accent.to_array();
        paint::glow(
            painter,
            *point,
            13.0,
            Color32::from_rgba_unmultiplied(r, g, b, 80),
        );
        painter.circle_filled(*point, 4.5, theme.accent);
        // Show the week's number when the mouse is near its point.
        if hover.is_some_and(|pos| (pos.x - point.x).abs() < plot.width() / count as f32 / 2.0) {
            painter.circle_stroke(*point, 8.0, Stroke::new(1.5, theme.text));
            let weeks_ago = weekly.len() - 1 - i;
            let when = match weeks_ago {
                0 => "this week".to_string(),
                1 => "last week".to_string(),
                n => format!("{n} weeks ago"),
            };
            painter.text(
                pos2(
                    point.x.clamp(rect.left() + 60.0, rect.right() - 60.0),
                    rect.top() + 4.0,
                ),
                egui::Align2::CENTER_TOP,
                format!("{} · {when}", short_playtime(weekly[i])),
                regular(13.0),
                theme.text,
            );
        }
    }
    painter.text(
        pos2(plot.left(), rect.bottom() - 4.0),
        egui::Align2::LEFT_BOTTOM,
        "12 weeks ago",
        regular(12.0),
        theme.muted,
    );
    painter.text(
        pos2(plot.right(), rect.bottom() - 4.0),
        egui::Align2::RIGHT_BOTTOM,
        "this week",
        regular(12.0),
        theme.muted,
    );
}
