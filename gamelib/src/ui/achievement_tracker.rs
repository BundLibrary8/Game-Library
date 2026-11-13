//! The achievement tracker: a panel on each game's page, and a screen with
//! every game's completion, recent unlocks and your rarest achievements.

use eframe::egui::{self, Sense, vec2};
use gamelib::game_achievements::{Completion, GameAchievement};
use gamelib::model::{Game, Source};
use gamelib::now;
use gamelib::worker::Job;

use super::theme::{Theme, bold, regular};
use super::{GameLibApp, Screen, paint};
use crate::text::format_ago;

/// What's typed into a game's "add an achievement" box: (game id, text).
#[derive(Default)]
pub struct TrackerState {
    pub new_name: Option<(i64, String)>,
}

/// One achievement as a row: a badge, its name and description, rarity.
/// Returns true if its tick box was clicked (hand-written ones only).
fn achievement_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    a: &GameAchievement,
    title: Option<&str>,
) -> (bool, bool) {
    let mut toggled = false;
    let mut delete = false;
    ui.horizontal(|ui| {
        ui.set_min_height(40.0);
        let (badge, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
        let colour = if a.achieved {
            theme.accent
        } else {
            theme.panel_alt
        };
        ui.painter().circle_filled(badge.center(), 16.0, colour);
        paint::star4(
            ui.painter(),
            badge.center(),
            8.0,
            if a.achieved {
                theme.on_accent
            } else {
                theme.muted
            },
        );
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let name = match title {
                Some(game) => format!("{} · {game}", a.name),
                None => a.name.clone(),
            };
            ui.add(
                egui::Label::new(
                    egui::RichText::new(name)
                        .font(bold(14.0))
                        .color(if a.achieved { theme.text } else { theme.muted }),
                )
                .truncate(),
            );
            let mut detail = a.description.clone().unwrap_or_default();
            if let Some(at) = a.unlocked_at {
                if !detail.is_empty() {
                    detail.push_str(" · ");
                }
                detail.push_str(&format!("unlocked {}", format_ago(at, now())));
            }
            if !detail.is_empty() {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(detail)
                            .font(regular(12.0))
                            .color(theme.muted),
                    )
                    .truncate(),
                );
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if a.manual && title.is_none() {
                if ui.small_button("Delete").clicked() {
                    delete = true;
                }
                let mut done = a.achieved;
                if ui.checkbox(&mut done, "").on_hover_text("Done").changed() {
                    toggled = true;
                }
            } else if let Some(percent) = a.global_percent {
                let rare = percent < 10.0;
                ui.label(
                    egui::RichText::new(format!("{percent:.1}% of players"))
                        .font(regular(12.0))
                        .color(if rare { theme.stardust } else { theme.muted }),
                );
            }
        });
    });
    (toggled, delete)
}

impl GameLibApp {
    /// The panel on a game's page.
    pub fn achievements_panel_for(&mut self, ui: &mut egui::Ui, game: &Game) {
        let theme = self.theme;
        let list = self.db.game_achievements(game.id).unwrap_or_default();
        let unlocked = list.iter().filter(|a| a.achieved).count();
        paint::panel(ui, theme.panel_frame().inner_margin(24.0), |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(paint::strong("Achievements", 16.0));
                if !list.is_empty() {
                    ui.label(paint::muted(
                        format!("· {unlocked} of {}", list.len()),
                        &theme,
                    ));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if game.source == Source::Steam && ui.button("Get from Steam").clicked() {
                        self.start_job(
                            Job::FetchAchievements {
                                game_id: Some(game.id),
                            },
                            "Reading achievements…",
                        );
                    }
                });
            });
            if !list.is_empty() {
                let fraction = unlocked as f32 / list.len() as f32;
                paint::bar(
                    ui,
                    &theme,
                    fraction,
                    theme.accent,
                    ui.available_width() - 60.0,
                );
            } else if game.source == Source::Steam {
                ui.label(paint::muted(
                    "Press \"Get from Steam\" (needs the Steam Web API key and your Steam account in Settings and Profile). You can also add your own goals below.",
                    &theme,
                ));
            } else {
                ui.label(paint::muted(
                    "This game's store doesn't share achievements, so make your own checklist: \"Beat the final boss\", \"Find every relic\"…",
                    &theme,
                ));
            }
            ui.add_space(4.0);
            let mut toggle = None;
            let mut delete = None;
            egui::ScrollArea::vertical()
                .id_salt("game_achievements")
                .max_height(340.0)
                .show(ui, |ui| {
                    for a in &list {
                        let (t, d) = achievement_row(ui, &theme, a, None);
                        if t {
                            toggle = Some((a.api_name.clone(), !a.achieved));
                        }
                        if d {
                            delete = Some(a.api_name.clone());
                        }
                    }
                });
            if let Some((api, done)) = toggle {
                self.check(self.db.set_manual_achieved(game.id, &api, done, now()));
            }
            if let Some(api) = delete {
                self.check(self.db.delete_manual_achievement(game.id, &api));
            }
            // Add your own.
            ui.add_space(4.0);
            if self.tracker.new_name.as_ref().map(|n| n.0) != Some(game.id) {
                self.tracker.new_name = Some((game.id, String::new()));
            }
            let mut add = None;
            ui.horizontal(|ui| {
                if let Some((_, text)) = self.tracker.new_name.as_mut() {
                    let response = ui.add(
                        egui::TextEdit::singleline(text)
                            .hint_text("Add your own: \"Beat the game on hard\"")
                            .desired_width(320.0),
                    );
                    let enter =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (ui.button("Add").clicked() || enter) && !text.trim().is_empty() {
                        add = Some(std::mem::take(text));
                    }
                }
            });
            if let Some(name) = add {
                self.check(self.db.add_manual_achievement(game.id, &name, None));
            }
        });
    }

    /// The Achievement tracker screen.
    pub fn tracker_screen(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let completion = self.db.achievement_completion().unwrap_or_default();
        let recent = self.db.recent_achievements(8).unwrap_or_default();
        let rarest = self.db.rarest_achievements(8).unwrap_or_default();
        let total = completion
            .values()
            .fold(Completion::default(), |sum, c| Completion {
                unlocked: sum.unlocked + c.unlocked,
                total: sum.total + c.total,
            });
        let perfect = completion
            .values()
            .filter(|c| c.total > 0 && c.unlocked == c.total)
            .count();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Achievement tracker").font(bold(32.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Get all from Steam")
                    .on_hover_text("Reads achievements for every Steam game you've played")
                    .clicked()
                {
                    self.start_job(
                        Job::FetchAchievements { game_id: None },
                        "Reading achievements…",
                    );
                }
            });
        });
        ui.label(paint::muted(
            "Each game's own achievements: from Steam, or a checklist you write on the game's page.",
            &theme,
        ));
        ui.add_space(12.0);
        paint::stat_row(
            ui,
            &theme,
            &[
                ("Unlocked", total.unlocked.to_string()),
                ("Overall", format!("{:.0}%", total.percent())),
                ("Games tracked", completion.len().to_string()),
                ("Perfect games", perfect.to_string()),
            ],
        );
        ui.add_space(16.0);

        let gap = 18.0;
        let half = ((ui.available_width() - gap) / 2.0).floor();
        let mut open = None;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(half - 38.0);
                ui.set_min_height(260.0);
                ui.label(paint::strong("Recently unlocked", 17.0));
                if recent.is_empty() {
                    ui.label(paint::muted("Nothing yet.", &theme));
                }
                for a in &recent {
                    let title = self.title_of(a.game_id);
                    achievement_row(ui, &theme, a, Some(&title));
                }
            });
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(half - 38.0);
                ui.set_min_height(260.0);
                ui.label(paint::strong("Your rarest", 17.0));
                if rarest.is_empty() {
                    ui.label(paint::muted(
                        "Rarity comes from Steam's player counts.",
                        &theme,
                    ));
                }
                for a in &rarest {
                    let title = self.title_of(a.game_id);
                    achievement_row(ui, &theme, a, Some(&title));
                }
            });
        });
        ui.add_space(16.0);

        // Every game, closest to 100% first.
        paint::panel(ui, theme.panel_frame(), |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("By game", 17.0));
            let mut games: Vec<(i64, Completion)> =
                completion.iter().map(|(id, c)| (*id, *c)).collect();
            games.sort_by(|a, b| b.1.percent().total_cmp(&a.1.percent()).then(a.0.cmp(&b.0)));
            if games.is_empty() {
                ui.label(paint::muted(
                    "No achievements yet. Press \"Get all from Steam\", or add your own on a game's page.",
                    &theme,
                ));
            }
            let bar_width = (ui.available_width() - 420.0).max(120.0);
            for (id, c) in games {
                ui.horizontal(|ui| {
                    let name = paint::left_label(
                        ui,
                        260.0,
                        egui::Label::new(self.title_of(id)).sense(Sense::click()),
                    );
                    if name
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        open = Some(id);
                    }
                    let colour = if c.unlocked == c.total {
                        theme.success
                    } else {
                        theme.accent
                    };
                    paint::bar(ui, &theme, c.percent() / 100.0, colour, bar_width);
                    ui.label(paint::muted(
                        format!("{} / {} ({:.0}%)", c.unlocked, c.total, c.percent()),
                        &theme,
                    ));
                });
            }
        });
        if let Some(id) = open {
            self.go(Screen::Details(id));
        }
    }
}
