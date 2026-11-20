use crate::glass::{GlassPalette, GlassWindowBackdrop};
use egui::{
    Align2, Color32, Context, CornerRadius, CursorIcon, FontFamily, FontId, Frame, Id, Margin,
    Painter, PointerButton, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2, ViewportCommand,
};
use std::hash::Hash;
use std::ops::RangeInclusive;
use std::time::Instant;

pub const OPTIONS_WINDOW_TITLE: &str = "Widget Settings";
pub const OPTIONS_WINDOW_WIDTH: f32 = 468.0;
pub const OPTIONS_WINDOW_HEIGHT: f32 = 512.0;
pub const SETTINGS_TITLEBAR_HEIGHT: f32 = 46.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Appearance,
    Weather,
    System,
}

impl SettingsTab {
    pub const ALL: [Self; 3] = [Self::Appearance, Self::Weather, Self::System];

    pub fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Weather => "Weather",
            Self::System => "System",
        }
    }
}

pub struct OptionsWindowState {
    pub open: bool,
    pub revealed: bool,
    pub activated: bool,
    pub active_tab: SettingsTab,
    pub logical_size: Vec2,
    pub locked_pos_px: [i32; 2],
    pub locked_size_px: [i32; 2],
    pub locked_logical_pos: Pos2,
    pub hwnd: Option<isize>,
    pub glass_backdrop: GlassWindowBackdrop,
    pub last_observed_pos_px: Option<[i32; 2]>,
    pub pos_dirty_since: Option<Instant>,
}

impl Default for OptionsWindowState {
    fn default() -> Self {
        Self {
            open: false,
            revealed: false,
            activated: false,
            active_tab: SettingsTab::default(),
            logical_size: Vec2::new(OPTIONS_WINDOW_WIDTH, OPTIONS_WINDOW_HEIGHT),
            locked_pos_px: [0, 0],
            locked_size_px: [0, 0],
            locked_logical_pos: Pos2::ZERO,
            hwnd: None,
            glass_backdrop: GlassWindowBackdrop::new(),
            last_observed_pos_px: None,
            pos_dirty_since: None,
        }
    }
}

impl OptionsWindowState {
    pub fn reset_keeping_tab(&mut self) {
        let active_tab = self.active_tab;
        *self = Self::default();
        self.active_tab = active_tab;
    }
}

/// Draws the frameless glass titlebar with app badge, drag region, and close button.
/// Returns `true` if the close button was clicked.
pub fn draw_settings_titlebar(
    opt_ctx: &Context,
    ui: &mut Ui,
    palette: &GlassPalette,
) -> bool {
    let (title_rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), SETTINGS_TITLEBAR_HEIGHT),
        Sense::hover(),
    );

    let close_size = 28.0;
    let close_rect = Rect::from_center_size(
        Pos2::new(
            title_rect.max.x - 16.0 - close_size * 0.5,
            title_rect.center().y,
        ),
        Vec2::splat(close_size),
    );
    let drag_rect = Rect::from_min_max(
        title_rect.min,
        Pos2::new(close_rect.min.x - 6.0, title_rect.max.y),
    );

    let close_resp = ui
        .interact(close_rect, Id::new("settings_titlebar_close"), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);

    let start_drag = opt_ctx.input(|i| {
        i.pointer.button_pressed(PointerButton::Primary)
            && i.pointer
                .interact_pos()
                .map_or(false, |p| drag_rect.contains(p))
    });
    if start_drag {
        opt_ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }

    let painter = ui.painter();

    // Small vector app badge matching the tray icon motif
    let badge_rect = Rect::from_center_size(
        Pos2::new(title_rect.min.x + 16.0 + 11.0, title_rect.center().y),
        Vec2::splat(22.0),
    );
    draw_mini_app_badge(painter, badge_rect);

    painter.text(
        Pos2::new(badge_rect.max.x + 10.0, title_rect.center().y),
        Align2::LEFT_CENTER,
        OPTIONS_WINDOW_TITLE,
        FontId::new(14.5, FontFamily::Proportional),
        palette.primary_text,
    );

    let close_hovered = close_resp.hovered();
    if close_hovered {
        painter.rect_filled(close_rect, CornerRadius::same(8), palette.danger_hover_fill);
    }
    let cross_color = if close_hovered {
        palette.item_hover_text
    } else {
        palette.secondary_text
    };
    let c = close_rect.center();
    let r = 4.4;
    let cross_stroke = Stroke::new(1.5_f32, cross_color);
    painter.line_segment([Pos2::new(c.x - r, c.y - r), Pos2::new(c.x + r, c.y + r)], cross_stroke);
    painter.line_segment([Pos2::new(c.x + r, c.y - r), Pos2::new(c.x - r, c.y + r)], cross_stroke);

    painter.line_segment(
        [
            Pos2::new(title_rect.min.x, title_rect.max.y),
            Pos2::new(title_rect.max.x, title_rect.max.y),
        ],
        Stroke::new(1.0_f32, palette.divider),
    );

    close_resp.clicked()
}

fn draw_mini_app_badge(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, CornerRadius::same(6), Color32::from_rgb(36, 114, 224));
    painter.rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 24)),
        egui::StrokeKind::Inside,
    );
    let sun_center = rect.min + Vec2::new(rect.width() * 0.36, rect.height() * 0.36);
    painter.circle_filled(sun_center, 3.8, Color32::from_rgb(255, 206, 58));

    let cloud_base = Rect::from_center_size(
        rect.min + Vec2::new(rect.width() * 0.56, rect.height() * 0.66),
        Vec2::new(11.0, 5.2),
    );
    let cloud_color = Color32::from_rgb(248, 251, 255);
    painter.rect_filled(cloud_base, CornerRadius::same(3), cloud_color);
    painter.circle_filled(
        rect.min + Vec2::new(rect.width() * 0.54, rect.height() * 0.54),
        3.2,
        cloud_color,
    );
}

/// Draws a recessed pill tab switcher for `Appearance`, `Weather`, and `System`.
pub fn draw_settings_tab_bar(
    ui: &mut Ui,
    palette: &GlassPalette,
    active_tab: &mut SettingsTab,
) {
    let (track_rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::hover());

    ui.painter().rect_filled(
        track_rect,
        CornerRadius::same(10),
        palette.segment_track_fill,
    );
    ui.painter().rect_stroke(
        track_rect,
        CornerRadius::same(10),
        Stroke::new(1.0_f32, palette.divider),
        egui::StrokeKind::Inside,
    );

    let inner = track_rect.shrink(3.0);
    let gap = 3.0;
    let count = SettingsTab::ALL.len() as f32;
    let tab_w = ((inner.width() - gap * (count - 1.0)) / count).max(40.0);

    for (idx, tab) in SettingsTab::ALL.iter().enumerate() {
        let min_x = inner.min.x + idx as f32 * (tab_w + gap);
        let tab_rect = Rect::from_min_size(Pos2::new(min_x, inner.min.y), Vec2::new(tab_w, inner.height()));
        let resp = ui
            .interact(tab_rect, Id::new(("settings_tab", idx)), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);

        let is_active = *active_tab == *tab;
        let is_hovered = resp.hovered();
        let painter = ui.painter();

        if is_active {
            painter.rect_filled(tab_rect, CornerRadius::same(8), palette.segment_active_fill);
            painter.rect_stroke(
                tab_rect,
                CornerRadius::same(8),
                Stroke::new(1.0_f32, palette.card_stroke),
                egui::StrokeKind::Inside,
            );
        } else if is_hovered {
            painter.rect_filled(tab_rect, CornerRadius::same(8), palette.control_hover_fill);
        }

        let text_color = if is_active || is_hovered {
            palette.primary_text
        } else {
            palette.secondary_text
        };

        let font = FontId::new(12.5, FontFamily::Proportional);
        let galley = painter.layout_no_wrap(tab.label().to_string(), font.clone(), text_color);
        let icon_w = 13.0;
        let icon_gap = 6.0;
        let total_w = icon_w + icon_gap + galley.size().x;
        let start_x = tab_rect.center().x - total_w * 0.5;
        let icon_center = Pos2::new(start_x + icon_w * 0.5, tab_rect.center().y);

        draw_tab_icon(painter, icon_center, *tab, text_color);
        painter.galley(
            Pos2::new(
                start_x + icon_w + icon_gap,
                tab_rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            text_color,
        );

        if resp.clicked() {
            *active_tab = *tab;
        }
    }
}

fn draw_tab_icon(painter: &Painter, center: Pos2, tab: SettingsTab, color: Color32) {
    let stroke = Stroke::new(1.25_f32, color);
    match tab {
        SettingsTab::Appearance => {
            // Overlapping glass panes glyph
            let back = Rect::from_center_size(center + Vec2::new(2.0, -1.5), Vec2::splat(7.5));
            let front = Rect::from_center_size(center + Vec2::new(-2.0, 1.5), Vec2::splat(7.5));
            painter.rect_stroke(back, CornerRadius::same(2), stroke, egui::StrokeKind::Middle);
            painter.rect_filled(front, CornerRadius::same(2), color.gamma_multiply(0.28));
            painter.rect_stroke(front, CornerRadius::same(2), stroke, egui::StrokeKind::Middle);
        }
        SettingsTab::Weather => {
            // Sun circle + cloud curve glyph
            painter.circle_stroke(center + Vec2::new(-2.2, -1.8), 2.8, stroke);
            let cloud = Rect::from_center_size(center + Vec2::new(1.6, 2.2), Vec2::new(8.2, 4.4));
            painter.rect_filled(cloud, CornerRadius::same(2), color.gamma_multiply(0.28));
            painter.rect_stroke(cloud, CornerRadius::same(2), stroke, egui::StrokeKind::Middle);
        }
        SettingsTab::System => {
            // Two horizontal slider tracks glyph
            let y1 = center.y - 2.6;
            let y2 = center.y + 2.6;
            painter.line_segment([Pos2::new(center.x - 5.2, y1), Pos2::new(center.x + 5.2, y1)], stroke);
            painter.line_segment([Pos2::new(center.x - 5.2, y2), Pos2::new(center.x + 5.2, y2)], stroke);
            painter.circle_filled(Pos2::new(center.x + 2.0, y1), 1.9, color);
            painter.circle_filled(Pos2::new(center.x - 2.0, y2), 1.9, color);
        }
    }
}

/// Renders a translucent glass section card with consistent inner padding and subtle hairline border.
pub fn glass_card(
    ui: &mut Ui,
    palette: &GlassPalette,
    title: &str,
    add_contents: impl FnOnce(&mut Ui),
) {
    Frame::NONE
        .fill(palette.card_fill)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(14, 12))
        .stroke(Stroke::new(1.0_f32, palette.card_stroke))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(title)
                    .size(12.5)
                    .strong()
                    .color(palette.secondary_text),
            );
            ui.add_space(6.0);
            add_contents(ui);
        });
}

/// Renders a full-width setting row with label, optional subtitle, and an animated pill toggle switch.
pub fn glass_toggle_row(
    ui: &mut Ui,
    id_salt: impl Hash,
    palette: &GlassPalette,
    title: &str,
    subtitle: Option<&str>,
    value: &mut bool,
) -> bool {
    let row_h = if subtitle.is_some() { 38.0 } else { 28.0 };
    let (row_rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);

    let mut changed = false;
    if resp.clicked() {
        *value = !*value;
        changed = true;
    }

    let id = ui.make_persistent_id(id_salt);
    let anim = ui.ctx().animate_bool(id, *value);

    let switch_size = Vec2::new(36.0, 20.0);
    let switch_rect = Rect::from_center_size(
        Pos2::new(row_rect.max.x - switch_size.x * 0.5, row_rect.center().y),
        switch_size,
    );
    let text_max_x = switch_rect.min.x - 10.0;

    let painter = ui.painter();
    if let Some(sub) = subtitle {
        painter.text(
            Pos2::new(row_rect.min.x, row_rect.center().y - 7.5),
            Align2::LEFT_CENTER,
            title,
            FontId::new(13.0, FontFamily::Proportional),
            palette.primary_text,
        );
        let sub_galley = painter.layout(
            sub.to_string(),
            FontId::new(11.0, FontFamily::Proportional),
            palette.muted_text,
            (text_max_x - row_rect.min.x).max(60.0),
        );
        painter.galley(
            Pos2::new(row_rect.min.x, row_rect.center().y + 2.5),
            sub_galley,
            palette.muted_text,
        );
    } else {
        painter.text(
            Pos2::new(row_rect.min.x, row_rect.center().y),
            Align2::LEFT_CENTER,
            title,
            FontId::new(13.0, FontFamily::Proportional),
            palette.primary_text,
        );
    }

    let off = palette.toggle_off_fill;
    let on = if resp.hovered() {
        palette.accent_hover_fill
    } else {
        palette.accent_fill
    };
    let track_color = lerp_color32(off, on, anim);

    painter.rect_filled(switch_rect, CornerRadius::same(10), track_color);
    if !*value && resp.hovered() {
        painter.rect_stroke(
            switch_rect,
            CornerRadius::same(10),
            Stroke::new(1.0_f32, palette.control_stroke),
            egui::StrokeKind::Inside,
        );
    }

    let knob_x = egui::lerp((switch_rect.min.x + 10.0)..=(switch_rect.max.x - 10.0), anim);
    let knob_center = Pos2::new(knob_x, switch_rect.center().y);
    painter.circle_filled(
        knob_center + Vec2::new(0.0, 1.0),
        7.0,
        Color32::from_black_alpha(38),
    );
    painter.circle_filled(knob_center, 7.0, palette.toggle_knob_fill);

    changed
}

/// Renders a segmented pill control inside a recessed glass track.
/// Passing `current: None` leaves all options unselected while still allowing clicks.
pub fn glass_segmented_control<T: Copy + PartialEq>(
    ui: &mut Ui,
    id_salt: impl Hash,
    palette: &GlassPalette,
    options: &[(T, &str)],
    current: Option<T>,
) -> Option<T> {
    if options.is_empty() {
        return None;
    }

    let (track_rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::hover());

    let painter = ui.painter();
    painter.rect_filled(track_rect, CornerRadius::same(8), palette.segment_track_fill);
    painter.rect_stroke(
        track_rect,
        CornerRadius::same(8),
        Stroke::new(1.0_f32, palette.divider),
        egui::StrokeKind::Inside,
    );

    let inner = track_rect.shrink(2.5);
    let gap = 2.5;
    let count = options.len() as f32;
    let seg_w = ((inner.width() - gap * (count - 1.0)) / count).max(24.0);

    let mut picked = None;
    let base_id = ui.make_persistent_id(id_salt);
    for (idx, (val, label)) in options.iter().enumerate() {
        let min_x = inner.min.x + idx as f32 * (seg_w + gap);
        let seg_rect =
            Rect::from_min_size(Pos2::new(min_x, inner.min.y), Vec2::new(seg_w, inner.height()));
        let resp = ui
            .interact(seg_rect, base_id.with(idx), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);

        let is_selected = current == Some(*val);
        let is_hovered = resp.hovered();

        if is_selected {
            ui.painter()
                .rect_filled(seg_rect, CornerRadius::same(6), palette.accent_fill);
        } else if is_hovered {
            ui.painter()
                .rect_filled(seg_rect, CornerRadius::same(6), palette.control_hover_fill);
        }

        let text_color = if is_selected {
            palette.accent_text_on_fill
        } else if is_hovered {
            palette.primary_text
        } else {
            palette.secondary_text
        };

        ui.painter().text(
            seg_rect.center(),
            Align2::CENTER_CENTER,
            *label,
            FontId::new(12.0, FontFamily::Proportional),
            text_color,
        );

        if resp.clicked() && current != Some(*val) {
            picked = Some(*val);
        }
    }
    picked
}

/// Renders a stacked slider row with label on the left, a compact value pill on the right,
/// and a full-width slider bar underneath.
pub fn glass_slider_row(
    ui: &mut Ui,
    palette: &GlassPalette,
    label: &str,
    value: &mut f32,
    range: RangeInclusive<f32>,
    step: f64,
    suffix: &str,
) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(label)
                    .size(12.5)
                    .color(palette.primary_text),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                Frame::NONE
                    .fill(palette.control_fill)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(7, 2))
                    .stroke(Stroke::new(1.0_f32, palette.card_stroke))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("{:.0}{suffix}", *value))
                                .size(11.5)
                                .color(palette.secondary_text),
                        );
                    });
            });
        });

        ui.scope(|ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 6.0).max(120.0);
            if ui
                .add(
                    egui::Slider::new(value, range)
                        .step_by(step)
                        .show_value(false),
                )
                .changed()
            {
                changed = true;
            }
        });
    });
    changed
}

pub fn glass_button(ui: &mut Ui, palette: &GlassPalette, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).size(12.0).color(palette.primary_text))
            .fill(palette.control_fill)
            .stroke(Stroke::new(1.0_f32, palette.control_stroke))
            .corner_radius(CornerRadius::same(8)),
    )
    .on_hover_cursor(CursorIcon::PointingHand)
}

pub fn glass_primary_button(ui: &mut Ui, palette: &GlassPalette, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(label)
                .size(12.0)
                .strong()
                .color(palette.accent_text_on_fill),
        )
        .fill(palette.accent_fill)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(8)),
    )
    .on_hover_cursor(CursorIcon::PointingHand)
}

pub fn glass_recording_button(ui: &mut Ui, palette: &GlassPalette, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(label)
                .size(12.0)
                .strong()
                .color(palette.item_hover_text),
        )
        .fill(palette.danger_hover_fill)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(8)),
    )
    .on_hover_cursor(CursorIcon::PointingHand)
}

/// Renders a translucent status pill with a 6px status indicator dot.
pub fn draw_status_pill(
    ui: &mut Ui,
    palette: &GlassPalette,
    dot_color: Color32,
    text: &str,
) {
    Frame::NONE
        .fill(palette.control_fill)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(10, 6))
        .stroke(Stroke::new(1.0_f32, palette.card_stroke))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                ui.painter()
                    .circle_filled(dot_rect.center(), 3.2, dot_color);
                ui.label(
                    RichText::new(text)
                        .size(11.5)
                        .color(palette.primary_text),
                );
            });
        });
}

/// Renders a full-width hoverable search result row showing city name and coordinates.
pub fn glass_location_result_row(
    ui: &mut Ui,
    id_salt: impl Hash,
    palette: &GlassPalette,
    display_name: &str,
    lat: f64,
    lon: f64,
) -> bool {
    let (row_rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    let hovered = resp.hovered();

    let painter = ui.painter();
    painter.rect_filled(
        row_rect,
        CornerRadius::same(8),
        if hovered {
            palette.control_hover_fill
        } else {
            palette.control_fill
        },
    );
    painter.rect_stroke(
        row_rect,
        CornerRadius::same(8),
        Stroke::new(
            1.0_f32,
            if hovered {
                palette.accent_fill
            } else {
                palette.card_stroke
            },
        ),
        egui::StrokeKind::Inside,
    );

    let coords = format!("{lat:.2}, {lon:.2}");
    let coords_galley = painter.layout_no_wrap(
        coords,
        FontId::new(11.0, FontFamily::Proportional),
        palette.muted_text,
    );
    let max_name_w = (row_rect.width() - coords_galley.size().x - 28.0).max(60.0);
    let name_galley = painter.layout(
        display_name.to_string(),
        FontId::new(12.0, FontFamily::Proportional),
        palette.primary_text,
        max_name_w,
    );

    painter.galley(
        Pos2::new(
            row_rect.min.x + 10.0,
            row_rect.center().y - name_galley.size().y * 0.5,
        ),
        name_galley,
        palette.primary_text,
    );
    painter.galley(
        Pos2::new(
            row_rect.max.x - 10.0 - coords_galley.size().x,
            row_rect.center().y - coords_galley.size().y * 0.5,
        ),
        coords_galley,
        palette.muted_text,
    );

    let _ = id_salt;
    resp.clicked()
}

fn lerp_color32(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let lerp_u8 = |c0: u8, c1: u8| -> u8 {
        ((c0 as f32) + ((c1 as f32) - (c0 as f32)) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color32::from_rgba_unmultiplied(
        lerp_u8(a.r(), b.r()),
        lerp_u8(a.g(), b.g()),
        lerp_u8(a.b(), b.b()),
        lerp_u8(a.a(), b.a()),
    )
}
