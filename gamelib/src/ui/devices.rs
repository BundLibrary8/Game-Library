//! The Devices screen: everything plugged into the PC, controllers with
//! their batteries, and paired Bluetooth devices with Connect and
//! Disconnect buttons.

use std::collections::HashSet;

use eframe::egui::{self, Color32, Sense, vec2};
use gamelib::devices::{BtDevice, Bus, Device, Kind};
use gamelib::worker::Job;

use super::theme::{Theme, bold, regular};
use super::{GameLibApp, paint};

/// Refresh the lists this often while the screen is open, in seconds.
const REFRESH_SECS: f64 = 20.0;

#[derive(Default)]
pub struct DevicesState {
    pub devices: Option<Result<Vec<Device>, String>>,
    pub bluetooth: Option<Result<Vec<BtDevice>, String>>,
    /// A scan is under way.
    pub scanning: bool,
    /// When the last scan was asked for (egui's clock).
    pub last_scan: Option<f64>,
    /// Bluetooth devices being connected or disconnected right now.
    pub busy: HashSet<u64>,
}

/// A small coloured "pill" with text in it.
fn pill(ui: &mut egui::Ui, text: &str, colour: Color32) {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), bold(11.5), colour);
    let size = galley.size() + vec2(16.0, 6.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter()
        .rect_filled(rect, size.y / 2.0, colour.gamma_multiply(0.18));
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, colour);
}

/// A row: a round icon badge, a title, a line under it, and room on the
/// right for buttons.
fn device_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    letter: &str,
    title: &str,
    detail: &str,
    right: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.set_min_height(48.0);
        let (badge, _) = ui.allocate_exact_size(vec2(38.0, 38.0), Sense::hover());
        ui.painter()
            .circle_filled(badge.center(), 19.0, theme.panel_alt);
        ui.painter().text(
            badge.center(),
            egui::Align2::CENTER_CENTER,
            letter,
            bold(15.0),
            theme.accent,
        );
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add(egui::Label::new(egui::RichText::new(title).font(bold(15.0))).truncate());
            ui.label(
                egui::RichText::new(detail)
                    .font(regular(12.5))
                    .color(theme.muted),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
    });
}

/// An error message with a capital letter and a full stop.
fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    let mut out: String = match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    };
    if !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

fn letter_for(kind: Kind) -> &'static str {
    match kind {
        Kind::Controller => "C",
        Kind::Keyboard => "K",
        Kind::Mouse => "M",
        Kind::Audio => "A",
        Kind::Camera => "W",
        Kind::Display => "D",
        Kind::Storage => "S",
        Kind::Printer => "P",
        Kind::Bluetooth => "B",
        Kind::Other => "•",
    }
}

impl GameLibApp {
    fn scan_devices(&mut self, time: f64) {
        if !self.devices.scanning {
            self.devices.scanning = true;
            self.devices.last_scan = Some(time);
            self.worker.send(Job::ScanDevices);
        }
    }

    pub fn devices_screen(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let time = ui.input(|i| i.time);
        // Look when the screen opens, then every so often while it's open.
        if self
            .devices
            .last_scan
            .is_none_or(|at| time - at > REFRESH_SECS)
        {
            self.scan_devices(time);
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(2));

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Devices").font(bold(32.0)));
            if self.devices.scanning {
                ui.spinner();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Refresh").clicked() {
                    self.scan_devices(time);
                }
                if cfg!(windows) && ui.button("Windows Bluetooth settings").clicked() {
                    self.check(gamelib::launcher::open_bluetooth_settings());
                }
            });
        });
        ui.label(paint::muted(
            "Everything plugged into or paired with this PC. Updates by itself every 20 seconds.",
            &theme,
        ));
        ui.add_space(12.0);

        // Bluetooth.
        let mut toggle: Option<(u64, bool)> = None;
        paint::panel(ui, theme.panel_frame(), |ui| {
            ui.set_width(ui.available_width());
            ui.label(paint::strong("Bluetooth", 17.0));
            ui.add_space(4.0);
            match &self.devices.bluetooth {
                None => {
                    ui.label(paint::muted("Looking…", &theme));
                }
                Some(Err(err)) => {
                    ui.label(paint::muted(sentence(err), &theme));
                }
                Some(Ok(list)) if list.is_empty() => {
                    ui.label(paint::muted(
                        "No paired devices. Pair one in Windows' Bluetooth settings first, then it appears here.",
                        &theme,
                    ));
                }
                Some(Ok(list)) => {
                    for device in list {
                        let busy = self.devices.busy.contains(&device.address);
                        let detail = format!("{} · {}", device.kind, device.address_text());
                        let name = if device.name.is_empty() {
                            "Unnamed device".to_string()
                        } else {
                            device.name.clone()
                        };
                        device_row(ui, &theme, "B", &name, &detail, |ui| {
                            if busy {
                                ui.spinner();
                                ui.label(paint::muted(
                                    if device.connected {
                                        "Disconnecting…"
                                    } else {
                                        "Connecting…"
                                    },
                                    &theme,
                                ));
                            } else if device.connected {
                                if ui.button("Disconnect").clicked() {
                                    toggle = Some((device.address, false));
                                }
                            } else if paint::accent_button(
                                ui,
                                &theme,
                                "Connect",
                                13.0,
                                vec2(16.0, 7.0),
                            )
                            .clicked()
                            {
                                toggle = Some((device.address, true));
                            }
                            ui.add_space(8.0);
                            if device.connected {
                                pill(ui, "Connected", theme.success);
                            } else {
                                pill(ui, "Not connected", theme.muted);
                            }
                        });
                    }
                    ui.add_space(4.0);
                    ui.label(
                        paint::muted(
                            "The device needs to be switched on and nearby to connect. Bluetooth LE-only devices are managed in Windows' settings.",
                            &theme,
                        )
                        .size(12.5),
                    );
                }
            }
        });
        if let Some((address, connect)) = toggle {
            self.devices.busy.insert(address);
            self.start_job(
                Job::SetBluetooth { address, connect },
                if connect {
                    "Connecting…"
                } else {
                    "Disconnecting…"
                },
            );
        }
        ui.add_space(14.0);

        // Controllers, live from the controller library (with batteries).
        let pads = self.pad.list();
        if !pads.is_empty() {
            paint::panel(ui, theme.panel_frame(), |ui| {
                ui.set_width(ui.available_width());
                ui.label(paint::strong("Game controllers", 17.0));
                ui.add_space(4.0);
                for (name, power) in &pads {
                    let detail = power.clone().unwrap_or_else(|| "Ready to play".into());
                    device_row(ui, &theme, "C", name, &detail, |ui| {
                        pill(ui, "Connected", theme.success);
                    });
                }
            });
            ui.add_space(14.0);
        }

        // Everything else, grouped by kind.
        match &self.devices.devices {
            None => {
                ui.label(paint::muted("Looking for devices…", &theme));
            }
            Some(Err(err)) => {
                ui.colored_label(theme.danger, sentence(err));
            }
            Some(Ok(list)) => {
                let list = list.clone();
                if list.is_empty() {
                    ui.label(paint::muted("No peripherals found.", &theme));
                }
                let gap = 14.0;
                let width = ui.available_width();
                let columns = ((width + gap) / (420.0 + gap)).floor().max(1.0) as usize;
                let card = (width - gap * (columns as f32 - 1.0)) / columns as f32;
                let groups: Vec<(Kind, Vec<&Device>)> = Kind::ALL
                    .into_iter()
                    .map(|kind| {
                        (
                            kind,
                            list.iter().filter(|d| d.kind == kind).collect::<Vec<_>>(),
                        )
                    })
                    .filter(|(_, devices)| !devices.is_empty())
                    .collect();
                for row in groups.chunks(columns) {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        for (kind, devices) in row {
                            paint::panel(ui, theme.panel_frame(), |ui| {
                                ui.set_width(card - 38.0);
                                ui.horizontal(|ui| {
                                    ui.label(paint::strong(kind.label(), 16.0));
                                    ui.label(paint::muted(devices.len().to_string(), &theme));
                                });
                                ui.add_space(4.0);
                                for device in devices {
                                    let mut detail = Vec::new();
                                    if let Some(maker) = &device.manufacturer {
                                        detail.push(maker.clone());
                                    }
                                    match device.bus {
                                        Bus::Usb => detail.push("USB".into()),
                                        Bus::Bluetooth => detail.push("Bluetooth".into()),
                                        Bus::Other => {}
                                    }
                                    let ok = device.is_ok();
                                    let status = device.status.clone();
                                    device_row(
                                        ui,
                                        &theme,
                                        letter_for(device.kind),
                                        &device.name,
                                        &detail.join(" · "),
                                        |ui| {
                                            if !ok {
                                                pill(ui, &status, theme.danger)
                                            }
                                        },
                                    );
                                }
                            });
                        }
                    });
                    ui.add_space(gap);
                }
            }
        }
    }

    /// The worker found devices.
    pub fn devices_found(
        &mut self,
        devices: Result<Vec<Device>, String>,
        bluetooth: Result<Vec<BtDevice>, String>,
    ) {
        self.devices.scanning = false;
        self.devices.devices = Some(devices);
        self.devices.bluetooth = Some(bluetooth);
    }

    /// A Bluetooth device was connected or disconnected (or wasn't).
    pub fn bluetooth_changed(&mut self, address: u64, connect: bool, result: Result<(), String>) {
        self.devices.busy.remove(&address);
        let name = match &self.devices.bluetooth {
            Some(Ok(list)) => list
                .iter()
                .find(|d| d.address == address)
                .map(|d| d.name.clone())
                .unwrap_or_default(),
            _ => String::new(),
        };
        match result {
            Ok(()) => {
                self.status = format!(
                    "{} {name}.",
                    if connect { "Connected" } else { "Disconnected" }
                );
                self.toast(
                    if connect {
                        "Bluetooth connected"
                    } else {
                        "Bluetooth disconnected"
                    },
                    &name,
                );
            }
            Err(err) => self.error = Some(err),
        }
        // Look again so the buttons show the new state.
        self.devices.last_scan = None;
    }
}
