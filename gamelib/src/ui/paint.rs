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

    /// Paint the sky over `rect`. `time` (seconds) makes the stars twinkle;
    /// pass `None` for a still sky.
    pub fn paint(&self, painter: &egui::Painter, rect: Rect, time: Option<f64>) {
        let at = |x: f32, y: f32| {
            pos2(
                rect.left() + x * rect.width(),
                rect.top() + y * rect.height(),
            )
        };
        let size = rect.width().max(rect.height());

        // Nebula: big, faint glows of purple and teal.
        glow(
            painter,
            at(0.12, 0.05),
            size * 0.62,
            Color32::from_rgba_unmultiplied(98, 64, 200, 105),
        );
        glow(
            painter,
            at(0.95, 0.95),
            size * 0.55,
            Color32::from_rgba_unmultiplied(20, 120, 150, 85),
        );
        glow(
            painter,
            at(0.55, 0.45),
            size * 0.35,
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

        for star in &self.stars {
            let twinkle = match time {
                Some(t) => 0.75 + 0.25 * ((t as f32) * 1.3 + star.phase).sin(),
                None => 1.0,
            };
            let alpha = (star.brightness * twinkle * 255.0) as u8;
            painter.circle_filled(
                at(star.x, star.y),
                star.size,
                Color32::from_rgba_unmultiplied(223, 232, 255, alpha),
            );
        }
    }
}

/// Paint the window's background for the current theme.
pub fn backdrop(
    painter: &egui::Painter,
    rect: Rect,
    theme: &Theme,
    stars: &StarMap,
    time: Option<f64>,
) {
    painter.rect_filled(rect, 0.0, theme.bg);
    match theme.backdrop {
        Backdrop::Plain => {}
        Backdrop::Constellation => stars.paint(painter, rect, time),
        Backdrop::Aurora => aurora(painter, rect),
        Backdrop::Synthwave => synthwave(painter, rect),
    }
}

/// Green and violet bands, like the northern lights.
fn aurora(painter: &egui::Painter, rect: Rect) {
    let size = rect.width().max(rect.height());
    let at = |x: f32, y: f32| {
        pos2(
            rect.left() + x * rect.width(),
            rect.top() + y * rect.height(),
        )
    };
    for (x, y, radius, colour) in [
        (0.15, 0.25, 0.55, (38, 190, 130, 80)),
        (0.45, 0.10, 0.45, (40, 150, 170, 60)),
        (0.80, 0.30, 0.55, (120, 80, 220, 80)),
        (0.60, 0.85, 0.50, (60, 60, 160, 50)),
    ] {
        let (r, g, b, a) = colour;
        glow(
            painter,
            at(x, y),
            size * radius,
            Color32::from_rgba_unmultiplied(r, g, b, a),
        );
    }
}

/// A pink horizon with a grid running towards it.
fn synthwave(painter: &egui::Painter, rect: Rect) {
    let horizon = rect.top() + rect.height() * 0.58;
    let sky = Rect::from_min_max(rect.min, pos2(rect.right(), horizon));
    gradient(
        painter,
        sky,
        0.0,
        Color32::from_rgb(20, 6, 38),
        Color32::from_rgb(150, 30, 110),
        vec2(0.0, 1.0),
    );
    glow(
        painter,
        pos2(rect.center().x, horizon),
        rect.width() * 0.35,
        Color32::from_rgba_unmultiplied(255, 90, 170, 110),
    );
    let ground = Rect::from_min_max(pos2(rect.left(), horizon), rect.max);
    painter.rect_filled(ground, 0.0, Color32::from_rgb(14, 4, 28));
    let line = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 80, 190, 70));
    // Lines fanning out from the vanishing point…
    for i in -12..=12 {
        let x = rect.center().x + i as f32 * rect.width() * 0.14;
        painter.line_segment(
            [
                pos2(rect.center().x + i as f32 * 14.0, horizon),
                pos2(x, rect.bottom()),
            ],
            line,
        );
    }
    // …and lines across, closer together near the horizon.
    for i in 1..=9 {
        let t = (i as f32 / 9.0).powi(2);
        let y = horizon + t * ground.height();
        painter.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], line);
    }
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
    format!("file://{}", path.display())
}

/// Paint a game's cover into `rect`: its image if it has one, otherwise a
/// gradient with the title on it. `title_size` 0 leaves the title off.
pub fn cover(ui: &Ui, rect: Rect, game: &Game, radius: f32, title_size: f32) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    match game.cover_path.as_deref().filter(|path| path.is_file()) {
        Some(path) => {
            egui::Image::from_uri(file_uri(path))
                .corner_radius(radius)
                .paint_at(ui, rect);
        }
        None => {
            let (top, bottom) = title_colours(&game.title);
            gradient(painter, rect, radius, top, bottom, vec2(0.25, 1.0));
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
    match banner {
        Some(Banner::Upload(path)) if path.is_file() => {
            // Fill the rectangle with the middle of the picture.
            egui::Image::from_uri(file_uri(path))
                .corner_radius(radius)
                .fit_to_exact_size(rect.size())
                .maintain_aspect_ratio(false)
                .paint_at(ui, rect);
        }
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
    match picture.filter(|p| p.is_file()) {
        Some(path) => {
            egui::Image::from_uri(file_uri(path))
                .corner_radius(radius)
                .fit_to_exact_size(rect.size())
                .maintain_aspect_ratio(false)
                .paint_at(ui, rect);
        }
        None => {
            let (from, to) = if theme.rail {
                (theme.accent, theme.accent_end)
            } else {
                (
                    lighten(theme.accent, 0.1),
                    Color32::from_rgb(0xe0, 0x5a, 0x1a),
                )
            };
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
        let radius = if theme.rail { size.y / 2.0 } else { 9.0 };
        if theme.rail && enabled {
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

/// The icons on the Constellation icon rail. They are drawn from simple
/// shapes rather than taken from a font, so they look the same everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Home,
    Library,
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
