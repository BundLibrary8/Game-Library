//! The Discover screen: Backlog Intelligence. How much of your library
//! you've played, what fits the time you have tonight, what to finish
//! next, what you've neglected, and games worth a look.

use eframe::egui::{self, Color32, Rect, Sense, Stroke, vec2};
use gamelib::backlog::{self, Goal, format_minutes};
use gamelib::model::Game;
use gamelib::now;

use super::library::{CardClick, game_grid};
use super::theme::{Theme, bold, regular};
use super::{GameLibApp, Screen, paint};

/// The free-time choices, in minutes.
const FREE_TIMES: [(u32, &str); 5] = [
    (30, "30 min"),
    (60, "1 hour"),
    (120, "2 hours"),
    (180, "3 hours"),
    (300, "All evening"),
];

/// One game as a row: a small cover, the title, a line of text and a play
/// button. `progress` draws a bar under the text.
pub fn game_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    game: &Game,
    text: &str,
    progress: Option<f32>,
    running: bool,
) -> Option<CardClick> {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 72.0), Sense::click());
    if !ui.is_rect_visible(rect) {
        return None;
    }
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, 12.0, theme.panel_alt.gamma_multiply(0.6));
    }
    let cover = Rect::from_min_size(rect.min + vec2(8.0, 6.0), vec2(40.0, 60.0));
    paint::cover(ui, cover, game, 6.0, 0.0);

    let play = Rect::from_center_size(
        egui::pos2(rect.right() - 46.0, rect.center().y),
        vec2(72.0, 32.0),
    );
    let left = cover.right() + 14.0;
    let text_clip = ui.painter().with_clip_rect(Rect::from_min_max(
        rect.min,
        egui::pos2(play.left() - 10.0, rect.bottom()),
    ));
    text_clip.text(
        egui::pos2(left, rect.top() + 12.0),
        egui::Align2::LEFT_TOP,
        &game.title,
        bold(15.5),
        theme.text,
    );
    text_clip.text(
        egui::pos2(left, rect.top() + 34.0),
        egui::Align2::LEFT_TOP,
        text,
        regular(13.0),
        theme.muted,
    );
    if let Some(fraction) = progress {
        let track = Rect::from_min_size(
            egui::pos2(left, rect.bottom() - 14.0),
            vec2((play.left() - 24.0 - left).max(40.0), 5.0),
        );
        ui.painter().rect_filled(track, 3.0, theme.panel_alt);
        let mut filled = track;
        filled.set_width((track.width() * fraction.clamp(0.0, 1.0)).max(5.0));
        ui.painter().rect_filled(filled, 3.0, theme.accent);
    }

    let can_play = game.installed && !running;
    let painter = ui.painter();
    if can_play {
        paint::gradient(
            painter,
            play,
            16.0,
            theme.accent,
            theme.accent_end,
            vec2(1.0, 0.0),
        );
    } else {
        painter.rect_filled(play, 16.0, theme.panel_alt);
    }
    painter.text(
        play.center(),
        egui::Align2::CENTER_CENTER,
        if running { "Running" } else { "Play" },
        bold(13.0),
        if can_play {
            theme.on_accent
        } else {
            theme.muted
        },
    );
    let over_play = ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|pos| play.contains(pos));
    let clicked = response.clicked();
    response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if clicked {
        return Some(if over_play && can_play {
            CardClick::Play(game.id)
        } else {
            CardClick::Open(game.id)
        });
    }
    None
}

impl GameLibApp {
    pub fn discover_screen(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Discover").font(bold(32.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.stardust_pill(ui);
            });
        });
        ui.label(paint::muted(
            "Backlog Intelligence: what you've played, what's waiting, and what fits the time you have.",
            &theme,
        ));
        ui.add_space(12.0);

        if self.data.visible.is_empty() {
            ui.label(paint::muted(
                "Scan for games first, and this page fills up with suggestions.",
                &theme,
            ));
            return;
        }

        let games = &self.data.visible;
        let extras = &self.data.extras;
        let summary = backlog::summary(games, extras);
        let mut click: Option<CardClick> = None;

        // The summary.
        paint::panel(ui, theme.panel_frame(), |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::caps("Your backlog", 12.0, theme.accent));
            ui.add_space(4.0);
            ui.label(paint::strong(summary.sentence(), 22.0));
            ui.add_space(10.0);
            let played_share = if summary.owned > 0 {
                summary.played as f32 / summary.owned as f32
            } else {
                0.0
            };
            paint::stat_row(
                ui,
                &theme,
                &[
                    ("Played", format!("{:.0}%", played_share * 100.0)),
                    ("Completed", summary.completed.to_string()),
                    ("In progress", summary.playing.to_string()),
                    ("Never launched", summary.never_launched.to_string()),
                    ("Time to clear", format!("~{} h", summary.hours_left)),
                ],
            );
            let habits = self.data.habits;
            if habits.sessions > 0 {
                ui.add_space(8.0);
                let hour = habits
                    .favourite_hour
                    .map(|h| format!(" You usually start around {h}:00 (UTC)."))
                    .unwrap_or_default();
                ui.label(paint::muted(
                    format!(
                        "Your sessions usually last about {}, and you've played {:.1} times a week lately.{hour}",
                        format_minutes(habits.typical_session_minutes.max(1)),
                        habits.sessions_per_week
                    ),
                    &theme,
                ));
            }
        });
        ui.add_space(16.0);

        // What to play tonight.
        paint::panel(ui, theme.panel_frame(), |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("What should I play tonight?", 19.0));
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(paint::muted("I have", &theme));
                for (minutes, label) in FREE_TIMES {
                    if paint::chip(ui, &theme, label, self.discover_free == minutes).clicked() {
                        self.discover_free = minutes;
                    }
                }
                ui.add_space(14.0);
                for goal in Goal::ALL {
                    if paint::chip(ui, &theme, goal.label(), self.discover_goal == goal).clicked() {
                        self.discover_goal = goal;
                    }
                }
            });
            ui.add_space(8.0);
            let picks = backlog::tonight(
                &self.data.visible,
                &self.data.extras,
                &self.data.taste,
                &self.data.habits,
                self.discover_free,
                self.discover_goal,
                4,
            );
            if picks.is_empty() {
                ui.label(paint::muted(
                    "Nothing fits. Try another choice, or mark games as Playing or Backlog on their pages.",
                    &theme,
                ));
            }
            for pick in picks {
                if let Some(game) = self.data.game(pick.game_id).cloned() {
                    let running = self.running.contains(&game.id);
                    if let Some(c) = game_row(ui, &theme, &game, &pick.reason, None, running) {
                        click = Some(c);
                    }
                }
            }
            ui.label(
                paint::muted(
                    "Lengths come from the \"Hours to beat\" on each game's page; without one, GameLib guesses from the genre.",
                    &theme,
                )
                .size(12.5),
            );
        });
        ui.add_space(16.0);

        // Complete next, and neglected, side by side.
        let gap = 18.0;
        let half = ((ui.available_width() - gap) / 2.0).floor();
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(half - 38.0);
                ui.set_min_height(300.0);
                ui.label(paint::strong("Complete next", 17.0));
                ui.label(paint::muted("Started games, closest to the end first.", &theme));
                ui.add_space(4.0);
                let next = backlog::complete_next(&self.data.visible, &self.data.extras, 5);
                if next.is_empty() {
                    ui.label(paint::muted(
                        "Nothing on the go. Games you play for an hour or mark as Playing show up here.",
                        &theme,
                    ));
                }
                for (id, left) in next {
                    let Some(game) = self.data.game(id).cloned() else {
                        continue;
                    };
                    let e = self.data.extras.get(&id).cloned().unwrap_or_default();
                    let progress = backlog::progress(&game, &e);
                    let text = format!(
                        "About {} left · {:.0}% through",
                        format_minutes(left),
                        progress * 100.0
                    );
                    let running = self.running.contains(&id);
                    if let Some(c) = game_row(ui, &theme, &game, &text, Some(progress), running) {
                        click = Some(c);
                    }
                }
            });
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(half - 38.0);
                ui.set_min_height(300.0);
                ui.horizontal(|ui| {
                    ui.label(paint::strong("Neglected", 17.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("Surprise me")
                            .on_hover_text("Open a random game you've never launched")
                            .clicked()
                        {
                            let untouched: Vec<i64> = self
                                .data
                                .visible
                                .iter()
                                .filter(|g| g.installed && g.playtime_secs == 0)
                                .map(|g| g.id)
                                .collect();
                            if !untouched.is_empty() {
                                let pick = (now() as usize ^ (ui.input(|i| i.time) * 1000.0) as usize)
                                    % untouched.len();
                                click = Some(CardClick::Open(untouched[pick]));
                            }
                        }
                    });
                });
                ui.label(paint::muted("Bought and forgotten, or left for a month.", &theme));
                ui.add_space(4.0);
                let found = backlog::neglected(&self.data.visible, &self.data.extras, now(), 5);
                if found.is_empty() {
                    ui.label(paint::muted("Nothing neglected. Nice work.", &theme));
                }
                for (id, why) in found {
                    let Some(game) = self.data.game(id).cloned() else {
                        continue;
                    };
                    let running = self.running.contains(&id);
                    if let Some(c) = game_row(ui, &theme, &game, &why, None, running) {
                        click = Some(c);
                    }
                }
            });
        });

        // Recommended for you.
        if !self.data.recommendations.is_empty() {
            ui.add_space(22.0);
            ui.label(paint::strong("Recommended for you", 19.0));
            ui.label(paint::muted(
                "Picked from the genres, tags and ratings of the games you play most.",
                &theme,
            ));
            ui.add_space(4.0);
            let picks: Vec<Game> = self
                .data
                .recommendations
                .iter()
                .filter_map(|pick| self.data.game(pick.game_id).cloned())
                .collect();
            if let Some(c) = game_grid(ui, &theme, &picks, &self.running, 160.0, Some(1)) {
                click = Some(c);
            }
        }

        // Short games: unplayed and quick to beat.
        let mut short: Vec<(u32, Game)> = self
            .data
            .visible
            .iter()
            .filter(|g| g.installed && g.playtime_secs == 0)
            .map(|g| {
                let e = self.data.extras.get(&g.id).cloned().unwrap_or_default();
                (backlog::length(g, &e).minutes, g.clone())
            })
            .filter(|(minutes, _)| *minutes <= 8 * 60)
            .collect();
        short.sort_by_key(|(minutes, g)| (*minutes, g.id));
        if !short.is_empty() {
            ui.add_space(22.0);
            ui.label(paint::strong("Short and sweet", 19.0));
            ui.label(paint::muted(
                "Unplayed games you could beat in a weekend.",
                &theme,
            ));
            ui.add_space(4.0);
            let games: Vec<Game> = short.into_iter().map(|(_, g)| g).collect();
            if let Some(c) = game_grid(ui, &theme, &games, &self.running, 160.0, Some(1)) {
                click = Some(c);
            }
        }

        // Your genres: a row per genre you own the most of.
        let mut genres: std::collections::HashMap<String, Vec<Game>> = Default::default();
        for game in self.data.visible.iter().filter(|g| g.playtime_secs < 3600) {
            for genre in &game.genres {
                genres.entry(genre.clone()).or_default().push(game.clone());
            }
        }
        let mut genres: Vec<(String, Vec<Game>)> =
            genres.into_iter().filter(|(_, g)| g.len() >= 2).collect();
        genres.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
        for (genre, games) in genres.into_iter().take(3) {
            ui.add_space(22.0);
            ui.horizontal(|ui| {
                ui.label(paint::strong(format!("{genre} you haven't played"), 19.0));
                ui.label(paint::muted(format!("{}", games.len()), &theme));
            });
            ui.add_space(4.0);
            if let Some(c) = game_grid(ui, &theme, &games, &self.running, 160.0, Some(1)) {
                click = Some(c);
            }
        }

        // A faint divider line at the bottom so the page doesn't just stop.
        ui.add_space(10.0);
        let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
        ui.painter().line_segment(
            [line.left_center(), line.right_center()],
            Stroke::new(1.0, Color32::from_white_alpha(18)),
        );

        match click {
            Some(CardClick::Open(id)) => self.go(Screen::Details(id)),
            Some(CardClick::Play(id)) => self.play(id),
            None => {}
        }
    }
}
