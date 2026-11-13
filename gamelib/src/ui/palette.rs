//! Global search and keyboard shortcuts.
//!
//! **Ctrl+K** opens the command palette: type part of a game's name, a
//! screen or an action, pick it with the arrow keys and press Enter.
//! The other shortcuts are listed in [`SHORTCUTS`] (and in the palette when
//! you type "?" or press F1).

use eframe::egui::{self, Color32, Key, Modifiers, Stroke, vec2};
use gamelib::worker::Job;

use super::dialogs::Dialog;
use super::theme::{bold, regular};
use super::{GameLibApp, Screen, paint};

/// Every keyboard shortcut, for the help list.
pub const SHORTCUTS: [(&str, &str); 13] = [
    ("Ctrl+K", "Search everything (games, screens, actions)"),
    ("Ctrl+F", "Search the library"),
    (
        "Ctrl+1 … Ctrl+9",
        "Home, Library, Discover, Assistant, Franchises, Stats, Shop, Settings, Library health",
    ),
    ("Ctrl+,", "Settings"),
    ("Ctrl+N", "Add a game"),
    ("F5", "Scan for games"),
    ("Ctrl+Enter", "Play the game whose page is open"),
    ("Esc", "Back from a game's page"),
    ("Ctrl+Shift+A", "Admin panel"),
    ("F11", "Couch Mode"),
    ("Ctrl + / Ctrl -", "Bigger or smaller text"),
    ("Ctrl+0", "Normal text size"),
    ("F1", "This list"),
];

/// Screens in the order Ctrl+1 … Ctrl+9 reach them.
const SCREENS: [(Screen, &str); 9] = [
    (Screen::Home, "Home"),
    (Screen::Library, "Library"),
    (Screen::Discover, "Discover"),
    (Screen::Assistant, "Assistant"),
    (Screen::Franchises, "Franchises"),
    (Screen::Stats, "Stats"),
    (Screen::Shop, "Shop"),
    (Screen::Settings, "Settings"),
    (Screen::Health, "Library health"),
];

/// Something the palette can do.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Go(Screen),
    Game(i64),
    Play(i64),
    Scan,
    AddGame,
    Couch,
    MeasureDisk,
    CheckUpdates,
    Wrapped,
}

#[derive(Default)]
pub struct Palette {
    pub open: bool,
    pub query: String,
    pub selected: usize,
    /// The library search box should take the cursor (Ctrl+F).
    pub focus_search: bool,
}

/// How well `query` matches `text`: higher is better, `None` is no match.
/// Words of the query must all appear; matches at the start score higher.
pub fn score(text: &str, query: &str) -> Option<u32> {
    let text = text.to_lowercase();
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(1);
    }
    let mut total = 0;
    for word in query.split_whitespace() {
        let at = text.find(word)?;
        total += if at == 0 {
            30
        } else if text[..at].ends_with(' ') {
            20
        } else {
            10
        };
    }
    if text == query {
        total += 50;
    }
    Some(total)
}

impl GameLibApp {
    /// The results for what's typed, best first.
    fn palette_results(&self) -> Vec<(String, String, Command)> {
        let query = self.palette.query.trim();
        let mut results: Vec<(u32, String, String, Command)> = Vec::new();
        let mut add = |label: String, kind: &str, command: Command, bonus: u32| {
            if let Some(score) = score(&label, query) {
                results.push((score + bonus, label, kind.to_string(), command));
            }
        };
        for game in &self.data.visible {
            add(game.title.clone(), "Game", Command::Game(game.id), 5);
            if !query.is_empty() && game.installed {
                add(
                    format!("Play {}", game.title),
                    "Play",
                    Command::Play(game.id),
                    0,
                );
            }
        }
        for (screen, name) in SCREENS {
            add(name.to_string(), "Go to", Command::Go(screen), 8);
        }
        add("Profile".into(), "Go to", Command::Go(Screen::Profile), 8);
        add(
            "Devices and Bluetooth".into(),
            "Go to",
            Command::Go(Screen::Devices),
            8,
        );
        add("Admin panel".into(), "Go to", Command::Go(Screen::Admin), 0);
        add("Scan for games".into(), "Action", Command::Scan, 6);
        add("Add a game".into(), "Action", Command::AddGame, 6);
        add("Couch Mode".into(), "Action", Command::Couch, 6);
        add(
            "Measure disk space".into(),
            "Action",
            Command::MeasureDisk,
            4,
        );
        add(
            "Check for updates".into(),
            "Action",
            Command::CheckUpdates,
            4,
        );
        add("GameLib Wrapped".into(), "Go to", Command::Wrapped, 4);
        results.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        results
            .into_iter()
            .take(9)
            .map(|(_, label, kind, command)| (label, kind, command))
            .collect()
    }

    pub fn run_command(&mut self, ctx: &egui::Context, command: Command) {
        match command {
            Command::Go(screen) => self.go(screen),
            Command::Game(id) => self.go(Screen::Details(id)),
            Command::Play(id) => self.play(id),
            Command::Scan => self.start_job(Job::ScanLibrary, "Scanning for games…"),
            Command::AddGame => self.dialog = Dialog::pick_game_to_add(),
            Command::Couch => self.enter_couch(ctx),
            Command::MeasureDisk => self.start_job(Job::MeasureDisk, "Measuring disk space…"),
            Command::CheckUpdates => self.start_job(Job::CheckForUpdates, "Checking for updates…"),
            Command::Wrapped => self.go(Screen::Wrapped(gamelib::patterns::year_of(
                gamelib::now(),
                super::local_offset(),
            ))),
        }
    }

    /// Global keyboard shortcuts. Typing in a text box doesn't trigger the
    /// ones without Ctrl.
    pub fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.egui_wants_keyboard_input();
        let mut run: Option<Command> = None;
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::COMMAND, Key::K) {
                self.palette.open = !self.palette.open;
                self.palette.query.clear();
                self.palette.selected = 0;
            }
            if i.consume_key(Modifiers::NONE, Key::F1) {
                self.palette.open = true;
                self.palette.query = "?".into();
            }
            if i.consume_key(Modifiers::COMMAND, Key::F) {
                run = Some(Command::Go(Screen::Library));
                self.palette.focus_search = true;
            }
            if i.consume_key(Modifiers::COMMAND, Key::Comma) {
                run = Some(Command::Go(Screen::Settings));
            }
            if i.consume_key(Modifiers::COMMAND, Key::N) {
                run = Some(Command::AddGame);
            }
            if i.consume_key(Modifiers::NONE, Key::F5) {
                run = Some(Command::Scan);
            }
            let digits = [
                Key::Num1,
                Key::Num2,
                Key::Num3,
                Key::Num4,
                Key::Num5,
                Key::Num6,
                Key::Num7,
                Key::Num8,
                Key::Num9,
            ];
            for (key, (screen, _)) in digits.into_iter().zip(SCREENS) {
                if i.consume_key(Modifiers::COMMAND, key) {
                    run = Some(Command::Go(screen));
                }
            }
            if let Screen::Details(id) = self.screen {
                if i.consume_key(Modifiers::COMMAND, Key::Enter) {
                    run = Some(Command::Play(id));
                }
                if !typing && !self.palette.open && i.consume_key(Modifiers::NONE, Key::Escape) {
                    run = Some(Command::Go(Screen::Library));
                }
            }
        });
        if let Some(command) = run {
            self.run_command(ctx, command);
        }
    }

    /// The command palette window.
    pub fn palette_window(&mut self, ctx: &egui::Context) {
        if !self.palette.open {
            return;
        }
        let theme = self.theme;
        let help = self.palette.query.trim() == "?";
        let results = if help {
            Vec::new()
        } else {
            self.palette_results()
        };
        self.palette.selected = self.palette.selected.min(results.len().saturating_sub(1));
        let mut chosen: Option<Command> = None;
        let mut close = false;
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                self.palette.selected =
                    (self.palette.selected + 1).min(results.len().saturating_sub(1));
            }
            if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                self.palette.selected = self.palette.selected.saturating_sub(1);
            }
            if i.consume_key(Modifiers::NONE, Key::Enter) {
                chosen = results.get(self.palette.selected).map(|r| r.2.clone());
            }
            if i.consume_key(Modifiers::NONE, Key::Escape) {
                close = true;
            }
        });
        let frame = egui::Frame::new()
            .fill(theme.opaque_panel())
            .stroke(Stroke::new(1.0, theme.border))
            .corner_radius(16.0)
            .inner_margin(18.0);
        let modal = egui::Modal::new(egui::Id::unique("palette"))
            .frame(frame)
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                ui.set_width(560.0);
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.palette.query)
                        .hint_text("Search games, screens and actions…  (? for shortcuts)")
                        .font(regular(18.0))
                        .desired_width(f32::INFINITY)
                        .margin(egui::Margin::symmetric(12, 10)),
                );
                edit.request_focus();
                if edit.changed() {
                    self.palette.selected = 0;
                }
                ui.add_space(8.0);
                if help {
                    ui.label(paint::strong("Keyboard shortcuts", 15.0));
                    egui::Grid::new("shortcut_list")
                        .num_columns(2)
                        .spacing([18.0, 6.0])
                        .show(ui, |ui| {
                            for (keys, what) in SHORTCUTS {
                                ui.label(
                                    egui::RichText::new(keys)
                                        .font(bold(13.5))
                                        .color(theme.accent),
                                );
                                ui.label(what);
                                ui.end_row();
                            }
                        });
                    return;
                }
                if results.is_empty() {
                    ui.label(paint::muted("Nothing matches.", &theme));
                }
                for (i, (label, kind, command)) in results.iter().enumerate() {
                    let selected = i == self.palette.selected;
                    let (rect, response) = ui.allocate_exact_size(
                        vec2(ui.available_width(), 36.0),
                        egui::Sense::click(),
                    );
                    if selected || response.hovered() {
                        ui.painter().rect_filled(rect, 8.0, theme.panel_alt);
                    }
                    ui.painter().text(
                        rect.left_center() + vec2(12.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        label,
                        regular(15.0),
                        theme.text,
                    );
                    ui.painter().text(
                        rect.right_center() - vec2(12.0, 0.0),
                        egui::Align2::RIGHT_CENTER,
                        kind,
                        regular(12.5),
                        theme.muted,
                    );
                    if response.clicked() {
                        chosen = Some(command.clone());
                    }
                }
                ui.add_space(6.0);
                ui.label(
                    paint::muted("↑ ↓ to choose · Enter to open · Esc to close", &theme).size(12.0),
                );
            });
        if modal.should_close() {
            close = true;
        }
        if let Some(command) = chosen {
            self.palette.open = false;
            self.run_command(ctx, command);
        } else if close {
            self.palette.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_score_by_where_the_words_are() {
        assert!(score("Elden Ring", "elden").unwrap() > score("Elden Ring", "ring").unwrap());
        assert!(score("Elden Ring", "ring").unwrap() > score("Spring", "ring").unwrap());
        assert_eq!(score("Elden Ring", "souls"), None);
        assert!(
            score("Elden Ring", "ring elden").is_some(),
            "word order doesn't matter"
        );
        assert!(score("Hades", "hades").unwrap() > score("Hades II", "hades").unwrap());
    }
}
