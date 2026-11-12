//! The library screen: a header, filter chips, and the grid of covers.

use eframe::egui::{self, Color32, Rect, Sense, Stroke, vec2};
use gamelib::db::SortBy;
use gamelib::model::Game;

use super::theme::{Theme, bold, regular, semibold};
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

        // Header. In Constellation there is no top bar, so the search box
        // and buttons live here.
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                if theme.rail {
                    ui.label(
                        paint::caps("Your universe", 12.5, theme.accent).extra_letter_spacing(3.0),
                    );
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    ui.label(egui::RichText::new("Library").font(bold(32.0)));
                    let count = self.data.games.len();
                    let noun = if count == 1 { "game" } else { "games" };
                    ui.label(paint::muted(format!("{count} {noun}"), &theme).size(19.0));
                });
            });
            if theme.rail {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.add_game_button(ui);
                    self.scan_button(ui);
                    self.stardust_pill(ui);
                    let width = (ui.available_width() - 24.0).clamp(140.0, 420.0);
                    self.search_box(ui, width);
                });
            }
        });
        ui.add_space(10.0);

        // Filter chips on the left, sort on the right.
        let mut chosen: Option<Filter> = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
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
            for (filter, label) in chips {
                if paint::chip(ui, &theme, &label, self.filter == filter).clicked() {
                    chosen = Some(filter);
                }
            }
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
