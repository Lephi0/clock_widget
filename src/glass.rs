use crate::config::{BackgroundMode, GlassTintPreset, WidgetConfig};
use crate::dwm::sync_window_blur;
use egui::{Color32, CornerRadius, Painter, Rect};

/// Matches Windows 11 `DWMWCP_ROUND` window corner curvature so the egui glass fill aligns cleanly with DWM Acrylic.
pub const DEFAULT_GLASS_CORNER_RADIUS: f32 = 9.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlassStyle {
    pub tint_preset: GlassTintPreset,
    pub opacity: f32,
    pub windows_is_light: bool,
    pub corner_radius: f32,
}

impl GlassStyle {
    pub fn from_config(config: &WidgetConfig, windows_is_light: bool) -> Self {
        Self {
            tint_preset: config.glass_tint_preset,
            opacity: config.glass_tint_opacity.clamp(0.05, 0.98),
            windows_is_light,
            corner_radius: DEFAULT_GLASS_CORNER_RADIUS,
        }
    }

    #[allow(dead_code)]
    pub fn with_corner_radius(mut self, corner_radius: f32) -> Self {
        self.corner_radius = corner_radius;
        self
    }

    #[allow(dead_code)]
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.05, 0.98);
        self
    }

    pub fn is_light(&self) -> bool {
        self.tint_preset.is_light_theme(self.windows_is_light)
    }

    pub fn tint_rgb(&self) -> [u8; 3] {
        self.tint_preset.base_rgb(self.windows_is_light)
    }

    #[allow(dead_code)]
    pub fn fill_color(&self) -> Color32 {
        self.fill_color_with_alpha(1.0)
    }

    pub fn fill_color_with_alpha(&self, alpha_scale: f32) -> Color32 {
        let [r, g, b] = self.tint_rgb();
        let base_alpha = self.opacity.clamp(0.05, 0.98);
        let alpha = (base_alpha * alpha_scale.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color32::from_rgba_unmultiplied(r, g, b, alpha)
    }

    pub fn corner_radius_egui(&self) -> CornerRadius {
        CornerRadius::same(self.corner_radius.round().clamp(0.0, 255.0) as u8)
    }

    pub fn palette(&self) -> GlassPalette {
        GlassPalette::for_theme(self.is_light())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlassPalette {
    pub is_light: bool,
    pub primary_text: Color32,
    pub secondary_text: Color32,
    pub accent_text: Color32,
    pub divider: Color32,
    pub item_hover_fill: Color32,
    pub item_hover_text: Color32,
    pub item_hover_secondary_text: Color32,
    pub danger_text: Color32,
    pub danger_hover_fill: Color32,
}

impl GlassPalette {
    pub fn for_theme(is_light: bool) -> Self {
        if is_light {
            Self {
                is_light: true,
                primary_text: Color32::from_rgb(20, 24, 34),
                secondary_text: Color32::from_rgb(68, 78, 98),
                accent_text: Color32::from_rgb(218, 58, 48),
                divider: Color32::from_rgba_unmultiplied(20, 30, 50, 26),
                item_hover_fill: Color32::from_rgba_unmultiplied(42, 112, 232, 210),
                item_hover_text: Color32::from_rgb(250, 252, 255),
                item_hover_secondary_text: Color32::from_rgba_unmultiplied(235, 242, 255, 225),
                danger_text: Color32::from_rgb(208, 42, 42),
                danger_hover_fill: Color32::from_rgba_unmultiplied(220, 52, 52, 215),
            }
        } else {
            Self {
                is_light: false,
                primary_text: Color32::from_rgb(250, 252, 255),
                secondary_text: Color32::from_rgba_unmultiplied(228, 236, 250, 215),
                accent_text: Color32::from_rgb(255, 108, 96),
                divider: Color32::from_rgba_unmultiplied(255, 255, 255, 26),
                item_hover_fill: Color32::from_rgba_unmultiplied(64, 132, 244, 185),
                item_hover_text: Color32::from_rgb(250, 252, 255),
                item_hover_secondary_text: Color32::from_rgba_unmultiplied(235, 242, 255, 225),
                danger_text: Color32::from_rgb(255, 108, 108),
                danger_hover_fill: Color32::from_rgba_unmultiplied(228, 62, 62, 195),
            }
        }
    }
}

/// Draws a translucent tinted glass fill layered over a Windows DWM Acrylic blur backdrop,
/// scaled by `alpha_scale` (`0.0..=1.0`).
pub fn draw_glass_surface_with_alpha(
    painter: &Painter,
    rect: Rect,
    style: &GlassStyle,
    alpha_scale: f32,
) {
    painter.rect_filled(
        rect,
        style.corner_radius_egui(),
        style.fill_color_with_alpha(alpha_scale),
    );
}

/// Draws a translucent tinted glass fill layered over a Windows DWM Acrylic blur backdrop.
pub fn draw_glass_surface(painter: &Painter, rect: Rect, style: &GlassStyle) {
    draw_glass_surface_with_alpha(painter, rect, style, 1.0);
}

/// Draws the glass surface when `mode` is `FrostedGlass`, skipping completely in `Transparent` mode.
pub fn draw_glass_background(
    painter: &Painter,
    rect: Rect,
    mode: BackgroundMode,
    style: &GlassStyle,
) {
    if mode == BackgroundMode::Transparent {
        return;
    }
    draw_glass_surface(painter, rect, style);
}

/// Tracks and deduplicates Windows DWM Acrylic backdrop synchronization for an individual native window.
#[derive(Debug, Default, Clone, Copy)]
pub struct GlassWindowBackdrop {
    last_hwnd: Option<isize>,
    last_mode: Option<BackgroundMode>,
    last_is_light: Option<bool>,
}

impl GlassWindowBackdrop {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn invalidate(&mut self) {
        self.last_mode = None;
        self.last_is_light = None;
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.last_hwnd = None;
        self.last_mode = None;
        self.last_is_light = None;
    }

    pub fn sync(&mut self, hwnd: Option<isize>, mode: BackgroundMode, is_light: bool) {
        let Some(raw) = hwnd else {
            return;
        };
        let style_dirty = crate::dwm::needs_caption_suppression(Some(raw));
        if style_dirty
            || self.last_hwnd != Some(raw)
            || self.last_mode != Some(mode)
            || self.last_is_light != Some(is_light)
        {
            sync_window_blur(Some(raw), mode, is_light);
            self.last_hwnd = Some(raw);
            self.last_mode = Some(mode);
            self.last_is_light = Some(is_light);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glass_style_opacity_clamping_and_fill() {
        let mut cfg = WidgetConfig::default();
        cfg.glass_tint_preset = GlassTintPreset::Dark;
        cfg.glass_tint_opacity = 0.0;

        let low = GlassStyle::from_config(&cfg, false);
        assert_eq!(low.opacity, 0.05);
        assert_eq!(low.fill_color(), Color32::from_rgba_unmultiplied(20, 22, 28, 13));

        let high = low.with_opacity(2.5);
        assert_eq!(high.opacity, 0.98);
        assert_eq!(
            high.fill_color(),
            Color32::from_rgba_unmultiplied(20, 22, 28, 250)
        );
    }

    #[test]
    fn test_glass_style_theme_resolution() {
        let mut cfg = WidgetConfig::default();
        cfg.glass_tint_opacity = 0.5;

        cfg.glass_tint_preset = GlassTintPreset::Dark;
        assert!(!GlassStyle::from_config(&cfg, true).is_light());

        cfg.glass_tint_preset = GlassTintPreset::Light;
        assert!(GlassStyle::from_config(&cfg, false).is_light());

        cfg.glass_tint_preset = GlassTintPreset::System;
        assert!(!GlassStyle::from_config(&cfg, false).is_light());
        let sys_light = GlassStyle::from_config(&cfg, true);
        assert!(sys_light.is_light());
        assert_eq!(
            sys_light.fill_color(),
            Color32::from_rgba_unmultiplied(240, 244, 250, 128)
        );
    }

    #[test]
    fn test_glass_palette_light_vs_dark() {
        let light = GlassPalette::for_theme(true);
        let dark = GlassPalette::for_theme(false);

        assert!(light.is_light);
        assert!(!dark.is_light);
        assert_ne!(light.primary_text, dark.primary_text);
        assert_ne!(light.secondary_text, dark.secondary_text);
        assert_ne!(light.danger_text, dark.danger_text);
        assert_ne!(light.divider, dark.divider);
    }

    #[test]
    fn test_glass_style_fill_color_with_alpha() {
        let mut cfg = WidgetConfig::default();
        cfg.glass_tint_preset = GlassTintPreset::Dark;
        cfg.glass_tint_opacity = 0.5;
        let style = GlassStyle::from_config(&cfg, false);

        assert_eq!(
            style.fill_color_with_alpha(0.0),
            Color32::from_rgba_unmultiplied(20, 22, 28, 0)
        );
        assert_eq!(style.fill_color_with_alpha(1.0), style.fill_color());
        assert_eq!(
            style.fill_color_with_alpha(0.5),
            Color32::from_rgba_unmultiplied(20, 22, 28, 64)
        );
    }
}
