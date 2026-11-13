//! The stats screen.

use eframe::egui::{self, vec2};
use gamelib::extras::Status;
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

        ui.add_space(18.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            // Activity heatmap: one square per day, brighter the more you played.
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(half - 38.0);
                ui.horizontal(|ui| {
                    ui.label(paint::strong("Activity", 17.0));
                    ui.label(paint::muted("· each square is a day, last 26 weeks", &theme));
                });
                ui.add_space(4.0);
                let now = now();
                let days = self
                    .db
                    .playtime_by_day(now - HEATMAP_WEEKS as i64 * 7 * SECS_PER_DAY, now + 1)
                    .unwrap_or_default();
                let by_day: std::collections::HashMap<i64, u64> =
                    days.iter().map(|d| (d.day_start / SECS_PER_DAY, d.secs)).collect();
                heatmap(ui, &theme, &by_day, now / SECS_PER_DAY, half - 38.0);
                let active = by_day.values().filter(|secs| **secs > 0).count();
                ui.label(paint::muted(format!("You played on {active} days."), &theme).size(13.5));
            });

            // Completion: how many games are at each stage.
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(half - 38.0);
                ui.label(paint::strong("Completion", 17.0));
                ui.add_space(6.0);
                let total = self.data.visible.len().max(1);
                let count = |status: Option<Status>| {
                    self.data
                        .visible
                        .iter()
                        .filter(|g| self.data.extras.get(&g.id).and_then(|e| e.status) == status)
                        .count()
                };
                let bar_width = (half - 38.0 - 250.0).max(80.0);
                let colours = [theme.muted, theme.accent, theme.success, theme.stardust, theme.danger];
                for (status, colour) in Status::ALL.into_iter().zip(colours) {
                    let games = count(Some(status));
                    ui.horizontal(|ui| {
                        name_cell(ui, status.label(), 150.0);
                        paint::bar(ui, &theme, games as f32 / total as f32, colour, bar_width);
                        ui.label(paint::muted(games.to_string(), &theme));
                    });
                }
                let unset = count(None);
                let finished = count(Some(Status::Finished));
                ui.add_space(4.0);
                ui.label(
                    paint::muted(
                        format!(
                            "{}% of your library finished. {unset} games have no status yet; set one on a game's page.",
                            finished * 100 / total
                        ),
                        &theme,
                    )
                    .size(13.5),
                );
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

/// How many weeks the activity heatmap covers.
const HEATMAP_WEEKS: usize = 26;

/// A calendar of squares: a column per week, a row per day of the week,
/// each square brighter the longer you played that day. `by_day` maps a
/// day number (days since 1970) to seconds played; `today` is today's.
fn heatmap(
    ui: &mut egui::Ui,
    theme: &super::theme::Theme,
    by_day: &std::collections::HashMap<i64, u64>,
    today: i64,
    width: f32,
) {
    let gap = 3.0;
    let cell =
        ((width - gap * (HEATMAP_WEEKS as f32 - 1.0)) / HEATMAP_WEEKS as f32).clamp(6.0, 18.0);
    let size = vec2(
        HEATMAP_WEEKS as f32 * (cell + gap) - gap,
        7.0 * (cell + gap) - gap,
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let most = by_day.values().copied().max().unwrap_or(0).max(1);
    // Day 0 (1 January 1970) was a Thursday; make Monday the top row.
    let weekday = |day: i64| (day + 3).rem_euclid(7);
    let last_column_start = today - weekday(today);
    let hover = response.hover_pos();
    let mut hovered: Option<(i64, u64)> = None;

    for week in 0..HEATMAP_WEEKS {
        for row in 0..7 {
            let day = last_column_start - (HEATMAP_WEEKS - 1 - week) as i64 * 7 + row;
            if day > today {
                continue; // the rest of this week hasn't happened yet
            }
            let min = rect.min + vec2(week as f32 * (cell + gap), row as f32 * (cell + gap));
            let square = egui::Rect::from_min_size(min, vec2(cell, cell));
            let secs = by_day.get(&day).copied().unwrap_or(0);
            let colour = if secs == 0 {
                theme.panel_alt
            } else {
                // Even a short session should be clearly visible.
                let strength = 0.3 + 0.7 * (secs as f32 / most as f32).sqrt();
                theme.accent.gamma_multiply(strength)
            };
            ui.painter().rect_filled(square, 3.0, colour);
            if hover.is_some_and(|pos| square.contains(pos)) {
                hovered = Some((day, secs));
            }
        }
    }
    if let Some((day, secs)) = hovered {
        let ago = today - day;
        let when = match ago {
            0 => "Today".to_string(),
            1 => "Yesterday".to_string(),
            n => format!("{n} days ago"),
        };
        let played = if secs == 0 {
            "nothing played".to_string()
        } else if secs < 60 {
            "under a minute".to_string()
        } else {
            short_playtime(secs)
        };
        response.on_hover_text_at_pointer(format!("{when}: {played}"));
    }
}
