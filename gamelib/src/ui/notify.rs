//! Notifications: little pop-ups in the corner of the window ("toasts"),
//! desktop notifications from Windows when GameLib isn't the window in
//! front, and the icon in the system tray.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui::{self, Id, Order, vec2};
use gamelib::worker::WorkerResult;

use super::paint;
use super::theme::{Theme, bold, regular};

/// How long a toast stays up, in seconds.
const TOAST_SECS: f64 = 6.0;

pub struct Toast {
    pub title: String,
    pub body: String,
    /// egui's clock when it appeared (set when first drawn).
    pub shown_at: Option<f64>,
}

/// Shared between the window and the worker threads.
#[derive(Clone)]
pub struct Shared {
    /// Desktop notifications are switched on.
    pub desktop: Arc<AtomicBool>,
    /// The window is visible and in front, so in-window toasts are enough.
    pub window_active: Arc<AtomicBool>,
    /// Game titles by id, so a notification can name the game.
    pub titles: Arc<Mutex<HashMap<i64, String>>>,
    /// Quit was chosen from the tray: really close this time.
    pub quitting: Arc<AtomicBool>,
}

impl Shared {
    pub fn new(desktop: bool) -> Shared {
        Shared {
            desktop: Arc::new(AtomicBool::new(desktop)),
            window_active: Arc::new(AtomicBool::new(true)),
            titles: Arc::new(Mutex::new(HashMap::new())),
            quitting: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Called on the worker threads for every result. A game that closes
    /// while GameLib is minimised or hidden in the tray gets a desktop
    /// notification straight away (the window may not be drawing).
    pub fn observe(&self, result: &WorkerResult) {
        if self.window_active.load(Ordering::Relaxed) || !self.desktop.load(Ordering::Relaxed) {
            return;
        }
        if let WorkerResult::SessionEnded { summary, .. } = result {
            let title = self
                .titles
                .lock()
                .ok()
                .and_then(|t| t.get(&summary.game_id).cloned())
                .unwrap_or_else(|| "Your game".into());
            desktop(
                &title,
                &session_text(summary.duration_secs, summary.stardust),
            );
        }
    }
}

/// "Played for 1h 20m. +13 Stardust"
pub fn session_text(duration_secs: u64, stardust: u32) -> String {
    let length = if duration_secs < 60 {
        "under a minute".to_string()
    } else {
        crate::text::short_playtime(duration_secs)
    };
    let mut text = format!("Played for {length}.");
    if stardust > 0 {
        text.push_str(&format!(" +{stardust} Stardust"));
    }
    text
}

/// Show a desktop notification (Windows only; elsewhere it's logged).
pub fn desktop(title: &str, body: &str) {
    #[cfg(windows)]
    {
        if let Err(err) = notify_rust::Notification::new()
            .appname("GameLib")
            .summary(title)
            .body(body)
            .show()
        {
            log::warn!("couldn't show a desktop notification: {err}");
        }
    }
    #[cfg(not(windows))]
    log::info!("notification: {title}: {body}");
}

/// Draw the toasts in the bottom-right corner, newest at the bottom, and
/// drop the ones whose time is up.
pub fn draw_toasts(ctx: &egui::Context, theme: &Theme, toasts: &mut Vec<Toast>, time: f64) {
    toasts.retain(|t| t.shown_at.is_none_or(|at| time - at < TOAST_SECS));
    if toasts.is_empty() {
        return;
    }
    let screen = ctx.content_rect();
    let mut bottom = screen.bottom() - 54.0;
    let mut dismissed = None;
    for (i, toast) in toasts.iter_mut().enumerate().rev().take(4) {
        let at = *toast.shown_at.get_or_insert(time);
        let age = time - at;
        // Slide in, then fade out over the last second.
        let slide = (1.0 - (age / 0.25).min(1.0)) as f32 * 30.0;
        let fade = (((TOAST_SECS - age) / 1.0).clamp(0.0, 1.0)) as f32;
        let width = 340.0;
        let response = egui::Area::new(Id::unique(("toast", i)))
            .order(Order::Foreground)
            .fixed_pos(egui::pos2(
                screen.right() - width - 20.0 + slide,
                bottom - 80.0,
            ))
            .show(ctx, |ui| {
                ui.multiply_opacity(fade);
                egui::Frame::new()
                    .fill(theme.opaque_panel())
                    .stroke(egui::Stroke::new(1.0, theme.accent.gamma_multiply(0.6)))
                    .corner_radius(12.0)
                    .inner_margin(egui::Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(width - 32.0);
                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::hover());
                            paint::star4(ui.painter(), rect.center(), 8.0, theme.accent);
                            ui.label(egui::RichText::new(&toast.title).font(bold(14.5)));
                        });
                        ui.label(
                            egui::RichText::new(&toast.body)
                                .font(regular(13.5))
                                .color(theme.muted),
                        );
                    })
                    .response
                    .interact(egui::Sense::click())
            });
        if response.inner.clicked() {
            dismissed = Some(i);
        }
        bottom -= response.response.rect.height() + 10.0;
    }
    if let Some(i) = dismissed {
        toasts.remove(i);
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
}

// --- The window's side ---------------------------------------------------------

impl super::GameLibApp {
    /// A pop-up in the corner of the window.
    pub fn toast(&mut self, title: &str, body: &str) {
        self.toasts.push(Toast {
            title: title.to_string(),
            body: body.to_string(),
            shown_at: None,
        });
    }

    /// A pop-up, and a desktop notification too if GameLib isn't the
    /// window in front.
    pub fn announce(&mut self, title: &str, body: &str) {
        self.toast(title, body);
        if !self.shared.window_active.load(Ordering::Relaxed)
            && self.shared.desktop.load(Ordering::Relaxed)
        {
            desktop(title, body);
        }
    }

    /// Put the icon in the system tray (Windows only).
    pub fn start_tray(&mut self, ctx: &egui::Context) {
        match create_tray(
            ctx.clone(),
            self.shared.clone(),
            Arc::clone(&self.tray_couch),
        ) {
            Ok(tray) => self.tray = Some(tray),
            Err(err) => log::info!("no tray icon: {err:#}"),
        }
    }

    pub fn stop_tray(&mut self) {
        self.tray = None;
    }

    /// Things to check every frame: whether the window is in front, the
    /// tray menu, closing to the tray, new achievements and goals, and
    /// keyboard shortcuts.
    pub fn window_events(&mut self, ctx: &egui::Context, _time: f64) {
        let active = ctx.input(|i| {
            let viewport = i.viewport();
            viewport.focused.unwrap_or(true) && !viewport.minimized.unwrap_or(false)
        });
        self.shared.window_active.store(active, Ordering::Relaxed);
        if let Ok(mut titles) = self.shared.titles.lock()
            && titles.len() != self.data.visible.len()
        {
            *titles = self
                .data
                .visible
                .iter()
                .map(|g| (g.id, g.title.clone()))
                .collect();
        }

        if self.tray_couch.swap(false, Ordering::Relaxed) {
            self.enter_couch(ctx);
        }
        // Closing the window keeps GameLib in the tray, if chosen.
        let closing = ctx.input(|i| i.viewport().close_requested());
        if closing
            && self.close_to_tray
            && self.tray.is_some()
            && !self.shared.quitting.load(Ordering::Relaxed)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            if !self.told_about_tray {
                self.told_about_tray = true;
                desktop(
                    "GameLib is still running",
                    "It's in the system tray, keeping track of your playtime. Right-click the icon to quit.",
                );
            }
        }

        // New achievements.
        let unlocked: std::collections::HashSet<&'static str> = self
            .data
            .achievements
            .iter()
            .filter(|a| a.unlocked())
            .map(|a| a.id)
            .collect();
        match &self.unlocked_seen {
            None => self.unlocked_seen = Some(unlocked),
            Some(seen) if *seen != unlocked => {
                let new: Vec<(String, String)> = self
                    .data
                    .achievements
                    .iter()
                    .filter(|a| a.unlocked() && !seen.contains(a.id))
                    .map(|a| (a.name.to_string(), a.description.to_string()))
                    .collect();
                self.unlocked_seen = Some(unlocked);
                for (name, description) in new {
                    self.announce(&format!("Achievement unlocked: {name}"), &description);
                }
            }
            Some(_) => {}
        }

        // The weekly goal.
        let now = gamelib::now();
        let offset = super::local_offset();
        if let Ok(Some((goal, progress))) = self.db.weekly_progress(now, offset)
            && progress.reached()
        {
            let week = gamelib::journal::week_start(now, offset);
            if self.goal_noticed != Some(week) {
                self.goal_noticed = Some(week);
                if goal.is_limit {
                    self.announce(
                        "Weekly limit reached",
                        &format!(
                            "You've played {} hours this week, your limit.",
                            progress.done_minutes / 60
                        ),
                    );
                } else {
                    self.announce(
                        "Weekly goal reached!",
                        &format!("{} hours of play this week.", progress.done_minutes / 60),
                    );
                }
            }
        }

        self.shortcuts(ctx);
        // Ctrl +/- and Ctrl 0 change the text size (egui handles the keys);
        // remember it.
        let zoom = ctx.zoom_factor();
        if (zoom - self.zoom.unwrap_or(1.0)).abs() > 0.001 {
            self.zoom = Some(zoom);
            self.check(self.db.set_setting(super::ZOOM_KEY, &format!("{zoom:.2}")));
        }
    }
}

// --- System tray (Windows) ---------------------------------------------------

/// The tray icon and its menu: Open GameLib, Couch Mode, Quit.
#[cfg(windows)]
pub struct Tray {
    _icon: tray_icon::TrayIcon,
}

#[cfg(not(windows))]
pub struct Tray;

/// A small star picture for the tray, drawn in code.
fn tray_picture() -> (Vec<u8>, u32) {
    const SIZE: u32 = 32;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let centre = (SIZE as f32 - 1.0) / 2.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = (x as f32 - centre).abs() / centre;
            let dy = (y as f32 - centre).abs() / centre;
            // A four-pointed star: inside when |x|^0.5 + |y|^0.5 is small.
            let inside = dx.sqrt() + dy.sqrt() < 1.0;
            let glow = (1.0 - (dx * dx + dy * dy).sqrt()).max(0.0);
            let i = ((y * SIZE + x) * 4) as usize;
            if inside {
                rgba[i..i + 4].copy_from_slice(&[0x8e, 0xe3, 0xff, 255]);
            } else if glow > 0.0 {
                rgba[i..i + 4].copy_from_slice(&[0x9b, 0x7c, 0xff, (glow * 140.0) as u8]);
            }
        }
    }
    (rgba, SIZE)
}

/// Put GameLib in the system tray. Clicking the icon (or "Open GameLib")
/// shows the window; "Quit" closes GameLib for real.
#[cfg(windows)]
pub fn create_tray(
    ctx: egui::Context,
    shared: Shared,
    couch: Arc<AtomicBool>,
) -> anyhow::Result<Tray> {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{MouseButton, TrayIconBuilder, TrayIconEvent};

    let open = MenuItem::new("Open GameLib", true, None);
    let couch_item = MenuItem::new("Couch Mode", true, None);
    let quit = MenuItem::new("Quit", true, None);
    let menu = Menu::new();
    menu.append(&open)?;
    menu.append(&couch_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit)?;
    let (rgba, size) = tray_picture();
    let icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("GameLib")
        .with_icon(tray_icon::Icon::from_rgba(rgba, size, size)?)
        .with_menu_on_left_click(false)
        .build()?;

    let show = {
        let ctx = ctx.clone();
        move || {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.request_repaint();
        }
    };
    let (open_id, couch_id, quit_id) = (
        open.id().clone(),
        couch_item.id().clone(),
        quit.id().clone(),
    );
    let show_from_menu = show.clone();
    let menu_ctx = ctx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == open_id {
            show_from_menu();
        } else if event.id == couch_id {
            couch.store(true, Ordering::Relaxed);
            show_from_menu();
        } else if event.id == quit_id {
            shared.quitting.store(true, Ordering::Relaxed);
            menu_ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            menu_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            menu_ctx.request_repaint();
        }
    }));
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        let clicked = matches!(
            event,
            TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            }
        );
        if clicked {
            show();
        }
    }));
    Ok(Tray { _icon: icon })
}

#[cfg(not(windows))]
pub fn create_tray(
    _ctx: egui::Context,
    _shared: Shared,
    _couch: Arc<AtomicBool>,
) -> anyhow::Result<Tray> {
    let _ = tray_picture;
    anyhow::bail!("the system tray icon is only available on Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_text_reads_well() {
        assert_eq!(session_text(30, 0), "Played for under a minute.");
        assert!(session_text(5400, 15).ends_with("+15 Stardust"));
    }

    #[test]
    fn the_tray_picture_is_a_star() {
        let (rgba, size) = tray_picture();
        assert_eq!(rgba.len(), (size * size * 4) as usize);
        let centre = ((size / 2 * size + size / 2) * 4 + 3) as usize;
        assert_eq!(rgba[centre], 255, "the middle is solid");
        assert_eq!(rgba[3], 0, "the corner is see-through");
    }
}
