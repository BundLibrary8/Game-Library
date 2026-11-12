//! The shop screen: spend Stardust on cosmetics.

use eframe::egui;
use gamelib::now;
use gamelib::shop::{self, ItemKind};

use super::GameLibApp;

fn kind_name(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::LibraryBackground => "Library background",
        ItemKind::ProfileBanner => "Profile banner",
        ItemKind::Badge => "Badge",
        ItemKind::NameChange => "Service",
    }
}

impl GameLibApp {
    pub fn shop_screen(&mut self, ui: &mut egui::Ui) {
        let mut buy: Option<&'static str> = None;
        let mut equip: Option<Option<&'static str>> = None;
        let equipped = self
            .db
            .equipped_library_background()
            .ok()
            .flatten()
            .map(|i| i.id);
        let credits = self.db.name_change_credits().unwrap_or(0);

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Shop");
                ui.label(egui::RichText::new(format!("✦ {}", self.stardust)).size(18.0).strong());
            });
            ui.weak(
                "Earn Stardust by playing: ✦10 per hour, ✦50 for finishing a game (once each, \
                 after an hour of play), ✦5 for the first game you launch each day. No real money.",
            );
            ui.add_space(8.0);

            egui::Grid::new("shop_items").num_columns(4).striped(true).spacing([16.0, 8.0]).show(ui, |ui| {
                for item in shop::items_for_sale() {
                    let price = item.price.unwrap_or(0);
                    let owned = self.db.owns_item(item.id).unwrap_or(false);
                    ui.strong(item.name);
                    ui.weak(kind_name(item.kind));
                    ui.label(format!("✦ {price}"));
                    ui.horizontal(|ui| {
                        if owned {
                            ui.label("✓ Owned");
                            if item.kind == ItemKind::LibraryBackground {
                                if equipped == Some(item.id) {
                                    if ui.button("Unequip").clicked() {
                                        equip = Some(None);
                                    }
                                } else if ui.button("Equip").clicked() {
                                    equip = Some(Some(item.id));
                                }
                            }
                        } else {
                            let can_afford = self.stardust >= price as i64;
                            let button = ui.add_enabled(can_afford, egui::Button::new("Buy"));
                            if button.clicked() {
                                buy = Some(item.id);
                            }
                            if !can_afford {
                                ui.weak(format!("need ✦{} more", price as i64 - self.stardust));
                            }
                        }
                        if item.kind == ItemKind::NameChange && credits > 0 {
                            ui.weak(format!("{credits} unused"));
                        }
                    });
                    ui.end_row();
                }
            });
            ui.weak("This temporary window doesn't draw library backgrounds yet; equipping one just saves the choice.");

            ui.add_space(16.0);
            ui.strong("Stardust history");
            let ledger = self.db.stardust_ledger().unwrap_or_default();
            if ledger.is_empty() {
                ui.weak("Nothing yet.");
            }
            egui::Grid::new("ledger").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                for entry in ledger.iter().take(15) {
                    ui.label(format!("{:+}", entry.amount));
                    let what = match entry.reason.as_str() {
                        "playtime" => "Playtime".to_string(),
                        "finished" => "Finished a game".to_string(),
                        "daily" => "First game of the day".to_string(),
                        "purchase" => {
                            let name = entry
                                .item_id
                                .as_deref()
                                .and_then(shop::find_item)
                                .map(|item| item.name)
                                .unwrap_or("something");
                            format!("Bought {name}")
                        }
                        other => other.to_string(),
                    };
                    ui.weak(what);
                    ui.end_row();
                }
            });
        });

        if let Some(item_id) = buy
            && self.check(self.db.purchase(item_id, now())).is_some()
        {
            let name = shop::find_item(item_id).map(|i| i.name).unwrap_or(item_id);
            self.status = format!("Bought {name}.");
        }
        if let Some(choice) = equip {
            self.check(self.db.equip_library_background(choice));
        }
    }
}
