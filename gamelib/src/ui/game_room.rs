//! The Game Room: your own virtual gaming room, drawn from your library.
//!
//! A window looks out onto your background. The TV shows what you're
//! playing (or last played) with a Play button. The shelf holds your
//! favourites (or most played, or most recent), a poster of your most
//! played game hangs on the wall, the trophy cabinet fills up as you unlock
//! achievements, and friends who are online sit on the couch.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, pos2, vec2};
use gamelib::model::Game;

use super::library::CardClick;
use super::theme::{bold, lighten, mix, regular};
use super::{GameLibApp, Screen, paint};

const SHELF_KEY: &str = "room.shelf";
const WALL_KEY: &str = "room.wall";

/// What goes on the shelf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shelf {
    Favourites,
    MostPlayed,
    Recent,
}

impl Shelf {
    const ALL: [Shelf; 3] = [Shelf::Favourites, Shelf::MostPlayed, Shelf::Recent];
    fn key(self) -> &'static str {
        match self {
            Shelf::Favourites => "favourites",
            Shelf::MostPlayed => "most_played",
            Shelf::Recent => "recent",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Shelf::Favourites => "Favourites",
            Shelf::MostPlayed => "Most played",
            Shelf::Recent => "Recently played",
        }
    }
}

/// Wall colours: (name, colour). The first follows the background.
const WALLS: [(&str, u32); 6] = [
    ("Theme", 0),
    ("Midnight", 0x1a1f3d),
    ("Forest", 0x1d3328),
    ("Plum", 0x3a1f3d),
    ("Ember", 0x3d241a),
    ("Slate", 0x2a2f36),
];

fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

impl GameLibApp {
    pub fn game_room_screen(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let time = ui.input(|i| i.time);
        let shelf = match self.db.get_setting(SHELF_KEY).ok().flatten().as_deref() {
            Some("most_played") => Shelf::MostPlayed,
            Some("recent") => Shelf::Recent,
            _ => Shelf::Favourites,
        };
        let wall_choice = self
            .db
            .get_setting(WALL_KEY)
            .ok()
            .flatten()
            .and_then(|w| WALLS.iter().position(|(name, _)| *name == w))
            .unwrap_or(0);

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Game Room").font(bold(32.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::ComboBox::from_id_salt("room_wall")
                    .selected_text(format!("Wall: {}", WALLS[wall_choice].0))
                    .show_ui(ui, |ui| {
                        for (name, _) in WALLS {
                            if ui
                                .selectable_label(WALLS[wall_choice].0 == name, name)
                                .clicked()
                            {
                                self.check(self.db.set_setting(WALL_KEY, name));
                            }
                        }
                    });
                egui::ComboBox::from_id_salt("room_shelf")
                    .selected_text(format!("Shelf: {}", shelf.label()))
                    .show_ui(ui, |ui| {
                        for option in Shelf::ALL {
                            if ui
                                .selectable_label(shelf == option, option.label())
                                .clicked()
                            {
                                self.check(self.db.set_setting(SHELF_KEY, option.key()));
                            }
                        }
                    });
            });
        });
        ui.label(paint::muted(
            "Your room, built from your library. Click a game on the shelf to open it.",
            &theme,
        ));
        ui.add_space(10.0);

        // Pick what's shown.
        let games = &self.data.visible;
        let mut shelf_games: Vec<&Game> = match shelf {
            Shelf::Favourites => games.iter().filter(|g| g.favourite).collect(),
            Shelf::MostPlayed => games.iter().filter(|g| g.playtime_secs > 0).collect(),
            Shelf::Recent => games.iter().filter(|g| g.last_played.is_some()).collect(),
        };
        match shelf {
            Shelf::Recent => shelf_games.sort_by_key(|g| std::cmp::Reverse(g.last_played)),
            _ => shelf_games.sort_by_key(|g| std::cmp::Reverse(g.playtime_secs)),
        }
        if shelf_games.is_empty() {
            // Nothing in that pile yet: show something rather than nothing.
            shelf_games = games.iter().collect();
        }
        shelf_games.truncate(10);
        let poster = games
            .iter()
            .filter(|g| g.playtime_secs > 0)
            .max_by_key(|g| g.playtime_secs);
        let running = games.iter().find(|g| self.running.contains(&g.id));
        let on_tv = running.or_else(|| {
            games
                .iter()
                .filter(|g| g.last_played.is_some())
                .max_by_key(|g| g.last_played)
        });
        let trophies = self
            .data
            .achievements
            .iter()
            .filter(|a| a.unlocked())
            .count();
        let visitors: Vec<_> = self
            .data
            .friends
            .iter()
            .filter(|f| f.playing.is_some() || f.state.is_online())
            .take(4)
            .cloned()
            .collect();

        // The room fills the width, at a fixed shape.
        let width = ui.available_width();
        let height = (width * 0.56).min(ui.available_height().max(480.0));
        let (room, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        let at = |x: f32, y: f32| -> Pos2 {
            pos2(
                room.left() + x * room.width(),
                room.top() + y * room.height(),
            )
        };
        let r = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(at(x0, y0), at(x1, y1));
        let painter = ui.painter().with_clip_rect(room);
        let wall = match WALLS[wall_choice].1 {
            0 => mix(theme.bg, theme.accent, 0.12),
            hex => rgb(hex),
        };
        let floor_y = 0.72;

        // Wall, floor and skirting board.
        paint::gradient(
            &painter,
            r(0.0, 0.0, 1.0, floor_y),
            18.0,
            lighten(wall, 0.06),
            wall,
            vec2(0.0, 1.0),
        );
        let floor = r(0.0, floor_y, 1.0, 1.0);
        painter.rect_filled(floor, 0.0, mix(wall, Color32::from_rgb(60, 40, 28), 0.6));
        for i in 0..14 {
            let x = room.left() + i as f32 / 14.0 * room.width();
            painter.line_segment(
                [
                    pos2(x, floor.top()),
                    pos2(x - room.width() * 0.04, floor.bottom()),
                ],
                Stroke::new(1.0, Color32::from_black_alpha(40)),
            );
        }
        painter.rect_filled(
            r(0.0, floor_y - 0.012, 1.0, floor_y),
            0.0,
            lighten(wall, 0.18),
        );
        // A soft lamp glow that flickers a little.
        let flicker = 1.0 + 0.04 * (time as f32 * 2.3).sin();
        paint::glow(
            &painter,
            at(0.93, 0.18),
            room.width() * 0.22 * flicker,
            Color32::from_rgba_unmultiplied(255, 200, 120, 40),
        );

        // The window, showing your background (animated).
        let window = r(0.04, 0.08, 0.27, 0.5);
        painter.rect_filled(window.expand(8.0), 6.0, lighten(wall, 0.25));
        let sky = painter.with_clip_rect(window);
        paint::backdrop(&sky, window, &theme, &self.stars, self.anim_time);
        painter.line_segment(
            [window.center_top(), window.center_bottom()],
            Stroke::new(5.0, lighten(wall, 0.25)),
        );
        painter.line_segment(
            [window.left_center(), window.right_center()],
            Stroke::new(5.0, lighten(wall, 0.25)),
        );

        // The poster.
        let mut click: Option<CardClick> = None;
        if let Some(game) = poster {
            let frame = r(0.295, 0.1, 0.365, 0.38);
            painter.rect_filled(frame.expand(6.0), 3.0, Color32::from_rgb(30, 22, 18));
            paint::cover(ui, frame, game, 2.0, 0.0);
            let response = ui.interact(frame, ui.scope_id().with("room_poster"), Sense::click());
            if response
                .on_hover_text(format!("{} · your most played", game.title))
                .clicked()
            {
                click = Some(CardClick::Open(game.id));
            }
        }

        // The shelf: two rows of covers on planks.
        let shelf_area = r(0.63, 0.06, 0.97, 0.5);
        let per_row = 5;
        let gap = shelf_area.width() * 0.02;
        let cover_w = (shelf_area.width() - gap * (per_row as f32 - 1.0)) / per_row as f32;
        let cover_h = (cover_w * 1.45).min(shelf_area.height() / 2.0 - 14.0);
        for (i, game) in shelf_games.iter().enumerate() {
            let row = i / per_row;
            let col = i % per_row;
            let bottom = shelf_area.top() + (row + 1) as f32 * (shelf_area.height() / 2.0) - 8.0;
            let rect = Rect::from_min_size(
                pos2(
                    shelf_area.left() + col as f32 * (cover_w + gap),
                    bottom - cover_h,
                ),
                vec2(cover_w, cover_h),
            );
            let response = ui.interact(
                rect,
                ui.scope_id().with(("room_shelf", game.id)),
                Sense::click(),
            );
            let lift = if response.hovered() { 6.0 } else { 0.0 };
            paint::cover(ui, rect.translate(vec2(0.0, -lift)), game, 3.0, 0.0);
            if response
                .on_hover_text(&game.title)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                click = Some(CardClick::Open(game.id));
            }
        }
        for row in 0..2 {
            let y = shelf_area.top() + (row + 1) as f32 * (shelf_area.height() / 2.0) - 8.0;
            painter.rect_filled(
                Rect::from_min_max(
                    pos2(shelf_area.left() - 10.0, y),
                    pos2(shelf_area.right() + 10.0, y + 9.0),
                ),
                2.0,
                Color32::from_rgb(92, 62, 40),
            );
        }

        // The desk and TV.
        let desk = r(0.38, 0.6, 0.625, 0.66);
        let tv = r(0.405, 0.2, 0.6, 0.56);
        painter.rect_filled(tv.expand(10.0), 10.0, Color32::from_rgb(14, 14, 18));
        painter.rect_filled(tv, 4.0, Color32::from_rgb(4, 6, 12));
        if let Some(game) = on_tv {
            let art = Rect::from_center_size(
                tv.center() - vec2(tv.width() * 0.22, 0.0),
                vec2(tv.height() * 0.62, tv.height() * 0.9),
            );
            paint::cover(ui, art, game, 4.0, 0.0);
            let text_left = art.right() + 16.0;
            // Long titles stop at the edge of the screen.
            let painter = painter.with_clip_rect(tv.shrink(4.0));
            let label = if running.is_some() {
                "NOW PLAYING"
            } else {
                "LAST PLAYED"
            };
            painter.text(
                pos2(text_left, art.top() + 6.0),
                egui::Align2::LEFT_TOP,
                label,
                bold(12.0),
                theme.accent,
            );
            painter.text(
                pos2(text_left, art.top() + 26.0),
                egui::Align2::LEFT_TOP,
                &game.title,
                bold(18.0),
                Color32::WHITE,
            );
            let play = Rect::from_min_size(pos2(text_left, art.bottom() - 40.0), vec2(96.0, 34.0));
            if running.is_none() && game.installed {
                paint::gradient(
                    &painter,
                    play,
                    17.0,
                    theme.accent,
                    theme.accent_end,
                    vec2(1.0, 0.0),
                );
                painter.text(
                    play.center(),
                    egui::Align2::CENTER_CENTER,
                    "Play",
                    bold(14.0),
                    theme.on_accent,
                );
                let response = ui.interact(play, ui.scope_id().with("room_play"), Sense::click());
                if response
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    click = Some(CardClick::Play(game.id));
                }
            } else if running.is_some() {
                painter.text(
                    play.left_center(),
                    egui::Align2::LEFT_CENTER,
                    "Running…",
                    regular(14.0),
                    theme.success,
                );
            }
        } else {
            painter.text(
                tv.center(),
                egui::Align2::CENTER_CENTER,
                "Play something!",
                bold(18.0),
                theme.muted,
            );
        }
        painter.rect_filled(
            r(0.485, 0.56, 0.52, 0.6),
            0.0,
            Color32::from_rgb(20, 20, 26),
        );
        painter.rect_filled(desk, 6.0, Color32::from_rgb(70, 48, 34));
        painter.rect_filled(
            r(0.395, 0.66, 0.405, 0.86),
            0.0,
            Color32::from_rgb(50, 34, 24),
        );
        painter.rect_filled(r(0.6, 0.66, 0.61, 0.86), 0.0, Color32::from_rgb(50, 34, 24));

        // The trophy cabinet.
        let cabinet = r(0.73, 0.55, 0.95, 0.9);
        painter.rect_filled(cabinet, 6.0, Color32::from_rgb(60, 42, 30));
        painter.rect_filled(
            cabinet.shrink(8.0),
            4.0,
            Color32::from_rgba_unmultiplied(160, 200, 255, 30),
        );
        let slots = 8;
        for i in 0..slots {
            let row = i / 4;
            let col = i % 4;
            let centre = pos2(
                cabinet.left() + (col as f32 + 0.5) / 4.0 * cabinet.width(),
                cabinet.top() + (row as f32 + 0.62) / 2.0 * cabinet.height(),
            );
            let won = i < trophies;
            let gold = if won {
                Color32::from_rgb(255, 200, 70)
            } else {
                Color32::from_white_alpha(25)
            };
            // A cup on a stand.
            painter.add(egui::Shape::convex_polygon(
                vec![
                    centre + vec2(-10.0, -18.0),
                    centre + vec2(10.0, -18.0),
                    centre + vec2(5.0, -2.0),
                    centre + vec2(-5.0, -2.0),
                ],
                gold,
                Stroke::NONE,
            ));
            painter.rect_filled(
                Rect::from_center_size(centre + vec2(0.0, 2.0), vec2(4.0, 8.0)),
                0.0,
                gold,
            );
            painter.rect_filled(
                Rect::from_center_size(centre + vec2(0.0, 8.0), vec2(16.0, 4.0)),
                1.0,
                gold,
            );
        }
        painter.text(
            cabinet.center_bottom() + vec2(0.0, 14.0),
            egui::Align2::CENTER_TOP,
            format!("{trophies} of {} trophies", self.data.achievements.len()),
            regular(12.5),
            theme.muted,
        );

        // The couch, with friends who are online.
        let couch = r(0.04, 0.74, 0.3, 0.93);
        painter.rect_filled(
            couch,
            14.0,
            mix(theme.accent, Color32::from_rgb(40, 30, 60), 0.6),
        );
        painter.rect_filled(
            r(0.04, 0.68, 0.3, 0.78),
            14.0,
            mix(theme.accent_end, Color32::from_rgb(40, 30, 60), 0.55),
        );
        for (i, friend) in visitors.iter().enumerate() {
            let centre = at(0.075 + i as f32 * 0.06, 0.71);
            let dot = if friend.playing.is_some() {
                theme.success
            } else {
                Color32::from_rgb(0x4f, 0x8d, 0xfd)
            };
            paint::friend_avatar(ui, centre, 18.0, &friend.name, dot, &theme);
            let tip = match &friend.playing {
                Some(game) => format!("{} is playing {game}", friend.name),
                None => format!("{} is online", friend.name),
            };
            ui.interact(
                Rect::from_center_size(centre, vec2(36.0, 36.0)),
                ui.scope_id().with(("visitor", i)),
                Sense::hover(),
            )
            .on_hover_text(tip);
        }
        if visitors.is_empty() {
            painter.text(
                couch.center(),
                egui::Align2::CENTER_CENTER,
                "Friends who are online sit here",
                regular(12.5),
                Color32::from_white_alpha(170),
            );
        }

        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));
        match click {
            Some(CardClick::Open(id)) => self.go(Screen::Details(id)),
            Some(CardClick::Play(id)) => self.play(id),
            None => {}
        }
    }
}
