use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Stroke, Vec2};

pub const OPTIONS_WINDOW_TITLE: &str = "Widget Settings";
pub const OPTIONS_WINDOW_WIDTH: f32 = 468.0;
pub const OPTIONS_WINDOW_HEIGHT: f32 = 512.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Appearance,
    Weather,
    System,
}

impl SettingsTab {
    pub const TABS: [(Self, &'static str); 3] = [
        (Self::Appearance, "Appearance"),
        (Self::Weather, "Weather"),
        (Self::System, "System"),
    ];
}

pub fn draw_mini_app_badge(painter: &Painter, rect: Rect) {
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

pub fn draw_settings_tab_icon(painter: &Painter, center: Pos2, tab: SettingsTab, color: Color32) {
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
            painter.line_segment(
                [Pos2::new(center.x - 5.2, y1), Pos2::new(center.x + 5.2, y1)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(center.x - 5.2, y2), Pos2::new(center.x + 5.2, y2)],
                stroke,
            );
            painter.circle_filled(Pos2::new(center.x + 2.0, y1), 1.9, color);
            painter.circle_filled(Pos2::new(center.x - 2.0, y2), 1.9, color);
        }
    }
}
