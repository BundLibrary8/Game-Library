//! The profile screen: name, bio, badges, banner, showcase and friends.

use anyhow::Result;
use eframe::egui;
use gamelib::db::Db;
use gamelib::friends::Friend;
use gamelib::now;
use gamelib::profile::{self, Banner};
use gamelib::shop::{self, ItemKind};
use gamelib::worker::Job;

use super::{GameLibApp, stat_tile};
use crate::text::format_duration;

/// The text boxes on the profile screen.
pub struct ProfileInputs {
    name: String,
    bio: String,
    steam_id: String,
}

impl ProfileInputs {
    pub fn load(db: &Db) -> Result<ProfileInputs> {
        let profile = db.profile(now())?;
        Ok(ProfileInputs {
            name: profile.display_name,
            bio: profile.bio.unwrap_or_default(),
            steam_id: profile.steam_id.unwrap_or_default(),
        })
    }
}

impl GameLibApp {
    pub fn profile_screen(&mut self, ui: &mut egui::Ui) {
        let loaded = (|| -> Result<_> {
            Ok((
                self.db.profile(now())?,
                self.db.library_stats()?,
                self.db.owned_items()?,
                self.db.badges()?,
                self.db.showcase()?,
                self.db.cached_friends()?,
                self.db.friends_updated_at()?,
                self.db.name_change_credits()?,
            ))
        })();
        let (profile, stats, owned, shown_badges, showcase, friends, friends_at, credits) =
            match loaded {
                Ok(values) => values,
                Err(err) => {
                    ui.colored_label(ui.visuals().error_fg_color, format!("{err:#}"));
                    return;
                }
            };
        let item_name = |id: &str| shop::find_item(id).map(|i| i.name).unwrap_or("?");
        let owned_of = |kind: ItemKind| -> Vec<&'static shop::ShopItem> {
            owned
                .iter()
                .filter_map(|id| shop::find_item(id))
                .filter(|item| item.kind == kind)
                .collect()
        };

        let mut save_name = false;
        let mut save_bio = false;
        let mut save_steam = false;
        let mut new_badges: Option<Vec<&str>> = None;
        let mut new_banner: Option<Option<Banner>> = None;
        let mut refresh_friends = false;

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(&profile.display_name);
                    for badge in &shown_badges {
                        ui.label(egui::RichText::new(format!("[{}]", item_name(badge))).strong());
                    }
                });
                if let Some(bio) = &profile.bio {
                    ui.label(bio);
                }
                let banner_text = match &profile.banner {
                    Some(Banner::Item(id)) => item_name(id).to_string(),
                    Some(Banner::Game(id)) => format!("art from game {id}"),
                    Some(Banner::Upload(path)) => path.display().to_string(),
                    None => "none".to_string(),
                };
                ui.weak(format!("Banner: {banner_text}"));

                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    stat_tile(
                        ui,
                        &format_duration(stats.total_playtime_secs),
                        "total playtime",
                    );
                    stat_tile(ui, &stats.game_count.to_string(), "games");
                    stat_tile(ui, &stats.finished_count.to_string(), "finished");
                    let most = stats
                        .most_played
                        .as_ref()
                        .map(|(title, _)| title.as_str())
                        .unwrap_or("-");
                    stat_tile(ui, most, "most played");
                });

                ui.add_space(12.0);
                ui.strong("Showcase");
                if showcase.is_empty() {
                    ui.weak("Pick up to 4 games with \"Show on profile\" in a game's details.");
                }
                ui.horizontal_wrapped(|ui| {
                    for game_id in &showcase {
                        ui.label(format!("• {}", self.title_of(*game_id)));
                    }
                });

                ui.add_space(12.0);
                ui.separator();
                ui.strong("Edit profile");
                let row = |ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [70.0, 20.0],
                            egui::Label::new(label).halign(egui::Align::LEFT),
                        );
                        add(ui);
                    });
                };
                row(ui, "Name", &mut |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.profile.name).desired_width(240.0));
                    save_name = ui.button("Save").clicked();
                    ui.weak(if !profile.name_chosen {
                        "your first name is free".to_string()
                    } else if credits > 0 {
                        format!("uses 1 of your {credits} Name changes")
                    } else {
                        "needs a Name change from the Shop".to_string()
                    });
                });
                row(ui, "Bio", &mut |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.profile.bio)
                            .char_limit(profile::MAX_BIO_CHARS)
                            .desired_width(240.0),
                    );
                    save_bio = ui.button("Save").clicked();
                });
                row(ui, "SteamID", &mut |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.profile.steam_id)
                            .hint_text("17 digits, e.g. 7656119…")
                            .desired_width(240.0),
                    );
                    save_steam = ui.button("Save").clicked();
                });

                ui.add_space(8.0);
                ui.label(format!("Badges to show (up to {})", profile::BADGE_SLOTS));
                ui.horizontal_wrapped(|ui| {
                    for badge in owned_of(ItemKind::Badge) {
                        let mut shown = shown_badges.iter().any(|id| id == badge.id);
                        if ui.checkbox(&mut shown, badge.name).changed() {
                            let mut list: Vec<&str> =
                                shown_badges.iter().map(String::as_str).collect();
                            if shown {
                                list.push(badge.id);
                            } else {
                                list.retain(|id| *id != badge.id);
                            }
                            new_badges = Some(list);
                        }
                    }
                });

                ui.add_space(8.0);
                ui.label("Banner");
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .selectable_label(profile.banner.is_none(), "None")
                        .clicked()
                    {
                        new_banner = Some(None);
                    }
                    for banner in owned_of(ItemKind::ProfileBanner) {
                        let chosen = profile.banner == Some(Banner::Item(banner.id.to_string()));
                        if ui.selectable_label(chosen, banner.name).clicked() {
                            new_banner = Some(Some(Banner::Item(banner.id.to_string())));
                        }
                    }
                });
                for earned in profile::EARNED_BANNERS {
                    if !owned.iter().any(|id| id == earned.item_id) {
                        ui.weak(format!(
                            "Locked: {} · {}",
                            item_name(earned.item_id),
                            earned.goal.describe()
                        ));
                    }
                }

                ui.add_space(12.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.strong("Steam friends");
                    refresh_friends = ui.button("Refresh").clicked();
                    if let Some(at) = friends_at {
                        ui.weak(format!(
                            "updated {} ago",
                            format_duration((now() - at).max(0) as u64)
                        ));
                    }
                });
                if friends.is_empty() {
                    ui.weak(
                    "Nothing yet. Enter your SteamID above and a Steam Web API key in Settings, \
                     then click Refresh. Your Steam friends list must not be private.",
                );
                }
                for friend in &friends {
                    friend_row(ui, friend);
                }
            });

        if save_name
            && let Some(spent) = self.check(self.db.set_display_name(&self.profile.name, now()))
        {
            self.status = if spent {
                "Name changed (used a Name change).".to_string()
            } else {
                "Name saved.".to_string()
            };
        }
        if save_bio {
            self.check(self.db.set_bio(&self.profile.bio, now()));
        }
        if save_steam {
            self.check(self.db.set_steam_id(Some(&self.profile.steam_id), now()));
        }
        if let Some(badges) = new_badges {
            self.check(self.db.set_badges(&badges));
        }
        if let Some(banner) = new_banner {
            self.check(self.db.set_banner(banner.as_ref(), now()));
        }
        if refresh_friends {
            self.start_job(Job::RefreshFriends, "Updating friends…");
        }
    }
}

fn friend_row(ui: &mut egui::Ui, friend: &Friend) {
    ui.horizontal(|ui| {
        let (dot, colour) = if friend.playing.is_some() {
            ("●", egui::Color32::from_rgb(120, 200, 90))
        } else if friend.state.is_online() {
            ("●", egui::Color32::from_rgb(90, 160, 230))
        } else {
            ("○", ui.visuals().weak_text_color())
        };
        ui.colored_label(colour, dot);
        ui.label(&friend.name);
        match &friend.playing {
            Some(game) => ui.weak(format!("playing {game}")),
            None => ui.weak(friend.state.as_str().replace('_', " ")),
        };
    });
}
