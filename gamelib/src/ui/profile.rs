//! The profile screen: banner, avatar, badges, stats, showcase, friends.

use eframe::egui::{self, Color32, Rect, Sense, Stroke, vec2};
use gamelib::friends::Friend;
use gamelib::now;
use gamelib::shop;
use gamelib::worker::Job;

use super::dialogs::{Dialog, ProfileDraft};
use super::theme::{Theme, bold, regular, semibold};
use super::{GameLibApp, Screen, paint};
use crate::text::{format_ago, short_playtime};

const BANNER_HEIGHT: f32 = 250.0;
const AVATAR_RADIUS: f32 = 72.0;
const SIDE_MARGIN: f32 = 38.0;

impl GameLibApp {
    /// Open the edit window, filled in with the current profile.
    pub fn edit_profile(&mut self) {
        self.dialog = Dialog::EditProfile(Box::new(ProfileDraft::from_data(&self.data)));
    }

    pub fn profile_screen(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let profile = self.data.profile.clone();
        let mut edit = false;
        let mut open_game: Option<i64> = None;

        // The banner runs edge to edge across the top.
        let (banner_rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), BANNER_HEIGHT), Sense::hover());
        paint::banner(
            ui,
            banner_rect,
            profile.banner.as_ref(),
            &self.data.visible,
            &theme,
            0.0,
        );
        // Fade the bottom of the banner into the page.
        let fade = Rect::from_min_max(
            egui::pos2(banner_rect.left(), banner_rect.bottom() - 90.0),
            banner_rect.max,
        );
        let page = theme.opaque(theme.bg);
        let [r, g, b, _] = page.to_array();
        paint::gradient(
            ui.painter(),
            fade,
            0.0,
            Color32::from_rgba_unmultiplied(r, g, b, 0),
            Color32::from_rgba_unmultiplied(r, g, b, 235),
            vec2(0.0, 1.0),
        );
        let change = Rect::from_min_size(
            egui::pos2(banner_rect.right() - 196.0, banner_rect.top() + 24.0),
            vec2(170.0, 40.0),
        );
        // Placed by hand over the banner, in its own little area so the
        // rest of the page's layout isn't disturbed.
        let mut over_banner = ui.new_child(egui::UiBuilder::new().max_rect(change));
        if over_banner
            .add_sized(
                change.size(),
                egui::Button::new(egui::RichText::new("🖼 Change banner").font(semibold(14.5))),
            )
            .clicked()
        {
            edit = true;
        }

        // The avatar overlaps the bottom of the banner.
        ui.add_space(-AVATAR_RADIUS - 4.0);
        ui.horizontal(|ui| {
            ui.add_space(SIDE_MARGIN + 8.0);
            let (rect, _) = ui.allocate_exact_size(
                vec2(AVATAR_RADIUS * 2.0 + 8.0, AVATAR_RADIUS * 2.0 + 8.0),
                Sense::hover(),
            );
            ui.painter()
                .circle_filled(rect.center(), AVATAR_RADIUS + 5.0, page);
            paint::avatar(
                ui,
                rect.center(),
                AVATAR_RADIUS,
                &profile.display_name,
                profile.avatar_path.as_deref(),
                &theme,
            );
            ui.add_space(22.0);

            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.add_space(34.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&profile.display_name).font(bold(34.0)));
                    ui.add_space(6.0);
                    for badge in &self.data.badges {
                        let name = shop::find_item(badge).map(|item| item.name).unwrap_or("?");
                        let (colour, _) = paint::item_colours(badge);
                        paint::badge_pill(ui, name, colour);
                    }
                });
                // "Playing Ember Road", from the playtime tracker.
                let playing = self.running.iter().next().map(|id| self.title_of(*id));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 7.0;
                    let (dot, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                    let (colour, text) = match &playing {
                        Some(title) => (theme.success, format!("Playing {title}")),
                        None => (theme.muted, "Not in a game".to_string()),
                    };
                    ui.painter().circle_filled(dot.center(), 5.5, colour);
                    ui.label(egui::RichText::new(text).font(semibold(15.5)).color(colour));
                });
                if let Some(bio) = &profile.bio {
                    ui.label(
                        egui::RichText::new(bio)
                            .size(16.0)
                            .color(theme.text.gamma_multiply(0.88)),
                    );
                }
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(SIDE_MARGIN);
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("✎ Edit profile").font(semibold(15.5)),
                        )
                        .min_size(vec2(0.0, 42.0)),
                    )
                    .clicked()
                {
                    edit = true;
                }
            });
        });
        ui.add_space(16.0);

        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(SIDE_MARGIN as i8, 0))
            .show(ui, |ui| {
                let stats = &self.data.stats;
                let most_played = stats
                    .most_played
                    .as_ref()
                    .map(|(title, _)| title.clone())
                    .unwrap_or_else(|| "Nothing yet".to_string());
                paint::stat_row(
                    ui,
                    &theme,
                    &[
                        ("Total playtime", short_playtime(stats.total_playtime_secs)),
                        ("Games", stats.game_count.to_string()),
                        ("Most played", most_played),
                        ("Finished", format!("{} games", stats.finished_count)),
                    ],
                );
                ui.add_space(18.0);

                let gap = 18.0;
                let friends_width = (ui.available_width() * 0.38).clamp(300.0, 600.0);
                let showcase_width = ui.available_width() - friends_width - gap;
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;

                    // Showcase.
                    paint::panel(ui, theme.panel_frame().inner_margin(24.0), |ui| {
                        ui.set_width(showcase_width - 50.0);
                        ui.set_min_height(400.0);
                        ui.horizontal(|ui| {
                            ui.label(paint::strong("Showcase", 20.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(paint::muted("Pick up to 4 games", &theme));
                            });
                        });
                        ui.add_space(6.0);
                        let slot_gap = 18.0;
                        let slot_width = ((showcase_width - 50.0 - slot_gap * 3.0) / 4.0).floor().min(190.0);
                        let slot = vec2(slot_width, slot_width * 1.5);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = slot_gap;
                            for game_id in &self.data.showcase {
                                let Some(game) = self.data.game(*game_id) else { continue };
                                let (rect, response) = ui.allocate_exact_size(slot, Sense::click());
                                paint::cover(ui, rect, game, 12.0, (slot_width / 10.0).clamp(13.0, 17.0));
                                if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    open_game = Some(*game_id);
                                }
                            }
                            if self.data.showcase.len() < gamelib::profile::SHOWCASE_SLOTS {
                                let (rect, response) = ui.allocate_exact_size(slot, Sense::click());
                                dashed_slot(ui, rect, &theme, "+ Add", response.hovered());
                                if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    edit = true;
                                }
                            }
                        });
                    });

                    // Friends.
                    paint::panel(ui, theme.panel_frame().inner_margin(24.0), |ui| {
                        ui.set_width(friends_width - 50.0);
                        ui.set_min_height(400.0);
                        let friends = &self.data.friends;
                        ui.horizontal(|ui| {
                            ui.label(paint::strong("Friends", 20.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let playing = friends.iter().filter(|f| f.playing.is_some()).count();
                                let online = friends
                                    .iter()
                                    .filter(|f| f.playing.is_none() && f.state.is_online())
                                    .count();
                                if !friends.is_empty() {
                                    ui.label(paint::muted(format!("{playing} playing · {online} online"), &theme));
                                }
                            });
                        });
                        ui.add_space(4.0);
                        if friends.is_empty() {
                            ui.label(paint::muted(
                                "Your Steam friends show up here. Link your SteamID in Edit profile, \
                                 add a Steam Web API key in Settings, then press Refresh. \
                                 Your Steam friends list must not be private.",
                                &theme,
                            ));
                        }
                        egui::ScrollArea::vertical()
                            .id_salt("friends")
                            .max_height(290.0)
                            .show(ui, |ui| {
                                for friend in friends {
                                    friend_row(ui, friend, &theme);
                                }
                            });
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            let note = match self.data.friends_at {
                                Some(at) => format!("From Steam · updated {}", format_ago(at, now())),
                                None => "From Steam".to_string(),
                            };
                            ui.label(paint::muted(note, &theme).size(13.5));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui
                                    .add(egui::Button::new(paint::muted("⟳ Refresh", &theme)).frame(false))
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    self.start_job(Job::RefreshFriends, "Updating friends…");
                                }
                            });
                        });
                    });
                });
            });

        if edit {
            self.edit_profile();
        }
        if let Some(game_id) = open_game {
            self.go(Screen::Details(game_id));
        }
    }
}

fn friend_row(ui: &mut egui::Ui, friend: &Friend, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::hover());
    let (dot, status, status_colour) = match &friend.playing {
        Some(game) => (theme.success, format!("Playing {game}"), theme.success),
        None if friend.state.is_online() => {
            let blue = Color32::from_rgb(0x4f, 0x8d, 0xfd);
            let text = match friend.state.as_str() {
                "online" => "Online".to_string(),
                other => {
                    let mut text = other.replace('_', " ");
                    if let Some(first) = text.get_mut(0..1) {
                        first.make_ascii_uppercase();
                    }
                    text
                }
            };
            (blue, text, blue)
        }
        None => (theme.muted, "Offline".to_string(), theme.muted),
    };
    let centre = egui::pos2(rect.left() + 22.0, rect.center().y);
    paint::friend_avatar(ui, centre, 20.0, &friend.name, dot, theme);
    let painter = ui.painter().with_clip_rect(rect);
    painter.text(
        egui::pos2(rect.left() + 56.0, rect.center().y - 11.0),
        egui::Align2::LEFT_CENTER,
        &friend.name,
        semibold(16.0),
        theme.text,
    );
    painter.text(
        egui::pos2(rect.left() + 56.0, rect.center().y + 11.0),
        egui::Align2::LEFT_CENTER,
        status,
        regular(14.0),
        status_colour,
    );
}

/// An empty slot drawn with a dashed outline and a label in the middle.
pub fn dashed_slot(ui: &egui::Ui, rect: Rect, theme: &Theme, label: &str, hovered: bool) {
    let colour = if hovered { theme.text } else { theme.muted };
    let stroke = Stroke::new(1.2, theme.border);
    let r = rect.shrink(1.0);
    for (from, to) in [
        (r.left_top(), r.right_top()),
        (r.right_top(), r.right_bottom()),
        (r.right_bottom(), r.left_bottom()),
        (r.left_bottom(), r.left_top()),
    ] {
        ui.painter()
            .add(egui::Shape::dashed_line(&[from, to], stroke, 6.0, 5.0));
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        regular(15.0),
        colour,
    );
}
