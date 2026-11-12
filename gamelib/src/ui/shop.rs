//! The shop screen: spend Stardust on cosmetics.

use eframe::egui::{self, Color32, Rect, Sense, Stroke, vec2};
use gamelib::now;
use gamelib::profile::Banner;
use gamelib::shop::{self, ItemKind, ShopItem};

use super::theme::{Backdrop, Theme, bold, regular, semibold};
use super::{GameLibApp, PREVIEW_SECS, paint};
use crate::text::thousands;

/// The filter chips along the top of the shop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    All,
    Backgrounds,
    Banners,
    Badges,
    NameChange,
    Owned,
}

const FEATURED_ID: &str = "bg_constellation";

fn kind_name(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::LibraryBackground => "Library background",
        ItemKind::ProfileBanner => "Profile banner",
        ItemKind::Badge => "Badge",
        ItemKind::NameChange => "Change your display name",
    }
}

/// What a click in the shop asked for.
enum Click {
    Buy(&'static str),
    Equip(Option<&'static str>),
    UseBanner(&'static str),
    Preview(&'static str),
}

impl GameLibApp {
    pub fn shop_screen(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let mut click: Option<Click> = None;

        // Header: title, and the balance on the right.
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Shop").font(bold(32.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::Frame::new()
                    .fill(theme.panel)
                    .stroke(Stroke::new(1.0, theme.border))
                    .corner_radius(22.0)
                    .inner_margin(egui::Margin::symmetric(20, 10))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "✦ {} Stardust",
                                thousands(self.data.stardust)
                            ))
                            .font(bold(18.0))
                            .color(theme.stardust),
                        );
                    });
                ui.label(paint::muted("Earn Stardust by playing ›", &theme));
            });
        });
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            for (category, label) in [
                (Category::All, "All"),
                (Category::Backgrounds, "Library backgrounds"),
                (Category::Banners, "Profile banners"),
                (Category::Badges, "Badges"),
                (Category::NameChange, "Name change"),
                (Category::Owned, "Owned"),
            ] {
                if paint::chip(ui, &theme, label, self.shop_category == category).clicked() {
                    self.shop_category = category;
                }
            }
        });
        ui.add_space(14.0);

        let owns = |id: &str| self.data.owned.iter().any(|owned| owned == id);

        // The featured item and "how you earn" side by side.
        if matches!(self.shop_category, Category::All | Category::Backgrounds) {
            let gap = 18.0;
            let total = ui.available_width();
            let earn_width = (total * 0.42).min(540.0);
            let featured_width = total - earn_width - gap;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                if let Some(c) = self.featured(ui, featured_width, 312.0, owns(FEATURED_ID)) {
                    click = Some(c);
                }
                paint::panel(ui, theme.panel_frame().inner_margin(22.0), |ui| {
                    ui.set_width(earn_width - 46.0);
                    ui.set_min_height(312.0 - 46.0);
                    ui.label(paint::strong("How you earn Stardust", 19.0));
                    ui.add_space(4.0);
                    for (what, reward) in [
                        (
                            "Every hour you play",
                            format!("+ ✦ {}", shop::STARDUST_PER_HOUR),
                        ),
                        (
                            "Finish a game (1h+ played)",
                            format!("+ ✦ {}", shop::FINISH_REWARD),
                        ),
                        (
                            "First game of the day",
                            format!("+ ✦ {}", shop::DAILY_REWARD),
                        ),
                        ("Hit a playtime milestone", "free banner".to_string()),
                    ] {
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(what).size(16.0));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(reward)
                                            .font(bold(16.0))
                                            .color(theme.stardust),
                                    );
                                },
                            );
                        });
                    }
                    ui.add_space(6.0);
                    ui.label(
                        paint::muted(
                            "No real money. Stardust can only be earned by playing.",
                            &theme,
                        )
                        .size(13.5),
                    );
                });
            });
            ui.add_space(18.0);
        }

        // The item cards.
        let items: Vec<&'static ShopItem> = shop::CATALOGUE
            .iter()
            .filter(|item| match self.shop_category {
                Category::All => item.price.is_some() || owns(item.id),
                Category::Backgrounds => item.kind == ItemKind::LibraryBackground,
                Category::Banners => {
                    item.kind == ItemKind::ProfileBanner && (item.price.is_some() || owns(item.id))
                }
                Category::Badges => {
                    item.kind == ItemKind::Badge && (item.price.is_some() || owns(item.id))
                }
                Category::NameChange => item.kind == ItemKind::NameChange,
                Category::Owned => owns(item.id),
            })
            // The featured item already has its big card.
            .filter(|item| {
                !(item.id == FEATURED_ID
                    && matches!(self.shop_category, Category::All | Category::Backgrounds))
            })
            .collect();

        if items.is_empty() {
            ui.label(paint::muted("Nothing here yet.", &theme));
        }
        let gap = 18.0;
        let columns = (((ui.available_width() + gap) / (270.0 + gap)).floor() as usize).clamp(1, 4);
        let card_width =
            ((ui.available_width() - gap * (columns as f32 - 1.0)) / columns as f32).floor();
        for row in items.chunks(columns) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for item in row {
                    if let Some(c) = self.item_card(ui, item, card_width, owns(item.id)) {
                        click = Some(c);
                    }
                }
            });
            ui.add_space(gap - 8.0);
        }

        // Where the Stardust came from and went.
        if matches!(self.shop_category, Category::All) && !self.data.ledger.is_empty() {
            ui.add_space(10.0);
            ui.label(paint::strong("Stardust history", 17.0));
            egui::Grid::new("ledger")
                .num_columns(2)
                .spacing([18.0, 4.0])
                .show(ui, |ui| {
                    for entry in self.data.ledger.iter().take(8) {
                        let colour = if entry.amount >= 0 {
                            theme.success
                        } else {
                            theme.muted
                        };
                        ui.label(egui::RichText::new(format!("{:+}", entry.amount)).color(colour));
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
                        ui.label(paint::muted(what, &theme));
                        ui.end_row();
                    }
                });
        }

        match click {
            Some(Click::Buy(item_id)) => {
                if self.check(self.db.purchase(item_id, now())).is_some() {
                    let name = shop::find_item(item_id).map(|i| i.name).unwrap_or(item_id);
                    self.status = format!("Bought {name}.");
                }
            }
            Some(Click::Equip(choice)) => {
                self.preview = None;
                self.check(self.db.equip_library_background(choice));
            }
            Some(Click::UseBanner(item_id)) => {
                let banner = Banner::Item(item_id.to_string());
                if self
                    .check(self.db.set_banner(Some(&banner), now()))
                    .is_some()
                {
                    self.status = "Banner set on your profile.".to_string();
                }
            }
            Some(Click::Preview(item_id)) => {
                let until = ui.ctx().input(|i| i.time) + PREVIEW_SECS;
                self.preview = Some((Backdrop::from_item(Some(item_id)), until));
                self.status = format!("Previewing for {PREVIEW_SECS:.0} seconds…");
            }
            None => {}
        }
    }

    /// The big card for the featured item.
    fn featured(&self, ui: &mut egui::Ui, width: f32, height: f32, owned: bool) -> Option<Click> {
        let theme = self.theme;
        let item = shop::find_item(FEATURED_ID)?;
        let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        let clipped = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        clipped.rect_filled(rect, 0.0, Color32::from_rgb(0x06, 0x09, 0x14));
        self.stars.paint(&clipped, rect, self.anim_time);
        round_off(ui.painter(), rect, 16.0, theme.bg);
        ui.painter().rect_stroke(
            rect,
            16.0,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(122, 215, 255, 110)),
            egui::StrokeKind::Inside,
        );

        let left = rect.left() + 30.0;
        let painter = ui.painter();
        painter.text(
            egui::pos2(left, rect.bottom() - 108.0),
            egui::Align2::LEFT_BOTTOM,
            "F E A T U R E D   ·   L I B R A R Y   B A C K G R O U N D",
            semibold(12.5),
            Color32::from_rgb(0x7a, 0xd7, 0xff),
        );
        painter.text(
            egui::pos2(left, rect.bottom() - 58.0),
            egui::Align2::LEFT_BOTTOM,
            item.name,
            bold(38.0),
            Color32::WHITE,
        );
        painter.text(
            egui::pos2(left, rect.bottom() - 30.0),
            egui::Align2::LEFT_BOTTOM,
            "A living star map behind your whole library.",
            regular(16.0),
            Color32::from_white_alpha(215),
        );

        // Buttons in the bottom-right corner.
        let mut click = None;
        let buttons = Rect::from_min_max(
            egui::pos2(rect.right() - 230.0, rect.bottom() - 132.0),
            rect.right_bottom() - vec2(26.0, 24.0),
        );
        let constellation = Theme::constellation();
        // The buttons get their own little area laid over the card, clipped
        // to it so the Buy button's glow doesn't spill outside.
        let mut over_card = ui.new_child(egui::UiBuilder::new().max_rect(buttons));
        over_card.set_clip_rect(rect.intersect(ui.clip_rect()));
        {
            let ui = &mut over_card;
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
                ui.spacing_mut().item_spacing.y = 10.0;
                if owned {
                    let equipped = self.data.equipped == Some(item.id);
                    let label = if equipped { "✓ Equipped" } else { "Equip" };
                    if paint::accent_button(ui, &constellation, label, 16.0, vec2(26.0, 13.0))
                        .clicked()
                    {
                        click = Some(Click::Equip(if equipped { None } else { Some(item.id) }));
                    }
                } else {
                    let price = item.price.unwrap_or(0);
                    let can_afford = self.data.stardust >= price as i64;
                    let buy = ui
                        .add_enabled_ui(can_afford, |ui| {
                            paint::accent_button(
                                ui,
                                &constellation,
                                &format!("Buy · ✦ {price}"),
                                16.0,
                                vec2(26.0, 13.0),
                            )
                        })
                        .inner;
                    if buy
                        .on_disabled_hover_text(format!(
                            "You need ✦{} more",
                            price as i64 - self.data.stardust
                        ))
                        .clicked()
                    {
                        click = Some(Click::Buy(item.id));
                    }
                    if ui
                        .button(egui::RichText::new("Preview").font(semibold(15.5)))
                        .on_hover_text("Try it on for 10 seconds")
                        .clicked()
                    {
                        click = Some(Click::Preview(item.id));
                    }
                }
            });
        }
        click
    }

    /// One item card: a picture, the name and kind, and a button.
    fn item_card(
        &self,
        ui: &mut egui::Ui,
        item: &'static ShopItem,
        width: f32,
        owned: bool,
    ) -> Option<Click> {
        let theme = self.theme;
        let mut click = None;
        let frame = egui::Frame::new()
            .fill(theme.panel)
            .stroke(Stroke::new(1.0, theme.border))
            .corner_radius(14.0)
            .inner_margin(14.0);
        paint::panel(ui, frame, |ui| {
            let inner = width - 30.0;
            ui.set_width(inner);
            // A background's picture can be clicked to try it on.
            let previewable = item.kind == ItemKind::LibraryBackground && !owned;
            let sense = if previewable {
                Sense::click()
            } else {
                Sense::hover()
            };
            let (art, art_response) =
                ui.allocate_exact_size(vec2(inner, (inner * 0.44).max(110.0)), sense);
            self.item_art(ui, art, item);
            if previewable
                && art_response
                    .on_hover_text("Click to preview for 10 seconds")
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
            {
                click = Some(Click::Preview(item.id));
            }
            ui.add_space(2.0);

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(paint::strong(item.name, 16.5));
                    ui.label(paint::muted(kind_name(item.kind), &theme).size(13.5));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if owned {
                        match item.kind {
                            ItemKind::LibraryBackground => {
                                let equipped = self.data.equipped == Some(item.id);
                                let label = if equipped { "✓ Equipped" } else { "Equip" };
                                if ui
                                    .button(egui::RichText::new(label).color(theme.success))
                                    .clicked()
                                {
                                    click = Some(Click::Equip(if equipped {
                                        None
                                    } else {
                                        Some(item.id)
                                    }));
                                }
                            }
                            ItemKind::ProfileBanner => {
                                let in_use = self.data.profile.banner
                                    == Some(Banner::Item(item.id.to_string()));
                                let label = if in_use { "✓ In use" } else { "Use" };
                                if ui
                                    .button(egui::RichText::new(label).color(theme.success))
                                    .clicked()
                                    && !in_use
                                {
                                    click = Some(Click::UseBanner(item.id));
                                }
                            }
                            _ => {
                                let _ = ui.add_enabled(
                                    false,
                                    egui::Button::new(
                                        egui::RichText::new("✓ Owned").color(theme.success),
                                    ),
                                );
                            }
                        }
                    } else if let Some(price) = item.price {
                        let can_afford = self.data.stardust >= price as i64;
                        let text = egui::RichText::new(format!("✦ {price}"))
                            .font(semibold(15.5))
                            .color(theme.stardust);
                        if ui
                            .add_enabled(can_afford, egui::Button::new(text))
                            .on_hover_text("Buy")
                            .on_disabled_hover_text(format!(
                                "You need ✦{} more",
                                price as i64 - self.data.stardust
                            ))
                            .clicked()
                        {
                            click = Some(Click::Buy(item.id));
                        }
                    }
                    if item.kind == ItemKind::NameChange && self.data.name_credits > 0 {
                        ui.label(
                            paint::muted(format!("{} unused", self.data.name_credits), &theme)
                                .size(13.0),
                        );
                    }
                });
            });
        });
        click
    }

    /// The picture on an item card.
    fn item_art(&self, ui: &mut egui::Ui, rect: Rect, item: &ShopItem) {
        let theme = self.theme;
        let painter = ui.painter();
        let (a, b) = paint::item_colours(item.id);
        match item.kind {
            ItemKind::LibraryBackground => {
                let preview_theme = Theme::for_backdrop(Backdrop::from_item(Some(item.id)));
                let clipped = painter.with_clip_rect(rect.intersect(ui.clip_rect()));
                paint::backdrop(&clipped, rect, &preview_theme, &self.stars, self.anim_time);
                round_off(painter, rect, 10.0, theme.opaque_panel());
            }
            ItemKind::ProfileBanner => paint::gradient(painter, rect, 10.0, a, b, vec2(1.0, 0.6)),
            ItemKind::Badge | ItemKind::NameChange => {
                painter.rect_filled(rect, 10.0, theme.panel_alt);
                painter.rect_stroke(
                    rect,
                    10.0,
                    Stroke::new(1.0, theme.border),
                    egui::StrokeKind::Inside,
                );
                let centre = rect.center();
                match item.id {
                    "badge_founder" => paint::star4(painter, centre, 22.0, Color32::WHITE),
                    "badge_backlog_slayer" => {
                        painter.text(centre, egui::Align2::CENTER_CENTER, "🏆", regular(40.0), a);
                    }
                    "badge_night_owl" => {
                        painter.text(centre, egui::Align2::CENTER_CENTER, "🦉", regular(42.0), a);
                    }
                    "badge_speedrunner" => {
                        painter.text(
                            centre,
                            egui::Align2::CENTER_CENTER,
                            "⏱",
                            regular(40.0),
                            Color32::WHITE,
                        );
                    }
                    "badge_coop_captain" => {
                        painter.text(centre, egui::Align2::CENTER_CENTER, "👥", regular(40.0), a);
                    }
                    _ => {
                        painter.text(
                            centre,
                            egui::Align2::CENTER_CENTER,
                            "✏",
                            regular(40.0),
                            Color32::WHITE,
                        );
                    }
                }
            }
        }
    }
}

/// Hide the square corners of a picture that was painted clipped to a
/// plain rectangle, so it looks rounded like everything else.
fn round_off(painter: &egui::Painter, rect: Rect, radius: f32, behind: Color32) {
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(radius * 0.45, behind),
        egui::StrokeKind::Outside,
    );
}
