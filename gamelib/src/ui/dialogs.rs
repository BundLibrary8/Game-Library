//! The pop-up windows: add a game, review a folder scan, edit the
//! profile, and a few small "type a name" prompts.
//!
//! Only one can be open at a time; which one is stored in [`Dialog`].

use std::path::PathBuf;

use eframe::egui::{self, Color32, Sense, Stroke, vec2};
use gamelib::model::DetectedGame;
use gamelib::profile::{self, BADGE_SLOTS, Banner, SHOWCASE_SLOTS};
use gamelib::shop::{self, ItemKind};
use gamelib::worker::Job;
use gamelib::{now, paths};

use super::profile::dashed_slot;
use super::smart::{self as smart_ui, SmartDraft};
use super::theme::{Theme, bold};
use super::{Data, GameLibApp, Screen, paint};

/// One row of the import review window.
pub struct ImportRow {
    pub ticked: bool,
    pub game: DetectedGame,
}

/// The profile as it is being edited. Nothing is saved until
/// "Save profile" is pressed.
pub struct ProfileDraft {
    pub name: String,
    pub bio: String,
    /// The SteamID being typed, when no account is linked.
    pub steam_id: String,
    /// The linked SteamID, if there is one.
    pub linked_steam_id: Option<String>,
    pub avatar: Option<PathBuf>,
    pub banner: Option<Banner>,
    pub showcase: Vec<i64>,
    pub badges: Vec<String>,
}

impl ProfileDraft {
    pub fn from_data(data: &Data) -> ProfileDraft {
        ProfileDraft {
            name: data.profile.display_name.clone(),
            bio: data.profile.bio.clone().unwrap_or_default(),
            steam_id: String::new(),
            linked_steam_id: data.profile.steam_id.clone(),
            avatar: data.profile.avatar_path.clone(),
            banner: data.profile.banner.clone(),
            showcase: data.showcase.clone(),
            badges: data.badges.clone(),
        }
    }
}

pub enum Dialog {
    None,
    /// An exe has been picked; asking for the game's title.
    AddGame {
        exe: PathBuf,
        title: String,
    },
    Import(Vec<ImportRow>),
    /// Which franchise a game belongs to, and where in the order.
    Franchise {
        game_id: i64,
        name: String,
        order: String,
    },
    Rename {
        game_id: i64,
        title: String,
    },
    CoverSearch {
        game_id: i64,
        title: String,
    },
    NewTag {
        game_id: i64,
        name: String,
        as_collection: bool,
    },
    NewCollection {
        name: String,
    },
    ConfirmRemove {
        game_id: i64,
    },
    EditProfile(Box<ProfileDraft>),
    /// Making or editing a smart collection.
    Smart(Box<SmartDraft>),
}

impl Dialog {
    /// Ask for an exe with the system's file picker, then open the window
    /// that asks for the title. Cancelling the picker opens nothing.
    pub fn pick_game_to_add() -> Dialog {
        let picked = rfd::FileDialog::new()
            .set_title("Choose the game's exe")
            .add_filter("Programs", &["exe"])
            .add_filter("All files", &["*"])
            .pick_file();
        match picked {
            Some(exe) => {
                // Guess the title from the folder the exe is in.
                let title = exe
                    .parent()
                    .and_then(|folder| folder.file_name())
                    .or_else(|| exe.file_stem())
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                Dialog::AddGame { exe, title }
            }
            None => Dialog::None,
        }
    }

    pub fn import(candidates: Vec<DetectedGame>) -> Dialog {
        Dialog::Import(
            candidates
                .into_iter()
                .map(|game| ImportRow { ticked: true, game })
                .collect(),
        )
    }
}

/// What the buttons at the bottom of a dialog said.
#[derive(PartialEq)]
enum Outcome {
    StayOpen,
    Confirm,
    Cancel,
}

/// The row of buttons at the bottom of a dialog.
fn dialog_buttons(ui: &mut egui::Ui, theme: &Theme, confirm: &str, can_confirm: bool) -> Outcome {
    let mut outcome = Outcome::StayOpen;
    ui.add_space(10.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let confirmed = ui
            .add_enabled_ui(can_confirm, |ui| {
                paint::accent_button(ui, theme, confirm, 15.0, vec2(22.0, 11.0))
            })
            .inner
            .clicked();
        if confirmed {
            outcome = Outcome::Confirm;
        }
        if ui
            .add(egui::Button::new("Cancel").min_size(vec2(0.0, 40.0)))
            .clicked()
        {
            outcome = Outcome::Cancel;
        }
    });
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        outcome = Outcome::Cancel;
    }
    outcome
}

/// A dialog that asks for one line of text. Enter confirms.
fn text_prompt(
    ui: &mut egui::Ui,
    theme: &Theme,
    heading: &str,
    note: &str,
    text: &mut String,
    confirm: &str,
) -> Outcome {
    ui.set_width(420.0);
    ui.label(egui::RichText::new(heading).font(bold(22.0)));
    if !note.is_empty() {
        ui.label(paint::muted(note, theme));
    }
    ui.add_space(4.0);
    let response = ui.add(
        egui::TextEdit::singleline(text)
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(12, 9)),
    );
    if !response.has_focus() && !response.lost_focus() {
        response.request_focus();
    }
    let entered = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let can_confirm = !text.trim().is_empty();
    let outcome = dialog_buttons(ui, theme, confirm, can_confirm);
    if entered && can_confirm {
        Outcome::Confirm
    } else {
        outcome
    }
}

impl GameLibApp {
    /// Draw whichever dialog is open, and act on its buttons.
    pub fn show_dialog(&mut self, ctx: &egui::Context) {
        if matches!(self.dialog, Dialog::None) {
            return;
        }
        let theme = self.theme;
        // Take the dialog out of `self` while it is drawn, so drawing it
        // can use the rest of `self` freely. It is put back if it stays open.
        let mut dialog = std::mem::replace(&mut self.dialog, Dialog::None);
        let mut outcome = Outcome::StayOpen;

        let frame = egui::Frame::new()
            .fill(theme.opaque_panel())
            .stroke(Stroke::new(1.0, theme.border))
            .corner_radius(16.0)
            .inner_margin(28.0);
        let modal = egui::Modal::new(egui::Id::unique("dialog"))
            .frame(frame)
            .backdrop_color(Color32::from_black_alpha(150))
            .show(ctx, |ui| match &mut dialog {
                Dialog::None => {}
                Dialog::AddGame { exe, title } => {
                    let note = exe.display().to_string();
                    outcome = text_prompt(ui, &theme, "Add game", &note, title, "Add game");
                }
                Dialog::Rename { title, .. } => {
                    outcome = text_prompt(ui, &theme, "Rename game", "", title, "Rename");
                }
                Dialog::CoverSearch { title, .. } => {
                    outcome = text_prompt(
                        ui,
                        &theme,
                        "Search for a cover",
                        "Looks on SteamGridDB under this title. Needs the API key from Settings.",
                        title,
                        "Search",
                    );
                }
                Dialog::NewTag {
                    name,
                    as_collection,
                    ..
                } => {
                    let heading = if *as_collection {
                        "New collection"
                    } else {
                        "Add a tag"
                    };
                    // The checkbox sits above the buttons, so draw the
                    // prompt's parts by hand here.
                    ui.set_width(420.0);
                    ui.label(egui::RichText::new(heading).font(bold(22.0)));
                    let response = ui.add(
                        egui::TextEdit::singleline(name)
                            .hint_text("Roguelite, Co-op nights, Backlog…")
                            .desired_width(f32::INFINITY)
                            .margin(egui::Margin::symmetric(12, 9)),
                    );
                    if !response.has_focus() && !response.lost_focus() {
                        response.request_focus();
                    }
                    let entered =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.checkbox(
                        as_collection,
                        "Make it a collection (shown as a filter in the library)",
                    );
                    let can_confirm = !name.trim().is_empty();
                    outcome = dialog_buttons(ui, &theme, "Add", can_confirm);
                    if entered && can_confirm {
                        outcome = Outcome::Confirm;
                    }
                }
                Dialog::Franchise { name, order, .. } => {
                    ui.set_width(420.0);
                    ui.label(egui::RichText::new("Franchise").font(bold(22.0)));
                    ui.label(paint::muted(
                        "The series this game belongs to. Leave the name empty to take it out of its franchise.",
                        &theme,
                    ));
                    ui.add(
                        egui::TextEdit::singleline(name)
                            .hint_text("Series name")
                            .desired_width(f32::INFINITY)
                            .margin(egui::Margin::symmetric(12, 9)),
                    );
                    ui.horizontal(|ui| {
                        ui.label("Place in the playing order");
                        ui.add(
                            egui::TextEdit::singleline(order)
                                .hint_text("1")
                                .desired_width(60.0)
                                .margin(egui::Margin::symmetric(12, 9)),
                        );
                    });
                    let order_ok = order.trim().is_empty() || order.trim().parse::<i64>().is_ok();
                    outcome = dialog_buttons(ui, &theme, "Save", order_ok);
                }
                Dialog::NewCollection { name } => {
                    outcome = text_prompt(
                        ui,
                        &theme,
                        "New collection",
                        "A collection is a group of games, like Backlog or Co-op nights.",
                        name,
                        "Create",
                    );
                }
                Dialog::ConfirmRemove { game_id } => {
                    ui.set_width(440.0);
                    ui.label(egui::RichText::new("Remove this game?").font(bold(22.0)));
                    ui.label(format!(
                        "{} will be removed from GameLib along with its playtime history. \
                         The game's own files are not touched.",
                        self.title_of(*game_id)
                    ));
                    outcome = dialog_buttons(ui, &theme, "Remove", true);
                }
                Dialog::Import(rows) => outcome = import_review(ui, &theme, rows),
                Dialog::EditProfile(draft) => outcome = self.edit_profile_dialog(ui, draft),
                Dialog::Smart(draft) => {
                    let matching = draft.build().ok().map(|c| {
                        gamelib::smart::filter(
                            &c.rules,
                            &self.data.visible,
                            &self.data.extras,
                            &self.data.all_tags,
                            now(),
                        )
                        .len()
                    });
                    match smart_ui::editor(ui, &theme, draft, matching) {
                        Some(smart_ui::Action::Save) => outcome = Outcome::Confirm,
                        Some(smart_ui::Action::Cancel) => outcome = Outcome::Cancel,
                        Some(smart_ui::Action::Delete) => {
                            if let Some(index) = draft.index {
                                self.delete_smart(index);
                            }
                            outcome = Outcome::Cancel;
                        }
                        None => {}
                    }
                }
            });
        if modal.should_close() && outcome == Outcome::StayOpen {
            outcome = Outcome::Cancel;
        }

        match outcome {
            Outcome::StayOpen => self.dialog = dialog,
            Outcome::Cancel => {}
            Outcome::Confirm => self.confirm_dialog(dialog),
        }
    }

    /// Carry out a confirmed dialog. If it fails, the error is shown and
    /// (for the profile) the dialog stays open so nothing typed is lost.
    fn confirm_dialog(&mut self, dialog: Dialog) {
        match dialog {
            Dialog::None => {}
            Dialog::AddGame { exe, title } => {
                let exe = paths::real_path(&exe);
                if let Some(id) = self.check(self.db.add_manual_game(&title, &exe, now())) {
                    self.status = format!("Added {}.", title.trim());
                    self.go(Screen::Details(id));
                    self.start_job(
                        Job::FetchMissingCovers { retry_all: false },
                        "Getting covers and info…",
                    );
                }
            }
            Dialog::Rename { game_id, title } => {
                self.check(self.db.set_title(game_id, &title));
            }
            Dialog::CoverSearch { game_id, title } => {
                self.start_job(
                    Job::FetchCover {
                        game_id,
                        search_title: Some(title.trim().to_string()),
                    },
                    "Searching for a cover…",
                );
            }
            Dialog::NewTag {
                game_id,
                name,
                as_collection,
            } => {
                let result = self
                    .db
                    .ensure_tag(&name, as_collection)
                    .and_then(|tag| self.db.tag_game(game_id, tag.id));
                self.check(result);
            }
            Dialog::Franchise {
                game_id,
                name,
                order,
            } => {
                let order = order.trim().parse::<i64>().ok();
                self.check(self.db.set_franchise(game_id, Some(&name), order));
            }
            Dialog::NewCollection { name } => {
                if let Some(tag) = self.check(self.db.ensure_tag(&name, true)) {
                    self.status = format!(
                        "Created {}. Add games to it from a game's ⋯ menu.",
                        tag.name
                    );
                }
            }
            Dialog::ConfirmRemove { game_id } => {
                if self.check(self.db.delete_game(game_id)).is_some() {
                    self.go(Screen::Library);
                }
            }
            Dialog::Import(rows) => {
                let games: Vec<DetectedGame> = rows
                    .into_iter()
                    .filter(|row| row.ticked && !row.game.title.trim().is_empty())
                    .map(|row| row.game)
                    .collect();
                self.start_job(Job::Import(games), "Adding games…");
            }
            Dialog::Smart(draft) => {
                let Ok(collection) = draft.build() else {
                    return;
                };
                let mut list = self.data.smart.clone();
                let index = match draft.index.filter(|i| *i < list.len()) {
                    Some(i) => {
                        list[i] = collection;
                        i
                    }
                    None => {
                        list.push(collection);
                        list.len() - 1
                    }
                };
                if self.check(self.db.set_smart_collections(&list)).is_some() {
                    self.show_filter(super::Filter::Smart(index));
                }
            }
            Dialog::EditProfile(draft) => {
                if let Err(err) = self.save_profile(&draft) {
                    self.error = Some(format!("{err:#}"));
                    self.dialog = Dialog::EditProfile(draft);
                } else {
                    self.status = "Profile saved.".to_string();
                }
                self.dirty = true;
            }
        }
    }

    fn save_profile(&self, draft: &ProfileDraft) -> anyhow::Result<()> {
        let db = &self.db;
        let now = now();
        if draft.name.trim() != self.data.profile.display_name {
            db.set_display_name(&draft.name, now)?;
        }
        db.set_bio(&draft.bio, now)?;
        db.set_avatar_path(draft.avatar.as_deref(), now)?;
        let steam_id = draft
            .linked_steam_id
            .as_deref()
            .or(Some(draft.steam_id.trim()).filter(|id| !id.is_empty()));
        db.set_steam_id(steam_id, now)?;
        db.set_banner(draft.banner.as_ref(), now)?;
        db.set_showcase(&draft.showcase)?;
        let badges: Vec<&str> = draft.badges.iter().map(String::as_str).collect();
        db.set_badges(&badges)?;
        Ok(())
    }

    /// The edit-profile window: picture, name, bio and Steam account on
    /// the left; banner, badges and showcase on the right.
    fn edit_profile_dialog(&mut self, ui: &mut egui::Ui, draft: &mut ProfileDraft) -> Outcome {
        let theme = self.theme;
        let ctx = ui.ctx().clone();
        ui.set_width(1040.0_f32.min(ctx.content_rect().width() - 120.0));
        ui.label(egui::RichText::new("Edit profile").font(bold(24.0)));
        ui.add_space(8.0);

        let tile = vec2(170.0, 72.0);
        egui::ScrollArea::vertical()
            .max_height(ctx.content_rect().height() - 260.0)
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 34.0;

                    // Left column.
                    ui.vertical(|ui| {
                        ui.set_width(300.0);
                        ui.vertical_centered(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(120.0, 120.0), Sense::hover());
                            paint::avatar(
                                ui,
                                rect.center(),
                                60.0,
                                &draft.name,
                                draft.avatar.as_deref(),
                                &theme,
                            );
                            if ui.button("Change picture…").clicked() {
                                let picked = rfd::FileDialog::new()
                                    .set_title("Choose a profile picture")
                                    .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
                                    .pick_file();
                                if let Some(source) = picked {
                                    // Copy it into GameLib's own folder so it
                                    // keeps working if the original moves.
                                    let copied = paths::profile_dir().and_then(|dir| {
                                        profile::import_image(&source, &dir, "avatar")
                                    });
                                    match copied {
                                        Ok(path) => {
                                            ctx.forget_image(&paint::file_uri(&path));
                                            draft.avatar = Some(path);
                                        }
                                        Err(err) => self.error = Some(format!("{err:#}")),
                                    }
                                }
                            }
                        });

                        ui.add_space(4.0);
                        ui.label(paint::caps("Display name", 11.5, theme.muted));
                        ui.add(
                            egui::TextEdit::singleline(&mut draft.name)
                                .char_limit(profile::MAX_NAME_CHARS)
                                .desired_width(f32::INFINITY)
                                .margin(egui::Margin::symmetric(12, 9)),
                        );
                        if draft.name.trim() != self.data.profile.display_name {
                            let note = if !self.data.profile.name_chosen {
                                "Your first name is free.".to_string()
                            } else if self.data.name_credits > 0 {
                                format!("Uses 1 of your {} Name changes.", self.data.name_credits)
                            } else {
                                "Changing it again needs a Name change from the Shop.".to_string()
                            };
                            ui.label(paint::muted(note, &theme).size(13.0));
                        }

                        ui.label(paint::caps("Bio", 11.5, theme.muted));
                        ui.add(
                            egui::TextEdit::multiline(&mut draft.bio)
                                .char_limit(profile::MAX_BIO_CHARS)
                                .desired_rows(3)
                                .desired_width(f32::INFINITY)
                                .margin(egui::Margin::symmetric(12, 9)),
                        );

                        ui.label(paint::caps(
                            "Steam account (for friends)",
                            11.5,
                            theme.muted,
                        ));
                        match draft.linked_steam_id.clone() {
                            Some(id) => {
                                ui.horizontal(|ui| {
                                    let (dot, _) =
                                        ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                                    ui.painter().circle_filled(dot.center(), 5.0, theme.success);
                                    let short = if id.len() > 11 {
                                        format!("{}…{}", &id[..7], &id[id.len() - 4..])
                                    } else {
                                        id
                                    };
                                    ui.label(format!("Linked · {short}"));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.small_button("Unlink").clicked() {
                                                draft.linked_steam_id = None;
                                            }
                                        },
                                    );
                                });
                            }
                            None => {
                                ui.add(
                                    egui::TextEdit::singleline(&mut draft.steam_id)
                                        .hint_text("Your 17-digit SteamID")
                                        .desired_width(f32::INFINITY)
                                        .margin(egui::Margin::symmetric(12, 9)),
                                );
                            }
                        }
                    });

                    // Right column.
                    ui.vertical(|ui| {
                        ui.label(paint::caps("Banner", 11.5, theme.muted));
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = vec2(14.0, 6.0);
                            // Your own image.
                            ui.vertical(|ui| {
                                let (rect, response) = ui.allocate_exact_size(tile, Sense::click());
                                let uploaded = matches!(draft.banner, Some(Banner::Upload(_)));
                                if uploaded {
                                    paint::banner(
                                        ui,
                                        rect,
                                        draft.banner.as_ref(),
                                        &[],
                                        &theme,
                                        9.0,
                                    );
                                    selected_ring(ui, rect, &theme);
                                } else {
                                    dashed_slot(ui, rect, &theme, "⬆ Upload", response.hovered());
                                }
                                if response
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    let picked = rfd::FileDialog::new()
                                        .set_title("Choose a banner image")
                                        .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
                                        .pick_file();
                                    if let Some(source) = picked {
                                        let copied = paths::profile_dir().and_then(|dir| {
                                            profile::import_image(&source, &dir, "banner")
                                        });
                                        match copied {
                                            Ok(path) => {
                                                ctx.forget_image(&paint::file_uri(&path));
                                                draft.banner = Some(Banner::Upload(path));
                                            }
                                            Err(err) => self.error = Some(format!("{err:#}")),
                                        }
                                    }
                                }
                                ui.label(paint::muted("Your own image", &theme).size(13.5));
                            });
                            // Art from the showcase games.
                            for game_id in draft.showcase.clone() {
                                let Some(game) = self.data.game(game_id) else {
                                    continue;
                                };
                                let choice = Banner::Game(game_id);
                                ui.vertical(|ui| {
                                    ui.set_width(tile.x);
                                    let (rect, response) =
                                        ui.allocate_exact_size(tile, Sense::click());
                                    paint::banner(
                                        ui,
                                        rect,
                                        Some(&choice),
                                        &self.data.visible,
                                        &theme,
                                        9.0,
                                    );
                                    if draft.banner.as_ref() == Some(&choice) {
                                        selected_ring(ui, rect, &theme);
                                    }
                                    if response
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .clicked()
                                    {
                                        draft.banner = Some(choice.clone());
                                    }
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&game.title).size(13.5),
                                        )
                                        .truncate(),
                                    );
                                });
                            }
                            // Banners bought in the shop.
                            for item in shop::CATALOGUE
                                .iter()
                                .filter(|i| i.kind == ItemKind::ProfileBanner && i.price.is_some())
                                .filter(|i| self.data.owned.iter().any(|id| id == i.id))
                            {
                                let choice = Banner::Item(item.id.to_string());
                                ui.vertical(|ui| {
                                    let (rect, response) =
                                        ui.allocate_exact_size(tile, Sense::click());
                                    paint::banner(ui, rect, Some(&choice), &[], &theme, 9.0);
                                    if draft.banner.as_ref() == Some(&choice) {
                                        selected_ring(ui, rect, &theme);
                                    }
                                    if response
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .clicked()
                                    {
                                        draft.banner = Some(choice.clone());
                                    }
                                    ui.label(egui::RichText::new(item.name).size(13.5));
                                });
                            }
                        });
                        if draft.banner.is_some() && ui.small_button("No banner").clicked() {
                            draft.banner = None;
                        }

                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(paint::caps("Earned banners", 11.5, theme.muted));
                            ui.label(paint::muted("· unlocked by playing", &theme).size(13.0));
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = vec2(14.0, 6.0);
                            for earned in profile::EARNED_ITEMS {
                                let Some(item) = shop::find_item(earned.item_id) else {
                                    continue;
                                };
                                if item.kind != ItemKind::ProfileBanner {
                                    continue;
                                }
                                let unlocked = self.data.owned.iter().any(|id| id == item.id);
                                let choice = Banner::Item(item.id.to_string());
                                ui.vertical(|ui| {
                                    ui.set_width(tile.x);
                                    let sense = if unlocked {
                                        Sense::click()
                                    } else {
                                        Sense::hover()
                                    };
                                    let (rect, response) = ui.allocate_exact_size(tile, sense);
                                    paint::banner(ui, rect, Some(&choice), &[], &theme, 9.0);
                                    if !unlocked {
                                        ui.painter().rect_filled(
                                            rect,
                                            9.0,
                                            Color32::from_black_alpha(150),
                                        );
                                        ui.painter().text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "🔒",
                                            egui::FontId::proportional(22.0),
                                            Color32::from_rgb(0xff, 0xcf, 0x4a),
                                        );
                                    } else if draft.banner.as_ref() == Some(&choice) {
                                        selected_ring(ui, rect, &theme);
                                    }
                                    if unlocked
                                        && response
                                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                                            .clicked()
                                    {
                                        draft.banner = Some(choice.clone());
                                    }
                                    let goal = earned.goal.describe();
                                    let text = if unlocked {
                                        format!("{goal} ✓")
                                    } else {
                                        goal
                                    };
                                    let colour = if unlocked { theme.text } else { theme.muted };
                                    ui.label(egui::RichText::new(text).size(13.5).color(colour));
                                });
                            }
                        });

                        ui.add_space(6.0);
                        ui.label(paint::caps(
                            &format!("Badges · show up to {BADGE_SLOTS}"),
                            11.5,
                            theme.muted,
                        ));
                        ui.horizontal_wrapped(|ui| {
                            let owned_badges: Vec<_> = shop::CATALOGUE
                                .iter()
                                .filter(|i| i.kind == ItemKind::Badge)
                                .filter(|i| self.data.owned.iter().any(|id| id == i.id))
                                .collect();
                            if owned_badges.is_empty() {
                                ui.label(paint::muted("You don't have any badges yet.", &theme));
                            }
                            for item in owned_badges {
                                let shown = draft.badges.iter().any(|id| id == item.id);
                                if paint::chip(ui, &theme, item.name, shown).clicked() {
                                    if shown {
                                        draft.badges.retain(|id| id != item.id);
                                    } else if draft.badges.len() < BADGE_SLOTS {
                                        draft.badges.push(item.id.to_string());
                                    }
                                }
                            }
                        });

                        ui.add_space(6.0);
                        ui.label(paint::caps("Showcase games", 11.5, theme.muted));
                        let slot = vec2(92.0, 138.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 14.0;
                            let mut remove = None;
                            for (index, game_id) in draft.showcase.iter().enumerate() {
                                let Some(game) = self.data.game(*game_id) else {
                                    continue;
                                };
                                let (rect, response) = ui.allocate_exact_size(slot, Sense::click());
                                paint::cover(ui, rect, game, 10.0, 13.0);
                                if response
                                    .on_hover_text("Click to remove")
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    remove = Some(index);
                                }
                            }
                            if let Some(index) = remove {
                                draft.showcase.remove(index);
                            }
                            if draft.showcase.len() < SHOWCASE_SLOTS {
                                let add = egui::Button::new(
                                    egui::RichText::new("+").size(22.0).color(theme.muted),
                                )
                                .min_size(slot)
                                .corner_radius(10.0);
                                egui::containers::menu::MenuButton::from_button(add).ui(ui, |ui| {
                                    ui.set_min_width(240.0);
                                    egui::ScrollArea::vertical()
                                        .max_height(300.0)
                                        .show(ui, |ui| {
                                            for game in &self.data.visible {
                                                if draft.showcase.contains(&game.id) {
                                                    continue;
                                                }
                                                if ui.button(&game.title).clicked() {
                                                    draft.showcase.push(game.id);
                                                    ui.close();
                                                }
                                            }
                                        });
                                });
                            }
                        });
                    });
                });
            });

        dialog_buttons(ui, &theme, "Save profile", !draft.name.trim().is_empty())
    }
}

/// An accent-coloured ring around the chosen banner.
fn selected_ring(ui: &egui::Ui, rect: egui::Rect, theme: &Theme) {
    ui.painter().rect_stroke(
        rect,
        9.0,
        Stroke::new(2.5, theme.accent),
        egui::StrokeKind::Outside,
    );
}

/// The import review window, shown after a folder scan: tick the games
/// to add and fix their titles.
fn import_review(ui: &mut egui::Ui, theme: &Theme, rows: &mut [ImportRow]) -> Outcome {
    ui.set_width(720.0);
    ui.label(egui::RichText::new("Review games found").font(bold(22.0)));
    ui.label(paint::muted(
        "Folder scan is guesswork. Untick anything that isn't a game, and fix any titles.",
        theme,
    ));
    ui.add_space(6.0);
    egui::ScrollArea::vertical()
        .max_height(360.0)
        .show(ui, |ui| {
            egui::Grid::new("import_rows")
                .num_columns(3)
                .striped(true)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    for row in rows.iter_mut() {
                        ui.checkbox(&mut row.ticked, "");
                        ui.add(
                            egui::TextEdit::singleline(&mut row.game.title).desired_width(230.0),
                        );
                        ui.add(
                            egui::Label::new(paint::muted(&row.game.source_id, theme).size(13.0))
                                .truncate(),
                        );
                        ui.end_row();
                    }
                });
        });
    let ticked = rows.iter().filter(|r| r.ticked).count();
    let noun = if ticked == 1 { "game" } else { "games" };
    dialog_buttons(ui, theme, &format!("Add {ticked} {noun}"), ticked > 0)
}
