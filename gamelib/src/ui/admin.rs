//! The admin panel: locked with the password in `admin_password.txt`.
//! Give (or take) Stardust, and use any picture as the background.

use eframe::egui::{self, Color32, vec2};
use gamelib::admin;
use gamelib::{now, paths};

use super::theme::bold;
use super::{GameLibApp, paint};
use crate::text::thousands;

/// What's typed into the admin panel.
#[derive(Default)]
pub struct AdminState {
    pub unlocked: bool,
    pub password: String,
    pub amount: String,
    /// The last thing that went wrong on the login form.
    pub error: Option<String>,
    /// The password box has been given the cursor already.
    pub focused: bool,
}

impl GameLibApp {
    pub fn admin_screen(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Admin panel").font(bold(32.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.admin.unlocked && ui.button("Lock").clicked() {
                    self.admin = Default::default();
                }
            });
        });
        let file = admin::password_file();
        let file_text = file
            .as_ref()
            .map(|f| f.display().to_string())
            .unwrap_or_else(|e| format!("{e:#}"));
        ui.add_space(10.0);

        if !self.admin.unlocked {
            self.admin_login(ui, &file_text);
            return;
        }

        // Stardust.
        paint::panel(ui, theme.panel_frame(), |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("Give Stardust", 17.0));
            ui.horizontal(|ui| {
                ui.label(paint::muted("Balance:", &theme));
                paint::stardust(ui, &theme, &thousands(self.data.stardust), 16.0);
            });
            ui.add_space(6.0);
            let mut give: Option<i64> = None;
            ui.horizontal(|ui| {
                for amount in [100, 500, 1_000, 10_000] {
                    if paint::chip(ui, &theme, &format!("+{}", thousands(amount)), false).clicked()
                    {
                        give = Some(amount);
                    }
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.admin.amount)
                        .hint_text("Amount, e.g. 250")
                        .desired_width(160.0),
                );
                let amount = self
                    .admin
                    .amount
                    .trim()
                    .replace([',', '_', ' '], "")
                    .parse::<i64>();
                ui.add_enabled_ui(amount.is_ok(), |ui| {
                    if paint::accent_button(ui, &theme, "Give", 14.0, vec2(20.0, 9.0)).clicked() {
                        give = amount.clone().ok();
                    }
                    if ui.button("Take away").clicked() {
                        give = amount.clone().ok().map(|a| -a.abs());
                    }
                });
            });
            ui.label(paint::muted(
                "Shows in the Stardust history as \"Given in the admin panel\".",
                &theme,
            ));
            if let Some(amount) = give
                && let Some(balance) = self.check(self.db.admin_give_stardust(amount, now()))
            {
                self.status = format!(
                    "{} {} Stardust. Balance: {}.",
                    if amount >= 0 { "Gave" } else { "Took away" },
                    thousands(amount.abs()),
                    thousands(balance)
                );
                self.admin.amount.clear();
                self.dirty = true;
            }
        });
        ui.add_space(14.0);

        // Background picture.
        paint::panel(ui, theme.panel_frame(), |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("Background picture", 17.0));
            ui.label(paint::muted(
                "Use any picture (PNG, JPEG or WebP) as the background behind every screen. It's cropped to fill the window and dimmed a little so text stays readable.",
                &theme,
            ));
            ui.add_space(8.0);
            if let Some((path, size)) = self.data.custom_background.clone() {
                let width = 320.0;
                let (rect, _) =
                    ui.allocate_exact_size(vec2(width, width * 9.0 / 16.0), egui::Sense::hover());
                let shown =
                    paint::picture_fill(ui.ctx(), ui.painter(), &path, size, rect, Color32::WHITE);
                if !shown {
                    ui.painter().rect_filled(rect, 8.0, theme.panel_alt);
                }
                ui.label(paint::muted(
                    format!("{} × {} pixels", size[0], size[1]),
                    &theme,
                ));
            } else {
                ui.label(paint::muted("Using the normal background.", &theme));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Choose picture…").clicked()
                    && let Some(picture) = rfd::FileDialog::new()
                        .set_title("Choose a background picture")
                        .add_filter("Pictures", &admin::IMAGE_EXTENSIONS)
                        .pick_file()
                {
                    let set = paths::data_dir().and_then(|dir| {
                        self.db
                            .set_custom_background(&picture, &dir.join("backgrounds"), now())
                    });
                    if self.check(set).is_some() {
                        self.status = "Background changed.".to_string();
                        self.dirty = true;
                    }
                }
                if self.data.custom_background.is_some()
                    && ui.button("Use the normal background").clicked()
                    && self.check(self.db.clear_custom_background()).is_some()
                {
                    self.status = "Back to the normal background.".to_string();
                    self.dirty = true;
                }
            });
        });
        ui.add_space(14.0);

        paint::panel(ui, theme.panel_frame(), |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("Password", 17.0));
            ui.label(paint::muted(
                "To change it, open this file in Notepad, replace the password line and save:",
                &theme,
            ));
            ui.add(egui::Label::new(paint::strong(&file_text, 14.0)).selectable(true));
        });
    }

    fn admin_login(&mut self, ui: &mut egui::Ui, file_text: &str) {
        let theme = self.theme;
        paint::panel(ui, theme.panel_frame(), |ui| {
            ui.set_width(460.0_f32.min(ui.available_width()));
            ui.label(paint::strong("Enter the admin password", 18.0));
            ui.add_space(8.0);
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.admin.password)
                    .password(true)
                    .hint_text("Password")
                    .desired_width(300.0),
            );
            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if !self.admin.focused {
                // Put the cursor in the box once, when the form first shows.
                self.admin.focused = true;
                response.request_focus();
            }
            ui.add_space(8.0);
            let unlock =
                paint::accent_button(ui, &theme, "Unlock", 14.0, vec2(22.0, 9.0)).clicked();
            if enter || unlock {
                let attempt = std::mem::take(&mut self.admin.password);
                match admin::password_file().and_then(|file| admin::check_password(&file, &attempt))
                {
                    Ok(true) => {
                        self.admin.unlocked = true;
                        self.admin.error = None;
                    }
                    Ok(false) => self.admin.error = Some("That isn't the password.".into()),
                    Err(err) => self.admin.error = Some(format!("{err:#}")),
                }
            }
            if let Some(error) = &self.admin.error {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(error).color(theme.danger));
            }
            ui.add_space(10.0);
            ui.label(paint::muted(
                format!(
                    "The password is kept in {file_text}. If that file doesn't exist yet, it's created with the password \"{}\"; change it there.",
                    admin::DEFAULT_PASSWORD
                ),
                &theme,
            ));
        });
    }
}
