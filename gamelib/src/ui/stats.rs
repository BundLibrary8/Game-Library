//! The stats screen.

use eframe::egui::{self, vec2};
use gamelib::model::Source;
use gamelib::now;

use super::details::weekly_chart;
use super::theme::{bold, source_colour, source_name};
use super::{GameLibApp, paint};
use crate::text::short_playtime;

const SECS_PER_DAY: i64 = 86_400;

impl GameLibApp {
    pub fn stats_screen(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let loaded = (|| -> anyhow::Result<_> {
            let now = now();
            let month = self.db.playtime_by_day(now - 30 * SECS_PER_DAY, now + 1)?;
            Ok((
                self.db.top_played(10)?,
                self.db.weekly_playtime(None, now, 12)?,
                month.iter().map(|d| d.secs).sum::<u64>(),
            ))
        })();
        let (top, weekly, last_30) = match loaded {
            Ok(values) => values,
            Err(err) => {
                ui.colored_label(theme.danger, format!("{err:#}"));
                return;
            }
        };
        let stats = self.data.stats.clone();
        let never_played = self
            .data
            .visible
            .iter()
            .filter(|g| g.playtime_secs == 0)
            .count();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Stats").font(bold(32.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.stardust_pill(ui);
            });
        });
        ui.add_space(10.0);
        paint::stat_row(
            ui,
            &theme,
            &[
                ("Total playtime", short_playtime(stats.total_playtime_secs)),
                ("Library size", format!("{} games", stats.game_count)),
                ("Last 30 days", short_playtime(last_30)),
                ("Never played", never_played.to_string()),
            ],
        );
        ui.add_space(18.0);

        let gap = 18.0;
        let half = ((ui.available_width() - gap) / 2.0).floor();
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            // Most played.
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(half - 38.0);
                ui.set_min_height(330.0);
                ui.label(paint::strong("Most played", 17.0));
                ui.add_space(6.0);
                if top.is_empty() {
                    ui.label(paint::muted(
                        "Nothing played yet. Launch a game and it will show up here.",
                        &theme,
                    ));
                }
                let longest = top.first().map(|g| g.playtime_secs).unwrap_or(1).max(1);
                let bar_width = (half - 38.0 - 260.0).max(80.0);
                for game in &top {
                    ui.horizontal(|ui| {
                        name_cell(ui, &game.title, 170.0);
                        let fraction = game.playtime_secs as f32 / longest as f32;
                        paint::bar(ui, &theme, fraction, theme.accent, bar_width);
                        ui.label(paint::muted(short_playtime(game.playtime_secs), &theme));
                    });
                }
            });

            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = gap;
                theme.panel_frame().show(ui, |ui| {
                    ui.set_width(half - 38.0);
                    ui.horizontal(|ui| {
                        ui.label(paint::strong("Weekly playtime", 17.0));
                        ui.label(paint::muted("· all games, last 12 weeks", &theme));
                    });
                    weekly_chart(ui, &theme, &weekly, half - 38.0, 150.0);
                });
                theme.panel_frame().show(ui, |ui| {
                    ui.set_width(half - 38.0);
                    ui.label(paint::strong("Games by source", 17.0));
                    ui.add_space(6.0);
                    let total = self.data.visible.len().max(1);
                    let bar_width = (half - 38.0 - 250.0).max(80.0);
                    for source in Source::ALL {
                        let count = self
                            .data
                            .visible
                            .iter()
                            .filter(|g| g.source == source)
                            .count();
                        if count == 0 {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            name_cell(ui, source_name(source), 150.0);
                            paint::bar(
                                ui,
                                &theme,
                                count as f32 / total as f32,
                                source_colour(source),
                                bar_width,
                            );
                            ui.label(paint::muted(count.to_string(), &theme));
                        });
                    }
                    if self.data.visible.is_empty() {
                        ui.label(paint::muted("No games yet.", &theme));
                    }
                    ui.allocate_space(vec2(0.0, 0.0));
                });
            });
        });
    }
}

/// A fixed-width, left-aligned name at the start of a bar row, cut short
/// with "…" if it doesn't fit.
fn name_cell(ui: &mut egui::Ui, text: &str, width: f32) {
    ui.allocate_ui_with_layout(
        vec2(width, 20.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(width);
            ui.add(egui::Label::new(text).truncate());
        },
    );
}
