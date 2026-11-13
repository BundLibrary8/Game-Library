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
        // The totals come from the play history, so they're worked out
        // when the library changes (or once a minute), not every frame.
        let time = ui.input(|i| i.time);
        let db = &self.db;
        let mut failed = None;
        let numbers = super::memo(&mut self.memos.stats, self.generation, time, 60.0, || {
            let loaded = (|| -> anyhow::Result<super::StatsNumbers> {
                let now = now();
                let month = db.playtime_by_day(now - 30 * SECS_PER_DAY, now + 1)?;
                let days =
                    db.playtime_by_day(now - HEATMAP_WEEKS as i64 * 7 * SECS_PER_DAY, now + 1)?;
                Ok(super::StatsNumbers {
                    top: db.top_played(10)?,
                    weekly: db.weekly_playtime(None, now, 12)?,
                    last_30: month.iter().map(|d| d.secs).sum::<u64>(),
                    heatmap: days
                        .iter()
                        .map(|d| (d.day_start / SECS_PER_DAY, d.secs))
                        .collect(),
                })
            })();
            std::rc::Rc::new(loaded.unwrap_or_else(|err| {
                failed = Some(format!("{err:#}"));
                super::StatsNumbers {
                    top: Vec::new(),
                    weekly: vec![0; 12],
                    last_30: 0,
                    heatmap: Default::default(),
                }
            }))
        });
        if let Some(err) = failed {
            self.memos.stats = None;
            ui.colored_label(theme.danger, err);
            return;
        }
        let (top, weekly, last_30) = (numbers.top.clone(), numbers.weekly.clone(), numbers.last_30);
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
                let year = gamelib::patterns::year_of(now(), super::local_offset());
                if paint::accent_button(
                    ui,
                    &theme,
                    &format!("Your {year} Wrapped"),
                    14.0,
                    vec2(18.0, 9.0),
                )
                .on_hover_text("A look back at your year in games")
                .clicked()
                {
                    self.go(super::Screen::Wrapped(year));
                }
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
                let by_day = &numbers.heatmap;
                heatmap(ui, &theme, by_day, now / SECS_PER_DAY, half - 38.0);
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

        ui.add_space(18.0);
        self.patterns_panels(ui, half, gap);
        ui.add_space(18.0);
        self.achievements_panel(ui);
    }

    /// When you play, your habits, and your genres.
    fn patterns_panels(&mut self, ui: &mut egui::Ui, half: f32, gap: f32) {
        let theme = self.theme;
        let p = self.data.patterns.clone();
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(half - 38.0);
                ui.set_min_height(300.0);
                ui.horizontal(|ui| {
                    ui.label(paint::strong("When you play", 17.0));
                    ui.label(paint::muted("· by the hour sessions start", &theme));
                });
                ui.add_space(6.0);
                column_chart(ui, &theme, &p.by_hour, half - 38.0, 110.0, |i| match i {
                    0 => Some("12am"),
                    6 => Some("6am"),
                    12 => Some("12pm"),
                    18 => Some("6pm"),
                    _ => None,
                });
                ui.add_space(12.0);
                ui.label(paint::muted("By day of the week", &theme).size(13.0));
                column_chart(ui, &theme, &p.by_weekday, half - 38.0, 70.0, |i| {
                    Some(["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][i])
                });
            });
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(half - 38.0);
                ui.set_min_height(300.0);
                ui.label(paint::strong("Your habits", 17.0));
                ui.add_space(6.0);
                egui::Grid::new("habits")
                    .num_columns(2)
                    .spacing([24.0, 8.0])
                    .show(ui, |ui| {
                        let mut row = |label: &str, value: String| {
                            ui.label(paint::muted(label, &theme));
                            ui.label(paint::strong(value, 15.0));
                            ui.end_row();
                        };
                        row("Sessions", p.sessions.to_string());
                        row("Average session", short_playtime(p.average_session_secs()));
                        if let Some((id, secs, _)) = p.longest_session {
                            row(
                                "Longest session",
                                format!("{} ({})", short_playtime(secs), self.title_of(id)),
                            );
                        }
                        row("Days played", p.days_played.to_string());
                        row("Current streak", format!("{} days", p.current_streak));
                        row("Longest streak", format!("{} days", p.longest_streak));
                    });
                let genres = gamelib::patterns::genre_hours(&self.data.visible, &p.by_game);
                if !genres.is_empty() {
                    ui.add_space(10.0);
                    ui.label(paint::strong("Top genres", 15.0));
                    let most = genres[0].1.max(1);
                    let bar_width = (half - 38.0 - 250.0).max(80.0);
                    for (genre, secs) in genres.iter().take(5) {
                        ui.horizontal(|ui| {
                            name_cell(ui, genre, 150.0);
                            paint::bar(
                                ui,
                                &theme,
                                *secs as f32 / most as f32,
                                theme.accent_end,
                                bar_width,
                            );
                            ui.label(paint::muted(short_playtime(*secs), &theme));
                        });
                    }
                }
            });
        });
    }

    /// GameLib's own achievements.
    fn achievements_panel(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let all = &self.data.achievements;
        let unlocked = all.iter().filter(|a| a.unlocked()).count();
        paint::panel(ui, theme.panel_frame(), |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(paint::strong("Achievements", 17.0));
                ui.label(paint::muted(
                    format!("· {unlocked} of {} unlocked", all.len()),
                    &theme,
                ));
            });
            ui.add_space(8.0);
            let gap = 12.0;
            let width = ui.available_width();
            let columns = ((width + gap) / (250.0 + gap)).floor().max(1.0) as usize;
            let tile = (width - gap * (columns as f32 - 1.0)) / columns as f32;
            for row in all.chunks(columns) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for a in row {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(tile, 74.0), egui::Sense::hover());
                        let painter = ui.painter();
                        let done = a.unlocked();
                        painter.rect_filled(
                            rect,
                            12.0,
                            if done { theme.panel_alt } else { theme.panel },
                        );
                        painter.rect_stroke(
                            rect,
                            12.0,
                            egui::Stroke::new(
                                1.0,
                                if done {
                                    theme.accent.gamma_multiply(0.6)
                                } else {
                                    theme.border
                                },
                            ),
                            egui::StrokeKind::Inside,
                        );
                        let badge = rect.left_center() + vec2(30.0, 0.0);
                        painter.circle_filled(
                            badge,
                            18.0,
                            if done { theme.accent } else { theme.panel_alt },
                        );
                        paint::star4(
                            painter,
                            badge,
                            9.0,
                            if done { theme.on_accent } else { theme.muted },
                        );
                        let left = rect.left() + 58.0;
                        let clip = painter.with_clip_rect(rect.shrink(4.0));
                        clip.text(
                            egui::pos2(left, rect.top() + 14.0),
                            egui::Align2::LEFT_TOP,
                            a.name,
                            super::theme::bold(15.0),
                            if done { theme.text } else { theme.muted },
                        );
                        clip.text(
                            egui::pos2(left, rect.top() + 34.0),
                            egui::Align2::LEFT_TOP,
                            a.description,
                            super::theme::regular(12.0),
                            theme.muted,
                        );
                        if !done {
                            let track = egui::Rect::from_min_size(
                                egui::pos2(left, rect.bottom() - 14.0),
                                vec2(rect.right() - left - 60.0, 4.0),
                            );
                            painter.rect_filled(track, 2.0, theme.panel_alt);
                            let mut fill = track;
                            fill.set_width((track.width() * a.fraction()).max(3.0));
                            painter.rect_filled(fill, 2.0, theme.accent);
                            painter.text(
                                egui::pos2(rect.right() - 12.0, track.center().y),
                                egui::Align2::RIGHT_CENTER,
                                format!("{}/{}", a.progress, a.goal),
                                super::theme::regular(11.5),
                                theme.muted,
                            );
                        }
                        response.on_hover_text(if done { "Unlocked" } else { "Not yet" });
                    }
                });
                ui.add_space(gap);
            }
        });
    }
}

/// A simple column chart: one column per value, with optional labels.
fn column_chart<const N: usize>(
    ui: &mut egui::Ui,
    theme: &super::theme::Theme,
    values: &[u64; N],
    width: f32,
    height: f32,
    label: impl Fn(usize) -> Option<&'static str>,
) {
    let (rect, response) = ui.allocate_exact_size(vec2(width, height + 18.0), egui::Sense::hover());
    let painter = ui.painter();
    let most = values.iter().copied().max().unwrap_or(0).max(1);
    let slot = width / N as f32;
    let hover = response.hover_pos();
    let mut hovered = None;
    for (i, value) in values.iter().enumerate() {
        let x = rect.left() + i as f32 * slot;
        let h = (*value as f32 / most as f32 * height).max(2.0);
        let column = egui::Rect::from_min_max(
            egui::pos2(x + slot * 0.15, rect.top() + height - h),
            egui::pos2(x + slot * 0.85, rect.top() + height),
        );
        let colour = if *value > 0 {
            theme.accent
        } else {
            theme.panel_alt
        };
        painter.rect_filled(column, 3.0, colour);
        if let Some(text) = label(i) {
            painter.text(
                egui::pos2(x + slot / 2.0, rect.top() + height + 4.0),
                egui::Align2::CENTER_TOP,
                text,
                super::theme::regular(11.0),
                theme.muted,
            );
        }
        if hover.is_some_and(|pos| pos.x >= x && pos.x < x + slot) {
            hovered = Some(*value);
        }
    }
    if let Some(secs) = hovered {
        response.on_hover_text_at_pointer(short_playtime(secs));
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
