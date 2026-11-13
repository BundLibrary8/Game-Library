//! Drawing helpers: gradients, the star map, covers, and the handful of
//! custom buttons the designs need.
//!
//! egui draws with a [`egui::Painter`]: you give it shapes (rectangles,
//! circles, lines, text) and positions. Gradients aren't built in, so they
//! are made here from a *mesh*: a set of coloured points joined into
//! triangles, which the graphics card blends between.

use std::f32::consts::TAU;

use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2};
use eframe::epaint::Mesh;
use gamelib::model::Game;
use gamelib::profile::Banner;

use super::theme::{self, Backdrop, Theme, bold, darken, lighten, mix, regular, semibold};

// --- Gradients -----------------------------------------------------------

/// The outline of a rounded rectangle, as a ring of points.
fn rounded_outline(rect: Rect, radius: f32) -> Vec<Pos2> {
    let r = radius
        .min(rect.width() / 2.0)
        .min(rect.height() / 2.0)
        .max(0.0);
    if r < 0.5 {
        return vec![
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
        ];
    }
    const STEPS: usize = 8;
    // Each corner: its centre, and the angle its arc starts at.
    let corners = [
        (pos2(rect.right() - r, rect.top() + r), -TAU / 4.0),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), TAU / 4.0),
        (pos2(rect.left() + r, rect.top() + r), TAU / 2.0),
    ];
    let mut points = Vec::with_capacity(4 * (STEPS + 1));
    for (centre, start) in corners {
        for step in 0..=STEPS {
            let angle = start + (TAU / 4.0) * step as f32 / STEPS as f32;
            points.push(centre + r * vec2(angle.cos(), angle.sin()));
        }
    }
    points
}

/// Fill a rounded rectangle with a gradient from `from` to `to`, running
/// in `direction` (`vec2(1.0, 0.0)` is left to right, `vec2(0.0, 1.0)` top
/// to bottom, `vec2(1.0, 1.0)` corner to corner).
pub fn gradient(
    painter: &egui::Painter,
    rect: Rect,
    radius: f32,
    from: Color32,
    to: Color32,
    direction: Vec2,
) {
    let span = (rect.size().x * direction.x + rect.size().y * direction.y).max(1.0);
    let colour_at = |p: Pos2| {
        let along = (p.x - rect.left()) * direction.x + (p.y - rect.top()) * direction.y;
        mix(from, to, along / span)
    };
    // A fan of triangles from the centre out to the outline.
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.center(), colour_at(rect.center()));
    let outline = rounded_outline(rect, radius);
    for point in &outline {
        mesh.colored_vertex(*point, colour_at(*point));
    }
    let count = outline.len() as u32;
    for i in 0..count {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % count);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// A soft round glow: `colour` in the middle fading to nothing at the edge.
pub fn glow(painter: &egui::Painter, centre: Pos2, radius: f32, colour: Color32) {
    const STEPS: u32 = 40;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(centre, colour);
    for step in 0..STEPS {
        let angle = TAU * step as f32 / STEPS as f32;
        mesh.colored_vertex(
            centre + radius * vec2(angle.cos(), angle.sin()),
            Color32::TRANSPARENT,
        );
    }
    for i in 0..STEPS {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % STEPS);
    }
    painter.add(egui::Shape::mesh(mesh));
}

// --- The star map --------------------------------------------------------

struct Star {
    /// Position as a fraction of the window (0.0 to 1.0).
    x: f32,
    y: f32,
    size: f32,
    brightness: f32,
    /// Where in its twinkle this star starts.
    phase: f32,
}

/// The stars and constellations of the Constellation background. Made once
/// at startup from a fixed seed, so the sky is the same every launch.
pub struct StarMap {
    stars: Vec<Star>,
    /// Each constellation is a path of points (window fractions).
    constellations: Vec<Vec<(f32, f32)>>,
}

impl StarMap {
    pub fn new() -> StarMap {
        // A tiny random-number generator. A fixed seed gives the same
        // "random" numbers every time.
        let mut seed: u32 = 0x5EED_1234;
        let mut random = move || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32
        };
        let stars = (0..260)
            .map(|_| Star {
                x: random(),
                y: random(),
                size: 0.5 + random() * random() * 1.6,
                brightness: 0.25 + random() * 0.65,
                phase: random() * TAU,
            })
            .collect();
        let constellations = vec![
            vec![(0.07, 0.15), (0.125, 0.11), (0.18, 0.16), (0.235, 0.13)],
            vec![
                (0.79, 0.085),
                (0.845, 0.15),
                (0.90, 0.12),
                (0.945, 0.20),
                (0.885, 0.26),
                (0.845, 0.15),
            ],
            vec![(0.14, 0.87), (0.18, 0.80), (0.165, 0.80)],
            vec![(0.545, 0.81), (0.585, 0.78), (0.64, 0.85)],
            vec![(0.90, 0.90), (0.96, 0.85), (0.945, 0.80)],
            vec![(0.225, 0.265), (0.165, 0.335)],
        ];
        StarMap {
            stars,
            constellations,
        }
    }

    /// Paint the Constellation sky over `rect`: nebula glows, constellation
    /// lines, and stars. `time` (seconds) makes the stars twinkle and sends
    /// the occasional shooting star across; pass `None` for a still sky.
    pub fn paint(&self, painter: &egui::Painter, rect: Rect, time: Option<f64>) {
        let at = |x: f32, y: f32| {
            pos2(
                rect.left() + x * rect.width(),
                rect.top() + y * rect.height(),
            )
        };
        let size = rect.width().max(rect.height());

        // Nebula: big, faint glows of purple and teal that slowly breathe.
        let breathe = |phase: f32| match time {
            Some(t) => 1.0 + 0.12 * ((t as f32) * 0.25 + phase).sin(),
            None => 1.0,
        };
        glow(
            painter,
            at(0.12, 0.05),
            size * 0.62 * breathe(0.0),
            Color32::from_rgba_unmultiplied(98, 64, 200, 105),
        );
        glow(
            painter,
            at(0.95, 0.95),
            size * 0.55 * breathe(2.0),
            Color32::from_rgba_unmultiplied(20, 120, 150, 85),
        );
        glow(
            painter,
            at(0.55, 0.45),
            size * 0.35 * breathe(4.0),
            Color32::from_rgba_unmultiplied(60, 60, 170, 28),
        );

        let line = Stroke::new(1.0, Color32::from_rgba_unmultiplied(143, 180, 255, 70));
        for path in &self.constellations {
            for pair in path.windows(2) {
                painter.line_segment([at(pair[0].0, pair[0].1), at(pair[1].0, pair[1].1)], line);
            }
            for &(x, y) in path {
                painter.circle_filled(
                    at(x, y),
                    4.5,
                    Color32::from_rgba_unmultiplied(143, 180, 255, 40),
                );
                painter.circle_filled(at(x, y), 2.0, Color32::from_rgb(223, 232, 255));
            }
        }

        self.paint_stars(painter, rect, time, 1.0);

        // A shooting star every few seconds, from a different place each time.
        if let Some(t) = time {
            const EVERY: f64 = 7.0;
            const LASTS: f64 = 0.9;
            let number = (t / EVERY).floor();
            let progress = ((t - number * EVERY) / LASTS) as f32;
            if progress < 1.0 {
                let random = |salt: f64| {
                    ((number * 12.9898 + salt).sin() * 43758.5453).rem_euclid(1.0) as f32
                };
                let start = at(0.1 + 0.7 * random(1.0), 0.04 + 0.3 * random(2.0));
                let travel = vec2(rect.width() * 0.22, rect.height() * 0.16);
                let head = start + travel * progress;
                let fade = (1.0 - progress).min(progress * 6.0).clamp(0.0, 1.0);
                // The tail is drawn as short pieces that get fainter.
                for piece in 0..6 {
                    let from = head - travel * 0.045 * piece as f32;
                    let to = head - travel * 0.045 * (piece + 1) as f32;
                    let alpha = (fade * 230.0 * (1.0 - piece as f32 / 6.0)) as u8;
                    painter.line_segment(
                        [from, to],
                        Stroke::new(1.8, Color32::from_rgba_unmultiplied(223, 232, 255, alpha)),
                    );
                }
            }
        }
    }

    /// Paint the star positions as sparks rising from the bottom of `rect`:
    /// each one drifts up, sways a little, flickers, and fades as it climbs.
    pub fn paint_sparks(&self, painter: &egui::Painter, rect: Rect, time: f32) {
        // Every other star is enough; a sky full of sparks is too busy.
        for star in self.stars.iter().step_by(2) {
            let speed = 0.012 + 0.03 * star.brightness;
            // How far up it has got: 0.0 at the bottom, 1.0 at the top.
            let climb = (star.y + time * speed).rem_euclid(1.0);
            let sway = 0.012 * (time * 0.7 + star.phase).sin();
            let centre = pos2(
                rect.left() + (star.x + sway) * rect.width(),
                rect.bottom() - climb * rect.height(),
            );
            let flicker = 0.7 + 0.3 * (time * 3.1 + star.phase * 5.0).sin();
            let alpha = ((1.0 - climb).powf(1.4) * flicker * 230.0) as u8;
            if star.size > 1.3 {
                glow(
                    painter,
                    centre,
                    star.size * 5.0,
                    Color32::from_rgba_unmultiplied(255, 140, 50, alpha / 3),
                );
            }
            painter.circle_filled(
                centre,
                star.size,
                Color32::from_rgba_unmultiplied(255, 180, 90, alpha),
            );
        }
    }

    /// Paint just the stars. `strength` (0.0 to 1.0) dims them, for
    /// backgrounds where they are only a detail.
    pub fn paint_stars(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        time: Option<f64>,
        strength: f32,
    ) {
        for star in &self.stars {
            let twinkle = match time {
                Some(t) => 0.72 + 0.28 * ((t as f32) * 1.3 + star.phase).sin(),
                None => 1.0,
            };
            let alpha = (star.brightness * twinkle * strength * 255.0) as u8;
            painter.circle_filled(
                pos2(
                    rect.left() + star.x * rect.width(),
                    rect.top() + star.y * rect.height(),
                ),
                star.size,
                Color32::from_rgba_unmultiplied(223, 232, 255, alpha),
            );
        }
    }
}

/// Paint the window's background for the current theme. `time` (seconds)
/// animates it; `None` draws it standing still.
pub fn backdrop(
    painter: &egui::Painter,
    rect: Rect,
    theme: &Theme,
    stars: &StarMap,
    time: Option<f64>,
) {
    painter.rect_filled(rect, 0.0, theme.bg);
    match theme.backdrop {
        Backdrop::Ember => embers(painter, rect, stars, time),
        Backdrop::Constellation => stars.paint(painter, rect, time),
        Backdrop::Aurora => aurora(painter, rect, stars, time),
        Backdrop::Synthwave => synthwave(painter, rect, stars, time),
    }
}

/// Warm glows low down, with sparks drifting up out of them.
fn embers(painter: &egui::Painter, rect: Rect, stars: &StarMap, time: Option<f64>) {
    let t = time.unwrap_or(0.0) as f32;
    let size = rect.width().max(rect.height());
    let at = |x: f32, y: f32| {
        pos2(
            rect.left() + x * rect.width(),
            rect.top() + y * rect.height(),
        )
    };
    // The fire's glow breathes slowly.
    let breathe = 1.0 + 0.08 * (t * 0.5).sin();
    glow(
        painter,
        at(0.15, 1.05),
        size * 0.6 * breathe,
        Color32::from_rgba_unmultiplied(230, 110, 30, 85),
    );
    glow(
        painter,
        at(0.9, 1.1),
        size * 0.55 / breathe,
        Color32::from_rgba_unmultiplied(200, 60, 40, 70),
    );
    glow(
        painter,
        at(0.55, -0.1),
        size * 0.45,
        Color32::from_rgba_unmultiplied(120, 60, 30, 40),
    );
    stars.paint_sparks(painter, rect, t);
}

/// One curtain of the northern lights: a wavy band that is brightest along
/// its lower edge and fades upwards.
#[allow(clippy::too_many_arguments)]
fn aurora_ribbon(
    painter: &egui::Painter,
    rect: Rect,
    centre: f32,
    sway: f32,
    height: f32,
    colour: (u8, u8, u8, u8),
    waves: f32,
    phase: f32,
    time: f32,
) {
    const COLUMNS: u32 = 72;
    let (r, g, b, a) = colour;
    let mut mesh = Mesh::default();
    for column in 0..=COLUMNS {
        let across = column as f32 / COLUMNS as f32;
        // Two waves of different lengths, drifting in opposite directions.
        let wave = 0.6 * (across * waves * TAU + time * 0.35 + phase).sin()
            + 0.4 * (across * waves * 2.3 * TAU - time * 0.22 + phase * 1.7).sin();
        // The curtain shimmers: columns brighten and dim as it moves.
        let shimmer = 0.6 + 0.4 * (across * 11.0 + time * 0.8 + phase * 3.0).sin();
        let x = rect.left() + across * rect.width();
        let edge = rect.top() + rect.height() * (centre + sway * wave);
        let alpha = (a as f32 * shimmer) as u8;
        mesh.colored_vertex(pos2(x, edge - rect.height() * height), Color32::TRANSPARENT);
        mesh.colored_vertex(
            pos2(x, edge),
            Color32::from_rgba_unmultiplied(r, g, b, alpha),
        );
        mesh.colored_vertex(
            pos2(x, edge + rect.height() * height * 0.12),
            Color32::TRANSPARENT,
        );
    }
    for column in 0..COLUMNS {
        let i = column * 3;
        // Upper band (fading up) and lower band (the short fade below).
        for (top, bottom) in [(0, 1), (1, 2)] {
            mesh.add_triangle(i + top, i + bottom, i + 3 + top);
            mesh.add_triangle(i + bottom, i + 3 + bottom, i + 3 + top);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// The northern lights: green, teal and violet curtains over a dark sky
/// with faint stars.
fn aurora(painter: &egui::Painter, rect: Rect, stars: &StarMap, time: Option<f64>) {
    let t = time.unwrap_or(0.0) as f32;
    let size = rect.width().max(rect.height());
    let at = |x: f32, y: f32| {
        pos2(
            rect.left() + x * rect.width(),
            rect.top() + y * rect.height(),
        )
    };
    glow(
        painter,
        at(0.2, 0.0),
        size * 0.6,
        Color32::from_rgba_unmultiplied(20, 110, 100, 60),
    );
    glow(
        painter,
        at(0.85, 1.0),
        size * 0.55,
        Color32::from_rgba_unmultiplied(70, 50, 160, 55),
    );
    stars.paint_stars(painter, rect, time, 0.55);
    aurora_ribbon(
        painter,
        rect,
        0.50,
        0.07,
        0.42,
        (150, 105, 245, 80),
        0.9,
        4.0,
        t,
    );
    aurora_ribbon(
        painter,
        rect,
        0.40,
        0.06,
        0.36,
        (60, 185, 225, 85),
        1.3,
        2.1,
        t,
    );
    aurora_ribbon(
        painter,
        rect,
        0.32,
        0.08,
        0.30,
        (70, 240, 160, 120),
        1.1,
        0.0,
        t,
    );
}

/// A striped sun setting behind a neon grid that rolls towards you.
fn synthwave(painter: &egui::Painter, rect: Rect, stars: &StarMap, time: Option<f64>) {
    let t = time.unwrap_or(0.0) as f32;
    let horizon = rect.top() + rect.height() * 0.58;
    let sky = Rect::from_min_max(rect.min, pos2(rect.right(), horizon));
    let ground = Rect::from_min_max(pos2(rect.left(), horizon), rect.max);
    let sky_top = Color32::from_rgb(16, 4, 34);
    let sky_low = Color32::from_rgb(120, 26, 104);
    gradient(painter, sky, 0.0, sky_top, sky_low, vec2(0.0, 1.0));
    let upper_sky = Rect::from_min_max(
        rect.min,
        pos2(rect.right(), rect.top() + rect.height() * 0.4),
    );
    stars.paint_stars(
        &painter.with_clip_rect(upper_sky.intersect(painter.clip_rect())),
        rect,
        time,
        0.6,
    );

    // The sun: yellow at the top, pink at the bottom, cut off by the
    // horizon, with dark stripes sliding down across its lower half.
    let radius = rect.height().min(rect.width()) * 0.19;
    let sun_centre = pos2(rect.center().x, horizon - radius * 0.42);
    let sun = Rect::from_center_size(sun_centre, Vec2::splat(radius * 2.0));
    let above_horizon = painter.with_clip_rect(sky.intersect(painter.clip_rect()));
    glow(
        &above_horizon,
        sun_centre,
        radius * 2.1,
        Color32::from_rgba_unmultiplied(255, 90, 170, 95),
    );
    gradient(
        &above_horizon,
        sun,
        radius,
        Color32::from_rgb(255, 222, 92),
        Color32::from_rgb(255, 60, 150),
        vec2(0.0, 1.0),
    );
    const STRIPES: u32 = 6;
    let slide = (t * 0.12).rem_euclid(1.0);
    for stripe in 0..STRIPES {
        let down = (stripe as f32 + slide) / STRIPES as f32; // 0 at the middle, 1 at the bottom
        let y = sun_centre.y + down * radius;
        let thickness = 2.0 + down * radius * 0.11;
        let band = Rect::from_min_max(pos2(sun.left(), y), pos2(sun.right(), y + thickness));
        above_horizon.rect_filled(band, 0.0, mix(sky_top, sky_low, 0.75));
    }

    painter.rect_filled(ground, 0.0, Color32::from_rgb(13, 3, 30));
    glow(
        &painter.with_clip_rect(ground.intersect(painter.clip_rect())),
        pos2(rect.center().x, horizon),
        rect.width() * 0.5,
        Color32::from_rgba_unmultiplied(255, 70, 180, 60),
    );
    let neon = |alpha: u8| Stroke::new(1.2, Color32::from_rgba_unmultiplied(255, 84, 196, alpha));
    // Lines fanning out from the vanishing point…
    for i in -14..=14 {
        let x = rect.center().x + i as f32 * rect.width() * 0.13;
        painter.line_segment(
            [
                pos2(rect.center().x + i as f32 * 10.0, horizon),
                pos2(x, rect.bottom()),
            ],
            neon(70),
        );
    }
    // …and lines across, which roll towards you: each one starts at the
    // horizon and speeds up as it gets closer.
    const ROWS: u32 = 12;
    let roll = (t * 0.45).rem_euclid(1.0);
    for row in 0..ROWS {
        let near = ((row as f32 + roll) / ROWS as f32).powf(2.2);
        let y = horizon + near * ground.height();
        painter.line_segment(
            [pos2(rect.left(), y), pos2(rect.right(), y)],
            neon((40.0 + 90.0 * near) as u8),
        );
    }
    painter.line_segment(
        [pos2(rect.left(), horizon), pos2(rect.right(), horizon)],
        Stroke::new(1.6, Color32::from_rgba_unmultiplied(255, 150, 220, 200)),
    );
}

// --- Covers --------------------------------------------------------------

/// Two colours for a game's placeholder cover, chosen from its title so a
/// game always gets the same pair.
pub fn title_colours(title: &str) -> (Color32, Color32) {
    const PAIRS: [(u32, u32); 12] = [
        (0xf27a3d, 0x8f2141),
        (0x3b74ee, 0x2b1b6b),
        (0x2c8a57, 0x12301f),
        (0xf03aa0, 0x4a1260),
        (0x23aebb, 0x143a5c),
        (0xb07f33, 0x3d2413),
        (0x86cc4a, 0x235e2c),
        (0x8493a8, 0x2b3340),
        (0x7352e8, 0x24195e),
        (0x9fd8f0, 0x1f4a70),
        (0xf0a23c, 0x5a2a12),
        (0xf04a4a, 0x4a1016),
    ];
    let sum: usize = title.bytes().map(usize::from).sum();
    let (a, b) = PAIRS[sum % PAIRS.len()];
    let colour = |hex: u32| Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
    (colour(a), colour(b))
}

/// The address egui's image loader uses for a file on disk.
pub fn file_uri(path: &std::path::Path) -> String {
    file_uri_from_text(&path.to_string_lossy())
}

/// Turn a path, written as text, into a `file://` address.
///
/// This needs care on Windows. egui reads anything after `file://` that
/// doesn't start with `/` as a network share, so `file://C:\x.jpg` would
/// be looked for at `\\C:\x.jpg` and never found. The standard form,
/// `file:///C:/x.jpg`, is read correctly on every system.
pub fn file_uri_from_text(path: &str) -> String {
    let path = gamelib::paths::strip_verbatim_prefix(path).replace('\\', "/");
    if path.starts_with("//") {
        // A network share (`\\server\share\…`): file://server/share/…
        format!("file:{path}")
    } else if path.starts_with('/') {
        format!("file://{path}") // Linux and macOS: file:///home/…
    } else {
        format!("file:///{path}") // Windows: file:///C:/Users/…
    }
}

/// Paths whose pictures failed to load, so each failure is logged once
/// rather than on every frame.
fn note_failed_picture(path: &std::path::Path, err: &impl std::fmt::Display) {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    static FAILED: OnceLock<Mutex<HashSet<std::path::PathBuf>>> = OnceLock::new();
    let mut failed = FAILED
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if failed.insert(path.to_path_buf()) {
        log::warn!("couldn't show the picture {}: {err}", path.display());
    }
}

/// Draw a picture file stretched to fill `rect`. Returns false if it
/// can't be shown (still loading, or unreadable), so the caller can draw
/// something in its place.
pub fn file_image(ui: &Ui, path: &std::path::Path, rect: Rect, radius: f32) -> bool {
    if !path.is_file() {
        return false;
    }
    let image = egui::Image::from_uri(file_uri(path))
        .corner_radius(radius)
        .fit_to_exact_size(rect.size())
        .maintain_aspect_ratio(false);
    match image.load_for_size(ui.ctx(), rect.size()) {
        Ok(egui::load::TexturePoll::Ready { .. }) => {
            image.paint_at(ui, rect);
            true
        }
        // egui asks for a redraw once the file has been read.
        Ok(egui::load::TexturePoll::Pending { .. }) => false,
        Err(err) => {
            note_failed_picture(path, &err);
            false
        }
    }
}

/// Paint a game's cover into `rect`: its cover picture if it has one,
/// otherwise a gradient with the game's own icon and its title on it.
/// `title_size` 0 leaves the title off.
pub fn cover(ui: &Ui, rect: Rect, game: &Game, radius: f32, title_size: f32) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let showed_cover = game
        .cover_path
        .as_deref()
        .is_some_and(|path| file_image(ui, path, rect, radius));
    // No cover (or it couldn't be read): a gradient with the icon and title.
    match showed_cover {
        true => {}
        false => {
            let (top, bottom) = title_colours(&game.title);
            gradient(painter, rect, radius, top, bottom, vec2(0.25, 1.0));
            // The game's own icon (read from its exe), on a soft plate.
            if let Some(icon) = game.icon_path.as_deref() {
                let side = (rect.width() * 0.4).clamp(24.0, 128.0);
                let centre = pos2(rect.center().x, rect.top() + rect.height() * 0.4);
                let plate = Rect::from_center_size(centre, Vec2::splat(side * 1.4));
                painter.rect_filled(plate, side * 0.3, Color32::from_black_alpha(70));
                painter.rect_stroke(
                    plate,
                    side * 0.3,
                    Stroke::new(1.0, Color32::from_white_alpha(40)),
                    egui::StrokeKind::Inside,
                );
                file_image(
                    ui,
                    icon,
                    Rect::from_center_size(centre, Vec2::splat(side)),
                    side * 0.12,
                );
            }
            if title_size > 0.0 {
                let pad = (rect.width() * 0.06).max(8.0);
                let galley = painter.layout(
                    game.title.clone(),
                    bold(title_size),
                    Color32::WHITE,
                    rect.width() - 2.0 * pad,
                );
                let pos = pos2(rect.left() + pad, rect.bottom() - pad - galley.size().y);
                painter.galley(pos, galley, Color32::WHITE);
            }
        }
    }
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, Color32::from_white_alpha(28)),
        egui::StrokeKind::Inside,
    );
    if !game.installed {
        painter.rect_filled(rect, radius, Color32::from_black_alpha(150));
    }
}

/// The small "STEAM" / "EPIC" label in a cover's top-left corner.
pub fn source_badge(ui: &Ui, cover_rect: Rect, game: &Game) {
    let painter = ui.painter();
    let galley = painter.layout_no_wrap(
        theme::source_badge(game.source).to_string(),
        bold(10.5),
        Color32::WHITE,
    );
    let pad = vec2(7.0, 4.0);
    let min = cover_rect.left_top() + vec2(10.0, 10.0);
    let rect = Rect::from_min_size(min, galley.size() + 2.0 * pad);
    painter.rect_filled(rect, 5.0, Color32::from_rgba_unmultiplied(10, 14, 28, 200));
    painter.galley(min + pad, galley, Color32::WHITE);
}

// --- Profile pieces ------------------------------------------------------

/// The two colours of a banner item from the shop or the earned list.
pub fn item_colours(item_id: &str) -> (Color32, Color32) {
    let (a, b): (u32, u32) = match item_id {
        "banner_nebula" => (0x8a5cf0, 0x120a2e),
        "banner_pixel_forest" => (0x4fae5a, 0x143a2a),
        "banner_100h" => (0xf0a020, 0x8a4a10),
        "banner_250h" => (0x3fc48a, 0x145a3c),
        "banner_500h" => (0x4a3a9a, 0x140f30),
        "banner_10_finished" => (0x1f6f78, 0x0c2a36),
        "bg_constellation" => (0x3a2a8a, 0x0a1830),
        "bg_aurora" => (0x1f9a6a, 0x6a3fd0),
        "bg_synthwave_grid" => (0xf03aa0, 0x1a0630),
        "badge_night_owl" => (0x8a5cf0, 0x3a2a6a),
        "badge_speedrunner" => (0x3b9dee, 0x1b3a6b),
        "badge_coop_captain" => (0x2fb7a4, 0x14403c),
        "badge_founder" => (0x4cc38a, 0x1c4a34),
        "badge_backlog_slayer" => (0xf5a524, 0x6a4410),
        _ => (0x5a6478, 0x1d212a),
    };
    let colour = |hex: u32| Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
    (colour(a), colour(b))
}

/// Paint a profile banner into `rect`. With no banner chosen, a quiet
/// default in the theme's accent colour is used.
pub fn banner(
    ui: &Ui,
    rect: Rect,
    banner: Option<&Banner>,
    games: &[Game],
    theme: &Theme,
    radius: f32,
) {
    let painter = ui.painter();
    if let Some(Banner::Upload(path)) = banner
        && file_image(ui, path, rect, radius)
    {
        return;
    }
    match banner {
        Some(Banner::Game(id)) => {
            let title = games
                .iter()
                .find(|g| g.id == *id)
                .map(|g| g.title.as_str())
                .unwrap_or("");
            let (a, b) = title_colours(title);
            ember_glow(painter, rect, radius, a, b);
        }
        Some(Banner::Item(id)) => {
            let (a, b) = item_colours(id);
            gradient(painter, rect, radius, a, b, vec2(1.0, 0.6));
        }
        _ => {
            let base = darken(theme.accent, 0.55);
            gradient(
                painter,
                rect,
                radius,
                darken(base, 0.5),
                base,
                vec2(1.0, 0.3),
            );
        }
    }
}

/// A dark banner with a bright spot of `bright` towards the right, like
/// the banner in the profile design.
fn ember_glow(painter: &egui::Painter, rect: Rect, radius: f32, bright: Color32, dark: Color32) {
    gradient(
        painter,
        rect,
        radius,
        darken(dark, 0.45),
        darken(bright, 0.35),
        vec2(1.0, 0.0),
    );
    let clipped = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let [r, g, b, _] = lighten(bright, 0.15).to_array();
    glow(
        &clipped,
        pos2(rect.left() + rect.width() * 0.72, rect.center().y),
        rect.width() * 0.3,
        Color32::from_rgba_unmultiplied(r, g, b, 170),
    );
}

/// A round avatar: the profile picture if there is one, otherwise the
/// first letter of the name on the accent colour.
pub fn avatar(
    ui: &Ui,
    centre: Pos2,
    radius: f32,
    name: &str,
    picture: Option<&std::path::Path>,
    theme: &Theme,
) {
    let rect = Rect::from_center_size(centre, Vec2::splat(radius * 2.0));
    let painter = ui.painter();
    match picture.is_some_and(|path| file_image(ui, path, rect, radius)) {
        true => {}
        false => {
            let (from, to) = (theme.accent, theme.accent_end);
            gradient(painter, rect, radius, from, to, vec2(0.6, 1.0));
            let initial = name
                .chars()
                .next()
                .unwrap_or('?')
                .to_uppercase()
                .to_string();
            painter.text(
                centre,
                egui::Align2::CENTER_CENTER,
                initial,
                bold(radius * 0.95),
                theme.on_accent,
            );
        }
    }
}

/// A small round avatar for a friend: their initial on a colour picked
/// from their name, with a status dot.
pub fn friend_avatar(ui: &Ui, centre: Pos2, radius: f32, name: &str, dot: Color32, theme: &Theme) {
    let painter = ui.painter();
    let (colour, _) = title_colours(name);
    painter.circle_filled(centre, radius, colour);
    let initial = name
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    painter.text(
        centre,
        egui::Align2::CENTER_CENTER,
        initial,
        bold(radius * 0.9),
        Color32::WHITE,
    );
    let dot_at = centre + vec2(radius * 0.72, radius * 0.72);
    painter.circle_filled(dot_at, 6.0, theme.opaque_panel());
    painter.circle_filled(dot_at, 4.0, dot);
}

// --- Buttons and labels --------------------------------------------------

/// The big accent-coloured button (Launch, Add game, Save profile…).
pub fn accent_button(
    ui: &mut Ui,
    theme: &Theme,
    text: &str,
    font_size: f32,
    padding: Vec2,
) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), bold(font_size), theme.on_accent);
    let size = galley.size() + 2.0 * padding;
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let enabled = ui.is_enabled();
        let lift = if response.hovered() && enabled {
            0.12
        } else {
            0.0
        };
        let fade = if enabled { 1.0 } else { 0.35 };
        let radius = size.y / 2.0;
        if enabled {
            // A soft glow under the button.
            let [r, g, b, _] = theme.accent.to_array();
            glow(
                ui.painter(),
                rect.center() + vec2(0.0, 6.0),
                rect.width() * 0.6,
                Color32::from_rgba_unmultiplied(r, g, b, 40),
            );
        }
        gradient(
            ui.painter(),
            rect,
            radius,
            lighten(theme.accent, lift).gamma_multiply(fade),
            lighten(theme.accent_end, lift).gamma_multiply(fade),
            vec2(1.0, 0.0),
        );
        ui.painter()
            .galley(rect.center() - galley.size() / 2.0, galley, theme.on_accent);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A pill-shaped filter chip. The selected one is filled in.
pub fn chip(ui: &mut Ui, theme: &Theme, text: &str, selected: bool) -> Response {
    let colour = if selected {
        theme.on_accent
    } else {
        theme.text
    };
    let font = if selected { bold(14.0) } else { regular(14.0) };
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, colour);
    let size = galley.size() + vec2(32.0, 16.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let radius = size.y / 2.0;
        if selected {
            painter.rect_filled(rect, radius, theme.chip_selected);
        } else {
            let fill = if response.hovered() {
                lighten(theme.panel_alt, 0.08)
            } else {
                theme.panel_alt
            };
            painter.rect_filled(rect, radius, fill);
            painter.rect_stroke(
                rect,
                radius,
                Stroke::new(1.0, theme.border),
                egui::StrokeKind::Inside,
            );
        }
        painter.galley(rect.center() - galley.size() / 2.0, galley, colour);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A small coloured pill, for badges next to the name.
pub fn badge_pill(ui: &mut Ui, text: &str, colour: Color32) -> Response {
    let text_colour = lighten(colour, 0.45);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), semibold(13.0), text_colour);
    let size = galley.size() + vec2(20.0, 10.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let [r, g, b, _] = colour.to_array();
        ui.painter().rect_filled(
            rect,
            size.y / 2.0,
            Color32::from_rgba_unmultiplied(r, g, b, 60),
        );
        ui.painter()
            .galley(rect.center() - galley.size() / 2.0, galley, text_colour);
    }
    response
}

/// A Stardust amount: the four-pointed star, then the text. The star is
/// drawn rather than typed, because not every font has that symbol.
pub fn stardust(ui: &mut Ui, theme: &Theme, text: &str, size: f32) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), bold(size), theme.stardust);
    let star = size * 0.5;
    let total = vec2(star * 2.0 + size * 0.35 + galley.size().x, galley.size().y);
    let (rect, response) = ui.allocate_exact_size(total, Sense::hover());
    if ui.is_rect_visible(rect) {
        star4(
            ui.painter(),
            pos2(rect.left() + star, rect.center().y),
            star,
            theme.stardust,
        );
        ui.painter().galley(
            pos2(rect.left() + star * 2.0 + size * 0.35, rect.top()),
            galley,
            theme.stardust,
        );
    }
    response
}

/// A button showing a Stardust amount (the balance, or a price).
pub fn stardust_button(ui: &mut Ui, theme: &Theme, text: &str, size: f32) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), bold(size), theme.stardust);
    let star = size * 0.5;
    let padding = vec2(13.0, 8.0);
    let inner = vec2(star * 2.0 + size * 0.35 + galley.size().x, galley.size().y);
    let (rect, response) = ui.allocate_exact_size(inner + 2.0 * padding, Sense::click());
    if ui.is_rect_visible(rect) {
        let enabled = ui.is_enabled();
        let fade = if enabled { 1.0 } else { 0.4 };
        let fill = if response.hovered() && enabled {
            lighten(theme.panel_alt, 0.08)
        } else {
            theme.panel_alt
        };
        let painter = ui.painter();
        painter.rect_filled(rect, 9.0, fill);
        painter.rect_stroke(
            rect,
            9.0,
            Stroke::new(1.0, theme.border),
            egui::StrokeKind::Inside,
        );
        let colour = theme.stardust.gamma_multiply(fade);
        let left = rect.left() + padding.x;
        star4(painter, pos2(left + star, rect.center().y), star, colour);
        painter.galley(
            pos2(left + star * 2.0 + size * 0.35, rect.top() + padding.y),
            galley,
            colour,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Small, spaced-out capitals: "TOTAL PLAYTIME", "COLLECTIONS".
pub fn caps(text: &str, size: f32, colour: Color32) -> egui::RichText {
    egui::RichText::new(text.to_uppercase())
        .font(semibold(size))
        .color(colour)
        .extra_letter_spacing(1.4)
}

/// Text in the muted colour.
pub fn muted(text: impl Into<String>, theme: &Theme) -> egui::RichText {
    egui::RichText::new(text).color(theme.muted)
}

/// Bold text at a given size.
pub fn strong(text: impl Into<String>, size: f32) -> egui::RichText {
    egui::RichText::new(text).font(bold(size))
}

/// Show a frame whose contents are stacked top to bottom, whatever
/// direction the surrounding layout runs in. (A plain `Frame::show`
/// inside a `ui.horizontal` would lay its contents out sideways.)
pub fn panel<R>(ui: &mut Ui, frame: egui::Frame, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    frame.show(ui, |ui| ui.vertical(add_contents).inner).inner
}

/// A boxed number with a caption above it: `PLAYTIME` / `112h 05m`.
pub fn stat_tile(ui: &mut Ui, theme: &Theme, caption: &str, value: &str, width: f32) {
    egui::Frame::new()
        .fill(theme.panel)
        .stroke(Stroke::new(1.0, theme.border))
        .corner_radius(12.0)
        .inner_margin(egui::Margin::symmetric(18, 14))
        .show(ui, |ui| {
            ui.set_width((width - 38.0).max(60.0));
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.label(caps(caption, 11.5, theme.muted));
                ui.add(egui::Label::new(strong(value, 22.0)).truncate());
            });
        });
}

/// A row of equal-width stat tiles filling the available width.
pub fn stat_row(ui: &mut Ui, theme: &Theme, tiles: &[(&str, String)]) {
    let gap = 14.0;
    let width = (ui.available_width() - gap * (tiles.len() as f32 - 1.0)) / tiles.len() as f32;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (caption, value) in tiles {
            stat_tile(ui, theme, caption, value, width);
        }
    });
}

/// A horizontal bar for the stats page: a track with a filled part.
pub fn bar(ui: &mut Ui, theme: &Theme, fraction: f32, colour: Color32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 12.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 6.0, theme.panel_alt);
    let filled = Rect::from_min_size(
        rect.min,
        vec2((rect.width() * fraction.clamp(0.0, 1.0)).max(6.0), 12.0),
    );
    painter.rect_filled(filled, 6.0, colour);
}

// --- Icons ---------------------------------------------------------------

/// The icons on the icon rail. They are drawn from simple
/// shapes rather than taken from a font, so they look the same everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Home,
    Library,
    Franchises,
    Stats,
    Shop,
    Settings,
}

pub fn icon(painter: &egui::Painter, icon: Icon, centre: Pos2, colour: Color32) {
    let stroke = Stroke::new(1.6, colour);
    let p = |x: f32, y: f32| centre + vec2(x, y);
    match icon {
        Icon::Home => {
            painter.line(vec![p(-9.0, -1.0), p(0.0, -9.0), p(9.0, -1.0)], stroke);
            painter.line(
                vec![p(-6.5, -2.0), p(-6.5, 8.0), p(6.5, 8.0), p(6.5, -2.0)],
                stroke,
            );
        }
        Icon::Library => {
            for row in 0..3 {
                for column in 0..3 {
                    let at = p(-7.0 + column as f32 * 7.0, -7.0 + row as f32 * 7.0);
                    painter.rect_filled(Rect::from_center_size(at, Vec2::splat(4.6)), 1.0, colour);
                }
            }
        }
        Icon::Franchises => {
            // Three cards fanned out, like a series on a shelf.
            for (i, x) in [-7.0_f32, 0.0, 7.0].into_iter().enumerate() {
                let rect = Rect::from_center_size(
                    p(x, 0.0),
                    vec2(5.0, 15.0 - (i as f32 - 1.0).abs() * 3.0),
                );
                painter.rect_filled(rect, 1.2, colour);
            }
        }
        Icon::Stats => {
            for (x, height) in [(-7.0, 8.0), (0.0, 16.0), (7.0, 11.0)] {
                let rect = Rect::from_min_max(p(x - 2.2, 8.0 - height), p(x + 2.2, 8.0));
                painter.rect_filled(rect, 1.0, colour);
            }
        }
        Icon::Shop => star4(painter, centre, 10.0, colour),
        Icon::Settings => {
            painter.circle_stroke(centre, 5.0, stroke);
            for i in 0..8 {
                let angle = TAU * i as f32 / 8.0;
                let direction = vec2(angle.cos(), angle.sin());
                painter.line_segment(
                    [centre + direction * 6.5, centre + direction * 9.5],
                    Stroke::new(2.4, colour),
                );
            }
        }
    }
}

/// A four-pointed star, the Stardust symbol.
pub fn star4(painter: &egui::Painter, centre: Pos2, radius: f32, colour: Color32) {
    let inner = radius * 0.28;
    let points: Vec<Pos2> = (0..8)
        .map(|i| {
            let angle = TAU * i as f32 / 8.0 - TAU / 4.0;
            let r = if i % 2 == 0 { radius } else { inner };
            centre + r * vec2(angle.cos(), angle.sin())
        })
        .collect();
    // Drawn as four triangles around the middle, because the star isn't
    // a convex shape.
    let mut mesh = Mesh::default();
    mesh.colored_vertex(centre, colour);
    for point in &points {
        mesh.colored_vertex(*point, colour);
    }
    for i in 0..8u32 {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % 8);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// A filled triangle pointing right: the "play" symbol.
pub fn play_triangle(painter: &egui::Painter, centre: Pos2, size: f32, colour: Color32) {
    let points = vec![
        centre + vec2(-size * 0.4, -size * 0.5),
        centre + vec2(size * 0.55, 0.0),
        centre + vec2(-size * 0.4, size * 0.5),
    ];
    painter.add(egui::Shape::convex_polygon(points, colour, Stroke::NONE));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What egui's file loader turns an address into on Windows (a copy of
    /// its rule, so it can be checked from any system).
    fn egui_windows_path(uri: &str) -> String {
        let rest = uri.strip_prefix("file://").expect("a file address");
        match rest.strip_prefix('/') {
            Some(path) => path.to_string(),
            None => format!("\\\\{rest}"),
        }
    }

    #[test]
    fn windows_paths_become_standard_file_addresses() {
        let path = r"C:\Users\Mo Smith\AppData\Roaming\gamelib\covers\5.jpg";
        let uri = file_uri_from_text(path);
        assert_eq!(
            uri,
            "file:///C:/Users/Mo Smith/AppData/Roaming/gamelib/covers/5.jpg"
        );
        assert_eq!(
            egui_windows_path(&uri),
            "C:/Users/Mo Smith/AppData/Roaming/gamelib/covers/5.jpg",
            "egui finds the file on the C: drive, not a network share"
        );
    }

    #[test]
    fn the_old_address_was_read_as_a_network_share() {
        // The bug this fixes: no slash after file://, so egui went looking
        // on a network share called C: and never found the cover.
        assert_eq!(
            egui_windows_path(r"file://C:\gamelib\covers\5.jpg"),
            r"\\C:\gamelib\covers\5.jpg"
        );
    }

    #[test]
    fn other_kinds_of_path() {
        assert_eq!(
            file_uri_from_text(r"\\?\C:\games\x.png"),
            "file:///C:/games/x.png"
        );
        assert_eq!(
            file_uri_from_text(r"\\nas\games\c.jpg"),
            "file://nas/games/c.jpg"
        );
        assert_eq!(
            egui_windows_path("file://nas/games/c.jpg"),
            r"\\nas/games/c.jpg"
        );
        assert_eq!(
            file_uri_from_text("/home/me/covers/5.jpg"),
            "file:///home/me/covers/5.jpg"
        );
    }
}
