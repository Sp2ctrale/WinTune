//! Navigation sidebar and page header, after the Windows 11 Settings app.
//! They return values (the selection changed) and never act.

use egui::{
    pos2, vec2, Align, Align2, CursorIcon, FontFamily, FontId, Frame, Layout, Margin, Rect,
    RichText, Rounding, Sense, Stroke, Ui, WidgetInfo, WidgetType,
};

use super::fonts::semibold;
use super::fonts::ICONS;
use super::icons::{glyph, icon_text, icons_available};
use super::surfaces::split_row;
use super::{current_palette, radius, space, text_size, Icon};

const NAV_WIDTH: f32 = 260.0;
const ITEM_HEIGHT: f32 = 40.0;

pub struct NavItem<'a, K> {
    pub key: K,
    pub icon: Icon,
    pub label: &'a str,
    pub badge: Option<usize>,
    pub enabled: bool,
    /// Shown on hover while the item is disabled; `None` shows nothing.
    pub disabled_hint: Option<&'a str>,
}

pub struct NavGroup<'a, K> {
    pub title: Option<&'a str>,
    pub items: Vec<NavItem<'a, K>>,
}

/// One row: subtle pill when selected with an accent bar on its left edge.
fn nav_item<K: PartialEq + Copy>(ui: &mut Ui, selected: &mut K, item: &NavItem<'_, K>) -> bool {
    let p = current_palette(ui.ctx());
    let is_selected = *selected == item.key;
    let sense = if item.enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ITEM_HEIGHT), sense);
    let response = if item.enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else if let Some(hint) = item.disabled_hint {
        response.on_hover_text(hint)
    } else {
        response
    };
    response.widget_info(|| {
        WidgetInfo::selected(
            WidgetType::SelectableLabel,
            item.enabled,
            is_selected,
            item.label,
        )
    });

    let mut changed = false;
    if item.enabled && response.clicked() && !is_selected {
        *selected = item.key;
        changed = true;
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let text_color = if item.enabled {
            p.text
        } else {
            p.text_disabled
        };
        if is_selected {
            painter.rect_filled(rect, Rounding::same(radius::CONTROL), p.surface_hover);
            let bar =
                Rect::from_center_size(pos2(rect.left() + 2.0, rect.center().y), vec2(3.0, 16.0));
            painter.rect_filled(bar, Rounding::same(1.5), p.accent);
        } else if item.enabled && response.hovered() {
            painter.rect_filled(
                rect,
                Rounding::same(radius::CONTROL),
                p.surface_hover.gamma_multiply(0.6),
            );
        }
        let mut text_x = rect.left() + space::M;
        if icons_available(ui.ctx()) {
            let galley = painter.layout_no_wrap(
                glyph(item.icon).to_string(),
                FontId::new(18.0, FontFamily::Name(ICONS.into())),
                text_color,
            );
            painter.galley(
                pos2(text_x, rect.center().y - galley.size().y / 2.0),
                galley,
                text_color,
            );
        }
        text_x += 18.0 + space::M;
        painter.text(
            pos2(text_x, rect.center().y),
            Align2::LEFT_CENTER,
            item.label,
            FontId::proportional(text_size::BODY),
            text_color,
        );
        if let Some(n) = item.badge.filter(|n| *n > 0) {
            let galley = painter.layout_no_wrap(
                n.to_string(),
                FontId::proportional(text_size::SECONDARY),
                p.on_accent,
            );
            let size = vec2(galley.size().x.max(8.0) + 2.0 * space::S, 18.0);
            let badge = Rect::from_center_size(
                pos2(rect.right() - space::M - size.x / 2.0, rect.center().y),
                size,
            );
            painter.rect_filled(badge, Rounding::same(9.0), p.accent);
            painter.galley(badge.center() - galley.size() / 2.0, galley, p.on_accent);
        }
    }
    changed
}

/// App identity drawn at the top of the sidebar.
pub struct Brand<'a> {
    pub logo: Option<egui::TextureId>,
    pub name: &'a str,
    pub caption: &'a str,
}

fn brand_header(ui: &mut Ui, brand: &Brand<'_>) {
    let p = current_palette(ui.ctx());
    ui.horizontal(|ui| {
        ui.add_space(space::S);
        if let Some(logo) = brand.logo {
            ui.add(egui::Image::new(egui::load::SizedTexture::new(
                logo,
                vec2(28.0, 28.0),
            )));
        }
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(
                RichText::new(brand.name)
                    .font(semibold(ui.ctx(), text_size::BODY))
                    .color(p.text),
            );
            ui.label(
                RichText::new(brand.caption)
                    .size(text_size::SECONDARY)
                    .color(p.text_secondary),
            );
        });
    });
    ui.add_space(space::M);
}

/// Left navigation. Returns true when the selection changed.
pub fn nav_sidebar<K: PartialEq + Copy>(
    ctx: &egui::Context,
    brand: &Brand<'_>,
    selected: &mut K,
    groups: &[NavGroup<'_, K>],
    footer: &[NavItem<'_, K>],
) -> bool {
    let p = current_palette(ctx);
    let mut changed = false;
    egui::SidePanel::left("wintune_nav")
        .exact_width(NAV_WIDTH)
        .resizable(false)
        .frame(
            Frame::none()
                .fill(p.page)
                .stroke(Stroke::new(1.0_f32, p.border))
                .inner_margin(Margin::symmetric(space::S, space::M)),
        )
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                for item in footer.iter().rev() {
                    changed |= nav_item(ui, selected, item);
                }
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    brand_header(ui, brand);
                    for (i, group) in groups.iter().enumerate() {
                        if i > 0 {
                            ui.add_space(space::S);
                        }
                        if let Some(title) = group.title {
                            ui.add_space(space::XS);
                            ui.label(
                                RichText::new(title)
                                    .size(text_size::SECONDARY)
                                    .color(p.text_secondary),
                            );
                            ui.add_space(space::XS);
                        }
                        for item in &group.items {
                            changed |= nav_item(ui, selected, item);
                        }
                    }
                });
            });
        });
    changed
}

/// Breadcrumb, large title, optional subtitle and right-aligned actions.
pub fn page_header(
    ui: &mut Ui,
    breadcrumb: &[&str],
    title: &str,
    subtitle: Option<&str>,
    actions: impl FnOnce(&mut Ui),
) {
    let p = current_palette(ui.ctx());
    if !breadcrumb.is_empty() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = space::XS;
            for (i, part) in breadcrumb.iter().enumerate() {
                if i > 0 {
                    match icon_text(ui.ctx(), Icon::ChevronRight, 10.0) {
                        Some(chevron) => ui.label(chevron.color(p.text_secondary)),
                        None => ui.label(
                            RichText::new(">")
                                .size(text_size::SECONDARY)
                                .color(p.text_secondary),
                        ),
                    };
                }
                ui.label(
                    RichText::new(*part)
                        .size(text_size::SECONDARY)
                        .color(p.text_secondary),
                );
            }
        });
        ui.add_space(space::XS);
    }
    split_row(ui, 40.0, actions, |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = space::XS;
            ui.label(
                RichText::new(title)
                    .font(semibold(ui.ctx(), text_size::TITLE))
                    .color(p.text),
            );
            if let Some(sub) = subtitle {
                ui.label(
                    RichText::new(sub)
                        .size(text_size::BODY)
                        .color(p.text_secondary),
                );
            }
        });
    });
    ui.add_space(space::L);
}
