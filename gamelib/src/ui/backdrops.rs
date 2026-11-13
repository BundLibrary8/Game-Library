//! More animated backgrounds: Firefly Forest, Deep Ocean, Sakura, Neon
//! Rain, Cosmic Nebula and Frost. Each paints the whole window; the glass
//! panels are drawn on top, as with every other background.
//!
//! Particles (fireflies, bubbles, petals, raindrops, snowflakes) don't
//! need storing: each one's position is worked out from its number and
//! the time, so the same frame always looks the same and nothing has to be
//! updated between frames.

use std::f32::consts::TAU;

use eframe::egui::{self, Color32, Pos2, Rect, Stroke, pos2, vec2};

use super::paint::{StarMap, glow, gradient};

/// A repeatable "random" number from 0 to 1 for particle `i`.
fn rand(i: u32, salt: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

fn at(rect: Rect, x: f32, y: f32) -> Pos2 {
    pos2(
        rect.left() + x * rect.width(),
        rect.top() + y * rect.height(),
    )
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Dark trees along the bottom of the window.
fn tree_line(painter: &egui::Painter, rect: Rect, colour: Color32, height: f32, seed: u32) {
    let count = 18;
    for i in 0..count {
        let x = rect.left() + (i as f32 + rand(i, seed) * 0.6) / count as f32 * rect.width();
        let tall = height * (0.55 + rand(i, seed + 1) * 0.6);
        let half = tall * 0.28;
        let base = rect.bottom() + 4.0;
        // A fir tree: three stacked triangles.
        for layer in 0..3 {
            let top = base - tall + layer as f32 * tall * 0.22;
            let width = half * (0.6 + layer as f32 * 0.25);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    pos2(x, top),
                    pos2(x + width, top + tall * 0.45),
                    pos2(x - width, top + tall * 0.45),
                ],
                colour,
                Stroke::NONE,
            ));
        }
        painter.rect_filled(
            Rect::from_min_max(pos2(x - 3.0, base - tall * 0.25), pos2(x + 3.0, base)),
            0.0,
            colour,
        );
    }
    painter.rect_filled(
        Rect::from_min_max(pos2(rect.left(), rect.bottom() - height * 0.12), rect.max),
        0.0,
        colour,
    );
}

/// Fireflies drifting through a dark forest.
pub fn fireflies(painter: &egui::Painter, rect: Rect, stars: &StarMap, time: Option<f64>) {
    let t = time.unwrap_or(0.0) as f32;
    gradient(
        painter,
        rect,
        0.0,
        Color32::from_rgb(6, 18, 22),
        Color32::from_rgb(10, 32, 24),
        vec2(0.0, 1.0),
    );
    stars.paint_stars(
        &painter.with_clip_rect(Rect::from_min_max(rect.min, at(rect, 1.0, 0.35))),
        rect,
        time,
        0.4,
    );
    let size = rect.width().max(rect.height());
    glow(
        painter,
        at(rect, 0.75, 0.12),
        size * 0.18,
        rgba(220, 240, 200, 40),
    ); // the moon's haze
    painter.circle_filled(at(rect, 0.75, 0.12), size * 0.025, rgba(235, 245, 220, 200));
    tree_line(
        painter,
        rect,
        Color32::from_rgb(8, 22, 16),
        rect.height() * 0.42,
        7,
    );
    tree_line(
        painter,
        rect,
        Color32::from_rgb(3, 10, 8),
        rect.height() * 0.3,
        11,
    );
    for i in 0..60 {
        let home_x = rand(i, 1);
        let home_y = 0.35 + rand(i, 2) * 0.6;
        let speed = 0.08 + rand(i, 3) * 0.12;
        let x = home_x + 0.04 * (t * speed + rand(i, 4) * TAU).sin();
        let y = home_y + 0.03 * (t * speed * 1.3 + rand(i, 5) * TAU).cos();
        // Each one blinks slowly on and off.
        let blink = ((t * (0.6 + rand(i, 6)) + rand(i, 7) * TAU).sin() * 0.5 + 0.5).powf(2.0);
        if blink < 0.05 {
            continue;
        }
        let p = at(rect, x, y);
        glow(painter, p, 14.0, rgba(210, 255, 120, (70.0 * blink) as u8));
        painter.circle_filled(p, 1.8, rgba(240, 255, 180, (255.0 * blink) as u8));
    }
}

/// Light rays from the surface and bubbles rising through deep water.
pub fn ocean(painter: &egui::Painter, rect: Rect, _stars: &StarMap, time: Option<f64>) {
    let t = time.unwrap_or(0.0) as f32;
    gradient(
        painter,
        rect,
        0.0,
        Color32::from_rgb(8, 52, 86),
        Color32::from_rgb(2, 8, 24),
        vec2(0.0, 1.0),
    );
    // Shafts of light swaying from the surface.
    for i in 0..7 {
        let sway = 0.03 * (t * 0.2 + i as f32).sin();
        let x = 0.1 + i as f32 * 0.14 + sway;
        let spread = 0.05 + rand(i, 20) * 0.06;
        let alpha = (18.0 + 14.0 * (t * 0.3 + i as f32 * 1.7).sin()) as u8;
        painter.add(egui::Shape::convex_polygon(
            vec![
                at(rect, x - 0.015, 0.0),
                at(rect, x + 0.015, 0.0),
                at(rect, x + spread, 0.9),
                at(rect, x - spread * 0.6, 0.9),
            ],
            rgba(150, 220, 255, alpha),
            Stroke::NONE,
        ));
    }
    // The rippling surface.
    let mut wave = Vec::new();
    for step in 0..=60 {
        let x = step as f32 / 60.0;
        let y = 0.02 + 0.008 * (x * 18.0 + t * 1.2).sin();
        wave.push(at(rect, x, y));
    }
    painter.line(wave, Stroke::new(2.0, rgba(180, 235, 255, 70)));
    glow(
        painter,
        at(rect, 0.5, 1.1),
        rect.width() * 0.5,
        rgba(0, 120, 160, 40),
    );
    // Bubbles: each rises from the bottom, wobbling, and starts again.
    for i in 0..70 {
        let speed = 0.03 + rand(i, 21) * 0.05;
        let rise = (t * speed + rand(i, 22)).rem_euclid(1.0);
        let x = rand(i, 23) + 0.01 * (t * 2.0 + i as f32).sin();
        let p = at(rect, x, 1.05 - rise * 1.1);
        let r = 1.5 + rand(i, 24) * 4.0;
        let fade = (rise * 4.0).min(1.0) * (1.0 - rise).min(0.3) / 0.3;
        painter.circle_stroke(
            p,
            r,
            Stroke::new(1.0, rgba(190, 240, 255, (120.0 * fade) as u8)),
        );
        painter.circle_filled(
            p + vec2(-r * 0.3, -r * 0.3),
            r * 0.25,
            rgba(255, 255, 255, (90.0 * fade) as u8),
        );
    }
}

/// Cherry blossom petals drifting down through a dusky sky.
pub fn sakura(painter: &egui::Painter, rect: Rect, stars: &StarMap, time: Option<f64>) {
    let t = time.unwrap_or(0.0) as f32;
    gradient(
        painter,
        rect,
        0.0,
        Color32::from_rgb(44, 18, 52),
        Color32::from_rgb(120, 52, 86),
        vec2(0.0, 1.0),
    );
    stars.paint_stars(
        &painter.with_clip_rect(Rect::from_min_max(rect.min, at(rect, 1.0, 0.3))),
        rect,
        time,
        0.35,
    );
    let size = rect.width().max(rect.height());
    glow(
        painter,
        at(rect, 0.2, 0.25),
        size * 0.25,
        rgba(255, 190, 210, 45),
    );
    painter.circle_filled(at(rect, 0.2, 0.25), size * 0.04, rgba(255, 225, 230, 150));
    // A dark branch reaching in from the top right, with blossom on it.
    let branch = [
        at(rect, 1.05, 0.05),
        at(rect, 0.88, 0.12),
        at(rect, 0.74, 0.11),
        at(rect, 0.62, 0.18),
    ];
    painter.line(
        branch.to_vec(),
        Stroke::new(7.0, Color32::from_rgb(40, 16, 30)),
    );
    painter.line_segment(
        [at(rect, 0.88, 0.12), at(rect, 0.82, 0.22)],
        Stroke::new(4.0, Color32::from_rgb(40, 16, 30)),
    );
    for i in 0..26 {
        let along = rand(i, 30);
        let p = at(rect, 0.62 + along * 0.4, 0.09 + rand(i, 31) * 0.12);
        painter.circle_filled(p, 5.0 + rand(i, 32) * 6.0, rgba(255, 170, 200, 170));
    }
    // Falling petals: sliding sideways in the breeze and turning over.
    for i in 0..80 {
        let speed = 0.025 + rand(i, 33) * 0.035;
        let fall = (t * speed + rand(i, 34)).rem_euclid(1.0);
        let x = (rand(i, 35) - fall * 0.25 + 0.03 * (t * 0.8 + i as f32).sin()).rem_euclid(1.0);
        let p = at(rect, x, fall * 1.1 - 0.05);
        let spin = t * (0.8 + rand(i, 36)) + i as f32;
        let w = 5.0 + rand(i, 37) * 3.0;
        let h = w * (0.35 + 0.35 * spin.sin().abs());
        let (s, c) = spin.sin_cos();
        let corner = |dx: f32, dy: f32| p + vec2(dx * c - dy * s, dx * s + dy * c);
        painter.add(egui::Shape::convex_polygon(
            vec![
                corner(-w, 0.0),
                corner(0.0, -h),
                corner(w, 0.0),
                corner(0.0, h),
            ],
            rgba(255, 186, 210, 200),
            Stroke::NONE,
        ));
    }
}

/// A city skyline at night, with rain falling past lit windows.
pub fn rain(painter: &egui::Painter, rect: Rect, _stars: &StarMap, time: Option<f64>) {
    let t = time.unwrap_or(0.0) as f32;
    gradient(
        painter,
        rect,
        0.0,
        Color32::from_rgb(6, 10, 24),
        Color32::from_rgb(30, 18, 52),
        vec2(0.0, 1.0),
    );
    glow(
        painter,
        at(rect, 0.5, 1.0),
        rect.width() * 0.6,
        rgba(255, 60, 160, 35),
    );
    glow(
        painter,
        at(rect, 0.15, 0.9),
        rect.width() * 0.3,
        rgba(40, 200, 255, 30),
    );
    // Two rows of buildings, the far one fainter.
    for (row, colour, seed) in [
        (0.55, Color32::from_rgb(18, 16, 38), 40),
        (0.68, Color32::from_rgb(8, 8, 18), 50),
    ] {
        let mut x = -0.02;
        let mut i = 0;
        while x < 1.02 {
            let w = 0.04 + rand(i, seed) * 0.07;
            let top = row - rand(i, seed + 1) * 0.3;
            let building = Rect::from_min_max(at(rect, x, top), at(rect, x + w, 1.0));
            painter.rect_filled(building, 0.0, colour);
            // Lit windows, a few flickering on and off.
            for wy in 0..12 {
                for wx in 0..4 {
                    let n = i * 97 + wy * 7 + wx;
                    if rand(n, seed + 2) > 0.28 {
                        continue;
                    }
                    let flicker = ((t * 0.2 + rand(n, seed + 3) * 10.0).sin() > -0.8) as u8;
                    let wpos = building.min
                        + vec2(
                            4.0 + wx as f32 * building.width() / 4.2,
                            8.0 + wy as f32 * 16.0,
                        );
                    if wpos.y + 7.0 > building.bottom() {
                        continue;
                    }
                    let hue = if rand(n, seed + 4) > 0.7 {
                        rgba(120, 220, 255, 140)
                    } else {
                        rgba(255, 200, 120, 130)
                    };
                    painter.rect_filled(
                        Rect::from_min_size(wpos, vec2(4.0, 6.0)),
                        0.0,
                        if flicker == 1 {
                            hue
                        } else {
                            Color32::TRANSPARENT
                        },
                    );
                }
            }
            x += w + 0.004;
            i += 1;
        }
    }
    // Rain: thin slanted streaks falling fast.
    for i in 0..220 {
        let speed = 0.6 + rand(i, 60) * 0.5;
        let fall = (t * speed + rand(i, 61)).rem_euclid(1.0);
        let x = rand(i, 62) - fall * 0.08;
        let top = at(rect, x, fall * 1.2 - 0.1);
        let length = 10.0 + rand(i, 63) * 14.0;
        painter.line_segment(
            [top, top + vec2(-length * 0.15, length)],
            Stroke::new(1.0, rgba(170, 200, 255, (40.0 + rand(i, 64) * 50.0) as u8)),
        );
    }
}

/// Swirling clouds of colour in deep space, turning very slowly.
pub fn nebula(painter: &egui::Painter, rect: Rect, stars: &StarMap, time: Option<f64>) {
    let t = time.unwrap_or(0.0) as f32;
    painter.rect_filled(rect, 0.0, Color32::from_rgb(5, 3, 14));
    let size = rect.width().max(rect.height());
    let centre = at(rect, 0.55, 0.45);
    let colours = [
        (230, 70, 160),
        (90, 70, 240),
        (40, 190, 230),
        (250, 140, 60),
    ];
    // Clouds arranged in a slow spiral.
    for i in 0..18 {
        let angle = i as f32 * 0.9 + t * 0.03 * (1.0 + (i % 3) as f32 * 0.3);
        let reach = 0.08 + i as f32 * 0.022;
        let p = centre + vec2(angle.cos(), angle.sin() * 0.6) * size * reach;
        let (r, g, b) = colours[i as usize % colours.len()];
        let breathe = 1.0 + 0.15 * (t * 0.2 + i as f32).sin();
        glow(
            painter,
            p,
            size * (0.11 + rand(i, 70) * 0.1) * breathe,
            rgba(r, g, b, 75),
        );
    }
    glow(painter, centre, size * 0.12, rgba(255, 230, 250, 110));
    stars.paint_stars(painter, rect, time, 1.0);
}

/// Snow falling over dark, frosted mountains.
pub fn frost(painter: &egui::Painter, rect: Rect, stars: &StarMap, time: Option<f64>) {
    let t = time.unwrap_or(0.0) as f32;
    gradient(
        painter,
        rect,
        0.0,
        Color32::from_rgb(8, 16, 34),
        Color32::from_rgb(40, 70, 104),
        vec2(0.0, 1.0),
    );
    stars.paint_stars(
        &painter.with_clip_rect(Rect::from_min_max(rect.min, at(rect, 1.0, 0.4))),
        rect,
        time,
        0.5,
    );
    // Two ranges of mountains with snowy tops.
    for (base, colour, snow, seed) in [
        (
            0.62,
            Color32::from_rgb(52, 76, 112),
            rgba(210, 230, 255, 150),
            80,
        ),
        (
            0.78,
            Color32::from_rgb(14, 24, 42),
            rgba(220, 235, 255, 150),
            90,
        ),
    ] {
        let peaks = 7;
        for i in 0..peaks {
            let x = (i as f32 + rand(i, seed) * 0.5) / peaks as f32 * 1.2 - 0.1;
            let height = 0.18 + rand(i, seed + 1) * 0.2;
            let width = 0.12 + rand(i, seed + 2) * 0.1;
            let peak = at(rect, x, base - height);
            painter.add(egui::Shape::convex_polygon(
                vec![peak, at(rect, x + width, 1.0), at(rect, x - width, 1.0)],
                colour,
                Stroke::NONE,
            ));
            // The snow cap: a smaller triangle at the top.
            let cap = 0.25;
            let left =
                peak + (at(rect, x - width, 1.0) - peak) * cap * (height / (1.0 - base + height));
            let right =
                peak + (at(rect, x + width, 1.0) - peak) * cap * (height / (1.0 - base + height));
            painter.add(egui::Shape::convex_polygon(
                vec![peak, right, left],
                snow,
                Stroke::NONE,
            ));
        }
    }
    // Snowflakes drifting down and sideways.
    for i in 0..160 {
        let speed = 0.02 + rand(i, 100) * 0.04;
        let fall = (t * speed + rand(i, 101)).rem_euclid(1.0);
        let x = (rand(i, 102) + 0.03 * (t * 0.5 + i as f32).sin()).rem_euclid(1.0);
        let r = 1.0 + rand(i, 103) * 2.2;
        painter.circle_filled(
            at(rect, x, fall * 1.1 - 0.05),
            r,
            rgba(240, 248, 255, (120.0 + rand(i, 104) * 120.0) as u8),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_numbers_repeat_and_stay_in_range() {
        for i in 0..1000 {
            let r = rand(i, 3);
            assert!((0.0..1.0).contains(&r));
            assert_eq!(r, rand(i, 3));
        }
        assert_ne!(rand(1, 1), rand(1, 2));
    }
}
