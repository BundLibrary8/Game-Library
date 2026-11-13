//! GameLib Wrapped: your year in games.

use eframe::egui::{self, Color32, Rect, Sense, vec2};
use gamelib::now;
use gamelib::patterns::{self, MONTHS, WEEKDAYS};

use super::theme::{bold, regular};
use super::{GameLibApp, Screen, local_offset, paint};
use crate::text::short_playtime;

fn hour_label(hour: usize) -> String {
    match hour {
        0 => "midnight".into(),
        12 => "noon".into(),
        h if h < 12 => format!("{h}am"),
        h => format!("{}pm", h - 12),
    }
}

impl GameLibApp {
    pub fn wrapped_screen(&mut self, ui: &mut egui::Ui, year: i32) {
        let theme = self.theme;
        let offset = local_offset();
        let finished = self.db.finished_dates().unwrap_or_default();
        let w = patterns::wrapped(
            year,
            &self.data.visible,
            &self.data.sessions,
            &finished,
            now(),
            offset,
        );

        // Years you can look back on: any with a session, and this one.
        let this_year = patterns::year_of(now(), offset);
        let mut years: Vec<i32> = self
            .data
            .sessions
            .iter()
            .map(|s| patterns::year_of(s.1, offset))
            .chain([this_year])
            .collect();
        years.sort_unstable();
        years.dedup();

        let mut go = None;
        ui.horizontal(|ui| {
            if ui.button("← Stats").clicked() {
                go = Some(Screen::Stats);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for y in years.iter().rev() {
                    if paint::chip(ui, &theme, &y.to_string(), *y == year).clicked() {
                        go = Some(Screen::Wrapped(*y));
                    }
                }
            });
        });
        ui.add_space(10.0);

        // The big banner.
        let width = ui.available_width();
        let (hero, _) = ui.allocate_exact_size(vec2(width, 200.0), Sense::hover());
        let painter = ui.painter();
        paint::gradient(
            painter,
            hero,
            22.0,
            theme.accent,
            theme.accent_end,
            vec2(1.0, 1.0),
        );
        painter.rect_filled(hero, 22.0, Color32::from_black_alpha(60));
        for i in 0..18 {
            // A scatter of stars across the banner.
            let x =
                hero.left() + ((i * 97 + year as usize * 13) % 1000) as f32 / 1000.0 * hero.width();
            let y = hero.top() + ((i * 53 + 17) % 100) as f32 / 100.0 * hero.height();
            paint::star4(
                painter,
                egui::pos2(x, y),
                2.0 + (i % 3) as f32,
                Color32::from_white_alpha(90),
            );
        }
        painter.text(
            hero.left_top() + vec2(34.0, 30.0),
            egui::Align2::LEFT_TOP,
            "GAMELIB WRAPPED",
            bold(14.0),
            Color32::from_white_alpha(220),
        );
        painter.text(
            hero.left_top() + vec2(34.0, 56.0),
            egui::Align2::LEFT_TOP,
            format!("Your {year} in games"),
            bold(40.0),
            Color32::WHITE,
        );
        let headline = if w.patterns.sessions == 0 {
            "Nothing played yet this year. Launch a game through GameLib and it'll count."
                .to_string()
        } else {
            format!(
                "{} across {} {}, on {} days. You're a {}.",
                short_playtime(w.patterns.total_secs),
                w.games_played,
                if w.games_played == 1 { "game" } else { "games" },
                w.patterns.days_played,
                w.persona
            )
        };
        painter.text(
            hero.left_top() + vec2(34.0, 120.0),
            egui::Align2::LEFT_TOP,
            headline,
            regular(18.0),
            Color32::WHITE,
        );
        ui.add_space(18.0);

        paint::stat_row(
            ui,
            &theme,
            &[
                (
                    "Hours played",
                    format!("{:.0}", w.patterns.total_secs as f64 / 3600.0),
                ),
                ("Sessions", w.patterns.sessions.to_string()),
                (
                    "Longest streak",
                    format!("{} days", w.patterns.longest_streak),
                ),
                ("Games added", w.games_added.to_string()),
                ("Games finished", w.games_finished.to_string()),
            ],
        );
        ui.add_space(18.0);

        let gap = 18.0;
        let half = ((ui.available_width() - gap) / 2.0).floor();
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(half - 38.0);
                ui.set_min_height(290.0);
                ui.label(paint::strong("Your top games", 17.0));
                ui.add_space(8.0);
                if w.top_games.is_empty() {
                    ui.label(paint::muted("Nothing played this year yet.", &theme));
                }
                for (rank, (id, secs)) in w.top_games.iter().enumerate() {
                    let Some(game) = self.data.game(*id).cloned() else {
                        continue;
                    };
                    let (row, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
                    let painter = ui.painter();
                    painter.text(
                        row.left_center() + vec2(4.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        format!("{}", rank + 1),
                        bold(22.0),
                        theme.accent,
                    );
                    let cover = Rect::from_min_size(row.min + vec2(34.0, 3.0), vec2(27.0, 40.0));
                    paint::cover(ui, cover, &game, 4.0, 0.0);
                    let painter = ui.painter();
                    painter.text(
                        egui::pos2(cover.right() + 12.0, row.center().y),
                        egui::Align2::LEFT_CENTER,
                        &game.title,
                        bold(15.5),
                        theme.text,
                    );
                    painter.text(
                        row.right_center() - vec2(6.0, 0.0),
                        egui::Align2::RIGHT_CENTER,
                        short_playtime(*secs),
                        regular(14.0),
                        theme.muted,
                    );
                    if response
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        go = Some(Screen::Details(*id));
                    }
                }
            });
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(half - 38.0);
                ui.set_min_height(290.0);
                ui.label(paint::strong("How you played", 17.0));
                ui.add_space(8.0);
                let mut fact = |caption: &str, value: String| {
                    ui.label(paint::caps(caption, 11.0, theme.muted));
                    ui.label(paint::strong(value, 18.0));
                    ui.add_space(4.0);
                };
                if let Some((genre, secs)) = &w.top_genre {
                    fact("Top genre", format!("{genre} · {}", short_playtime(*secs)));
                }
                if let Some((month, secs)) = w.busiest_month {
                    fact(
                        "Busiest month",
                        format!("{} · {}", MONTHS[month], short_playtime(secs)),
                    );
                }
                if let Some((day, _)) = w.busiest_weekday {
                    fact("Favourite day", WEEKDAYS[day].to_string());
                }
                if let Some(hour) = w.peak_hour {
                    fact("Peak hour", format!("Around {}", hour_label(hour)));
                }
                if let Some((id, secs, _)) = w.patterns.longest_session {
                    fact(
                        "Longest session",
                        format!("{} · {}", short_playtime(secs), self.title_of(id)),
                    );
                }
                if w.patterns.sessions == 0 {
                    ui.label(paint::muted("Play something and this fills in.", &theme));
                }
            });
        });

        if let Some(screen) = go {
            self.go(screen);
        }
    }
}
