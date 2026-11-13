//! The loading animation shown when GameLib starts: the logo star grows in
//! with a ring of orbiting stars, a progress bar fills, and then the whole
//! thing fades away to reveal the library.

use std::f32::consts::TAU;

use eframe::egui::{self, Color32, Id, Order, Rect, Sense, vec2};

use super::paint::{self, StarMap};
use super::theme::{Theme, bold, regular};

/// How long the splash is fully shown, then how long it takes to fade, in
/// seconds.
const SHOW: f64 = 2.0;
const FADE: f64 = 0.45;

/// What the splash says while the bar fills.
const MESSAGES: [&str; 4] = [
    "Waking up the stars…",
    "Loading your library…",
    "Counting your playtime…",
    "Polishing the covers…",
];

pub struct Splash {
    /// egui's clock on the first frame, once it has been drawn.
    started: Option<f64>,
}

impl Splash {
    pub fn new() -> Splash {
        Splash { started: None }
    }

    /// Draw the splash over everything. Returns false once it has finished,
    /// so the caller can drop it.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        theme: &Theme,
        stars: &StarMap,
        picture: Option<&(std::path::PathBuf, [usize; 2])>,
        time: f64,
    ) -> bool {
        let started = *self.started.get_or_insert(time);
        let t = time - started;
        if t >= SHOW + FADE {
            return false;
        }
        // Fully there, then fading out (smoothly).
        let fade = if t < SHOW {
            1.0
        } else {
            let x = ((t - SHOW) / FADE) as f32;
            1.0 - x * x * (3.0 - 2.0 * x)
        };
        let screen = ctx.content_rect();

        // An invisible area over the window catches clicks while it shows.
        // An area over the whole window: it is drawn above everything and
        // catches clicks while the splash shows. (egui only draws layers
        // that belong to an area, so the painting happens inside it.)
        let painter = egui::Area::new(Id::unique("splash"))
            .order(Order::Foreground)
            .fixed_pos(screen.min)
            .interactable(true)
            .show(ctx, |ui| {
                ui.allocate_rect(screen, Sense::click_and_drag());
                ui.painter().clone()
            })
            .inner;
        let mut painter = painter.with_clip_rect(screen);
        painter.set_opacity(fade);
        paint::backdrop(&painter, screen, theme, stars, Some(time));
        if let Some((path, size)) = picture
            && paint::picture_fill(ctx, &painter, path, *size, screen, Color32::WHITE)
        {
            painter.rect_filled(screen, 0.0, Color32::from_black_alpha(110));
        }

        let centre = screen.center() - vec2(0.0, 50.0);
        let grow = ease_out((t / 0.7).min(1.0) as f32);
        let spin = time as f32 * 1.4;

        // A soft glow that breathes.
        let breathe = 0.85 + 0.15 * (time as f32 * 3.0).sin();
        let [r, g, b, _] = theme.accent.to_array();
        paint::glow(
            &painter,
            centre,
            110.0 * grow * breathe,
            Color32::from_rgba_unmultiplied(r, g, b, 90),
        );

        // A ring of small stars orbiting the logo, each fading in turn.
        let orbit = 62.0 * grow;
        for i in 0..10 {
            let angle = spin + TAU * i as f32 / 10.0;
            let at = centre + vec2(angle.cos(), angle.sin()) * orbit;
            let twinkle = 0.35 + 0.65 * ((time as f32 * 4.0 - i as f32 * 0.6).sin() * 0.5 + 0.5);
            let colour = if i % 2 == 0 {
                theme.accent
            } else {
                theme.accent_end
            };
            paint::star4(&painter, at, 5.0 * grow, colour.gamma_multiply(twinkle));
        }

        // The logo: a big four-pointed star.
        paint::star4(&painter, centre, 34.0 * grow, theme.accent);
        paint::star4(
            &painter,
            centre,
            18.0 * grow,
            Color32::WHITE.gamma_multiply(0.9),
        );

        // The name, rising into place.
        let rise = 18.0 * (1.0 - ease_out(((t - 0.2) / 0.6).clamp(0.0, 1.0) as f32));
        let text_alpha = ((t - 0.2) / 0.5).clamp(0.0, 1.0) as f32;
        painter.text(
            centre + vec2(0.0, 112.0 + rise),
            egui::Align2::CENTER_CENTER,
            "GameLib",
            bold(44.0),
            theme.text.gamma_multiply(text_alpha),
        );
        painter.text(
            centre + vec2(0.0, 152.0 + rise),
            egui::Align2::CENTER_CENTER,
            theme.tagline(),
            regular(15.0),
            theme.accent.gamma_multiply(text_alpha),
        );

        // The progress bar, filling quickly then slowing at the end.
        let bar = Rect::from_center_size(centre + vec2(0.0, 200.0), vec2(260.0, 6.0));
        painter.rect_filled(bar, 3.0, theme.panel_alt);
        let filled = ease_out((t / SHOW).min(1.0) as f32);
        if filled > 0.0 {
            let mut fill = bar;
            fill.set_width((bar.width() * filled).max(6.0));
            paint::gradient(
                &painter,
                fill,
                3.0,
                theme.accent,
                theme.accent_end,
                vec2(1.0, 0.0),
            );
            // A bright spark at the leading edge.
            paint::glow(
                &painter,
                fill.right_center(),
                14.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 80),
            );
        }
        let message =
            MESSAGES[((t / SHOW * MESSAGES.len() as f64) as usize).min(MESSAGES.len() - 1)];
        painter.text(
            bar.center() + vec2(0.0, 24.0),
            egui::Align2::CENTER_CENTER,
            message,
            regular(13.5),
            theme.muted.gamma_multiply(text_alpha),
        );

        ctx.request_repaint();
        true
    }
}

/// Fast at first, gently settling at the end.
fn ease_out(x: f32) -> f32 {
    1.0 - (1.0 - x).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_runs_from_zero_to_one() {
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
        assert!(ease_out(0.5) > 0.5, "eases out: past halfway at half time");
    }
}
