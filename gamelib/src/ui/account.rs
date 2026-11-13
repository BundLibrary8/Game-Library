//! GameLib accounts (Supabase): logging in and signing up, your account,
//! and GameLib friends with their libraries and stats.

use std::collections::HashMap;

use eframe::egui::{self, Sense, vec2};
use gamelib::cloud::{Friendship, Profile, SharedGame, SharedStats};
use gamelib::worker::{CloudJob, CloudResult, Job, Secret};

use super::theme::{bold, regular};
use super::{GameLibApp, Screen, paint};
use crate::text::{format_ago, short_playtime};

#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub enum FormMode {
    #[default]
    LogIn,
    SignUp,
}

#[derive(Default)]
pub struct CloudState {
    /// Who's logged in: their profile and email. `None` until checked.
    pub account: Option<(Profile, String)>,
    /// The saved login has been checked (or there wasn't one).
    pub checked: bool,
    /// A job is under way.
    pub busy: bool,
    pub friends: Option<Vec<Friendship>>,
    /// The friend whose library is open.
    pub viewing: Option<String>,
    pub libraries: HashMap<String, (Vec<SharedGame>, Option<SharedStats>)>,
    pub mode: FormMode,
    pub username: String,
    pub email: String,
    pub password: String,
    pub add_name: String,
    pub anon_key: String,
    pub message: Option<String>,
    pub search: String,
    /// The full-window log-in screen is showing.
    pub login_screen: bool,
    /// When the log-in screen's entrance started.
    pub login_shown_at: Option<f64>,
    /// The email box has been given the keyboard once.
    pub login_focused: bool,
    pub display_name: String,
    pub bio: String,
}

impl GameLibApp {
    pub fn cloud_job(&mut self, job: CloudJob, status: &str) {
        self.cloud.busy = true;
        self.cloud.message = None;
        self.start_job(Job::Cloud(job), status);
    }

    /// The saved login, checked at startup.
    pub fn start_cloud(&mut self) {
        let ready = self
            .db
            .cloud_project()
            .map(|p| !p.anon_key.is_empty())
            .unwrap_or(false);
        let logged_in = self.db.cloud_session().ok().flatten().is_some();
        // Open on the log-in screen, unless it has been turned off.
        let skip = self
            .db
            .get_setting(super::login::SKIP_KEY)
            .ok()
            .flatten()
            .is_some_and(|v| v == "1");
        self.cloud.login_screen = ready && !skip;
        if ready && logged_in {
            self.cloud_job(CloudJob::Check, "Checking your GameLib account…");
        } else {
            self.cloud.checked = true;
        }
    }

    pub fn cloud_result(&mut self, result: CloudResult) {
        self.cloud.busy = false;
        match result {
            CloudResult::Account(account) => {
                let first = !self.cloud.checked || self.cloud.account.is_none();
                self.cloud.checked = true;
                if let Some((profile, _)) = &account {
                    self.cloud.display_name = profile.display_name.clone();
                    self.cloud.bio = profile.bio.clone();
                }
                let logged_in = account.is_some();
                let was_logged_in = self.cloud.account.is_some();
                self.cloud.account = account;
                self.cloud.password.clear();
                if logged_in {
                    self.cloud.login_screen = false;
                }
                if logged_in && first {
                    self.start_job(Job::Cloud(CloudJob::Friends), "Loading your friends…");
                    self.start_job(Job::Cloud(CloudJob::Share), "Sharing your library…");
                }
                if !logged_in {
                    // Logged out: back to the log-in screen.
                    if was_logged_in {
                        self.cloud.login_screen = true;
                        self.cloud.login_shown_at = None;
                        self.cloud.login_focused = false;
                    }
                    self.cloud.friends = None;
                    self.cloud.viewing = None;
                    self.cloud.libraries.clear();
                }
            }
            CloudResult::ConfirmEmail => {
                self.cloud.message = Some(
                    "Account made! Supabase has emailed you a link: open it, then log in here."
                        .into(),
                );
                self.cloud.mode = FormMode::LogIn;
                self.cloud.password.clear();
            }
            CloudResult::Friends(friends) => self.cloud.friends = Some(friends),
            CloudResult::FriendLibrary {
                user_id,
                games,
                stats,
            } => {
                self.cloud.libraries.insert(user_id, (games, stats));
            }
            CloudResult::Done(message) => self.status = message,
            CloudResult::Failed(message) => {
                self.cloud.checked = true;
                self.cloud.message = Some(message.clone());
                self.error = Some(message);
            }
        }
    }

    /// After a session: share the new playtime with friends.
    pub fn cloud_after_session(&mut self) {
        if self.cloud.account.is_some() {
            self.start_job(Job::Cloud(CloudJob::Share), "Sharing your library…");
        }
    }

    /// The anon key prompt, the log-in / sign-up form, or nothing (logged in).
    /// Returns true if logged in.
    fn cloud_gate(&mut self, ui: &mut egui::Ui) -> bool {
        let theme = self.theme;
        let ready = self
            .db
            .cloud_project()
            .map(|p| !p.anon_key.is_empty())
            .unwrap_or(false);
        if !ready {
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(ui.available_width().min(640.0));
                ui.label(paint::strong("Connect GameLib to Supabase", 18.0));
                ui.label(paint::muted(
                    "Accounts are stored in your Supabase project. Paste its anon (public) key: Supabase dashboard → Project Settings → API → Project API keys → anon public. Then run supabase/schema.sql in the SQL editor once.",
                    &theme,
                ));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.cloud.anon_key)
                            .password(true)
                            .hint_text("anon key (starts eyJ… or sb_publishable_…)")
                            .desired_width(380.0),
                    );
                    if ui.button("Save").clicked() {
                        let key = self.cloud.anon_key.trim().to_string();
                        if key.len() < 20 {
                            self.error = Some("That key looks too short.".into());
                        } else if self.check(self.db.set_cloud_anon_key(&key)).is_some() {
                            self.status = "Supabase key saved.".into();
                        }
                    }
                });
            });
            return false;
        }
        if !self.cloud.checked {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(paint::muted("Checking your account…", &theme));
            });
            return false;
        }
        if self.cloud.account.is_some() {
            return true;
        }

        let mut submit = false;
        paint::panel(ui, theme.panel_frame(), |ui| {
            ui.set_width(ui.available_width().min(460.0));
            ui.horizontal(|ui| {
                if paint::chip(ui, &theme, "Log in", self.cloud.mode == FormMode::LogIn).clicked() {
                    self.cloud.mode = FormMode::LogIn;
                }
                if paint::chip(
                    ui,
                    &theme,
                    "Create an account",
                    self.cloud.mode == FormMode::SignUp,
                )
                .clicked()
                {
                    self.cloud.mode = FormMode::SignUp;
                }
            });
            ui.add_space(8.0);
            let signing_up = self.cloud.mode == FormMode::SignUp;
            egui::Grid::new("cloud_form")
                .num_columns(2)
                .spacing([12.0, 10.0])
                .show(ui, |ui| {
                    if signing_up {
                        ui.label("Username");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.cloud.username)
                                .hint_text("how friends find you")
                                .desired_width(260.0),
                        );
                        ui.end_row();
                    }
                    ui.label("Email");
                    ui.add(egui::TextEdit::singleline(&mut self.cloud.email).desired_width(260.0));
                    ui.end_row();
                    ui.label("Password");
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.cloud.password)
                            .password(true)
                            .hint_text(if signing_up {
                                "at least 8 characters"
                            } else {
                                ""
                            })
                            .desired_width(260.0),
                    );
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        submit = true;
                    }
                    ui.end_row();
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let label = if signing_up {
                    "Create account"
                } else {
                    "Log in"
                };
                let clicked = ui
                    .add_enabled_ui(!self.cloud.busy, |ui| {
                        paint::accent_button(ui, &theme, label, 15.0, vec2(22.0, 10.0))
                    })
                    .inner
                    .clicked();
                submit |= clicked;
                if self.cloud.busy {
                    ui.spinner();
                }
            });
            if let Some(message) = &self.cloud.message {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(message).color(theme.muted));
            }
            if signing_up {
                ui.label(
                    paint::muted(
                        "Your username, library and stats are shared only with friends you accept.",
                        &theme,
                    )
                    .size(12.5),
                );
            }
        });
        if submit {
            self.cloud_submit();
        }
        false
    }

    /// Check the log-in / sign-up form and send it.
    pub fn cloud_submit(&mut self) {
        if self.cloud.busy {
            return;
        }
        let email = self.cloud.email.trim().to_string();
        let password = Secret(self.cloud.password.clone());
        let check = gamelib::cloud::check_email(&email).and_then(|()| match self.cloud.mode {
            FormMode::SignUp => gamelib::cloud::check_username(&self.cloud.username)
                .and_then(|()| gamelib::cloud::check_password(&password.0)),
            FormMode::LogIn => Ok(()),
        });
        match check {
            Err(err) => self.cloud.message = Some(format!("{err}")),
            Ok(()) => match self.cloud.mode {
                FormMode::SignUp => self.cloud_job(
                    CloudJob::SignUp {
                        username: self.cloud.username.trim().to_string(),
                        email,
                        password,
                    },
                    "Creating your account…",
                ),
                FormMode::LogIn => {
                    self.cloud_job(CloudJob::LogIn { email, password }, "Logging in…")
                }
            },
        }
    }

    /// The account screen.
    pub fn account_screen(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        ui.label(egui::RichText::new("GameLib account").font(bold(32.0)));
        ui.add_space(10.0);
        if !self.cloud_gate(ui) {
            return;
        }
        let Some((profile, email)) = self.cloud.account.clone() else {
            return;
        };
        paint::panel(ui, theme.panel_frame(), |ui| {
            ui.set_width(ui.available_width().min(640.0));
            ui.label(paint::strong(format!("@{}", profile.username), 20.0));
            ui.label(paint::muted(&email, &theme));
            ui.add_space(8.0);
            egui::Grid::new("account_profile")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Display name");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.cloud.display_name)
                            .desired_width(280.0),
                    );
                    ui.end_row();
                    ui.label("About you");
                    ui.add(
                        egui::TextEdit::multiline(&mut self.cloud.bio)
                            .desired_rows(2)
                            .desired_width(280.0),
                    );
                    ui.end_row();
                });
            if ui.button("Save profile").clicked() {
                self.cloud_job(
                    CloudJob::UpdateProfile {
                        display_name: self.cloud.display_name.clone(),
                        bio: self.cloud.bio.clone(),
                    },
                    "Saving…",
                );
            }
            ui.add_space(10.0);
            let mut share = self.db.cloud_sharing().unwrap_or(true);
            if ui
                .checkbox(
                    &mut share,
                    "Share my library and stats with my GameLib friends",
                )
                .changed()
            {
                self.check(self.db.set_cloud_sharing(share));
                let job = if share {
                    CloudJob::Share
                } else {
                    CloudJob::Unshare
                };
                self.cloud_job(job, "Updating…");
            }
            ui.horizontal(|ui| {
                if ui.button("Share now").clicked() {
                    self.cloud_job(CloudJob::Share, "Sharing your library…");
                }
                if ui.button("Log out").clicked() {
                    self.cloud_job(CloudJob::LogOut, "Logging out…");
                }
            });
        });
    }

    /// The GameLib friends tab of the Friends page.
    pub fn gamelib_friends(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        if !self.cloud_gate(ui) {
            return;
        }
        if let Some(user_id) = self.cloud.viewing.clone() {
            self.friend_view(ui, &user_id);
            return;
        }
        let me = self
            .cloud
            .account
            .as_ref()
            .map(|a| a.0.username.clone())
            .unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(paint::muted(format!("Logged in as @{me}"), &theme));
            if ui.small_button("Account").clicked() {
                self.go(Screen::Account);
            }
            if ui.small_button("Refresh").clicked() {
                self.cloud_job(CloudJob::Friends, "Loading your friends…");
            }
        });
        ui.add_space(8.0);
        // Add a friend.
        let mut add = false;
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.cloud.add_name)
                    .hint_text("Their GameLib username")
                    .desired_width(260.0),
            );
            add = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            add |= paint::accent_button(ui, &theme, "Add friend", 14.0, vec2(18.0, 8.0)).clicked();
        });
        if add && !self.cloud.add_name.trim().is_empty() {
            let name = std::mem::take(&mut self.cloud.add_name);
            self.cloud_job(CloudJob::AddFriend(name), "Sending a friend request…");
        }
        ui.add_space(12.0);

        let Some(friends) = self.cloud.friends.clone() else {
            ui.spinner();
            return;
        };
        let incoming: Vec<&Friendship> = friends
            .iter()
            .filter(|f| !f.accepted && !f.outgoing)
            .collect();
        let outgoing: Vec<&Friendship> = friends
            .iter()
            .filter(|f| !f.accepted && f.outgoing)
            .collect();
        let accepted: Vec<&Friendship> = friends.iter().filter(|f| f.accepted).collect();
        let mut job = None;

        if !incoming.is_empty() {
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(ui.available_width());
                ui.label(paint::strong("Friend requests", 17.0));
                for f in &incoming {
                    ui.horizontal(|ui| {
                        ui.label(paint::strong(f.other.name(), 14.5));
                        ui.label(paint::muted(format!("@{}", f.other.username), &theme));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Decline").clicked() {
                                job = Some(CloudJob::Respond {
                                    request: f.id,
                                    accept: false,
                                });
                            }
                            if paint::accent_button(ui, &theme, "Accept", 13.0, vec2(14.0, 6.0))
                                .clicked()
                            {
                                job = Some(CloudJob::Respond {
                                    request: f.id,
                                    accept: true,
                                });
                            }
                        });
                    });
                }
            });
            ui.add_space(12.0);
        }

        paint::panel(ui, theme.panel_frame(), |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong(format!("Friends · {}", accepted.len()), 17.0));
            if accepted.is_empty() {
                ui.label(paint::muted(
                    "No friends yet. Ask yours for their GameLib username and add them above.",
                    &theme,
                ));
            }
            for f in &accepted {
                ui.horizontal(|ui| {
                    ui.set_min_height(44.0);
                    let (avatar, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                    paint::friend_avatar(
                        ui,
                        avatar.center(),
                        18.0,
                        f.other.name(),
                        theme.accent,
                        &theme,
                    );
                    ui.vertical(|ui| {
                        ui.label(paint::strong(f.other.name(), 15.0));
                        ui.label(paint::muted(format!("@{}", f.other.username), &theme).size(12.5));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Remove").clicked() {
                            job = Some(CloudJob::RemoveFriend(f.other.id.clone()));
                        }
                        if paint::accent_button(ui, &theme, "View library", 13.0, vec2(14.0, 6.0))
                            .clicked()
                        {
                            self.cloud.viewing = Some(f.other.id.clone());
                            self.cloud.search.clear();
                            job = Some(CloudJob::FriendLibrary(f.other.id.clone()));
                        }
                    });
                });
            }
        });
        if !outgoing.is_empty() {
            ui.add_space(12.0);
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(ui.available_width());
                ui.label(paint::strong("Waiting for them to accept", 15.0));
                for f in &outgoing {
                    ui.horizontal(|ui| {
                        ui.label(format!("{} (@{})", f.other.name(), f.other.username));
                        if ui.small_button("Cancel").clicked() {
                            job = Some(CloudJob::RemoveFriend(f.other.id.clone()));
                        }
                    });
                }
            });
        }
        if let Some(job) = job {
            self.cloud_job(job, "Updating friends…");
        }
    }

    /// One friend's stats and library.
    fn friend_view(&mut self, ui: &mut egui::Ui, user_id: &str) {
        let theme = self.theme;
        let friend = self
            .cloud
            .friends
            .as_ref()
            .and_then(|list| list.iter().find(|f| f.other.id == user_id))
            .map(|f| f.other.clone());
        ui.horizontal(|ui| {
            if ui.button("← Friends").clicked() {
                self.cloud.viewing = None;
            }
            if let Some(friend) = &friend {
                ui.label(egui::RichText::new(friend.name()).font(bold(24.0)));
                ui.label(paint::muted(format!("@{}", friend.username), &theme));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Refresh").clicked() {
                    self.cloud_job(CloudJob::FriendLibrary(user_id.to_string()), "Loading…");
                }
            });
        });
        if let Some(bio) = friend
            .as_ref()
            .map(|f| f.bio.clone())
            .filter(|b| !b.is_empty())
        {
            ui.label(paint::muted(bio, &theme));
        }
        ui.add_space(10.0);
        let Some((games, stats)) = self.cloud.libraries.get(user_id).cloned() else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(paint::muted("Loading their library…", &theme));
            });
            return;
        };
        match &stats {
            Some(s) => {
                paint::stat_row(
                    ui,
                    &theme,
                    &[
                        (
                            "Playtime",
                            short_playtime(s.total_playtime_secs.max(0) as u64),
                        ),
                        ("Games", s.game_count.to_string()),
                        ("Played", s.played_count.to_string()),
                        ("Finished", s.finished_count.to_string()),
                        ("Sessions", s.session_count.to_string()),
                        ("Achievements", s.achievements_unlocked.to_string()),
                    ],
                );
                ui.add_space(6.0);
                let mut lines = Vec::new();
                if let Some(top) = &s.top_game {
                    lines.push(format!(
                        "Most played: {top} ({})",
                        short_playtime(s.top_game_secs.max(0) as u64)
                    ));
                }
                if let (Some(title), Some(at)) = (&s.last_played_title, s.last_played_at) {
                    lines.push(format!(
                        "Last played {title} {}",
                        format_ago(at, gamelib::now())
                    ));
                }
                ui.label(paint::muted(lines.join("   ·   "), &theme));
            }
            None => {
                ui.label(paint::muted("They haven't shared their stats yet.", &theme));
            }
        }
        ui.add_space(12.0);

        // Their library, with the games you both have marked.
        let mine: std::collections::HashSet<String> = self
            .data
            .visible
            .iter()
            .flat_map(|g| {
                [
                    format!("{}:{}", g.source.as_str(), g.source_id),
                    g.title.to_lowercase(),
                ]
            })
            .collect();
        paint::panel(ui, theme.panel_frame(), |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(paint::strong(
                    format!("Library · {} games", games.len()),
                    17.0,
                ));
                let both = games
                    .iter()
                    .filter(|g| {
                        mine.contains(&g.game_key) || mine.contains(&g.title.to_lowercase())
                    })
                    .count();
                ui.label(paint::muted(format!("· {both} you both have"), &theme));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.cloud.search)
                            .hint_text("Search their games…")
                            .desired_width(220.0),
                    );
                });
            });
            if games.is_empty() {
                ui.label(paint::muted("They haven't shared any games yet.", &theme));
            }
            let search = self.cloud.search.trim().to_lowercase();
            let width = ui.available_width();
            egui::Grid::new("friend_library")
                .num_columns(5)
                .spacing([18.0, 6.0])
                .striped(true)
                .min_col_width(60.0)
                .show(ui, |ui| {
                    for heading in ["Game", "Playtime", "Status", "Rating", ""] {
                        ui.label(
                            egui::RichText::new(heading)
                                .font(bold(12.5))
                                .color(theme.muted),
                        );
                    }
                    ui.end_row();
                    for g in games
                        .iter()
                        .filter(|g| search.is_empty() || g.title.to_lowercase().contains(&search))
                    {
                        let title = if g.favourite {
                            format!("★ {}", g.title)
                        } else {
                            g.title.clone()
                        };
                        paint::left_label(
                            ui,
                            (width * 0.4).max(200.0),
                            egui::Label::new(egui::RichText::new(title).font(bold(14.0))),
                        );
                        ui.label(short_playtime(g.playtime_secs.max(0) as u64));
                        let status = g
                            .status
                            .as_deref()
                            .and_then(gamelib::extras::Status::parse)
                            .map(|s| s.label())
                            .unwrap_or("");
                        ui.label(
                            egui::RichText::new(status)
                                .font(regular(13.0))
                                .color(theme.muted),
                        );
                        ui.label(
                            g.rating
                                .map(|r| "★".repeat(r.clamp(0, 5) as usize))
                                .unwrap_or_default(),
                        );
                        if mine.contains(&g.game_key) || mine.contains(&g.title.to_lowercase()) {
                            ui.label(
                                egui::RichText::new("you have it")
                                    .font(regular(12.0))
                                    .color(theme.success),
                            );
                        } else {
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });
        });
    }
}
