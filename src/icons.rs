use crate::weather::WeatherConditionKind;
use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};

/// Draws an iOS-inspired vector weather condition icon inside `rect`.
pub fn draw_weather_icon(
    painter: &Painter,
    rect: Rect,
    kind: WeatherConditionKind,
    draw_shadow: bool,
) {
    let center = rect.center();
    let scale = (rect.width().min(rect.height())) / 40.0;

    if draw_shadow {
        let shadow_offset = Vec2::new(0.0, 2.0 * scale);
        painter.circle_filled(
            center + shadow_offset,
            15.0 * scale,
            Color32::from_black_alpha(65),
        );
    }

    match kind {
        WeatherConditionKind::ClearDay => {
            draw_sun(painter, center, 11.5 * scale, scale, true);
        }
        WeatherConditionKind::ClearNight => {
            draw_moon_and_stars(painter, center, 11.0 * scale, scale);
        }
        WeatherConditionKind::PartlyCloudyDay => {
            draw_sun(
                painter,
                center + Vec2::new(-5.5 * scale, -5.0 * scale),
                8.2 * scale,
                scale,
                true,
            );
            draw_cloud(
                painter,
                center + Vec2::new(2.5 * scale, 4.0 * scale),
                scale * 0.92,
                Color32::from_rgb(246, 250, 255),
                Color32::from_rgb(214, 226, 244),
            );
        }
        WeatherConditionKind::PartlyCloudyNight => {
            draw_moon_and_stars(
                painter,
                center + Vec2::new(-6.0 * scale, -5.5 * scale),
                7.8 * scale,
                scale * 0.85,
            );
            draw_cloud(
                painter,
                center + Vec2::new(2.5 * scale, 4.0 * scale),
                scale * 0.92,
                Color32::from_rgb(230, 238, 252),
                Color32::from_rgb(192, 206, 230),
            );
        }
        WeatherConditionKind::Cloudy => {
            draw_cloud(
                painter,
                center + Vec2::new(-4.0 * scale, -3.5 * scale),
                scale * 0.78,
                Color32::from_rgb(195, 208, 228),
                Color32::from_rgb(168, 184, 208),
            );
            draw_cloud(
                painter,
                center + Vec2::new(2.0 * scale, 2.5 * scale),
                scale * 0.95,
                Color32::from_rgb(245, 249, 255),
                Color32::from_rgb(212, 224, 240),
            );
        }
        WeatherConditionKind::Fog => {
            draw_cloud(
                painter,
                center + Vec2::new(0.0, -3.5 * scale),
                scale * 0.85,
                Color32::from_rgb(225, 233, 245),
                Color32::from_rgb(195, 206, 222),
            );
            let bar_color = Color32::from_rgba_unmultiplied(210, 224, 242, 220);
            for (idx, width_factor) in [0.68_f32, 0.54, 0.40].iter().enumerate() {
                let y = center.y + (6.5 + idx as f32 * 4.2) * scale;
                let half_w = 15.0 * width_factor * scale;
                painter.line_segment(
                    [Pos2::new(center.x - half_w, y), Pos2::new(center.x + half_w, y)],
                    Stroke::new(2.3 * scale, bar_color),
                );
            }
        }
        WeatherConditionKind::Drizzle => {
            draw_cloud(
                painter,
                center + Vec2::new(0.0, -3.5 * scale),
                scale * 0.90,
                Color32::from_rgb(232, 240, 252),
                Color32::from_rgb(196, 212, 232),
            );
            draw_rain_drops(painter, center + Vec2::new(0.0, 8.5 * scale), scale, 2);
        }
        WeatherConditionKind::Rain => {
            draw_cloud(
                painter,
                center + Vec2::new(0.0, -4.0 * scale),
                scale * 0.92,
                Color32::from_rgb(216, 228, 245),
                Color32::from_rgb(176, 194, 218),
            );
            draw_rain_drops(painter, center + Vec2::new(0.0, 8.5 * scale), scale, 3);
        }
        WeatherConditionKind::Thunderstorm => {
            draw_cloud(
                painter,
                center + Vec2::new(0.0, -4.5 * scale),
                scale * 0.92,
                Color32::from_rgb(176, 190, 212),
                Color32::from_rgb(138, 152, 178),
            );
            // Lightning bolt
            let p1 = center + Vec2::new(1.5 * scale, 1.5 * scale);
            let p2 = center + Vec2::new(-3.2 * scale, 8.2 * scale);
            let p3 = center + Vec2::new(1.2 * scale, 8.2 * scale);
            let p4 = center + Vec2::new(-2.2 * scale, 15.2 * scale);
            let bolt_stroke = Stroke::new(2.4 * scale, Color32::from_rgb(255, 212, 56));
            painter.line_segment([p1, p2], bolt_stroke);
            painter.line_segment([p2, p3], bolt_stroke);
            painter.line_segment([p3, p4], bolt_stroke);
        }
        WeatherConditionKind::Snow => {
            draw_cloud(
                painter,
                center + Vec2::new(0.0, -4.0 * scale),
                scale * 0.90,
                Color32::from_rgb(240, 246, 255),
                Color32::from_rgb(206, 220, 240),
            );
            let snow_color = Color32::from_rgb(175, 228, 255);
            for dx in [-6.5_f32, 0.0, 6.5] {
                let dy = if dx == 0.0 { 11.2 } else { 8.5 };
                let pt = center + Vec2::new(dx * scale, dy * scale);
                painter.circle_filled(pt, 2.1 * scale, snow_color);
            }
        }
    }
}

fn draw_sun(painter: &Painter, center: Pos2, radius: f32, scale: f32, with_rays: bool) {
    // Soft outer glow
    painter.circle_filled(
        center,
        radius * 1.28,
        Color32::from_rgba_unmultiplied(255, 198, 42, 52),
    );

    if with_rays {
        let ray_stroke = Stroke::new(2.0 * scale, Color32::from_rgb(255, 206, 56));
        for i in 0..8 {
            let angle = (i as f32) * std::f32::consts::TAU / 8.0;
            let dir = Vec2::angled(angle);
            let p_in = center + dir * (radius + 2.2 * scale);
            let p_out = center + dir * (radius + 5.2 * scale);
            painter.line_segment([p_in, p_out], ray_stroke);
        }
    }

    // Core sun body
    painter.circle_filled(center, radius, Color32::from_rgb(255, 186, 36));
    painter.circle_filled(
        center + Vec2::new(-radius * 0.18, -radius * 0.18),
        radius * 0.78,
        Color32::from_rgb(255, 218, 76),
    );
}

fn draw_moon_and_stars(painter: &Painter, center: Pos2, radius: f32, scale: f32) {
    painter.circle_filled(
        center,
        radius * 1.25,
        Color32::from_rgba_unmultiplied(168, 196, 255, 40),
    );
    painter.circle_filled(center, radius, Color32::from_rgb(245, 224, 138));
    // Crescent cutout hint with soft indigo overlay
    painter.circle_filled(
        center + Vec2::new(radius * 0.42, -radius * 0.28),
        radius * 0.76,
        Color32::from_rgb(42, 54, 88),
    );
    // Twinkling stars
    let star_color = Color32::from_rgb(255, 244, 192);
    painter.circle_filled(
        center + Vec2::new(radius * 0.95, -radius * 0.75),
        1.5 * scale,
        star_color,
    );
    painter.circle_filled(
        center + Vec2::new(-radius * 0.95, -radius * 0.65),
        1.2 * scale,
        star_color,
    );
}

fn draw_cloud(
    painter: &Painter,
    center: Pos2,
    scale: f32,
    top_color: Color32,
    bottom_color: Color32,
) {
    // Base pill
    let base_rect = Rect::from_center_size(
        center + Vec2::new(0.0, 2.8 * scale),
        Vec2::new(24.0 * scale, 10.5 * scale),
    );
    painter.rect_filled(base_rect, 5.2 * scale, bottom_color);

    // Puffs
    painter.circle_filled(
        center + Vec2::new(-6.2 * scale, 0.8 * scale),
        6.2 * scale,
        bottom_color,
    );
    painter.circle_filled(
        center + Vec2::new(5.8 * scale, 1.2 * scale),
        5.6 * scale,
        bottom_color,
    );
    painter.circle_filled(
        center + Vec2::new(-0.8 * scale, -2.2 * scale),
        7.8 * scale,
        top_color,
    );
    painter.circle_filled(
        center + Vec2::new(-5.8 * scale, 0.2 * scale),
        5.5 * scale,
        top_color,
    );
    painter.circle_filled(
        center + Vec2::new(5.2 * scale, 0.6 * scale),
        5.0 * scale,
        top_color,
    );
}

fn draw_rain_drops(painter: &Painter, center: Pos2, scale: f32, count: usize) {
    let drop_stroke = Stroke::new(2.2 * scale, Color32::from_rgb(76, 186, 255));
    let offsets: &[f32] = if count == 2 {
        &[-4.0, 4.0]
    } else {
        &[-6.5, 0.0, 6.5]
    };
    for &dx in offsets {
        let top = center + Vec2::new(dx * scale, -2.2 * scale);
        let bottom = center + Vec2::new((dx - 2.0) * scale, 4.5 * scale);
        painter.line_segment([top, bottom], drop_stroke);
    }
}
