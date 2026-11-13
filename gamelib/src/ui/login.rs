//! The log-in screen GameLib opens on: a wall of your own covers drifting
//! behind the window's background, the logo and what GameLib does, and a
//! glass card to log in or create an account.

use std::f32::consts::TAU;

use eframe::egui::{self, Color32, Rect, Sense, Stroke, pos2, vec2};
use gamelib::model::Game;

use super::account::FormMode;
use super::paint;
use super::theme::{bold, regular, semibold};
use super::{GameLibApp, notify};

/// The setting that stops GameLib opening on this screen.
pub const SKIP_KEY: &str = "ui.skip_login";

/// What GameLib does, beside the card on a wide window.
const FEATURES: [(&str, &str); 3] = [
    (
        "Every launcher, one library",
        "Steam, Epic, GOG, Battle.net, Riot, EA and your own folders.",
    ),
    (
        "Your friends' games",
        "Add friends by username and see their libraries and stats.",
    ),
    (
        "Every hour counted",
        "Playtime, goals, achievements and your year in games.",
    ),
];

/// Cover size in the wall behind the card.
const COVER: egui::Vec2 = egui::Vec2::new(150.0, 225.0);
const COVER_GAP: f32 = 18.0;

/// Fast at first, gently settling at the end.
fn ease_out(x: f32) -> f32 {
    1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3)
}

/// Which game sits in a slot of the cover wall. `step` is how many slots
/// the column has scrolled, so a cover keeps its game as it moves.
fn wall_slot(column: usize, row: usize, step: i64, count: usize) -> usize {
    (column as i64 * 7 + (row as i64 - step) * 3).rem_euclid(count as i64) as usize
}

impl GameLibApp {
    /// The full-window log-in screen.
    pub fn login_screen(&mut self, ui: &mut egui::Ui, time: f64) {
        let ctx = ui.ctx().clone();
        let theme = self.theme;
        let screen = ui.max_rect();
        // Its entrance starts when the loading animation has gone.
        let shown_at = match (self.splash.is_none(), self.cloud.login_shown_at) {
            (true, None) => {
                self.cloud.login_shown_at = Some(time);
                time
            }
            (_, Some(at)) => at,
            (false, None) => time,
        };
        let enter = ease_out(((time - shown_at) / 0.8) as f32);

        // The background, your covers drifting over it, then a shade that is
        // darkest behind the text so it stays readable.
        paint::backdrop(ui.painter(), screen, &theme, &self.stars, Some(time));
        if let Some((path, size)) = &self.data.custom_background
            && paint::picture_fill(&ctx, ui.painter(), path, *size, screen, Color32::WHITE)
        {
            ui.painter()
                .rect_filled(screen, 0.0, Color32::from_black_alpha(90));
        }
        let covers: Vec<&Game> = self
            .data
            .visible
            .iter()
            .filter(|g| g.cover_path.is_some())
            .take(48)
            .collect();
        if covers.len() >= 4 {
            cover_wall(ui, screen, &covers, time, 0.32 * enter);
        }
        let [r, g, b, _] = theme.bg.to_array();
        paint::gradient(
            ui.painter(),
            screen,
            0.0,
            Color32::from_rgba_unmultiplied(r, g, b, 235),
            Color32::from_rgba_unmultiplied(r, g, b, 120),
            vec2(1.0, 0.25),
        );

        let wide = screen.width() >= 1050.0;
        if wide {
            self.login_hero(ui, screen, time, enter);
        }

        // The card, rising into place.
        let rise = 24.0 * (1.0 - enter);
        let (anchor, offset) = if wide {
            (
                egui::Align2::RIGHT_CENTER,
                vec2(-(screen.width() * 0.09).max(48.0), rise),
            )
        } else {
            (egui::Align2::CENTER_CENTER, vec2(0.0, rise))
        };
        let [ar, ag, ab, _] = theme.accent.to_array();
        egui::Area::new(egui::Id::unique("login_card"))
            .anchor(anchor, offset)
            .order(egui::Order::Middle)
            .show(&ctx, |ui| {
                ui.set_opacity(enter);
                let rect = ui.max_rect();
                // A soft accent glow behind the card.
                paint::glow(
                    ui.painter(),
                    rect.center() + vec2(0.0, 220.0),
                    300.0,
                    Color32::from_rgba_unmultiplied(ar, ag, ab, 36),
                );
                let [pr, pg, pb, _] = theme.opaque_panel().to_array();
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(pr, pg, pb, 238))
                    .stroke(Stroke::new(1.0, theme.border))
                    .corner_radius(22.0)
                    .inner_margin(34.0)
                    .shadow(egui::Shadow {
                        offset: [0, 18],
                        blur: 48,
                        spread: 0,
                        color: Color32::from_black_alpha(110),
                    })
                    .show(ui, |ui| {
                        ui.set_width(372.0);
                        self.login_card(ui, time, wide);
                    });
            });

        notify::draw_toasts(&ctx, &self.theme, &mut self.toasts, time);
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }

    /// The logo, the name and what GameLib does, left of the card.
    fn login_hero(&self, ui: &egui::Ui, screen: Rect, time: f64, enter: f32) {
        let theme = self.theme;
        let painter = ui.painter();
        let left = screen.left() + (screen.width() * 0.08).max(56.0);
        let top = screen.center().y - 190.0 + 20.0 * (1.0 - enter);
        let fade = |c: Color32| c.gamma_multiply(enter);

        // The logo star with its glow and orbiting sparks.
        let logo = pos2(left + 38.0, top + 38.0);
        let breathe = 0.85 + 0.15 * (time as f32 * 2.2).sin();
        let [r, g, b, _] = theme.accent.to_array();
        paint::glow(
            painter,
            logo,
            90.0 * breathe,
            Color32::from_rgba_unmultiplied(r, g, b, (80.0 * enter) as u8),
        );
        for i in 0..6 {
            let angle = time as f32 * 0.9 + TAU * i as f32 / 6.0;
            let at = logo + vec2(angle.cos(), angle.sin()) * 52.0;
            let colour = if i % 2 == 0 {
                theme.accent
            } else {
                theme.accent_end
            };
            paint::star4(painter, at, 4.0, fade(colour));
        }
        paint::star4(painter, logo, 30.0, fade(theme.accent));
        paint::star4(
            painter,
            logo,
            15.0,
            fade(Color32::WHITE.gamma_multiply(0.9)),
        );

        painter.text(
            pos2(left, top + 104.0),
            egui::Align2::LEFT_TOP,
            "GameLib",
            bold(60.0),
            fade(theme.text),
        );
        painter.text(
            pos2(left + 2.0, top + 176.0),
            egui::Align2::LEFT_TOP,
            theme.tagline(),
            regular(17.0),
            fade(theme.accent),
        );

        // Each feature fades in a little after the one before.
        for (i, (title, detail)) in FEATURES.iter().enumerate() {
            let y = top + 236.0 + i as f32 * 62.0;
            let own = ease_out(enter * 1.6 - i as f32 * 0.25);
            let x = left + 14.0 * (1.0 - own);
            let dot = pos2(x + 14.0, y + 14.0);
            painter.circle_filled(dot, 14.0, fade(theme.panel_alt).gamma_multiply(own));
            paint::star4(painter, dot, 7.0, theme.accent.gamma_multiply(own * enter));
            painter.text(
                pos2(x + 40.0, y + 2.0),
                egui::Align2::LEFT_TOP,
                *title,
                semibold(16.0),
                theme.text.gamma_multiply(own * enter),
            );
            painter.text(
                pos2(x + 40.0, y + 24.0),
                egui::Align2::LEFT_TOP,
                *detail,
                regular(13.5),
                theme.muted.gamma_multiply(own * enter),
            );
        }
    }

    /// What's inside the card: checking the saved login, or the form.
    fn login_card(&mut self, ui: &mut egui::Ui, time: f64, wide: bool) {
        let theme = self.theme;
        if !wide {
            // No hero beside it, so the card carries the logo.
            ui.vertical_centered(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
                let [r, g, b, _] = theme.accent.to_array();
                paint::glow(
                    ui.painter(),
                    rect.center(),
                    56.0,
                    Color32::from_rgba_unmultiplied(r, g, b, 70),
                );
                paint::star4(ui.painter(), rect.center(), 24.0, theme.accent);
                paint::star4(ui.painter(), rect.center(), 12.0, Color32::WHITE);
                ui.label(egui::RichText::new("GameLib").font(bold(30.0)));
            });
            ui.add_space(10.0);
        }

        if !self.cloud.checked {
            // A saved login is being checked: an orbit of stars while it is.
            ui.vertical_centered(|ui| {
                ui.add_space(30.0);
                let (rect, _) = ui.allocate_exact_size(vec2(90.0, 90.0), Sense::hover());
                for i in 0..8 {
                    let angle = time as f32 * 2.4 + TAU * i as f32 / 8.0;
                    let at = rect.center() + vec2(angle.cos(), angle.sin()) * 30.0;
                    let fade = 0.25 + 0.75 * (i as f32 / 8.0);
                    paint::star4(ui.painter(), at, 6.0, theme.accent.gamma_multiply(fade));
                }
                ui.add_space(14.0);
                ui.label(egui::RichText::new("Signing you in…").font(semibold(18.0)));
                ui.label(paint::muted("Checking your saved login.", &theme));
                ui.add_space(30.0);
            });
            return;
        }

        let signing_up = self.cloud.mode == FormMode::SignUp;
        ui.label(
            egui::RichText::new(if signing_up {
                "Create your account"
            } else {
                "Welcome back"
            })
            .font(bold(26.0))
            .color(theme.text),
        );
        ui.label(paint::muted(
            if signing_up {
                "Pick a username your friends can find you by."
            } else {
                "Log in to see your friends and their games."
            },
            &theme,
        ));
        ui.add_space(18.0);

        // Log in | Create account, as one pill with a sliding highlight.
        let (track, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
        ui.painter().rect_filled(track, 20.0, theme.field_fill());
        let slide =
            ui.ctx()
                .animate_bool_with_time(egui::Id::unique("login_mode_slide"), signing_up, 0.18);
        let half = track.width() / 2.0;
        let knob = Rect::from_min_size(
            track.min + vec2(4.0 + slide * (half - 4.0), 4.0),
            vec2(half - 4.0, track.height() - 8.0),
        );
        paint::gradient(
            ui.painter(),
            knob,
            16.0,
            theme.accent,
            theme.accent_end,
            vec2(1.0, 0.0),
        );
        for (i, (label, mode)) in [
            ("Log in", FormMode::LogIn),
            ("Create account", FormMode::SignUp),
        ]
        .into_iter()
        .enumerate()
        {
            let rect = Rect::from_min_size(
                track.min + vec2(i as f32 * half, 0.0),
                vec2(half, track.height()),
            );
            let response = ui
                .interact(rect, ui.scope_id().with(("login_mode", i)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.clicked() && self.cloud.mode != mode {
                self.cloud.mode = mode;
                self.cloud.message = None;
            }
            let selected = self.cloud.mode == mode;
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                semibold(14.5),
                if selected {
                    theme.on_accent
                } else if response.hovered() {
                    theme.text
                } else {
                    theme.muted
                },
            );
        }
        ui.add_space(18.0);

        // The fields, each with a small caption above.
        let mut submit = false;
        let width = ui.available_width();
        let field =
            |ui: &mut egui::Ui, caption: &str, text: &mut String, hint: &str, secret: bool| {
                ui.label(paint::caps(caption, 11.5, theme.muted));
                ui.add_space(2.0);
                let response = ui.add(
                    egui::TextEdit::singleline(text)
                        .password(secret)
                        .hint_text(hint)
                        .font(regular(15.5))
                        .margin(vec2(12.0, 10.0))
                        .desired_width(width),
                );
                ui.add_space(10.0);
                response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
            };
        if signing_up {
            submit |= field(
                ui,
                "Username",
                &mut self.cloud.username,
                "3–24 letters, numbers or _",
                false,
            );
        }
        let email_id = ui.next_auto_id();
        submit |= field(ui, "Email", &mut self.cloud.email, "you@example.com", false);
        if !self.cloud.login_focused {
            self.cloud.login_focused = true;
            ui.memory_mut(|m| m.request_focus(email_id));
        }
        let hint = if signing_up {
            "at least 8 characters"
        } else {
            "your password"
        };
        submit |= field(ui, "Password", &mut self.cloud.password, hint, true);

        if let Some(message) = &self.cloud.message {
            egui::Frame::new()
                .fill(theme.panel_alt)
                .corner_radius(10.0)
                .inner_margin(egui::Margin::symmetric(12, 9))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new(message).color(theme.text).size(13.5));
                });
            ui.add_space(10.0);
        }

        // The big button, the full width of the card.
        let busy = self.cloud.busy;
        let (rect, response) = ui.allocate_exact_size(vec2(width, 48.0), Sense::click());
        let lift = if response.hovered() && !busy {
            0.12
        } else {
            0.0
        };
        let [r, g, b, _] = theme.accent.to_array();
        paint::glow(
            ui.painter(),
            rect.center() + vec2(0.0, 8.0),
            width * 0.55,
            Color32::from_rgba_unmultiplied(r, g, b, 45),
        );
        paint::gradient(
            ui.painter(),
            rect,
            24.0,
            super::theme::lighten(theme.accent, lift),
            super::theme::lighten(theme.accent_end, lift),
            vec2(1.0, 0.0),
        );
        let label = match (busy, signing_up) {
            (true, true) => "Creating your account…",
            (true, false) => "Logging in…",
            (false, true) => "Create account",
            (false, false) => "Log in",
        };
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            bold(16.0),
            theme.on_accent,
        );
        if !busy {
            submit |= response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked();
        }
        if submit {
            self.cloud_submit();
        }

        // Or go in without an account.
        ui.add_space(16.0);
        let (line, _) = ui.allocate_exact_size(vec2(width, 16.0), Sense::hover());
        let gap = 22.0;
        for (from, to) in [
            (line.left(), line.center().x - gap),
            (line.center().x + gap, line.right()),
        ] {
            ui.painter().line_segment(
                [pos2(from, line.center().y), pos2(to, line.center().y)],
                Stroke::new(1.0, theme.border),
            );
        }
        ui.painter().text(
            line.center(),
            egui::Align2::CENTER_CENTER,
            "or",
            regular(12.5),
            theme.muted,
        );
        ui.add_space(10.0);
        ui.vertical_centered(|ui| {
            let skip = ui
                .add(
                    egui::Label::new(
                        egui::RichText::new("Continue without an account  →")
                            .font(semibold(14.5))
                            .color(theme.accent),
                    )
                    .sense(Sense::click()),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Everything except GameLib friends works offline");
            if skip.clicked() {
                self.cloud.login_screen = false;
            }
            let mut never = self
                .db
                .get_setting(SKIP_KEY)
                .ok()
                .flatten()
                .is_some_and(|v| v == "1");
            if ui
                .checkbox(
                    &mut never,
                    paint::muted("Don't open on this screen", &theme).size(12.5),
                )
                .changed()
            {
                self.check(self.db.set_setting(SKIP_KEY, if never { "1" } else { "0" }));
            }
        });
        ui.add_space(4.0);
        ui.vertical_centered(|ui| {
            ui.label(
                paint::muted(
                    "Your library is shared only with friends you accept.",
                    &theme,
                )
                .size(12.0),
            );
        });
    }
}

/// Columns of covers sliding slowly up and down.
fn cover_wall(ui: &mut egui::Ui, screen: Rect, covers: &[&Game], time: f64, opacity: f32) {
    let cell = COVER.y + COVER_GAP;
    let columns = (screen.width() / (COVER.x + COVER_GAP)).ceil() as usize + 1;
    let rows = (screen.height() / cell).ceil() as usize + 2;
    ui.scope(|ui| {
        ui.set_opacity(opacity);
        for column in 0..columns {
            // Every other column runs the other way, at its own speed.
            let speed = 10.0 + (column % 3) as f64 * 5.0;
            let travel = time * speed * if column % 2 == 0 { 1.0 } else { -1.0 };
            let step = (travel / cell as f64).floor() as i64;
            let offset = (travel - step as f64 * cell as f64) as f32;
            let x = screen.left() - 40.0 + column as f32 * (COVER.x + COVER_GAP);
            for row in 0..rows {
                let y = screen.top() - cell + row as f32 * cell + offset;
                let game = covers[wall_slot(column, row, step, covers.len())];
                paint::cover(ui, Rect::from_min_size(pos2(x, y), COVER), game, 12.0, 0.0);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cover_keeps_its_game_as_the_column_scrolls() {
        // After one step down, row 1 shows what row 0 showed before.
        for count in [4, 7, 30] {
            for column in 0..5 {
                assert_eq!(
                    wall_slot(column, 0, 0, count),
                    wall_slot(column, 1, 1, count)
                );
                assert_eq!(
                    wall_slot(column, 3, -2, count),
                    wall_slot(column, 1, -4, count)
                );
                assert!(wall_slot(column, 2, -9, count) < count);
            }
        }
    }

    #[test]
    fn the_entrance_eases_from_nothing_to_full() {
        assert_eq!(ease_out(-1.0), 0.0);
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(2.0), 1.0);
        assert!(ease_out(0.5) > 0.5);
    }
}
