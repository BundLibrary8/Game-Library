//! The Friends screen: what your Steam friends are playing, the games each
//! of them owns, what you have in common, and what's popular among them.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use eframe::egui::{self, Sense};
use gamelib::friend_games::{self, FriendGame};
use gamelib::now;
use gamelib::worker::Job;

use super::profile::friend_row;
use super::theme::{bold, regular};
use super::{GameLibApp, Screen, paint};
use crate::text::format_ago;

/// The friends' game lists, as loaded from the database.
pub struct FriendGamesNow {
    pub games: HashMap<String, Vec<FriendGame>>,
    pub checked: HashMap<String, (i64, bool)>,
}

#[derive(Default)]
pub struct FriendsState {
    pub search: String,
    /// The friend whose whole game list is open.
    pub open: Option<String>,
}

fn hours(minutes: u64) -> String {
    if minutes < 60 {
        format!("{minutes}m")
    } else {
        format!("{}h", minutes / 60)
    }
}

impl GameLibApp {
    fn friend_games_now(&mut self, time: f64) -> Rc<FriendGamesNow> {
        let db = &self.db;
        super::memo(
            &mut self.memos.friend_games,
            self.generation,
            time,
            120.0,
            || {
                Rc::new(FriendGamesNow {
                    games: db.all_friend_games().unwrap_or_default(),
                    checked: db.friend_games_checked().unwrap_or_default(),
                })
            },
        )
    }

    /// Friends who own a game (for its page): their names.
    pub fn friends_who_own(
        &mut self,
        game: &gamelib::model::Game,
        time: f64,
    ) -> Vec<(String, u64)> {
        let lists = self.friend_games_now(time);
        let one = [game.clone()];
        let mut owners: Vec<(String, u64)> = self
            .data
            .friends
            .iter()
            .filter_map(|friend| {
                lists.games.get(&friend.steam_id)?.iter().find_map(|g| {
                    friend_games::match_library(g, &one)
                        .map(|_| (friend.name.clone(), g.playtime_minutes))
                })
            })
            .collect();
        owners.sort_by_key(|(name, minutes)| (std::cmp::Reverse(*minutes), name.to_lowercase()));
        owners
    }

    pub fn friends_screen(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let time = ui.input(|i| i.time);
        let lists = self.friend_games_now(time);
        let friends = self.data.friends.clone();
        let library = self.data.visible.clone();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Friends").font(bold(32.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Refresh their games")
                    .on_hover_text("Asks Steam which games each friend owns")
                    .clicked()
                {
                    self.start_job(Job::FetchFriendGames, "Reading your friends' games…");
                }
                if ui.button("Refresh friends").clicked() {
                    self.start_job(Job::RefreshFriends, "Updating friends…");
                }
            });
        });
        ui.label(paint::muted(
            "From Steam: needs the Steam Web API key in Settings and your Steam account on your profile. Friends who keep their game details private can't be shown.",
            &theme,
        ));
        ui.add_space(12.0);
        if friends.is_empty() {
            ui.label(paint::muted(
                "No friends loaded yet. Press \"Refresh friends\".",
                &theme,
            ));
            return;
        }

        // What's popular: games owned by the most friends.
        let mut popular: HashMap<String, (String, usize, u64)> = HashMap::new();
        for games in lists.games.values() {
            let mut seen = HashSet::new();
            for g in games {
                if seen.insert(g.app_id.clone()) {
                    let entry = popular
                        .entry(g.app_id.clone())
                        .or_insert((g.name.clone(), 0, 0));
                    entry.1 += 1;
                    entry.2 += g.recent_minutes;
                }
            }
        }
        let mut popular: Vec<(String, String, usize, u64)> = popular
            .into_iter()
            .map(|(app, (name, owners, recent))| (app, name, owners, recent))
            .collect();
        popular.sort_by(|a, b| b.3.cmp(&a.3).then(b.2.cmp(&a.2)).then(a.1.cmp(&b.1)));

        let gap = 18.0;
        let half = ((ui.available_width() - gap) / 2.0).floor();
        let mut open_game = None;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(half - 38.0);
                ui.set_min_height(300.0);
                ui.label(paint::strong("Hot with your friends", 17.0));
                ui.label(paint::muted(
                    "Most played by your friends in the last two weeks.",
                    &theme,
                ));
                ui.add_space(4.0);
                if popular.is_empty() {
                    ui.label(paint::muted("Press \"Refresh their games\".", &theme));
                }
                for (app, name, owners, recent) in popular.iter().take(10) {
                    let probe = FriendGame {
                        steam_id: String::new(),
                        app_id: app.clone(),
                        name: name.clone(),
                        playtime_minutes: 0,
                        recent_minutes: 0,
                    };
                    let mine = friend_games::match_library(&probe, &library).map(|g| g.id);
                    ui.horizontal(|ui| {
                        let label = paint::left_label(
                            ui,
                            half - 290.0,
                            egui::Label::new(egui::RichText::new(name).font(bold(14.0)))
                                .sense(Sense::click()),
                        );
                        if let Some(id) = mine
                            && label
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                        {
                            open_game = Some(id);
                        }
                        ui.label(paint::muted(
                            format!(
                                "{owners} {} · {} lately",
                                if *owners == 1 { "friend" } else { "friends" },
                                hours(*recent)
                            ),
                            &theme,
                        ));
                        if mine.is_some() {
                            ui.label(
                                egui::RichText::new("In your library")
                                    .font(regular(11.5))
                                    .color(theme.success),
                            );
                        }
                    });
                }
            });
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(half - 38.0);
                ui.set_min_height(300.0);
                ui.label(paint::strong("Right now", 17.0));
                ui.add_space(4.0);
                let mut online: Vec<_> = friends
                    .iter()
                    .filter(|f| f.playing.is_some() || f.state.is_online())
                    .collect();
                online.truncate(8);
                if online.is_empty() {
                    ui.label(paint::muted("Nobody's online.", &theme));
                }
                for friend in online {
                    friend_row(ui, friend, &theme);
                }
            });
        });
        ui.add_space(16.0);

        // Each friend.
        ui.horizontal(|ui| {
            ui.label(paint::strong("Everyone", 19.0));
            ui.add(
                egui::TextEdit::singleline(&mut self.friends.search)
                    .hint_text("Find a friend or a game…")
                    .desired_width(260.0),
            );
        });
        ui.add_space(6.0);
        let search = self.friends.search.trim().to_lowercase();
        let columns = ((ui.available_width() + gap) / (420.0 + gap))
            .floor()
            .max(1.0) as usize;
        let card = (ui.available_width() - gap * (columns as f32 - 1.0)) / columns as f32;
        let shown: Vec<_> = friends
            .iter()
            .filter(|f| {
                search.is_empty()
                    || f.name.to_lowercase().contains(&search)
                    || lists.games.get(&f.steam_id).is_some_and(|games| {
                        games
                            .iter()
                            .any(|g| g.name.to_lowercase().contains(&search))
                    })
            })
            .collect();
        let mut toggle_open = None;
        for row in shown.chunks(columns) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for friend in row {
                    paint::panel(ui, theme.panel_frame(), |ui| {
                        ui.set_width(card - 38.0);
                        friend_row(ui, friend, &theme);
                        let games = lists.games.get(&friend.steam_id);
                        match (games, lists.checked.get(&friend.steam_id)) {
                            (_, Some((_, false))) => {
                                ui.label(paint::muted("Keeps their game list private.", &theme));
                            }
                            (Some(games), Some((at, true))) => {
                                let common = games
                                    .iter()
                                    .filter(|g| friend_games::match_library(g, &library).is_some())
                                    .count();
                                ui.label(paint::muted(
                                    format!(
                                        "{} games · {common} in common with you · checked {}",
                                        games.len(),
                                        format_ago(*at, now())
                                    ),
                                    &theme,
                                ));
                                let open =
                                    self.friends.open.as_deref() == Some(friend.steam_id.as_str());
                                let list: Vec<&FriendGame> = if open {
                                    games.iter().collect()
                                } else {
                                    games.iter().take(5).collect()
                                };
                                for g in list {
                                    ui.horizontal(|ui| {
                                        let mine =
                                            friend_games::match_library(g, &library).map(|m| m.id);
                                        let label = ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&g.name).font(regular(13.5)),
                                            )
                                            .truncate()
                                            .sense(Sense::click()),
                                        );
                                        if let Some(id) = mine
                                            && label.clicked()
                                        {
                                            open_game = Some(id);
                                        }
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.label(
                                                    paint::muted(hours(g.playtime_minutes), &theme)
                                                        .size(12.5),
                                                );
                                                if g.recent_minutes > 0 {
                                                    ui.label(
                                                        egui::RichText::new("playing lately")
                                                            .font(regular(11.5))
                                                            .color(theme.accent),
                                                    );
                                                }
                                                if mine.is_some() {
                                                    ui.label(
                                                        egui::RichText::new("you have it")
                                                            .font(regular(11.5))
                                                            .color(theme.success),
                                                    );
                                                }
                                            },
                                        );
                                    });
                                }
                                if games.len() > 5 {
                                    let text = if open {
                                        "Show fewer".to_string()
                                    } else {
                                        format!("Show all {}", games.len())
                                    };
                                    if ui.small_button(text).clicked() {
                                        toggle_open = Some(friend.steam_id.clone());
                                    }
                                }
                            }
                            _ => {
                                ui.label(paint::muted("Games not loaded yet.", &theme));
                            }
                        }
                    });
                }
            });
            ui.add_space(gap);
        }
        if let Some(id) = toggle_open {
            self.friends.open = if self.friends.open.as_deref() == Some(id.as_str()) {
                None
            } else {
                Some(id)
            };
        }
        if let Some(id) = open_game {
            self.go(Screen::Details(id));
        }
    }
}
