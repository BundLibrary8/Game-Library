//! Couch Mode: a full-screen, controller-friendly way to browse and play,
//! for a PC plugged into a TV.
//!
//! - The selected game's art fills the background, with its title, its
//!   playtime and a big Play button on the left.
//! - A row of covers runs along the bottom. Left and right move through
//!   them; the selected cover grows and gets a white outline.
//! - Tabs along the top (Home, Library, Collections, Search) are switched
//!   with the shoulder buttons.
//! - A plays, Y shows details, X favourites, B goes back.
//!
//! Everything also works with a keyboard: arrows, Enter, Esc, Q/E for the
//! tabs, I for details, F for favourite. Controllers are read with the
//! `gilrs` library, which egui doesn't include.

use eframe::egui::{self, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use gamelib::db::GameQuery;
use gamelib::model::Game;
use gamelib::now;

use super::theme::{bold, regular, semibold, source_name};
use super::{GameLibApp, paint};
use crate::text::{format_last_played, played_label};

/// One press of each thing a player can do, gathered from the controller
/// and the keyboard for this frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    /// A / Enter: play, or choose.
    pub a: bool,
    /// B / Esc: back.
    pub b: bool,
    /// X / F: favourite.
    pub x: bool,
    /// Y / I: details.
    pub y: bool,
    /// Shoulder buttons / Q and E: previous and next tab.
    pub lb: bool,
    pub rb: bool,
    /// Start (or the Xbox button) / F11: go in or out of Couch Mode.
    pub start: bool,
}

impl Input {
    pub fn merge(self, other: Input) -> Input {
        Input {
            left: self.left || other.left,
            right: self.right || other.right,
            up: self.up || other.up,
            down: self.down || other.down,
            a: self.a || other.a,
            b: self.b || other.b,
            x: self.x || other.x,
            y: self.y || other.y,
            lb: self.lb || other.lb,
            rb: self.rb || other.rb,
            start: self.start || other.start,
        }
    }
}

/// How long a direction must be held before it starts repeating, and then
/// how often it repeats (seconds).
const REPEAT_DELAY: f64 = 0.4;
const REPEAT_EVERY: f64 = 0.11;
/// How far the stick must be pushed to count.
const STICK_THRESHOLD: f32 = 0.6;

/// Turns "this direction is held" into single steps, repeating while held,
/// like a keyboard does.
#[derive(Debug, Default)]
struct Repeater {
    held: i8,
    next: f64,
}

impl Repeater {
    /// `direction` is -1, 0 or 1. Returns the step to take this frame.
    fn step(&mut self, direction: i8, time: f64) -> i8 {
        if direction == 0 {
            self.held = 0;
            return 0;
        }
        if direction != self.held {
            self.held = direction;
            self.next = time + REPEAT_DELAY;
            return direction;
        }
        if time >= self.next {
            self.next = time + REPEAT_EVERY;
            return direction;
        }
        0
    }
}

/// Game controllers.
pub struct Pad {
    gilrs: Option<gilrs::Gilrs>,
    horizontal: Repeater,
    vertical: Repeater,
}

impl Pad {
    pub fn new() -> Pad {
        let gilrs = match gilrs::Gilrs::new() {
            Ok(gilrs) => Some(gilrs),
            Err(err) => {
                log::info!("controllers unavailable: {err}");
                None
            }
        };
        Pad {
            gilrs,
            horizontal: Repeater::default(),
            vertical: Repeater::default(),
        }
    }

    /// The controllers connected now: name, and battery if known.
    pub fn list(&self) -> Vec<(String, Option<String>)> {
        let Some(gilrs) = &self.gilrs else {
            return Vec::new();
        };
        gilrs
            .gamepads()
            .filter(|(_, pad)| pad.is_connected())
            .map(|(_, pad)| {
                let power = match pad.power_info() {
                    gilrs::PowerInfo::Wired => Some("Wired".to_string()),
                    gilrs::PowerInfo::Discharging(level) => Some(format!("Battery {level}%")),
                    gilrs::PowerInfo::Charging(level) => Some(format!("Charging, {level}%")),
                    gilrs::PowerInfo::Charged => Some("Fully charged".to_string()),
                    gilrs::PowerInfo::Unknown => None,
                };
                (pad.name().to_string(), power)
            })
            .collect()
    }

    /// Is any controller plugged in?
    pub fn connected(&self) -> bool {
        self.gilrs
            .as_ref()
            .is_some_and(|g| g.gamepads().any(|(_, pad)| pad.is_connected()))
    }

    /// What the controllers did since the last frame.
    pub fn poll(&mut self, time: f64) -> Input {
        use gilrs::{Axis, Button, EventType};
        let Some(gilrs) = self.gilrs.as_mut() else {
            return Input::default();
        };
        let mut input = Input::default();
        while let Some(event) = gilrs.next_event() {
            if let EventType::ButtonPressed(button, _) = event.event {
                match button {
                    Button::South => input.a = true,
                    Button::East => input.b = true,
                    Button::West => input.x = true,
                    Button::North => input.y = true,
                    Button::LeftTrigger => input.lb = true,
                    Button::RightTrigger => input.rb = true,
                    Button::Start | Button::Mode => input.start = true,
                    _ => {}
                }
            }
        }
        // Directions are read from the current state, so holding the
        // D-pad or stick repeats.
        let (mut h, mut v) = (0i8, 0i8);
        for (_, pad) in gilrs.gamepads() {
            let x = pad.value(Axis::LeftStickX);
            let y = pad.value(Axis::LeftStickY);
            if pad.is_pressed(Button::DPadLeft) || x < -STICK_THRESHOLD {
                h = -1;
            } else if pad.is_pressed(Button::DPadRight) || x > STICK_THRESHOLD {
                h = 1;
            }
            if pad.is_pressed(Button::DPadUp) || y > STICK_THRESHOLD {
                v = -1;
            } else if pad.is_pressed(Button::DPadDown) || y < -STICK_THRESHOLD {
                v = 1;
            }
        }
        match self.horizontal.step(h, time) {
            -1 => input.left = true,
            1 => input.right = true,
            _ => {}
        }
        match self.vertical.step(v, time) {
            -1 => input.up = true,
            1 => input.down = true,
            _ => {}
        }
        input
    }
}

/// The keyboard's version of the controller, for this frame. In Search,
/// letters are typed rather than used as shortcuts.
pub fn keyboard_input(ctx: &egui::Context, typing: bool) -> Input {
    ctx.input(|i| {
        let key = |k: Key| i.key_pressed(k);
        Input {
            left: key(Key::ArrowLeft),
            right: key(Key::ArrowRight),
            up: key(Key::ArrowUp),
            down: key(Key::ArrowDown),
            a: key(Key::Enter) || (!typing && key(Key::Space)),
            b: key(Key::Escape),
            x: !typing && key(Key::F),
            y: !typing && key(Key::I),
            lb: (!typing && key(Key::Q))
                || key(Key::PageUp)
                || (key(Key::Tab) && i.modifiers.shift),
            rb: (!typing && key(Key::E))
                || key(Key::PageDown)
                || (key(Key::Tab) && !i.modifiers.shift),
            start: key(Key::F11),
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Home,
    Library,
    Collections,
    Search,
}

const TABS: [(Tab, &str); 4] = [
    (Tab::Home, "Home"),
    (Tab::Library, "Library"),
    (Tab::Collections, "Collections"),
    (Tab::Search, "Search"),
];

/// The letters on the on-screen keyboard in Search.
const KEYS: &[&str] = &[
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "Space",
    "Delete", "Clear",
];

/// One thing in the row along the bottom.
#[derive(Debug, Clone)]
enum Item {
    Game(Box<Game>),
    /// A collection: its tag id, name and how many games are in it.
    Collection(i64, String, u64),
}

/// Where Couch Mode is up to.
pub struct Couch {
    tab: Tab,
    /// The selected position in each tab's row.
    selected: [usize; 4],
    /// The collection opened from the Collections tab, and its games.
    open_collection: Option<(String, Vec<Game>)>,
    collection_selected: usize,
    details: bool,
    query: String,
    /// In Search: is the on-screen keyboard (rather than the row) in use?
    on_keyboard: bool,
    key: usize,
    /// When the selection last changed, for fading the background in.
    changed_at: f64,
}

impl Couch {
    pub fn new() -> Couch {
        Couch {
            tab: Tab::Home,
            selected: [0; 4],
            open_collection: None,
            collection_selected: 0,
            details: false,
            query: String::new(),
            on_keyboard: false,
            key: 0,
            changed_at: 0.0,
        }
    }

    /// Is the player typing (Search), so letter keys aren't shortcuts?
    pub fn typing(&self) -> bool {
        self.tab == Tab::Search
    }

    fn tab_index(&self) -> usize {
        TABS.iter()
            .position(|(tab, _)| *tab == self.tab)
            .unwrap_or(0)
    }

    fn selection(&mut self) -> &mut usize {
        if self.tab == Tab::Collections && self.open_collection.is_some() {
            &mut self.collection_selected
        } else {
            let index = self.tab_index();
            &mut self.selected[index]
        }
    }
}

/// What Couch Mode asks the app to do.
enum Outcome {
    Stay,
    Leave,
}

impl GameLibApp {
    /// Go into Couch Mode: full screen, controller-friendly.
    pub fn enter_couch(&mut self, ctx: &egui::Context) {
        self.couch = Some(Couch::new());
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
    }

    pub fn leave_couch(&mut self, ctx: &egui::Context) {
        self.couch = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
    }

    /// The row of things to choose from on the current tab.
    fn couch_items(&self, couch: &Couch) -> Vec<Item> {
        let games = &self.data.visible;
        let mut list: Vec<Game> = match couch.tab {
            Tab::Home => {
                // Most recently played first, then the newest additions.
                let mut list = games.clone();
                list.sort_by_key(|g| std::cmp::Reverse((g.last_played.unwrap_or(0), g.added_at)));
                list
            }
            Tab::Library => {
                let mut list = games.clone();
                list.sort_by_key(|g| g.title.to_lowercase());
                list
            }
            Tab::Collections => match &couch.open_collection {
                Some((_, games)) => games.clone(),
                None => {
                    return self
                        .data
                        .collections()
                        .map(|(tag, count)| Item::Collection(tag.id, tag.name.clone(), *count))
                        .collect();
                }
            },
            Tab::Search => {
                let query = couch.query.trim().to_lowercase();
                games
                    .iter()
                    .filter(|g| !query.is_empty() && g.title.to_lowercase().contains(&query))
                    .cloned()
                    .collect()
            }
        };
        if couch.tab == Tab::Library {
            list.dedup_by_key(|g| g.id);
        }
        list.into_iter()
            .map(|game| Item::Game(Box::new(game)))
            .collect()
    }

    /// Draw Couch Mode over the whole window and act on the player's input.
    pub fn couch_screen(&mut self, ui: &mut egui::Ui, input: Input) {
        let Some(mut couch) = self.couch.take() else {
            return;
        };
        let ctx = ui.ctx().clone();
        let time = ctx.input(|i| i.time);

        // Typed text goes into Search.
        if couch.tab == Tab::Search {
            let (typed, backspace) = ctx.input(|i| {
                let typed: String = i
                    .events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Text(text) => Some(text.clone()),
                        _ => None,
                    })
                    .collect();
                (typed, i.key_pressed(Key::Backspace))
            });
            if !typed.is_empty() {
                couch.query.push_str(&typed);
                couch.selected[3] = 0;
            }
            if backspace {
                couch.query.pop();
            }
        }

        let items = self.couch_items(&couch);
        let count = items.len();
        {
            let selected = couch.selection();
            if *selected >= count {
                *selected = count.saturating_sub(1);
            }
        }
        let outcome = self.couch_handle_input(&mut couch, &items, input, time);
        // The input may have changed what's shown.
        let items = self.couch_items(&couch);
        let selected = (*couch.selection()).min(items.len().saturating_sub(1));
        self.couch_draw(ui, &mut couch, &items, selected, time);

        match outcome {
            Outcome::Stay => self.couch = Some(couch),
            Outcome::Leave => {
                self.couch = Some(couch);
                self.leave_couch(&ctx);
            }
        }
        // Keep animations and controller polling smooth.
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }

    fn couch_handle_input(
        &mut self,
        couch: &mut Couch,
        items: &[Item],
        input: Input,
        time: f64,
    ) -> Outcome {
        let count = items.len();
        let current = items.get(*couch.selection()).cloned();

        // Details open: only play, favourite and close.
        if couch.details {
            if input.b || input.y {
                couch.details = false;
            }
            if let Some(Item::Game(game)) = &current {
                if input.a && game.installed {
                    self.play(game.id);
                    couch.details = false;
                }
                if input.x {
                    self.check(self.db.set_favourite(game.id, !game.favourite));
                }
            }
            return Outcome::Stay;
        }

        if input.start {
            return Outcome::Leave;
        }

        // Tabs.
        if input.lb || input.rb {
            let index = couch.tab_index();
            let next = if input.rb {
                (index + 1) % TABS.len()
            } else {
                (index + TABS.len() - 1) % TABS.len()
            };
            couch.tab = TABS[next].0;
            couch.on_keyboard = couch.tab == Tab::Search && couch.query.is_empty();
            couch.changed_at = time;
            return Outcome::Stay;
        }

        // The on-screen keyboard in Search.
        if couch.tab == Tab::Search && couch.on_keyboard {
            if input.left {
                couch.key = (couch.key + KEYS.len() - 1) % KEYS.len();
            }
            if input.right {
                couch.key = (couch.key + 1) % KEYS.len();
            }
            if input.a {
                match KEYS[couch.key] {
                    "Space" => couch.query.push(' '),
                    "Delete" => {
                        couch.query.pop();
                    }
                    "Clear" => couch.query.clear(),
                    letter => couch.query.push_str(&letter.to_lowercase()),
                }
                couch.selected[3] = 0;
            }
            if input.down && count > 0 {
                couch.on_keyboard = false;
            }
            if input.b {
                if couch.query.is_empty() {
                    return Outcome::Leave;
                }
                couch.query.pop();
            }
            return Outcome::Stay;
        }

        let before = *couch.selection();
        if input.left && before > 0 {
            *couch.selection() = before - 1;
        }
        if input.right && before + 1 < count {
            *couch.selection() = before + 1;
        }
        if *couch.selection() != before {
            couch.changed_at = time;
        }
        if input.up && couch.tab == Tab::Search {
            couch.on_keyboard = true;
        }

        match &current {
            Some(Item::Game(game)) => {
                if input.a && game.installed && !self.running.contains(&game.id) {
                    self.play(game.id);
                }
                if input.y {
                    couch.details = true;
                }
                if input.x {
                    self.check(self.db.set_favourite(game.id, !game.favourite));
                }
            }
            Some(Item::Collection(tag_id, name, _)) if input.a => {
                let games = self
                    .db
                    .query_games(&GameQuery {
                        tag_id: Some(*tag_id),
                        ..GameQuery::default()
                    })
                    .unwrap_or_default();
                couch.open_collection = Some((name.clone(), games));
                couch.collection_selected = 0;
                couch.changed_at = time;
                return Outcome::Stay;
            }
            _ => {}
        }

        if input.b {
            if couch.open_collection.is_some() {
                couch.open_collection = None;
                couch.changed_at = time;
            } else {
                return Outcome::Leave;
            }
        }
        Outcome::Stay
    }

    fn couch_draw(
        &mut self,
        ui: &mut egui::Ui,
        couch: &mut Couch,
        items: &[Item],
        selected: usize,
        time: f64,
    ) {
        let screen = ui.max_rect();
        let (w, h) = (screen.width(), screen.height());
        let painter = ui.painter().clone();
        let margin = w * 0.045;
        let current = items.get(selected);

        // --- Background: the selected game's colours and art.
        painter.rect_filled(screen, 0.0, Color32::from_rgb(8, 4, 14));
        let fade = ((time - couch.changed_at) / 0.35).clamp(0.0, 1.0) as f32;
        let (bright, dark) = match current {
            Some(Item::Game(game)) => paint::title_colours(&game.title),
            Some(Item::Collection(_, name, _)) => paint::title_colours(name),
            None => (
                Color32::from_rgb(90, 40, 140),
                Color32::from_rgb(20, 10, 40),
            ),
        };
        let glow = |colour: Color32, alpha: f32| {
            let [r, g, b, _] = colour.to_array();
            Color32::from_rgba_unmultiplied(r, g, b, (alpha * fade) as u8)
        };
        paint::glow(
            &painter,
            pos2(screen.left() + w * 0.66, screen.top() + h * 0.32),
            w * 0.55,
            glow(bright, 150.0),
        );
        paint::glow(
            &painter,
            pos2(screen.right(), screen.top()),
            w * 0.5,
            glow(dark, 120.0),
        );
        if let Some(Item::Game(game)) = current
            && let Some(cover) = game.cover_path.as_deref()
        {
            // The cover, tall, on the right, fading into the dark on its left.
            let art_h = h;
            let art = Rect::from_min_size(
                pos2(screen.right() - art_h * 0.667 * 1.1, screen.top()),
                vec2(art_h * 0.667 * 1.1, art_h),
            );
            let tint = Color32::from_white_alpha((90.0 * fade) as u8);
            paint::file_image_tinted(ui, cover, art, 0.0, tint);
            let edge = Rect::from_min_size(art.min, vec2(art.width() * 0.6, art.height()));
            paint::gradient(
                &painter,
                edge,
                0.0,
                Color32::from_rgb(8, 4, 14),
                Color32::TRANSPARENT,
                vec2(1.0, 0.0),
            );
        }
        // Darken the bottom so the row of covers stands out.
        let bottom = Rect::from_min_max(pos2(screen.left(), screen.top() + h * 0.55), screen.max);
        paint::gradient(
            &painter,
            bottom,
            0.0,
            Color32::TRANSPARENT,
            Color32::from_black_alpha(200),
            vec2(0.0, 1.0),
        );

        // --- Top bar: name, tabs, clock.
        let top = screen.top() + h * 0.05;
        let name = painter.text(
            pos2(margin, top),
            egui::Align2::LEFT_CENTER,
            "GameLib",
            bold(h * 0.026),
            Color32::WHITE,
        );
        painter.text(
            pos2(name.right() + 12.0, top),
            egui::Align2::LEFT_CENTER,
            "COUCH MODE",
            semibold(h * 0.014),
            Color32::from_white_alpha(150),
        );
        let mut x = screen.left() + w * 0.32;
        for (tab, label) in TABS {
            let active = tab == couch.tab;
            let colour = if active {
                Color32::WHITE
            } else {
                Color32::from_white_alpha(140)
            };
            let rect = painter.text(
                pos2(x, top),
                egui::Align2::LEFT_CENTER,
                label,
                semibold(h * 0.02),
                colour,
            );
            if active {
                painter.line_segment(
                    [
                        pos2(rect.left(), rect.bottom() + 6.0),
                        pos2(rect.right(), rect.bottom() + 6.0),
                    ],
                    Stroke::new(3.0, Color32::WHITE),
                );
            }
            x = rect.right() + w * 0.025;
        }
        let clock = chrono::Local::now().format("%H:%M").to_string();
        painter.text(
            pos2(screen.right() - margin, top),
            egui::Align2::RIGHT_CENTER,
            clock,
            regular(h * 0.02),
            Color32::from_white_alpha(200),
        );

        // --- The selected thing, large, on the left.
        // In Search, the search box and keyboard take the top of the screen.
        let searching = couch.tab == Tab::Search;
        let text_top = screen.top() + if searching { h * 0.37 } else { h * 0.2 };
        let title_size = if searching { h * 0.055 } else { h * 0.075 };
        let max_text = w * 0.5;
        match current {
            Some(Item::Game(game)) => {
                let mut facts = vec![source_name(game.source).to_uppercase()];
                facts.extend(game.genres.iter().take(2).map(|g| g.to_uppercase()));
                caps_line(
                    &painter,
                    pos2(margin, text_top),
                    &facts.join("  ·  "),
                    h * 0.015,
                );
                let title = painter.layout(
                    game.title.clone(),
                    bold(title_size),
                    Color32::WHITE,
                    max_text,
                );
                let title_h = title.size().y;
                painter.galley(pos2(margin, text_top + h * 0.03), title, Color32::WHITE);
                let info_y = text_top + h * 0.045 + title_h;
                let mut info = played_label(game.playtime_secs);
                if game.last_played.is_some() {
                    info.push_str(&format!(
                        " · Last played {}",
                        format_last_played(game.last_played, now()).to_lowercase()
                    ));
                }
                if game.favourite {
                    info.push_str(" · ★ Favourite");
                }
                painter.text(
                    pos2(margin, info_y),
                    egui::Align2::LEFT_TOP,
                    info,
                    regular(h * 0.02),
                    Color32::from_white_alpha(210),
                );

                if !searching {
                    // Play and Details buttons (also clickable with a mouse).
                    let buttons_y = info_y + h * 0.06;
                    let running = self.running.contains(&game.id);
                    let play_label = if running {
                        "Running…"
                    } else if game.installed {
                        "▶  Play"
                    } else {
                        "Not installed"
                    };
                    let play =
                        Rect::from_min_size(pos2(margin, buttons_y), vec2(h * 0.17, h * 0.068));
                    painter.rect_filled(
                        play,
                        10.0,
                        if game.installed {
                            Color32::WHITE
                        } else {
                            Color32::from_white_alpha(90)
                        },
                    );
                    painter.text(
                        play.center(),
                        egui::Align2::CENTER_CENTER,
                        play_label,
                        bold(h * 0.022),
                        Color32::from_rgb(20, 10, 30),
                    );
                    let details = Rect::from_min_size(
                        pos2(play.right() + 14.0, buttons_y),
                        vec2(h * 0.13, h * 0.068),
                    );
                    painter.rect_filled(details, 10.0, Color32::from_white_alpha(30));
                    painter.text(
                        details.center(),
                        egui::Align2::CENTER_CENTER,
                        "Details",
                        semibold(h * 0.02),
                        Color32::WHITE,
                    );
                    if ui
                        .interact(play, ui.scope_id().with("couch-play"), Sense::click())
                        .clicked()
                        && game.installed
                        && !running
                    {
                        self.play(game.id);
                    }
                    if ui
                        .interact(details, ui.scope_id().with("couch-details"), Sense::click())
                        .clicked()
                    {
                        couch.details = true;
                    }
                }
            }
            Some(Item::Collection(_, name, games)) => {
                caps_line(&painter, pos2(margin, text_top), "COLLECTION", h * 0.015);
                painter.text(
                    pos2(margin, text_top + h * 0.03),
                    egui::Align2::LEFT_TOP,
                    name,
                    bold(h * 0.075),
                    Color32::WHITE,
                );
                painter.text(
                    pos2(margin, text_top + h * 0.13),
                    egui::Align2::LEFT_TOP,
                    format!("{games} games · press A to open"),
                    regular(h * 0.02),
                    Color32::from_white_alpha(210),
                );
            }
            None => {
                let message = match couch.tab {
                    Tab::Search if couch.query.trim().is_empty() => {
                        "Type to search, or use the keyboard below."
                    }
                    Tab::Search => "Nothing matches.",
                    Tab::Collections => "No collections yet. Make one from a game's page.",
                    _ => "No games yet. Press Scan in the normal view.",
                };
                painter.text(
                    pos2(margin, text_top + h * 0.05),
                    egui::Align2::LEFT_TOP,
                    message,
                    semibold(h * 0.03),
                    Color32::from_white_alpha(220),
                );
            }
        }
        if let Some((name, _)) = &couch.open_collection
            && couch.tab == Tab::Collections
        {
            caps_line(
                &painter,
                pos2(margin, text_top - h * 0.04),
                &format!("{name}  ·  B to go back"),
                h * 0.014,
            );
        }

        // --- Search box and on-screen keyboard.
        if couch.tab == Tab::Search {
            let box_rect = Rect::from_min_size(
                pos2(margin, screen.top() + h * 0.11),
                vec2(w * 0.4, h * 0.06),
            );
            painter.rect_filled(box_rect, 10.0, Color32::from_white_alpha(25));
            let shown = if couch.query.is_empty() {
                "Search…".to_string()
            } else {
                format!("{}|", couch.query)
            };
            painter.text(
                box_rect.left_center() + vec2(16.0, 0.0),
                egui::Align2::LEFT_CENTER,
                shown,
                regular(h * 0.026),
                Color32::from_white_alpha(if couch.query.is_empty() { 120 } else { 255 }),
            );
            let mut key_y = box_rect.bottom() + h * 0.02;
            let mut kx = margin;
            for (index, key) in KEYS.iter().enumerate() {
                let size = if key.len() > 1 {
                    vec2(h * 0.09, h * 0.045)
                } else {
                    vec2(h * 0.045, h * 0.045)
                };
                // Wrap onto a second row on narrower screens.
                if kx + size.x > screen.right() - margin {
                    kx = margin;
                    key_y += size.y + 6.0;
                }
                let rect = Rect::from_min_size(pos2(kx, key_y), size);
                let active = couch.on_keyboard && index == couch.key;
                painter.rect_filled(
                    rect,
                    6.0,
                    if active {
                        Color32::WHITE
                    } else {
                        Color32::from_white_alpha(22)
                    },
                );
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    *key,
                    semibold(h * 0.017),
                    if active {
                        Color32::BLACK
                    } else {
                        Color32::from_white_alpha(220)
                    },
                );
                kx = rect.right() + 5.0;
            }
        }

        // --- The row of covers.
        let base_h = h * 0.24;
        let base_w = base_h * 0.667;
        let gap = w * 0.012;
        let row_bottom = screen.top() + h * 0.88;
        let scroll_target = selected.saturating_sub(3) as f32;
        let scroll = ui.ctx().animate_value_with_time(
            ui.scope_id().with("couch-scroll"),
            scroll_target,
            0.2,
        );
        let mut x = margin - scroll * (base_w + gap);
        let row_dimmed = couch.tab == Tab::Search && couch.on_keyboard;
        for (index, item) in items.iter().enumerate() {
            let key = match item {
                Item::Game(game) => game.id,
                Item::Collection(id, _, _) => -id,
            };
            let grow = ui.ctx().animate_bool_with_time(
                ui.scope_id().with(("couch-grow", key)),
                index == selected && !row_dimmed,
                0.15,
            );
            let size = vec2(base_w, base_h) * (1.0 + 0.22 * grow);
            let rect = Rect::from_min_size(pos2(x, row_bottom - size.y), size);
            x += size.x + gap;
            if rect.right() < screen.left() || rect.left() > screen.right() {
                continue;
            }
            let title_size = (size.x / 11.0).clamp(11.0, 17.0);
            match item {
                Item::Game(game) => paint::cover(ui, rect, game, 10.0, title_size),
                Item::Collection(_, name, _) => {
                    let (a, b) = paint::title_colours(name);
                    paint::gradient(&painter, rect, 10.0, a, b, vec2(0.25, 1.0));
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        name,
                        bold(title_size * 1.3),
                        Color32::WHITE,
                    );
                }
            }
            if row_dimmed {
                painter.rect_filled(rect, 10.0, Color32::from_black_alpha(110));
            }
            if grow > 0.05 {
                painter.rect_stroke(
                    rect.expand(3.0),
                    13.0,
                    Stroke::new(3.0 * grow, Color32::WHITE),
                    egui::StrokeKind::Outside,
                );
            }
            // Clicking a cover selects it.
            if ui
                .interact(
                    rect,
                    ui.scope_id().with(("couch-item", key)),
                    Sense::click(),
                )
                .clicked()
                && index != selected
            {
                *couch.selection() = index;
                couch.changed_at = time;
                couch.on_keyboard = false;
            }
        }

        // --- Button hints, bottom right.
        let controller = self.pad.connected();
        let hints: [(&str, &str, Color32); 4] = if controller {
            [
                ("A", "Play", Color32::from_rgb(76, 195, 110)),
                ("Y", "Details", Color32::from_rgb(240, 196, 60)),
                ("X", "Favourite", Color32::from_rgb(70, 140, 240)),
                ("B", "Back", Color32::from_rgb(232, 90, 90)),
            ]
        } else {
            [
                ("Enter", "Play", Color32::from_rgb(76, 195, 110)),
                ("I", "Details", Color32::from_rgb(240, 196, 60)),
                ("F", "Favourite", Color32::from_rgb(70, 140, 240)),
                ("Esc", "Back", Color32::from_rgb(232, 90, 90)),
            ]
        };
        let mut hx = screen.right() - margin;
        let hy = screen.top() + h * 0.955;
        for (button, label, colour) in hints.iter().rev() {
            let text = painter.text(
                pos2(hx, hy),
                egui::Align2::RIGHT_CENTER,
                *label,
                regular(h * 0.018),
                Color32::from_white_alpha(210),
            );
            let radius = h * 0.016;
            // Wide enough for the text ("Esc", "Enter"), round for one letter.
            let label_w = painter
                .layout_no_wrap(button.to_string(), bold(h * 0.015), Color32::BLACK)
                .size()
                .x;
            let badge_w = (label_w + radius * 1.1).max(radius * 2.0);
            let centre = Pos2::new(text.left() - badge_w / 2.0 - 8.0, hy);
            painter.rect_filled(
                Rect::from_center_size(centre, vec2(badge_w, radius * 2.0)),
                radius,
                *colour,
            );
            painter.text(
                centre,
                egui::Align2::CENTER_CENTER,
                *button,
                bold(h * 0.015),
                Color32::BLACK,
            );
            hx = centre.x - badge_w / 2.0 - 22.0;
        }
        let tabs_hint = if controller {
            "LB / RB  change tab"
        } else {
            "Q / E  change tab"
        };
        painter.text(
            pos2(margin, hy),
            egui::Align2::LEFT_CENTER,
            tabs_hint,
            regular(h * 0.016),
            Color32::from_white_alpha(120),
        );

        // --- Details, over everything.
        if couch.details
            && let Some(Item::Game(game)) = current
        {
            painter.rect_filled(screen, 0.0, Color32::from_black_alpha(190));
            let panel = Rect::from_center_size(screen.center(), vec2(w * 0.62, h * 0.62));
            painter.rect_filled(panel, 18.0, Color32::from_rgb(22, 16, 34));
            painter.rect_stroke(
                panel,
                18.0,
                Stroke::new(1.0, Color32::from_white_alpha(40)),
                egui::StrokeKind::Inside,
            );
            let pad = h * 0.04;
            let cover = Rect::from_min_size(
                panel.min + Vec2::splat(pad),
                vec2(panel.height() * 0.55, panel.height() * 0.82),
            );
            paint::cover(ui, cover, game, 12.0, 16.0);
            let tx = cover.right() + pad;
            let text_w = panel.right() - pad - tx;
            painter.text(
                pos2(tx, cover.top()),
                egui::Align2::LEFT_TOP,
                &game.title,
                bold(h * 0.045),
                Color32::WHITE,
            );
            let extras = self.data.extras.get(&game.id).cloned().unwrap_or_default();
            let sessions = self.db.session_count(game.id).unwrap_or(0);
            let mut facts = vec![
                format!("Played: {}", played_label(game.playtime_secs)),
                format!(
                    "Last played: {}",
                    format_last_played(game.last_played, now())
                ),
                format!("Sessions: {sessions}"),
                format!("From: {}", source_name(game.source)),
            ];
            if let Some(status) = extras.status {
                facts.push(format!("Status: {}", status.label()));
            }
            if let Some(rating) = extras.rating {
                facts.push(format!("Your rating: {}", "★".repeat(rating as usize)));
            }
            let mut y = cover.top() + h * 0.07;
            for fact in facts {
                painter.text(
                    pos2(tx, y),
                    egui::Align2::LEFT_TOP,
                    fact,
                    regular(h * 0.021),
                    Color32::from_white_alpha(220),
                );
                y += h * 0.032;
            }
            if let Some(description) = &game.description {
                let galley = painter.layout(
                    description.clone(),
                    regular(h * 0.019),
                    Color32::from_white_alpha(190),
                    text_w,
                );
                painter.galley(pos2(tx, y + h * 0.015), galley, Color32::WHITE);
            }
            painter.text(
                pos2(tx, cover.bottom()),
                egui::Align2::LEFT_BOTTOM,
                if controller {
                    "A Play  ·  X Favourite  ·  B Close"
                } else {
                    "Enter Play  ·  F Favourite  ·  Esc Close"
                },
                semibold(h * 0.018),
                Color32::from_white_alpha(170),
            );
        }
    }
}

/// Small, widely spaced capitals.
fn caps_line(painter: &egui::Painter, pos: Pos2, text: &str, size: f32) {
    let galley = painter.layout_job(egui::text::LayoutJob::single_section(
        text.to_string(),
        egui::TextFormat {
            font_id: semibold(size),
            color: Color32::from_white_alpha(190),
            extra_letter_spacing: size * 0.25,
            ..Default::default()
        },
    ));
    painter.galley(pos, galley, Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holding_a_direction_repeats_after_a_pause() {
        let mut repeat = Repeater::default();
        assert_eq!(repeat.step(1, 0.0), 1, "the first push moves at once");
        assert_eq!(repeat.step(1, 0.2), 0, "still held, but not yet repeating");
        assert_eq!(repeat.step(1, 0.41), 1, "repeats after the delay");
        assert_eq!(repeat.step(1, 0.45), 0);
        assert_eq!(repeat.step(1, 0.53), 1, "then repeats steadily");
        assert_eq!(repeat.step(0, 0.6), 0, "let go");
        assert_eq!(repeat.step(-1, 0.61), -1, "a new push moves at once");
    }

    #[test]
    fn inputs_combine() {
        let pad = Input {
            a: true,
            ..Default::default()
        };
        let keys = Input {
            left: true,
            ..Default::default()
        };
        let both = pad.merge(keys);
        assert!(both.a && both.left && !both.b);
    }
}
