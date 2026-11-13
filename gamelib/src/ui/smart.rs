//! The window for making and editing a smart collection.

use eframe::egui;
use gamelib::extras::Status;
use gamelib::model::Source;
use gamelib::smart::{Played, Rules, SmartCollection};

use super::paint;
use super::theme::{Theme, bold, source_name};

/// A smart collection being edited. Numbers are kept as typed text until
/// it's saved.
pub struct SmartDraft {
    /// Which saved collection this is, or `None` for a new one.
    pub index: Option<usize>,
    pub name: String,
    pub rules: Rules,
    pub min_hours: String,
    pub max_hours: String,
    pub max_length: String,
    pub idle_days: String,
}

fn number_text(value: Option<f32>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

fn parse<T: std::str::FromStr>(text: &str) -> Result<Option<T>, String> {
    let text = text.trim().replace(',', ".");
    if text.is_empty() {
        return Ok(None);
    }
    text.parse()
        .map(Some)
        .map_err(|_| format!("\"{text}\" isn't a number"))
}

impl SmartDraft {
    pub fn new() -> SmartDraft {
        SmartDraft::from(
            None,
            &SmartCollection {
                name: String::new(),
                rules: Rules::default(),
            },
        )
    }

    pub fn from(index: Option<usize>, collection: &SmartCollection) -> SmartDraft {
        let rules = &collection.rules;
        SmartDraft {
            index,
            name: collection.name.clone(),
            rules: rules.clone(),
            min_hours: number_text(rules.min_hours),
            max_hours: number_text(rules.max_hours),
            max_length: number_text(rules.max_length_hours),
            idle_days: rules.idle_days.map(|d| d.to_string()).unwrap_or_default(),
        }
    }

    /// The finished collection, or what's wrong with it.
    pub fn build(&self) -> Result<SmartCollection, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("Give it a name".into());
        }
        let mut rules = self.rules.clone();
        rules.min_hours = parse(&self.min_hours)?;
        rules.max_hours = parse(&self.max_hours)?;
        rules.max_length_hours = parse(&self.max_length)?;
        rules.idle_days = parse(&self.idle_days)?;
        Ok(SmartCollection {
            name: name.to_string(),
            rules,
        })
    }
}

/// Toggle `value` in `list`.
fn toggle(list: &mut Vec<String>, value: &str) {
    if let Some(at) = list.iter().position(|v| v == value) {
        list.remove(at);
    } else {
        list.push(value.to_string());
    }
}

/// What was clicked at the bottom of the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Save,
    Delete,
    Cancel,
}

/// Draw the editor. `matching` is how many games the rules match right
/// now.
pub fn editor(
    ui: &mut egui::Ui,
    theme: &Theme,
    draft: &mut SmartDraft,
    matching: Option<usize>,
) -> Option<Action> {
    let mut result = None;
    ui.set_width(780.0);
    let heading = if draft.index.is_some() {
        "Edit smart collection"
    } else {
        "New smart collection"
    };
    ui.label(egui::RichText::new(heading).font(bold(22.0)));
    ui.label(paint::muted(
        "A saved filter that keeps itself up to date. Games are in it while they match every rule. Leave a rule empty to ignore it.",
        theme,
    ));
    ui.add_space(10.0);
    // Scrolls on a small window, so the buttons stay in view.
    let max_height = (ui.ctx().content_rect().height() - 300.0).max(200.0);
    egui::ScrollArea::vertical()
        .max_height(max_height)
        .show(ui, |ui| {
            egui::Grid::new("smart_rules")
                .num_columns(2)
                .spacing([14.0, 10.0])
                .show(ui, |ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut draft.name).desired_width(300.0));
                    ui.end_row();

                    ui.label("From");
                    // Two fixed rows: chips that wrap by themselves overlap the
                    // next row of the grid.
                    ui.vertical(|ui| {
                        for row in Source::ALL.chunks(4) {
                            ui.horizontal(|ui| {
                                for source in row {
                                    let on =
                                        draft.rules.sources.iter().any(|s| s == source.as_str());
                                    if paint::chip(ui, theme, source_name(*source), on).clicked() {
                                        toggle(&mut draft.rules.sources, source.as_str());
                                    }
                                }
                            });
                        }
                    });
                    ui.end_row();

                    ui.label("Status");
                    ui.horizontal(|ui| {
                        let on = draft.rules.statuses.iter().any(|s| s == "none");
                        if paint::chip(ui, theme, "No status", on).clicked() {
                            toggle(&mut draft.rules.statuses, "none");
                        }
                        for status in Status::ALL {
                            let on = draft.rules.statuses.iter().any(|s| s == status.as_str());
                            if paint::chip(ui, theme, status.label(), on).clicked() {
                                toggle(&mut draft.rules.statuses, status.as_str());
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("Played");
                    ui.horizontal(|ui| {
                        for (value, label) in [
                            (Played::Any, "Any"),
                            (Played::Never, "Never played"),
                            (Played::Played, "Played"),
                        ] {
                            if paint::chip(ui, theme, label, draft.rules.played == value).clicked()
                            {
                                draft.rules.played = value;
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("Hours played");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut draft.min_hours)
                                .hint_text("min")
                                .desired_width(60.0),
                        );
                        ui.label("to");
                        ui.add(
                            egui::TextEdit::singleline(&mut draft.max_hours)
                                .hint_text("max")
                                .desired_width(60.0),
                        );
                    });
                    ui.end_row();

                    ui.label("Beatable within");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut draft.max_length)
                                .hint_text("hours")
                                .desired_width(60.0),
                        );
                        ui.label(paint::muted(
                            "hours (from Hours to beat, or a genre guess)",
                            theme,
                        ));
                    });
                    ui.end_row();

                    ui.label("Not played for");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut draft.idle_days)
                                .hint_text("days")
                                .desired_width(60.0),
                        );
                        ui.label(paint::muted("days or more", theme));
                    });
                    ui.end_row();

                    ui.label("Rated at least");
                    ui.horizontal(|ui| {
                        if paint::chip(ui, theme, "Any", draft.rules.min_rating.is_none()).clicked()
                        {
                            draft.rules.min_rating = None;
                        }
                        for stars in 1..=5u8 {
                            let on = draft.rules.min_rating == Some(stars);
                            if paint::chip(ui, theme, &format!("{stars}+"), on).clicked() {
                                draft.rules.min_rating = Some(stars);
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("Genre");
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.rules.genre)
                            .hint_text("e.g. RPG")
                            .desired_width(200.0),
                    );
                    ui.end_row();
                    ui.label("Tag or collection");
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.rules.tag)
                            .hint_text("e.g. Co-op nights")
                            .desired_width(200.0),
                    );
                    ui.end_row();
                    ui.label("Title contains");
                    ui.add(egui::TextEdit::singleline(&mut draft.rules.title).desired_width(200.0));
                    ui.end_row();

                    ui.label("");
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut draft.rules.installed_only, "Installed only");
                        ui.checkbox(&mut draft.rules.favourites_only, "Favourites only");
                    });
                    ui.end_row();
                });
        });

    ui.add_space(8.0);
    let built = draft.build();
    match (&built, matching) {
        (Err(problem), _) => {
            ui.label(egui::RichText::new(problem).color(theme.danger));
        }
        (Ok(_), Some(count)) => {
            ui.label(paint::muted(
                format!(
                    "{count} {} right now.",
                    if count == 1 {
                        "game matches"
                    } else {
                        "games match"
                    }
                ),
                theme,
            ));
        }
        _ => {}
    }
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if draft.index.is_some() && ui.button("Delete").clicked() {
            result = Some(Action::Delete);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let can_save = built.is_ok();
            let save = ui
                .add_enabled_ui(can_save, |ui| {
                    paint::accent_button(ui, theme, "Save", 15.0, egui::vec2(22.0, 11.0))
                })
                .inner
                .clicked();
            if save {
                result = Some(Action::Save);
            }
            if ui
                .add(egui::Button::new("Cancel").min_size(egui::vec2(0.0, 40.0)))
                .clicked()
            {
                result = Some(Action::Cancel);
            }
        });
    });
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        result = Some(Action::Cancel);
    }
    result
}
