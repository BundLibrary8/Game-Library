//! Colours, fonts and the two looks of the app.
//!
//! - **Ember** is the default: dark panels with an amber accent, a top
//!   bar and a sidebar.
//! - **Constellation** is used while the Constellation library background
//!   is equipped: a star map behind see-through "glass" panels, an icon
//!   rail instead of the sidebar, and an ice-blue to violet accent.
//!
//! Every screen asks the [`Theme`] for its colours, so the same screen
//! code draws both looks.

use std::sync::Arc;

use eframe::egui::{self, Color32, FontFamily, FontId};
use gamelib::model::Source;

/// What is painted behind everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backdrop {
    Plain,
    Constellation,
    Aurora,
    Synthwave,
}

impl Backdrop {
    /// The backdrop for an equipped library-background item.
    pub fn from_item(item_id: Option<&str>) -> Backdrop {
        match item_id {
            Some("bg_constellation") => Backdrop::Constellation,
            Some("bg_aurora") => Backdrop::Aurora,
            Some("bg_synthwave_grid") => Backdrop::Synthwave,
            _ => Backdrop::Plain,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub backdrop: Backdrop,
    /// True for the Constellation look: icon rail, no top bar.
    pub rail: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub panel_alt: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    /// The accent runs from `accent` to `accent_end` on big buttons.
    pub accent: Color32,
    pub accent_end: Color32,
    /// Text drawn on top of the accent.
    pub on_accent: Color32,
    /// Fill of the selected filter chip.
    pub chip_selected: Color32,
    pub success: Color32,
    pub danger: Color32,
    /// The colour of Stardust amounts.
    pub stardust: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

fn rgba(hex: u32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, alpha)
}

impl Theme {
    pub fn ember() -> Theme {
        Theme {
            backdrop: Backdrop::Plain,
            rail: false,
            bg: rgb(0x0f1115),
            panel: rgb(0x161920),
            panel_alt: rgb(0x1d212a),
            border: rgb(0x2a2f3a),
            text: rgb(0xe8eaf0),
            muted: rgb(0x8b93a7),
            accent: rgb(0xf5a524),
            accent_end: rgb(0xf5a524),
            on_accent: rgb(0x1a1204),
            chip_selected: rgb(0xe8eaf0),
            success: rgb(0x4cc38a),
            danger: rgb(0xef5f5f),
            stardust: rgb(0xa99bff),
        }
    }

    pub fn constellation() -> Theme {
        Theme {
            backdrop: Backdrop::Constellation,
            rail: true,
            bg: rgb(0x060914),
            panel: rgba(0x0e142a, 150),
            panel_alt: rgba(0x1c2648, 170),
            border: rgba(0x96b4ff, 40),
            text: rgb(0xeef2ff),
            muted: rgb(0x8d98bd),
            accent: rgb(0x7ad7ff),
            accent_end: rgb(0xb18cff),
            on_accent: rgb(0x0b1020),
            chip_selected: rgb(0x7ad7ff),
            success: rgb(0x5fe0a0),
            danger: rgb(0xff7b8a),
            stardust: rgb(0xb18cff),
        }
    }

    pub fn for_backdrop(backdrop: Backdrop) -> Theme {
        match backdrop {
            Backdrop::Constellation => Theme::constellation(),
            Backdrop::Plain => Theme::ember(),
            // The other backgrounds keep Ember's layout and colours, with
            // slightly see-through panels so the picture shows.
            Backdrop::Aurora | Backdrop::Synthwave => Theme {
                backdrop,
                panel: rgba(0x161920, 205),
                panel_alt: rgba(0x1d212a, 225),
                ..Theme::ember()
            },
        }
    }

    /// Tell egui about the colours, so ordinary buttons, text boxes and
    /// menus match the theme.
    pub fn apply(&self, ctx: &egui::Context) {
        let mut style = (*ctx.global_style()).clone();
        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = Color32::TRANSPARENT;
        v.window_fill = self.opaque_panel();
        v.window_stroke = egui::Stroke::new(1.0, self.border);
        v.window_corner_radius = 14.0.into();
        v.menu_corner_radius = 10.0.into();
        v.extreme_bg_color = self.field_fill();
        v.faint_bg_color = self.panel_alt;
        v.override_text_color = Some(self.text);
        v.hyperlink_color = self.accent;
        v.selection.bg_fill = self.accent.gamma_multiply(0.35);
        v.selection.stroke = egui::Stroke::new(1.0, self.accent);

        let radius: egui::CornerRadius = 9.0.into();
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = self.panel;
        w.noninteractive.bg_stroke = egui::Stroke::new(1.0, self.border);
        w.noninteractive.fg_stroke = egui::Stroke::new(1.0, self.text);
        w.noninteractive.corner_radius = radius;
        for (state, lift) in [
            (&mut w.inactive, 0.0),
            (&mut w.hovered, 0.08),
            (&mut w.active, 0.14),
            (&mut w.open, 0.08),
        ] {
            let fill = lighten(self.opaque(self.panel_alt), lift);
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.bg_stroke = egui::Stroke::new(1.0, lighten(self.opaque(self.border), lift));
            state.fg_stroke = egui::Stroke::new(1.0, self.text);
            state.corner_radius = radius;
            state.expansion = 0.0;
        }

        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 7.0);
        style.spacing.interact_size.y = 30.0;
        style.spacing.menu_margin = egui::Margin::same(8);
        style.spacing.window_margin = egui::Margin::same(20);
        style.text_styles = [
            (egui::TextStyle::Small, regular(12.0)),
            (egui::TextStyle::Body, regular(15.0)),
            (egui::TextStyle::Button, semibold(14.5)),
            (egui::TextStyle::Heading, bold(26.0)),
            (egui::TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        // Same style whether Windows is set to light or dark mode.
        ctx.all_styles_mut(|s| *s = style.clone());
    }

    /// The fill of text boxes.
    pub fn field_fill(&self) -> Color32 {
        if self.rail {
            rgba(0x0a1022, 200)
        } else {
            rgb(0x12151b)
        }
    }

    /// The panel colour with no see-through, for pop-up windows (which
    /// would be unreadable with the page showing through).
    pub fn opaque_panel(&self) -> Color32 {
        self.opaque(self.panel)
    }

    /// A see-through colour as it looks over the background colour.
    pub fn opaque(&self, colour: Color32) -> Color32 {
        let [r, g, b, a] = colour.to_srgba_unmultiplied();
        let alpha = a as f32 / 255.0;
        let mix = |over: u8, under: u8| (over as f32 * alpha + under as f32 * (1.0 - alpha)) as u8;
        Color32::from_rgb(
            mix(r, self.bg.r()),
            mix(g, self.bg.g()),
            mix(b, self.bg.b()),
        )
    }

    /// A rounded panel: glass in Constellation, solid in Ember.
    pub fn panel_frame(&self) -> egui::Frame {
        egui::Frame::new()
            .fill(self.panel)
            .stroke(egui::Stroke::new(1.0, self.border))
            .corner_radius(14.0)
            .inner_margin(18.0)
    }
}

/// Mix a colour towards white. `amount` is 0.0 (unchanged) to 1.0 (white).
pub fn lighten(colour: Color32, amount: f32) -> Color32 {
    let mix = |c: u8| (c as f32 + (255.0 - c as f32) * amount) as u8;
    Color32::from_rgba_premultiplied(
        mix(colour.r()),
        mix(colour.g()),
        mix(colour.b()),
        colour.a(),
    )
}

/// Mix a colour towards black. `amount` is 0.0 (unchanged) to 1.0 (black).
pub fn darken(colour: Color32, amount: f32) -> Color32 {
    let mix = |c: u8| (c as f32 * (1.0 - amount)) as u8;
    Color32::from_rgb(mix(colour.r()), mix(colour.g()), mix(colour.b()))
}

/// Blend from one colour to another. `t` is 0.0 (all `a`) to 1.0 (all `b`).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let channel = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(
        channel(a.r(), b.r()),
        channel(a.g(), b.g()),
        channel(a.b(), b.b()),
        channel(a.a(), b.a()),
    )
}

// --- Fonts ---------------------------------------------------------------

const SEMIBOLD: &str = "inter-semibold";
const BOLD: &str = "inter-bold";

/// Load the Inter font (bundled in `assets/fonts/`) in three weights.
/// egui's own fonts stay as fallbacks, for symbols Inter doesn't have.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let fallbacks = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    let weights: [(&str, &'static [u8], FontFamily); 3] = [
        (
            "inter-regular",
            include_bytes!("../../assets/fonts/Inter-Regular.otf"),
            FontFamily::Proportional,
        ),
        (
            SEMIBOLD,
            include_bytes!("../../assets/fonts/Inter-SemiBold.otf"),
            FontFamily::Name(SEMIBOLD.into()),
        ),
        (
            BOLD,
            include_bytes!("../../assets/fonts/Inter-Bold.otf"),
            FontFamily::Name(BOLD.into()),
        ),
    ];
    for (name, bytes, family) in weights {
        fonts.font_data.insert(
            name.to_string(),
            Arc::new(egui::FontData::from_static(bytes)),
        );
        let mut list = vec![name.to_string()];
        list.extend(fallbacks.iter().cloned());
        fonts.families.insert(family, list);
    }
    ctx.set_fonts(fonts);
}

pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}

// --- Sources -------------------------------------------------------------

pub fn source_name(source: Source) -> &'static str {
    match source {
        Source::Steam => "Steam",
        Source::Epic => "Epic Games",
        Source::Gog => "GOG",
        Source::Folder => "Local folders",
        Source::Manual => "Added by hand",
    }
}

/// The short label on a cover's badge.
pub fn source_badge(source: Source) -> &'static str {
    match source {
        Source::Steam => "STEAM",
        Source::Epic => "EPIC",
        Source::Gog => "GOG",
        Source::Folder | Source::Manual => "LOCAL",
    }
}

/// Each source has its own colour dot, used the same way everywhere.
pub fn source_colour(source: Source) -> Color32 {
    match source {
        Source::Steam => rgb(0x4f8dfd),
        Source::Epic => rgb(0xc9ced9),
        Source::Gog => rgb(0xa970ff),
        Source::Folder => rgb(0x4cc38a),
        Source::Manual => rgb(0x2fb7a4),
    }
}
