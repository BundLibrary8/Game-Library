//! The library screen: a header, filter chips, and the grid of covers.

use eframe::egui::{self, Color32, Rect, Sense, Stroke, vec2};
use gamelib::db::SortBy;
use gamelib::model::{Game, Source};

use super::dialogs::Dialog;
use super::smart::SmartDraft;
use super::theme::{Theme, bold, regular, semibold, source_name};
use super::{Filter, GameLibApp, Screen, paint};
use crate::text::played_label;

/// What a click on a game card asked for.
pub enum CardClick {
    Open(i64),
    Play(i64),
}

fn sort_name(sort: SortBy) -> &'static str {
    match sort {
        SortBy::Title => "Title",
        SortBy::LastPlayed => "Last played",
        SortBy::Playtime => "Most played",
        SortBy::RecentlyAdded => "Recently added",
    }
}

impl GameLibApp {
    pub fn library_screen(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;

        // Header: title on the left; search and the main buttons on the right.
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(
                    paint::caps(theme.tagline(), 12.5, theme.accent).extra_letter_spacing(3.0),
                );
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    ui.label(egui::RichText::new("Library").font(bold(32.0)));
                    let count = self.data.games.len();
                    let noun = if count == 1 { "game" } else { "games" };
                    ui.label(paint::muted(format!("{count} {noun}"), &theme).size(19.0));
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.add_game_button(ui);
                self.scan_button(ui);
                self.stardust_pill(ui);
                let width = (ui.available_width() - 24.0).clamp(140.0, 420.0);
                self.search_box(ui, width);
            });
        });
        ui.add_space(10.0);

        // Filter chips on the left, sort on the right.
        let mut chosen: Option<Filter> = None;
        let mut new_collection = false;
        let mut new_smart = false;
        let mut edit_smart: Option<usize> = None;
        let mut add_preset: Option<gamelib::smart::SmartCollection> = None;
        ui.horizontal(|ui| {
            // The chips wrap onto a second line when there are lots of them,
            // leaving room for the sort menu on the right.
            let chips_width = (ui.available_width() - 230.0).max(200.0);
            ui.allocate_ui(vec2(chips_width, 0.0), |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 8.0);
                    let mut chips: Vec<(Filter, String)> = vec![
                        (Filter::All, "All".to_string()),
                        (Filter::Installed, "Installed".to_string()),
                        (Filter::Favourites, "Favourites ★".to_string()),
                    ];
                    chips.extend(
                        self.data
                            .collections()
                            .map(|(tag, _)| (Filter::Tag(tag.id), tag.name.clone())),
                    );
                    chips.extend(
                        self.data
                            .smart
                            .iter()
                            .enumerate()
                            .map(|(i, c)| (Filter::Smart(i), c.name.clone())),
                    );
                    // A filter picked from "More" that has no chip of its own gets
                    // a chip while it is in use, so you can see what's applied.
                    if !chips.iter().any(|(filter, _)| *filter == self.filter) {
                        let label = match self.filter {
                            Filter::RecentlyPlayed => "Recently played".to_string(),
                            Filter::Hidden => "Hidden".to_string(),
                            Filter::Source(source) => source_name(source).to_string(),
                            Filter::Tag(id) => self
                                .data
                                .tags
                                .iter()
                                .find(|(tag, _)| tag.id == id)
                                .map(|(tag, _)| tag.name.clone())
                                .unwrap_or_else(|| "Tag".to_string()),
                            _ => String::new(),
                        };
                        chips.push((self.filter, label));
                    }
                    for (filter, label) in chips {
                        let mut chip = paint::chip(ui, &theme, &label, self.filter == filter);
                        if let Filter::Smart(i) = filter
                            && let Some(collection) = self.data.smart.get(i)
                        {
                            chip = chip.on_hover_text(format!(
                                "Smart collection: {}. Right-click to edit.",
                                collection.rules.describe()
                            ));
                            if chip.secondary_clicked() {
                                edit_smart = Some(i);
                            }
                        }
                        if chip.clicked() {
                            chosen = Some(filter);
                        }
                    }
                    if let Filter::Smart(i) = self.filter
                        && ui.button("Edit rules").clicked()
                    {
                        edit_smart = Some(i);
                    }
                    // Everything else you can filter by, and making a collection.
                    ui.menu_button("More…", |ui| {
                        ui.set_min_width(210.0);
                        let visible = &self.data.visible;
                        let mut row = |ui: &mut egui::Ui, filter: Filter, label: String| {
                            if ui.selectable_label(self.filter == filter, label).clicked() {
                                chosen = Some(filter);
                                ui.close();
                            }
                        };
                        let recent = visible.iter().filter(|g| g.last_played.is_some()).count();
                        row(
                            ui,
                            Filter::RecentlyPlayed,
                            format!("Recently played ({recent})"),
                        );
                        if self.data.hidden_count > 0 {
                            row(
                                ui,
                                Filter::Hidden,
                                format!("Hidden ({})", self.data.hidden_count),
                            );
                        }
                        ui.separator();
                        ui.label(paint::caps("Sources", 11.0, theme.muted));
                        for source in Source::ALL {
                            let count = visible.iter().filter(|g| g.source == source).count();
                            if count > 0 {
                                row(
                                    ui,
                                    Filter::Source(source),
                                    format!("{} ({count})", source_name(source)),
                                );
                            }
                        }
                        let tags: Vec<_> = self
                            .data
                            .tags
                            .iter()
                            .filter(|(t, _)| !t.is_collection)
                            .collect();
                        if !tags.is_empty() {
                            ui.separator();
                            ui.label(paint::caps("Tags", 11.0, theme.muted));
                            for (tag, count) in tags {
                                row(ui, Filter::Tag(tag.id), format!("{} ({count})", tag.name));
                            }
                        }
                        ui.separator();
                        if ui.button("+ New collection…").clicked() {
                            new_collection = true;
                            ui.close();
                        }
                        if ui
                            .button("+ New smart collection…")
                            .on_hover_text("A saved filter, like \"short games I haven't played\"")
                            .clicked()
                        {
                            new_smart = true;
                            ui.close();
                        }
                        let presets: Vec<_> = gamelib::smart::presets()
                            .into_iter()
                            .filter(|p| !self.data.smart.iter().any(|s| s.name == p.name))
                            .collect();
                        if !presets.is_empty() {
                            ui.menu_button("Add a ready-made one", |ui| {
                                for preset in presets {
                                    if ui
                                        .button(&preset.name)
                                        .on_hover_text(preset.rules.describe())
                                        .clicked()
                                    {
                                        add_preset = Some(preset);
                                        ui.close();
                                    }
                                }
                            });
                        }
                    });
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let before = self.sort;
                egui::ComboBox::from_id_salt("sort")
                    .selected_text(format!("Sort: {}", sort_name(self.sort)))
                    .show_ui(ui, |ui| {
                        for sort in [
                            SortBy::LastPlayed,
                            SortBy::Title,
                            SortBy::Playtime,
                            SortBy::RecentlyAdded,
                        ] {
                            ui.selectable_value(&mut self.sort, sort, sort_name(sort));
                        }
                    });
                self.dirty |= self.sort != before;
            });
        });
        if let Some(filter) = chosen {
            self.show_filter(filter);
        }
        if new_smart {
            self.dialog = Dialog::Smart(Box::new(SmartDraft::new()));
        }
        if let Some(index) = edit_smart
            && let Some(collection) = self.data.smart.get(index)
        {
            self.dialog = Dialog::Smart(Box::new(SmartDraft::from(Some(index), collection)));
        }
        if let Some(preset) = add_preset {
            let mut list = self.data.smart.clone();
            list.push(preset);
            if self.check(self.db.set_smart_collections(&list)).is_some() {
                self.show_filter(Filter::Smart(list.len() - 1));
            }
        }
        if new_collection {
            self.dialog = Dialog::NewCollection {
                name: String::new(),
            };
        }
        ui.add_space(16.0);

        if self.data.games.is_empty() {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                if self.data.visible.is_empty() {
                    ui.label(egui::RichText::new("No games yet").font(bold(24.0)));
                    ui.label(paint::muted(
                        "Use Scan to find your Steam, Epic and GOG games, or Add game to add one by hand.",
                        &theme,
                    ));
                } else {
                    ui.label(egui::RichText::new("Nothing matches").font(bold(24.0)));
                    ui.label(paint::muted("Try a different search or filter.", &theme));
                }
            });
            return;
        }

        let click = game_grid(ui, &theme, &self.data.games, &self.running, 190.0, None);
        match click {
            Some(CardClick::Open(id)) => self.go(Screen::Details(id)),
            Some(CardClick::Play(id)) => self.play(id),
            None => {}
        }
    }
}

/// Draw games as a grid of cards that fills the width. `min_width` is the
/// narrowest a card may be; `max_rows` limits the grid (for Home's
/// "Recently added" row).
pub fn game_grid(
    ui: &mut egui::Ui,
    theme: &Theme,
    games: &[Game],
    running: &std::collections::HashSet<i64>,
    min_width: f32,
    max_rows: Option<usize>,
) -> Option<CardClick> {
    let gap = 22.0;
    let available = ui.available_width();
    let columns = (((available + gap) / (min_width + gap)).floor() as usize).max(1);
    let card_width = ((available - gap * (columns as f32 - 1.0)) / columns as f32).floor();
    let cover_size = vec2(card_width, card_width * 1.5);

    let mut click = None;
    for row in games.chunks(columns).take(max_rows.unwrap_or(usize::MAX)) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for game in row {
                if let Some(c) = game_card(ui, theme, game, cover_size, running.contains(&game.id))
                {
                    click = Some(c);
                }
            }
        });
        ui.add_space(14.0);
    }
    click
}

/// One card: the cover with its source badge, then the title and
/// playtime underneath. Hovering shows a play button.
fn game_card(
    ui: &mut egui::Ui,
    theme: &Theme,
    game: &Game,
    cover_size: egui::Vec2,
    running: bool,
) -> Option<CardClick> {
    let text_height = 50.0;
    let (rect, response) = ui.allocate_exact_size(
        vec2(cover_size.x, cover_size.y + text_height),
        Sense::click(),
    );
    if !ui.is_rect_visible(rect) {
        return None;
    }
    let cover_rect = Rect::from_min_size(rect.min, cover_size);
    let title_size = (cover_size.x / 11.0).clamp(15.0, 22.0);
    paint::cover(ui, cover_rect, game, 16.0, title_size);
    paint::source_badge(ui, cover_rect, game);

    let painter = ui.painter();
    if game.favourite {
        painter.text(
            cover_rect.right_top() + vec2(-14.0, 12.0),
            egui::Align2::RIGHT_TOP,
            "★",
            regular(17.0),
            Color32::from_rgb(0xff, 0xcf, 0x4a),
        );
    }

    // Title and playtime under the cover.
    let title = painter.layout_no_wrap(game.title.clone(), semibold(16.0), theme.text);
    let clip = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    clip.galley(cover_rect.left_bottom() + vec2(0.0, 9.0), title, theme.text);
    let small = if running {
        "Running…".to_string()
    } else if !game.installed {
        "Not installed".to_string()
    } else {
        played_label(game.playtime_secs)
    };
    clip.text(
        cover_rect.left_bottom() + vec2(0.0, 31.0),
        egui::Align2::LEFT_TOP,
        small,
        regular(13.5),
        if running { theme.success } else { theme.muted },
    );

    let mut click = None;
    if response.hovered() {
        painter.rect_stroke(
            cover_rect,
            16.0,
            Stroke::new(2.0, theme.accent),
            egui::StrokeKind::Outside,
        );
        // The play button: a circle in the bottom-right of the cover.
        if game.installed && !running {
            let centre = cover_rect.right_bottom() - vec2(34.0, 34.0);
            let play_rect = Rect::from_center_size(centre, vec2(48.0, 48.0));
            paint::gradient(
                painter,
                play_rect,
                24.0,
                theme.accent,
                theme.accent_end,
                vec2(1.0, 0.0),
            );
            paint::play_triangle(painter, centre, 16.0, theme.on_accent);
            let over_play = ui
                .input(|i| i.pointer.hover_pos())
                .is_some_and(|pos| play_rect.contains(pos));
            if response.clicked() && over_play {
                click = Some(CardClick::Play(game.id));
            }
        }
    }
    if click.is_none() && response.clicked() {
        click = Some(CardClick::Open(game.id));
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand);
    click
}
