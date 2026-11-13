//! The Assistant screen: ask questions about your library in plain words.
//! Built-in answers work offline; with an Anthropic API key in Settings,
//! questions go to Claude instead.

use eframe::egui::{self, Rect, Sense, vec2};
use gamelib::assistant::{self, Library, Turn};
use gamelib::db::keys;
use gamelib::now;
use gamelib::worker::Job;

use super::theme::bold;
use super::{GameLibApp, Screen, paint};

/// One message in the conversation.
pub struct Entry {
    pub from_user: bool,
    pub text: String,
    /// Games the answer mentions, shown as small covers.
    pub games: Vec<i64>,
    /// Answered by Claude (rather than the built-in assistant).
    pub by_claude: bool,
}

#[derive(Default)]
pub struct Chat {
    pub input: String,
    pub entries: Vec<Entry>,
    /// Waiting for Claude to answer.
    pub waiting: bool,
}

impl GameLibApp {
    /// The Anthropic API key, from Settings or the ANTHROPIC_API_KEY
    /// environment variable.
    fn anthropic_key(&self) -> Option<String> {
        self.db
            .get_setting(keys::ANTHROPIC_KEY)
            .ok()
            .flatten()
            .filter(|key| !key.trim().is_empty())
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
            .filter(|key| !key.trim().is_empty())
    }

    fn use_claude(&self) -> bool {
        self.anthropic_key().is_some()
            && self
                .db
                .get_bool_setting(keys::ASSISTANT_USE_CLAUDE, true)
                .unwrap_or(true)
    }

    fn library(&self) -> Library<'_> {
        Library {
            games: &self.data.visible,
            extras: &self.data.extras,
            tags: &self.data.taste_tags,
            taste: &self.data.taste,
            habits: self.data.habits,
            now: now(),
        }
    }

    /// Ask a question: answered straight away by the built-in assistant,
    /// or sent to Claude.
    pub fn ask(&mut self, question: &str) {
        let question = question.trim();
        if question.is_empty() || self.chat.waiting {
            return;
        }
        self.chat.entries.push(Entry {
            from_user: true,
            text: question.to_string(),
            games: Vec::new(),
            by_claude: false,
        });
        self.chat.input.clear();
        if let Some(api_key) = self.anthropic_key().filter(|_| self.use_claude()) {
            let context = assistant::library_context(&self.library());
            // Claude sees the conversation so far, but only the parts it
            // took part in; the API wants user and assistant turns to
            // alternate, so neighbours from the same side are joined.
            let mut history: Vec<Turn> = Vec::new();
            for entry in self
                .chat
                .entries
                .iter()
                .filter(|e| e.from_user || e.by_claude)
            {
                let role = if entry.from_user { "user" } else { "assistant" };
                match history.last_mut() {
                    Some(last) if last.role == role => {
                        last.text.push_str("\n\n");
                        last.text.push_str(&entry.text);
                    }
                    _ => history.push(Turn {
                        role: role.to_string(),
                        text: entry.text.clone(),
                    }),
                }
            }
            self.chat.waiting = true;
            self.start_job(
                Job::AskClaude {
                    api_key,
                    context,
                    history,
                },
                "Asking Claude…",
            );
        } else {
            let answer = assistant::answer(question, &self.library());
            self.chat.entries.push(Entry {
                from_user: false,
                text: answer.text,
                games: answer.games,
                by_claude: false,
            });
        }
    }

    /// Claude has answered (or failed to).
    pub fn claude_replied(&mut self, reply: Result<String, String>) {
        self.chat.waiting = false;
        let (text, by_claude) = match reply {
            Ok(text) => (text, true),
            Err(err) => (format!("Couldn't ask Claude: {err}"), false),
        };
        // Covers for any games of yours the answer names.
        let mut games: Vec<(usize, i64)> = self
            .data
            .visible
            .iter()
            .filter(|g| g.title.len() >= 3)
            .filter_map(|g| text.find(&g.title).map(|at| (at, g.id)))
            .collect();
        games.sort();
        self.chat.entries.push(Entry {
            from_user: false,
            text,
            games: games.into_iter().map(|(_, id)| id).take(8).collect(),
            by_claude,
        });
    }

    pub fn assistant_screen(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let claude = self.use_claude();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Assistant").font(bold(32.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !self.chat.entries.is_empty() && ui.button("New conversation").clicked() {
                    self.chat.entries.clear();
                }
            });
        });
        ui.label(paint::muted(
            if claude {
                "Answered by Claude, using a summary of your library (titles, hours, status, ratings, genres). Turn it off in Settings."
            } else {
                "Built-in answers from your library. Works offline. Add an Anthropic API key in Settings to ask Claude anything."
            },
            &theme,
        ));
        ui.add_space(12.0);

        let mut open = None;
        if self.chat.entries.is_empty() {
            ui.label(paint::strong("Ask me about your games", 19.0));
            ui.add_space(6.0);
        }
        let width = ui.available_width();
        for entry in &self.chat.entries {
            let bubble = if entry.from_user {
                egui::Frame::new()
                    .fill(theme.accent.gamma_multiply(0.22))
                    .corner_radius(14.0)
                    .inner_margin(egui::Margin::symmetric(16, 10))
            } else {
                theme.panel_frame()
            };
            let layout = if entry.from_user {
                egui::Layout::right_to_left(egui::Align::Min)
            } else {
                egui::Layout::left_to_right(egui::Align::Min)
            };
            ui.allocate_ui_with_layout(vec2(width, 0.0), layout, |ui| {
                paint::panel(ui, bubble, |ui| {
                    ui.set_max_width(width * 0.75);
                    if entry.from_user {
                        // Shrink the bubble to fit a short question.
                        let text_width = ui
                            .painter()
                            .layout_no_wrap(
                                entry.text.clone(),
                                egui::FontId::proportional(15.0),
                                theme.text,
                            )
                            .size()
                            .x;
                        ui.set_width(text_width.min(width * 0.75));
                    }
                    if !entry.from_user {
                        let who = if entry.by_claude { "Claude" } else { "GameLib" };
                        ui.label(paint::caps(who, 11.0, theme.accent));
                    }
                    ui.label(egui::RichText::new(&entry.text).size(15.0));
                    if !entry.games.is_empty() {
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            for id in &entry.games {
                                let Some(game) = self.data.game(*id) else {
                                    continue;
                                };
                                let (rect, response) =
                                    ui.allocate_exact_size(vec2(64.0, 96.0), Sense::click());
                                paint::cover(
                                    ui,
                                    Rect::from_min_size(rect.min, rect.size()),
                                    game,
                                    8.0,
                                    0.0,
                                );
                                if response
                                    .on_hover_text(&game.title)
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    open = Some(*id);
                                }
                            }
                        });
                    }
                });
            });
            ui.add_space(10.0);
        }
        if self.chat.waiting {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(paint::muted("Claude is thinking…", &theme));
            });
            ui.add_space(10.0);
        }

        // Suggested questions.
        let mut asked = None;
        ui.horizontal_wrapped(|ui| {
            for example in assistant::EXAMPLES {
                if paint::chip(ui, &theme, example, false).clicked() {
                    asked = Some(example.to_string());
                }
            }
        });
        ui.add_space(10.0);

        // The question box.
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.chat.input)
                    .hint_text("Ask anything about your library…")
                    .font(egui::FontId::proportional(16.0))
                    .desired_width(ui.available_width() - 110.0)
                    .margin(egui::Margin::symmetric(12, 10)),
            );
            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let button = ui.add_enabled_ui(!self.chat.waiting, |ui| {
                paint::accent_button(ui, &theme, "Ask", 15.0, vec2(26.0, 11.0))
            });
            if enter || button.inner.clicked() {
                asked = Some(self.chat.input.clone());
                response.request_focus();
            }
        });

        if let Some(question) = asked {
            self.ask(&question);
        }
        if let Some(id) = open {
            self.go(Screen::Details(id));
        }
    }
}
