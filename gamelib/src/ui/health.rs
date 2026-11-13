//! The Library health screen: install checks, disk space, and a report of
//! anything worth fixing.

use std::collections::HashMap;

use eframe::egui::{self, vec2};
use gamelib::health::{self, Issue, format_bytes};
use gamelib::worker::Job;

use super::theme::bold;
use super::{GameLibApp, Screen, paint};

impl GameLibApp {
    /// Ask where a game's exe is now, and use it.
    pub fn locate_exe(&mut self, game_id: i64) {
        if let Some(exe) = rfd::FileDialog::new()
            .set_title("Where is the game's exe?")
            .pick_file()
        {
            self.check(
                self.db
                    .set_launch(game_id, &gamelib::model::LaunchTarget::exe(exe)),
            );
            self.check(self.db.refresh_exe_installed());
            self.dirty = true;
        }
    }

    pub fn health_screen(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        // Checking every game's files and every drive's free space is too
        // slow to do 60 times a second: it's redone every 10 seconds, or
        // when the library changes.
        let time = ui.input(|i| i.time);
        let (games, extras, db) = (&self.data.visible, &self.data.extras, &self.db);
        let cached = super::memo(&mut self.memos.health, self.generation, time, 10.0, || {
            let sizes = db.install_sizes().unwrap_or_default();
            // Free space is looked up once per drive, not once per game.
            let mut drive_cache: HashMap<std::path::PathBuf, Option<(std::path::PathBuf, f64)>> =
                HashMap::new();
            let report = health::check_library(games, extras, &sizes, |dir| {
                let key = dir.components().take(2).collect::<std::path::PathBuf>();
                drive_cache
                    .entry(key)
                    .or_insert_with(|| health::drive_of(dir))
                    .clone()
            });
            std::rc::Rc::new((report, sizes))
        });
        let (report, sizes) = (&cached.0, &cached.1);

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Library health").font(bold(32.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Measure disk space")
                    .on_hover_text("Adds up the size of every installed game's folder. Takes a while on big libraries.")
                    .clicked()
                {
                    self.start_job(Job::MeasureDisk, "Measuring disk space…");
                }
            });
        });
        ui.label(paint::muted(
            "Checks that every game can still start, and lists anything worth tidying up.",
            &theme,
        ));
        ui.add_space(12.0);

        let score_colour = match report.score {
            90..=100 => theme.success,
            60..=89 => theme.stardust,
            _ => theme.danger,
        };
        paint::stat_row(
            ui,
            &theme,
            &[
                ("Health score", format!("{} / 100", report.score)),
                ("Problems", report.problems.to_string()),
                ("Suggestions", report.suggestions.to_string()),
                (
                    "Disk used",
                    if report.measured_games > 0 {
                        format!(
                            "{} ({} games)",
                            format_bytes(report.measured_bytes),
                            report.measured_games
                        )
                    } else {
                        "Not measured".to_string()
                    },
                ),
            ],
        );
        let (bar, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), 6.0), egui::Sense::hover());
        ui.painter().rect_filled(bar, 3.0, theme.panel_alt);
        let mut fill = bar;
        fill.set_width(bar.width() * report.score as f32 / 100.0);
        ui.painter().rect_filled(fill, 3.0, score_colour);
        ui.add_space(16.0);

        let gap = 18.0;
        let half = ((ui.available_width() - gap) / 2.0).floor();
        let mut go = None;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            // Drives.
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(half - 38.0);
                ui.set_min_height(200.0);
                ui.label(paint::strong("Drives", 17.0));
                ui.add_space(6.0);
                if report.drives.is_empty() {
                    ui.label(paint::muted(
                        "No installed games with a folder to check.",
                        &theme,
                    ));
                }
                for (drive, free_gb, count) in &report.drives {
                    ui.horizontal(|ui| {
                        ui.label(paint::strong(drive.display().to_string(), 14.5));
                        ui.label(paint::muted(format!("{count} games"), &theme));
                    });
                    let low = *free_gb < 20.0;
                    ui.label(
                        egui::RichText::new(format!("{free_gb:.0} GB free")).color(if low {
                            theme.danger
                        } else {
                            theme.muted
                        }),
                    );
                    if low {
                        ui.label(
                            paint::muted("Getting full: games may fail to update.", &theme)
                                .size(12.5),
                        );
                    }
                    ui.add_space(6.0);
                }
            });
            // Biggest games.
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(half - 38.0);
                ui.set_min_height(200.0);
                ui.label(paint::strong("Biggest games", 17.0));
                ui.add_space(6.0);
                let mut biggest: Vec<(i64, u64)> = sizes.iter().map(|(id, b)| (*id, *b)).collect();
                biggest.sort_by_key(|(id, bytes)| (std::cmp::Reverse(*bytes), *id));
                if biggest.is_empty() {
                    ui.label(paint::muted(
                        "Press \"Measure disk space\" to see how much room each game takes.",
                        &theme,
                    ));
                }
                let most = biggest.first().map(|b| b.1).unwrap_or(1).max(1);
                for (id, bytes) in biggest.into_iter().take(8) {
                    let Some(game) = self.data.game(id) else {
                        continue;
                    };
                    ui.horizontal(|ui| {
                        let name = ui.add(
                            egui::Label::new(game.title.clone())
                                .truncate()
                                .sense(egui::Sense::click()),
                        );
                        if name.clicked() {
                            go = Some(Screen::Details(id));
                        }
                    });
                    ui.horizontal(|ui| {
                        paint::bar(
                            ui,
                            &theme,
                            bytes as f32 / most as f32,
                            theme.accent,
                            half - 160.0,
                        );
                        ui.label(paint::muted(format_bytes(bytes), &theme));
                    });
                }
            });
        });
        ui.add_space(16.0);

        // The report.
        paint::panel(ui, theme.panel_frame(), |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("What to look at", 17.0));
            ui.add_space(6.0);
            if report.games.is_empty() {
                ui.label(paint::muted(
                    "Nothing. Your library is in great shape.",
                    &theme,
                ));
            }
            for (id, issues) in report.games.iter().take(60) {
                let Some(game) = self.data.game(*id).cloned() else {
                    continue;
                };
                ui.horizontal(|ui| {
                    let problem = issues.iter().any(Issue::is_problem);
                    let (dot, _) = ui.allocate_exact_size(vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter().circle_filled(
                        dot.center(),
                        4.0,
                        if problem {
                            theme.danger
                        } else {
                            theme.stardust
                        },
                    );
                    if ui
                        .add(
                            egui::Label::new(paint::strong(&game.title, 14.5))
                                .sense(egui::Sense::click()),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        go = Some(Screen::Details(*id));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if issues.iter().any(|i| matches!(i, Issue::ExeMissing(_)))
                            && ui.small_button("Locate exe…").clicked()
                        {
                            self.locate_exe(*id);
                        }
                        if issues.contains(&Issue::NoCover)
                            && ui.small_button("Find cover").clicked()
                        {
                            self.start_job(
                                Job::FetchCover {
                                    game_id: *id,
                                    search_title: None,
                                },
                                "Looking for a cover…",
                            );
                        }
                    });
                });
                for issue in issues {
                    let text = match issue {
                        Issue::Duplicate(other) => {
                            format!("{} (\"{}\")", issue.describe(), self.title_of(*other))
                        }
                        _ => issue.describe(),
                    };
                    ui.label(paint::muted(format!("      {text}"), &theme).size(13.0));
                }
                ui.add_space(4.0);
            }
            if report.games.len() > 60 {
                ui.label(paint::muted(
                    format!("…and {} more.", report.games.len() - 60),
                    &theme,
                ));
            }
        });
        if let Some(screen) = go {
            self.go(screen);
        }
    }
}
