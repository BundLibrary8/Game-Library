//! The library screen: the grid of games, the details panel on the right,
//! and the "add game" and "import review" windows.

use std::path::{Path, PathBuf};

use eframe::egui;
use gamelib::model::{Game, LaunchTarget};
use gamelib::now;
use gamelib::worker::Job;

use super::{GameLibApp, source_name};
use crate::text::{format_duration, format_last_played};

const CARD_SIZE: egui::Vec2 = egui::vec2(140.0, 210.0);

/// Text boxes and half-finished actions on the library screen.
#[derive(Default)]
pub struct LibraryInputs {
    new_tag: String,
    new_tag_is_collection: bool,
    /// The title being edited, and which game it belongs to.
    title_edit: Option<(i64, String)>,
    cover_search: String,
    /// The game whose "Remove" button has been clicked once.
    confirm_remove: Option<i64>,
    /// An exe picked with "+ Add game", waiting for its title.
    pending_add: Option<(PathBuf, String)>,
}

impl LibraryInputs {
    /// Ask for an exe with the system's file picker, then open the little
    /// window that asks for the title.
    pub fn add_game_from_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title("Choose the game's exe")
            .add_filter("Programs", &["exe"])
            .add_filter("All files", &["*"])
            .pick_file();
        if let Some(exe) = picked {
            let title = exe
                .parent()
                .and_then(|folder| folder.file_name())
                .or_else(|| exe.file_stem())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.pending_add = Some((exe, title));
        }
    }
}

/// The address egui's image loader uses for a file on disk.
pub fn file_uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// A colour for a game's placeholder tile, chosen from its title so each
/// game always gets the same one.
fn tile_colour(title: &str) -> egui::Color32 {
    const COLOURS: [(u8, u8, u8); 6] = [
        (62, 84, 122),
        (96, 70, 120),
        (56, 104, 96),
        (122, 82, 60),
        (110, 62, 80),
        (72, 96, 60),
    ];
    let sum: usize = title.bytes().map(usize::from).sum();
    let (r, g, b) = COLOURS[sum % COLOURS.len()];
    egui::Color32::from_rgb(r, g, b)
}

/// What a click on the screen asked for. Clicks are collected while
/// drawing and carried out afterwards, so the game list isn't changed
/// while it is being drawn.
enum Action {
    Select(i64),
    Play(i64),
    SetFavourite(i64, bool),
    SetHidden(i64, bool),
    Rename(i64, String),
    AddTag(i64, String, bool),
    RemoveTag(i64, i64),
    Finish(i64),
    Remove(i64),
    LocateExe(i64),
    ChangeCover(i64),
    SearchCover(i64, String),
    ToggleShowcase(i64),
}

impl GameLibApp {
    pub fn library_screen(&mut self, ui: &mut egui::Ui) {
        let mut actions = Vec::new();

        let selected = self
            .selected
            .and_then(|id| self.games.iter().find(|g| g.id == id).cloned());
        if let Some(game) = &selected {
            egui::Panel::right("details")
                .default_size(280.0)
                .min_size(240.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .show(ui, |ui| self.details(ui, game, &mut actions));
                });
        }

        egui::CentralPanel::default().show(ui, |ui| {
            if self.games.is_empty() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    if self.search.trim().is_empty() && self.filter == super::Filter::All {
                        ui.heading("No games yet");
                        ui.label("Click Scan to find your Steam, Epic and GOG games,");
                        ui.label("or + Add game to add one by hand.");
                    } else {
                        ui.heading("Nothing matches");
                        ui.label("Try a different search or filter.");
                    }
                });
                return;
            }
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);
                        for game in &self.games {
                            let is_selected = self.selected == Some(game.id);
                            let running = self.running.contains(&game.id);
                            let response = game_card(ui, game, is_selected, running);
                            if response.double_clicked() && game.installed {
                                actions.push(Action::Play(game.id));
                            } else if response.clicked() {
                                actions.push(Action::Select(game.id));
                            }
                        }
                    });
                });
        });

        for action in actions {
            self.apply(action);
        }
        self.add_game_window(ui.ctx());
    }

    fn details(&mut self, ui: &mut egui::Ui, game: &Game, actions: &mut Vec<Action>) {
        let id = game.id;
        ui.add_space(6.0);
        ui.vertical_centered(|ui| cover(ui, game, CARD_SIZE * 1.3));
        ui.add_space(6.0);

        // Title, with a small edit mode.
        match &mut self.library.title_edit {
            Some((edit_id, text)) if *edit_id == id => {
                let mut save = false;
                let mut cancel = false;
                ui.horizontal(|ui| {
                    let lost_focus = ui.text_edit_singleline(text).lost_focus();
                    save = ui.button("Save").clicked()
                        || (lost_focus && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    cancel = ui.button("Cancel").clicked();
                });
                if save {
                    actions.push(Action::Rename(id, text.clone()));
                }
                if save || cancel {
                    self.library.title_edit = None;
                }
            }
            _ => {
                ui.horizontal_wrapped(|ui| {
                    ui.heading(&game.title);
                    if ui.small_button("Rename").clicked() {
                        self.library.title_edit = Some((id, game.title.clone()));
                    }
                });
            }
        }
        ui.weak(match game.release_year {
            Some(year) => format!("{} · {year}", source_name(game.source)),
            None => source_name(game.source).to_string(),
        });
        ui.add_space(6.0);

        // Play.
        let running = self.running.contains(&id);
        let play_text = if running {
            "Running…"
        } else if game.installed {
            "▶  PLAY"
        } else {
            "Not installed"
        };
        let play = egui::Button::new(egui::RichText::new(play_text).size(18.0).strong())
            .min_size(egui::vec2(ui.available_width(), 40.0));
        if ui.add_enabled(game.installed && !running, play).clicked() {
            actions.push(Action::Play(id));
        }
        if !game.installed
            && matches!(game.launch, LaunchTarget::Exe { .. })
            && ui.button("Locate exe…").clicked()
        {
            actions.push(Action::LocateExe(id));
        }

        ui.add_space(6.0);
        egui::Grid::new("details_facts")
            .num_columns(2)
            .show(ui, |ui| {
                ui.weak("Playtime");
                ui.label(format_duration(game.playtime_secs));
                ui.end_row();
                ui.weak("Last played");
                ui.label(format_last_played(game.last_played, now()));
                ui.end_row();
            });

        ui.add_space(6.0);
        let mut favourite = game.favourite;
        if ui.checkbox(&mut favourite, "★ Favourite").changed() {
            actions.push(Action::SetFavourite(id, favourite));
        }

        // Tags and collections.
        ui.add_space(6.0);
        ui.label(egui::RichText::new("TAGS AND COLLECTIONS").small().weak());
        let tags = self.db.tags_for_game(id).unwrap_or_default();
        let is_finished = tags
            .iter()
            .any(|t| t.name == gamelib::shop::FINISHED_COLLECTION);
        ui.horizontal_wrapped(|ui| {
            for tag in &tags {
                if ui
                    .button(format!("{} ✕", tag.name))
                    .on_hover_text("Remove from this game")
                    .clicked()
                {
                    actions.push(Action::RemoveTag(id, tag.id));
                }
            }
            if tags.is_empty() {
                ui.weak("none yet");
            }
        });
        ui.horizontal(|ui| {
            let edit = egui::TextEdit::singleline(&mut self.library.new_tag)
                .hint_text("New tag…")
                .desired_width(120.0);
            let entered =
                ui.add(edit).lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("Add").clicked() || entered) && !self.library.new_tag.trim().is_empty() {
                actions.push(Action::AddTag(
                    id,
                    std::mem::take(&mut self.library.new_tag),
                    self.library.new_tag_is_collection,
                ));
            }
        });
        ui.checkbox(
            &mut self.library.new_tag_is_collection,
            "as a collection (shows in the sidebar)",
        );

        if !is_finished
            && ui
                .button("Mark as finished")
                .on_hover_text(
                    "Adds it to Finished. Pays ✦50 once, if you've played at least an hour.",
                )
                .clicked()
        {
            actions.push(Action::Finish(id));
        }

        if let Some(description) = &game.description {
            ui.add_space(6.0);
            ui.label(description);
        }
        if !game.genres.is_empty() {
            ui.weak(game.genres.join(" · "));
        }

        // Cover.
        ui.add_space(8.0);
        ui.label(egui::RichText::new("COVER").small().weak());
        if ui.button("Change cover…").clicked() {
            actions.push(Action::ChangeCover(id));
        }
        ui.horizontal(|ui| {
            let edit = egui::TextEdit::singleline(&mut self.library.cover_search)
                .hint_text(&game.title)
                .desired_width(120.0);
            ui.add(edit);
            if ui
                .button("Search again")
                .on_hover_text("Look on SteamGridDB under a different title")
                .clicked()
            {
                let typed = self.library.cover_search.trim();
                let title = if typed.is_empty() {
                    game.title.clone()
                } else {
                    typed.to_string()
                };
                actions.push(Action::SearchCover(id, title));
            }
        });

        // Less common actions.
        ui.add_space(8.0);
        ui.separator();
        let showcased = self.db.showcase().unwrap_or_default().contains(&id);
        if ui
            .button(if showcased {
                "Remove from profile showcase"
            } else {
                "Show on profile"
            })
            .clicked()
        {
            actions.push(Action::ToggleShowcase(id));
        }
        ui.horizontal(|ui| {
            if ui.button(if game.hidden { "Unhide" } else { "Hide" }).clicked() {
                actions.push(Action::SetHidden(id, !game.hidden));
            }
            if self.library.confirm_remove == Some(id) {
                if ui
                    .button(egui::RichText::new("Really remove?").color(ui.visuals().error_fg_color))
                    .on_hover_text("Deletes the game's entry and its playtime history. The game's files are not touched.")
                    .clicked()
                {
                    actions.push(Action::Remove(id));
                }
                if ui.button("Keep").clicked() {
                    self.library.confirm_remove = None;
                }
            } else if ui.button("Remove…").clicked() {
                self.library.confirm_remove = Some(id);
            }
        });
        ui.add_space(8.0);
    }

    fn apply(&mut self, action: Action) {
        let db = &self.db;
        let result = match action {
            Action::Select(id) => {
                if self.selected != Some(id) {
                    self.library.confirm_remove = None;
                    self.library.title_edit = None;
                    self.library.cover_search.clear();
                }
                self.selected = Some(id);
                return;
            }
            Action::Play(id) => {
                self.selected = Some(id);
                if !self.running.contains(&id) {
                    self.running.insert(id);
                    self.start_job(Job::Launch { game_id: id }, "Starting…");
                }
                return;
            }
            Action::SetFavourite(id, on) => db.set_favourite(id, on),
            Action::SetHidden(id, on) => db.set_hidden(id, on),
            Action::Rename(id, title) => db.set_title(id, &title),
            Action::AddTag(id, name, is_collection) => db
                .ensure_tag(&name, is_collection)
                .and_then(|tag| db.tag_game(id, tag.id)),
            Action::RemoveTag(id, tag_id) => db.untag_game(id, tag_id),
            Action::Finish(id) => db.finish_game(id, now()).and_then(|reward| {
                let banners = db.grant_earned_banners(now())?;
                self.status = format!("Marked as finished. +✦{reward}");
                if !banners.is_empty() {
                    self.status.push_str(" New profile banner unlocked!");
                }
                Ok(())
            }),
            Action::Remove(id) => {
                self.selected = None;
                self.library.confirm_remove = None;
                db.delete_game(id)
            }
            Action::LocateExe(id) => {
                match rfd::FileDialog::new()
                    .set_title("Where is the game's exe?")
                    .pick_file()
                {
                    Some(exe) => db.set_launch(id, &LaunchTarget::exe(exe)),
                    None => return,
                }
            }
            Action::ChangeCover(id) => {
                let picked = rfd::FileDialog::new()
                    .set_title("Choose a cover image")
                    .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
                    .pick_file();
                match picked {
                    Some(image) => db.set_cover_manual(id, &image),
                    None => return,
                }
            }
            Action::SearchCover(id, title) => {
                self.start_job(
                    Job::FetchCover {
                        game_id: id,
                        search_title: Some(title),
                    },
                    "Searching for a cover…",
                );
                return;
            }
            Action::ToggleShowcase(id) => db.showcase().and_then(|mut games| {
                match games.iter().position(|g| *g == id) {
                    Some(index) => {
                        games.remove(index);
                    }
                    None => games.push(id),
                }
                db.set_showcase(&games)
            }),
        };
        self.check(result);
    }

    /// The little window that asks for a title after "+ Add game".
    fn add_game_window(&mut self, ctx: &egui::Context) {
        let Some((exe, title)) = &mut self.library.pending_add else {
            return;
        };
        let mut add = false;
        let mut cancel = false;
        egui::Window::new("Add game")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.weak(exe.display().to_string());
                ui.horizontal(|ui| {
                    ui.label("Title");
                    ui.text_edit_singleline(title);
                });
                ui.horizontal(|ui| {
                    add = ui
                        .add_enabled(!title.trim().is_empty(), egui::Button::new("Add"))
                        .clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        if add {
            let (exe, title) = self.library.pending_add.take().unwrap_or_default();
            let exe = gamelib::paths::real_path(&exe);
            if let Some(id) = self.check(self.db.add_manual_game(&title, &exe, now())) {
                self.selected = Some(id);
                self.status = format!("Added {}.", title.trim());
            }
        } else if cancel {
            self.library.pending_add = None;
        }
    }

    /// The import review window, shown after a folder scan: tick the games
    /// to add and fix their titles.
    pub fn import_window(&mut self, ctx: &egui::Context) {
        let Some(rows) = &mut self.import else {
            return;
        };
        let mut add = false;
        let mut cancel = false;
        egui::Window::new("Review games found")
            .collapsible(false)
            .default_size([640.0, 420.0])
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Folder scan is guesswork. Untick anything that isn't a game, and fix any titles.");
                ui.separator();
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    egui::Grid::new("import_rows").num_columns(3).striped(true).show(ui, |ui| {
                        for row in rows.iter_mut() {
                            ui.checkbox(&mut row.ticked, "");
                            ui.add(egui::TextEdit::singleline(&mut row.game.title).desired_width(200.0));
                            ui.weak(&row.game.source_id);
                            ui.end_row();
                        }
                    });
                });
                ui.separator();
                let ticked = rows.iter().filter(|r| r.ticked).count();
                ui.horizontal(|ui| {
                    add = ui
                        .add_enabled(ticked > 0, egui::Button::new(format!("Add selected ({ticked})")))
                        .clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        if add {
            let games = self
                .import
                .take()
                .unwrap_or_default()
                .into_iter()
                .filter(|row| row.ticked && !row.game.title.trim().is_empty())
                .map(|row| row.game)
                .collect();
            self.start_job(Job::Import(games), "Adding games…");
        } else if cancel {
            self.import = None;
        }
    }
}

/// Draw a game's cover, or a coloured tile with its title if it has none.
/// Returns the response so the caller can tell if it was clicked.
fn cover(ui: &mut egui::Ui, game: &Game, size: egui::Vec2) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    match game.cover_path.as_deref().filter(|path| path.is_file()) {
        Some(path) => {
            egui::Image::from_uri(file_uri(path))
                .corner_radius(6.0)
                .paint_at(ui, rect);
        }
        None => {
            let painter = ui.painter();
            painter.rect_filled(rect, 6.0, tile_colour(&game.title));
            let galley = painter.layout(
                game.title.clone(),
                egui::FontId::proportional(15.0),
                egui::Color32::WHITE,
                rect.width() - 16.0,
            );
            let pos = rect.center() - galley.size() / 2.0;
            painter.galley(pos, galley, egui::Color32::WHITE);
        }
    }
    if !game.installed {
        // Grey out games that aren't installed.
        ui.painter()
            .rect_filled(rect, 6.0, egui::Color32::from_black_alpha(150));
    }
    response
}

/// One card in the grid: cover, title, and a line of small print.
fn game_card(ui: &mut egui::Ui, game: &Game, selected: bool, running: bool) -> egui::Response {
    ui.allocate_ui(egui::vec2(CARD_SIZE.x, CARD_SIZE.y + 44.0), |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            let response = cover(ui, game, CARD_SIZE);
            if selected || response.hovered() {
                let colour = if selected {
                    ui.visuals().selection.bg_fill
                } else {
                    ui.visuals().widgets.hovered.bg_stroke.color
                };
                ui.painter().rect_stroke(
                    response.rect.expand(2.0),
                    8.0,
                    egui::Stroke::new(2.0, colour),
                    egui::StrokeKind::Outside,
                );
            }
            let star = if game.favourite { "★ " } else { "" };
            ui.add(egui::Label::new(format!("{star}{}", game.title)).truncate());
            let small = if running {
                "Running…".to_string()
            } else if !game.installed {
                "Not installed".to_string()
            } else if game.playtime_secs > 0 {
                format_duration(game.playtime_secs)
            } else {
                source_name(game.source).to_string()
            };
            ui.add(egui::Label::new(egui::RichText::new(small).small().weak()).truncate());
            response
        })
        .inner
    })
    .inner
}
