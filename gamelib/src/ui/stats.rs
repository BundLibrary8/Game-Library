//! The stats screen.

use eframe::egui;
use gamelib::db::GameQuery;
use gamelib::now;

use super::{GameLibApp, source_name, stat_tile};
use crate::text::format_duration;

const SECS_PER_DAY: i64 = 86_400;

impl GameLibApp {
    pub fn stats_screen(&mut self, ui: &mut egui::Ui) {
        let loaded = (|| -> anyhow::Result<_> {
            let stats = self.db.library_stats()?;
            let top = self.db.top_played(10)?;
            let by_source = self.db.playtime_by_source()?;
            let now = now();
            let since = |days: i64| -> anyhow::Result<u64> {
                let rows = self
                    .db
                    .playtime_by_day(now - days * SECS_PER_DAY, now + 1)?;
                Ok(rows.iter().map(|d| d.secs).sum())
            };
            let never_played = self
                .db
                .query_games(&GameQuery::default())?
                .iter()
                .filter(|g| g.playtime_secs == 0)
                .count();
            Ok((stats, top, by_source, since(7)?, since(30)?, never_played))
        })();
        let (stats, top, by_source, last_7, last_30, never_played) = match loaded {
            Ok(values) => values,
            Err(err) => {
                ui.colored_label(ui.visuals().error_fg_color, format!("{err:#}"));
                return;
            }
        };

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.heading("Stats");
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                stat_tile(ui, &format_duration(stats.total_playtime_secs), "total playtime");
                stat_tile(ui, &stats.game_count.to_string(), "games");
                stat_tile(ui, &format_duration(last_7), "last 7 days");
                stat_tile(ui, &format_duration(last_30), "last 30 days");
                stat_tile(ui, &stats.finished_count.to_string(), "finished");
                stat_tile(ui, &never_played.to_string(), "never played");
            });

            ui.add_space(16.0);
            ui.strong("Most played");
            ui.add_space(4.0);
            if top.is_empty() {
                ui.weak("Nothing played yet. Launch a game from the library and it will show up here.");
            }
            let longest = top.first().map(|g| g.playtime_secs).unwrap_or(1).max(1);
            egui::Grid::new("top_played").num_columns(3).spacing([12.0, 6.0]).show(ui, |ui| {
                for (rank, game) in top.iter().enumerate() {
                    ui.weak(format!("{}.", rank + 1));
                    ui.label(&game.title);
                    let fraction = game.playtime_secs as f32 / longest as f32;
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .desired_width(320.0)
                            .text(format_duration(game.playtime_secs)),
                    );
                    ui.end_row();
                }
            });

            if !by_source.is_empty() {
                ui.add_space(16.0);
                ui.strong("Playtime by source");
                ui.add_space(4.0);
                let total = stats.total_playtime_secs.max(1);
                egui::Grid::new("by_source").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    for (source, secs) in by_source {
                        ui.label(source_name(source));
                        ui.add(
                            egui::ProgressBar::new(secs as f32 / total as f32)
                                .desired_width(320.0)
                                .text(format_duration(secs)),
                        );
                        ui.end_row();
                    }
                });
            }
        });
    }
}
