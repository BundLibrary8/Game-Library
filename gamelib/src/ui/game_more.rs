//! More of a game's page: launch profiles, the session journal, an hours
//! goal, and the screenshot gallery.

use std::path::PathBuf;

use eframe::egui::{self, Color32, Rect, Sense, vec2};
use gamelib::model::Game;
use gamelib::profiles::{self, LaunchProfile};
use gamelib::screenshots::{self, Shot};
use gamelib::worker::ProfileChoice;
use gamelib::{journal, now};

use super::{GameLibApp, paint};
use crate::text::{format_ago, short_playtime};

/// A launch profile being added or edited.
pub struct ProfileDraft {
    pub game_id: i64,
    pub id: Option<i64>,
    pub name: String,
    pub args: String,
}

/// The game-page state that has to last between frames.
#[derive(Default)]
pub struct GamePageState {
    pub profile: Option<ProfileDraft>,
    /// The session note being written: (session id, text).
    pub note: Option<(i64, String)>,
    /// The hours goal being typed: (game id, text).
    pub goal: Option<(i64, String)>,
    /// Screenshots found for a game, so the folders aren't read every
    /// frame: (game id, screenshots).
    pub shots: Option<(i64, Vec<Shot>)>,
    /// The screenshot shown large.
    pub viewer: Option<(Vec<PathBuf>, usize)>,
}

impl GameLibApp {
    /// The small ▾ next to Launch: pick a profile to launch with.
    pub fn launch_menu(&mut self, ui: &mut egui::Ui, game: &Game, enabled: bool) {
        let list = self.db.launch_profiles(game.id).unwrap_or_default();
        if list.is_empty() {
            return;
        }
        let button = egui::Button::new(egui::RichText::new("▾").size(20.0))
            .min_size(vec2(44.0, 56.0))
            .corner_radius(14.0);
        ui.add_enabled_ui(enabled, |ui| {
            egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
                ui.set_min_width(220.0);
                ui.label(paint::caps("Launch with", 11.0, self.theme.muted));
                if ui.button("No extra options").clicked() {
                    self.play_with(game.id, ProfileChoice::Plain);
                    ui.close();
                }
                for profile in &list {
                    let label = if profile.is_default {
                        format!("{} (default)", profile.name)
                    } else {
                        profile.name.clone()
                    };
                    if ui.button(label).on_hover_text(&profile.args).clicked() {
                        self.play_with(game.id, ProfileChoice::Profile(profile.id));
                        ui.close();
                    }
                }
            });
        });
    }

    /// Launch profiles, the hours goal and the session journal.
    pub fn launch_and_journal_panel(&mut self, ui: &mut egui::Ui, game: &Game) {
        let theme = self.theme;
        let list = self.db.launch_profiles(game.id).unwrap_or_default();
        let gap = 18.0;
        let half = ((ui.available_width() - gap) / 2.0).floor();
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            paint::panel(ui, theme.panel_frame().inner_margin(24.0), |ui| {
                ui.set_width(half - 48.0);
                ui.label(paint::strong("Launch profiles", 16.0));
                if !profiles::supports_options(game) {
                    ui.label(paint::muted(
                        "This game starts through its launcher, which doesn't accept launch options from other programs. Set them in the launcher instead.",
                        &theme,
                    ));
                } else {
                    ui.label(paint::muted(
                        "Named sets of launch options, like \"Windowed\" (-windowed). Pick one with ▾ next to Launch.",
                        &theme,
                    ));
                }
                ui.add_space(6.0);
                let mut action: Option<ProfileAction> = None;
                for profile in &list {
                    ui.horizontal(|ui| {
                        let mut default = profile.is_default;
                        if ui
                            .checkbox(&mut default, "")
                            .on_hover_text("Use this when you press Launch")
                            .changed()
                        {
                            action = Some(ProfileAction::Default(default.then_some(profile.id)));
                        }
                        ui.label(paint::strong(&profile.name, 14.5));
                        ui.label(paint::muted(&profile.args, &theme).monospace());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Delete").clicked() {
                                action = Some(ProfileAction::Delete(profile.id));
                            }
                            if ui.small_button("Edit").clicked() {
                                action = Some(ProfileAction::Edit(profile.clone()));
                            }
                        });
                    });
                }
                if list.is_empty() {
                    ui.label(paint::muted("No profiles yet.", &theme));
                }
                ui.add_space(6.0);
                let editing_here = self
                    .game_page
                    .profile
                    .as_ref()
                    .is_some_and(|d| d.game_id == game.id);
                if editing_here {
                    let mut save = false;
                    let mut cancel = false;
                    if let Some(draft) = self.game_page.profile.as_mut() {
                        egui::Grid::new("profile_edit").num_columns(2).show(ui, |ui| {
                            ui.label("Name");
                            ui.add(egui::TextEdit::singleline(&mut draft.name).hint_text("Windowed"));
                            ui.end_row();
                            ui.label("Options");
                            ui.add(
                                egui::TextEdit::singleline(&mut draft.args)
                                    .hint_text("-windowed -w 1280 -h 720")
                                    .code_editor(),
                            );
                            ui.end_row();
                        });
                        ui.horizontal(|ui| {
                            save = ui.button("Save profile").clicked();
                            cancel = ui.button("Cancel").clicked();
                        });
                    }
                    if save && let Some(draft) = &self.game_page.profile {
                        let saved =
                            self.db
                                .save_launch_profile(game.id, draft.id, &draft.name, &draft.args);
                        if self.check(saved).is_some() {
                            self.game_page.profile = None;
                        }
                    }
                    if cancel {
                        self.game_page.profile = None;
                    }
                } else if profiles::supports_options(game) && ui.button("+ Add profile").clicked() {
                    self.game_page.profile = Some(ProfileDraft {
                        game_id: game.id,
                        id: None,
                        name: String::new(),
                        args: String::new(),
                    });
                }
                match action {
                    Some(ProfileAction::Default(id)) => {
                        self.check(self.db.set_default_launch_profile(game.id, id));
                    }
                    Some(ProfileAction::Delete(id)) => {
                        self.check(self.db.delete_launch_profile(id));
                    }
                    Some(ProfileAction::Edit(profile)) => {
                        self.game_page.profile = Some(ProfileDraft {
                            game_id: game.id,
                            id: Some(profile.id),
                            name: profile.name,
                            args: profile.args,
                        });
                    }
                    None => {}
                }

                // The hours goal.
                ui.add_space(14.0);
                ui.label(paint::strong("Playtime goal", 16.0));
                let goal = self.db.game_goal(game.id).ok().flatten();
                if self.game_page.goal.as_ref().map(|g| g.0) != Some(game.id) {
                    let text = goal.map(|m| format!("{}", m / 60)).unwrap_or_default();
                    self.game_page.goal = Some((game.id, text));
                }
                ui.horizontal(|ui| {
                    ui.label("Play");
                    let mut save = false;
                    if let Some((_, text)) = self.game_page.goal.as_mut() {
                        let response = ui.add(
                            egui::TextEdit::singleline(text)
                                .hint_text("20")
                                .desired_width(60.0),
                        );
                        save = response.lost_focus();
                    }
                    ui.label("hours of this game");
                    if save && let Some((_, text)) = &self.game_page.goal {
                        let minutes = text.trim().parse::<f32>().ok().map(|h| (h * 60.0) as u32);
                        self.check(self.db.set_game_goal(game.id, minutes));
                    }
                });
                if let Some(minutes) = goal {
                    let progress = journal::Progress {
                        done_minutes: (game.playtime_secs / 60) as u32,
                        goal_minutes: minutes,
                    };
                    paint::bar(
                        ui,
                        &theme,
                        progress.fraction(),
                        if progress.reached() { theme.success } else { theme.accent },
                        half - 100.0,
                    );
                    ui.label(paint::muted(
                        if progress.reached() {
                            "Goal reached!".to_string()
                        } else {
                            format!(
                                "{} of {} hours",
                                progress.done_minutes / 60,
                                progress.goal_minutes / 60
                            )
                        },
                        &theme,
                    ));
                }
            });

            paint::panel(ui, theme.panel_frame().inner_margin(24.0), |ui| {
                ui.set_width(half - 48.0);
                ui.label(paint::strong("Session journal", 16.0));
                ui.label(paint::muted("Your sessions, newest first. Add a note to remember what happened.", &theme));
                ui.add_space(6.0);
                let entries = self.db.journal(game.id).unwrap_or_default();
                if entries.is_empty() {
                    ui.label(paint::muted("No sessions yet. Play the game through GameLib and they appear here.", &theme));
                }
                egui::ScrollArea::vertical()
                    .id_salt("journal")
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for entry in entries.iter().take(100) {
                            ui.horizontal(|ui| {
                                ui.label(paint::strong(format_ago(entry.started_at, now()), 13.5));
                                ui.label(paint::muted(short_playtime(entry.duration_secs), &theme));
                            });
                            let editing = self.game_page.note.as_ref().map(|n| n.0) == Some(entry.session_id);
                            if editing {
                                let mut done = false;
                                if let Some((_, text)) = self.game_page.note.as_mut() {
                                    ui.add(
                                        egui::TextEdit::multiline(text)
                                            .desired_rows(2)
                                            .desired_width(f32::INFINITY)
                                            .hint_text("What happened this session?"),
                                    );
                                    done = ui.button("Save note").clicked();
                                }
                                if done && let Some((id, text)) = self.game_page.note.take() {
                                    self.check(self.db.set_session_note(id, &text));
                                }
                            } else {
                                match &entry.note {
                                    Some(note) => {
                                        let response = ui.add(egui::Label::new(note).sense(Sense::click()));
                                        if response.on_hover_text("Click to edit").clicked() {
                                            self.game_page.note = Some((entry.session_id, note.clone()));
                                        }
                                    }
                                    None => {
                                        if ui.small_button("+ Add a note").clicked() {
                                            self.game_page.note = Some((entry.session_id, String::new()));
                                        }
                                    }
                                }
                            }
                            ui.separator();
                        }
                    });
            });
        });
    }

    /// The screenshot gallery.
    pub fn screenshots_panel(&mut self, ui: &mut egui::Ui, game: &Game) {
        let theme = self.theme;
        let fresh = self.game_page.shots.as_ref().map(|s| s.0) == Some(game.id);
        if !fresh {
            let own = self.db.screenshots_dir(game.id).ok().flatten();
            let titles: Vec<String> = self.data.visible.iter().map(|g| g.title.clone()).collect();
            let shots = screenshots::find(
                game,
                own.as_deref(),
                screenshots::steam_root().as_deref(),
                screenshots::captures_folder().as_deref(),
                &titles,
            );
            self.game_page.shots = Some((game.id, shots));
        }
        let shots: Vec<Shot> = self
            .game_page
            .shots
            .as_ref()
            .map(|s| s.1.clone())
            .unwrap_or_default();
        paint::panel(ui, theme.panel_frame().inner_margin(24.0), |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(paint::strong("Screenshots", 16.0));
                ui.label(paint::muted(format!("· {}", shots.len()), &theme));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Refresh").clicked() {
                        self.game_page.shots = None;
                    }
                    if ui.button("Choose folder…").clicked()
                        && let Some(dir) = rfd::FileDialog::new()
                            .set_title("Choose the folder this game's screenshots are in")
                            .pick_folder()
                    {
                        self.check(self.db.set_screenshots_dir(game.id, Some(&dir)));
                        self.game_page.shots = None;
                    }
                });
            });
            if shots.is_empty() {
                ui.label(paint::muted(
                    "None found. GameLib looks in Steam's screenshot folder, the Xbox Game Bar's Videos\\Captures folder, and a folder you choose.",
                    &theme,
                ));
                return;
            }
            let gap = 10.0;
            let width = ui.available_width();
            let columns = ((width + gap) / (220.0 + gap)).floor().max(1.0) as usize;
            let tile = (width - gap * (columns as f32 - 1.0)) / columns as f32;
            let paths: Vec<PathBuf> = shots.iter().map(|s| s.path.clone()).collect();
            for (row_index, row) in shots.chunks(columns).take(4).enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (i, shot) in row.iter().enumerate() {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(tile, tile * 9.0 / 16.0), Sense::click());
                        if ui.is_rect_visible(rect) && !paint::file_image(ui, &shot.path, rect, 8.0)
                        {
                            ui.painter().rect_filled(rect, 8.0, theme.panel_alt);
                        }
                        if response.on_hover_cursor(egui::CursorIcon::ZoomIn).clicked() {
                            self.game_page.viewer = Some((paths.clone(), row_index * columns + i));
                        }
                    }
                });
                ui.add_space(gap);
            }
            if shots.len() > columns * 4 {
                ui.label(paint::muted(
                    format!("Showing the newest {} of {}.", columns * 4, shots.len()),
                    &theme,
                ));
            }
        });
    }

    /// A screenshot shown big, with arrows to step through them.
    pub fn screenshot_viewer(&mut self, ctx: &egui::Context) {
        let Some((paths, index)) = self.game_page.viewer.clone() else {
            return;
        };
        let theme = self.theme;
        let mut close = false;
        let mut step: i64 = 0;
        let screen = ctx.content_rect();
        let modal = egui::Modal::new(egui::Id::unique("screenshot_viewer"))
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_black_alpha(230))
                    .inner_margin(16.0),
            )
            .backdrop_color(Color32::from_black_alpha(200))
            .show(ctx, |ui| {
                let size = vec2(screen.width() - 120.0, screen.height() - 160.0);
                let (rect, response) = ui.allocate_exact_size(size, Sense::click());
                if let Some(path) = paths.get(index) {
                    let image = egui::Image::from_uri(paint::file_uri(path))
                        .fit_to_exact_size(size)
                        .maintain_aspect_ratio(true);
                    ui.put(Rect::from_center_size(rect.center(), size), image);
                }
                if response.clicked() {
                    close = true;
                }
                ui.horizontal(|ui| {
                    if ui.button("← Previous").clicked() {
                        step = -1;
                    }
                    ui.label(paint::muted(
                        format!("{} of {}", index + 1, paths.len()),
                        &theme,
                    ));
                    if ui.button("Next →").clicked() {
                        step = 1;
                    }
                    if let Some(path) = paths.get(index) {
                        ui.label(paint::muted(path.display().to_string(), &theme).size(12.0));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                });
            });
        ctx.input(|i| {
            if i.key_pressed(egui::Key::ArrowLeft) {
                step = -1;
            }
            if i.key_pressed(egui::Key::ArrowRight) {
                step = 1;
            }
            if i.key_pressed(egui::Key::Escape) {
                close = true;
            }
        });
        if close || modal.should_close() {
            self.game_page.viewer = None;
        } else if step != 0 && !paths.is_empty() {
            let next = (index as i64 + step).rem_euclid(paths.len() as i64) as usize;
            self.game_page.viewer = Some((paths, next));
        }
    }
}

enum ProfileAction {
    Default(Option<i64>),
    Delete(i64),
    Edit(LaunchProfile),
}
