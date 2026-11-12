//! The home screen: jump back into recent games, and see what's new.

use eframe::egui::{self, Color32, Rect, Sense, Stroke, vec2};
use gamelib::model::Game;
use gamelib::now;

use super::library::{CardClick, game_grid};
use super::theme::{bold, regular};
use super::{GameLibApp, Screen, paint};
use crate::text::{format_last_played, played_label};

impl GameLibApp {
    pub fn home_screen(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        ui.label(egui::RichText::new("Home").font(bold(32.0)));
        ui.add_space(8.0);

        if self.data.visible.is_empty() {
            ui.add_space(50.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("Welcome to GameLib").font(bold(24.0)));
                ui.label(paint::muted(
                    "Use Scan to find your Steam, Epic and GOG games, or Add game to add one by hand.",
                    &theme,
                ));
            });
            return;
        }

        // Continue playing: the three most recently played games.
        let mut recent: Vec<&Game> = self
            .data
            .visible
            .iter()
            .filter(|g| g.last_played.is_some())
            .collect();
        recent.sort_by_key(|g| std::cmp::Reverse(g.last_played));
        recent.truncate(3);

        let mut click = None;
        ui.label(paint::strong("Continue playing", 19.0));
        if recent.is_empty() {
            ui.label(paint::muted("Games you play will show up here.", &theme));
        } else {
            let gap = 18.0;
            let width = ((ui.available_width() - gap * 2.0) / 3.0).floor();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for game in recent {
                    let running = self.running.contains(&game.id);
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(width, 170.0), Sense::click());
                    let painter = ui.painter();
                    let (bright, dark) = paint::title_colours(&game.title);
                    paint::gradient(painter, rect, 16.0, bright, dark, vec2(1.0, 0.7));
                    painter.rect_filled(rect, 16.0, Color32::from_black_alpha(70));
                    let border = if response.hovered() {
                        theme.accent
                    } else {
                        Color32::from_white_alpha(28)
                    };
                    painter.rect_stroke(
                        rect,
                        16.0,
                        Stroke::new(1.5, border),
                        egui::StrokeKind::Inside,
                    );

                    // A small cover on the left of the tile.
                    let cover = Rect::from_min_size(rect.min + vec2(16.0, 16.0), vec2(92.0, 138.0));
                    paint::cover(ui, cover, game, 10.0, 0.0);
                    let text_left = cover.right() + 16.0;
                    let clip = ui.painter().with_clip_rect(rect.shrink(8.0));
                    clip.text(
                        egui::pos2(text_left, rect.top() + 22.0),
                        egui::Align2::LEFT_TOP,
                        &game.title,
                        bold(19.0),
                        Color32::WHITE,
                    );
                    clip.text(
                        egui::pos2(text_left, rect.top() + 50.0),
                        egui::Align2::LEFT_TOP,
                        format!(
                            "{} · {}",
                            format_last_played(game.last_played, now()),
                            played_label(game.playtime_secs)
                        ),
                        regular(13.5),
                        Color32::from_white_alpha(200),
                    );

                    // The play button.
                    let play = Rect::from_min_size(
                        egui::pos2(text_left, rect.bottom() - 62.0),
                        vec2(104.0, 40.0),
                    );
                    let label = if running { "Running…" } else { "▶  Play" };
                    paint::gradient(
                        ui.painter(),
                        play,
                        10.0,
                        theme.accent,
                        theme.accent_end,
                        vec2(1.0, 0.0),
                    );
                    ui.painter().text(
                        play.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        bold(14.5),
                        theme.on_accent,
                    );
                    let over_play = ui
                        .input(|i| i.pointer.hover_pos())
                        .is_some_and(|pos| play.contains(pos));
                    if response.clicked() {
                        click = Some(if over_play && game.installed && !running {
                            CardClick::Play(game.id)
                        } else {
                            CardClick::Open(game.id)
                        });
                    }
                    response.on_hover_cursor(egui::CursorIcon::PointingHand);
                }
            });
        }

        ui.add_space(26.0);
        ui.label(paint::strong("Recently added", 19.0));
        ui.add_space(4.0);
        let mut newest: Vec<Game> = self.data.visible.clone();
        newest.sort_by_key(|g| std::cmp::Reverse((g.added_at, g.id)));
        if let Some(c) = game_grid(ui, &theme, &newest, &self.running, 170.0, Some(1)) {
            click = Some(c);
        }

        match click {
            Some(CardClick::Open(id)) => self.go(Screen::Details(id)),
            Some(CardClick::Play(id)) => self.play(id),
            None => {}
        }
    }
}
