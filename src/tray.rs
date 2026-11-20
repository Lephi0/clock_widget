use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    BringToFront,
    ResetPosition,
    ToggleAlwaysOnTop,
    ToggleBackground,
    OpenOptions,
    CloseApp,
}

pub struct TrayController {
    tray_icon: Option<TrayIcon>,
    aot_check_item: Option<CheckMenuItem>,
    id_bring_front: MenuId,
    id_reset_pos: MenuId,
    id_toggle_aot: MenuId,
    id_toggle_bg: MenuId,
    id_options: MenuId,
    id_close: MenuId,
}

impl TrayController {
    pub fn new(enabled: bool, always_on_top: bool) -> Self {
        let mut controller = Self {
            tray_icon: None,
            aot_check_item: None,
            id_bring_front: MenuId::new("tray_bring_front"),
            id_reset_pos: MenuId::new("tray_reset_pos"),
            id_toggle_aot: MenuId::new("tray_toggle_aot"),
            id_toggle_bg: MenuId::new("tray_toggle_bg"),
            id_options: MenuId::new("tray_options"),
            id_close: MenuId::new("tray_close"),
        };
        controller.set_enabled(enabled, always_on_top);
        controller
    }

    pub fn set_enabled(&mut self, enabled: bool, always_on_top: bool) {
        if enabled {
            if self.tray_icon.is_none() {
                if let Ok((tray, aot_item)) = self.build_tray(always_on_top) {
                    self.tray_icon = Some(tray);
                    self.aot_check_item = Some(aot_item);
                }
            } else {
                self.sync_always_on_top(always_on_top);
            }
        } else {
            // Dropping TrayIcon removes it immediately from the Windows notification area
            self.aot_check_item = None;
            self.tray_icon = None;
        }
    }

    pub fn sync_always_on_top(&self, always_on_top: bool) {
        if let Some(item) = &self.aot_check_item {
            item.set_checked(always_on_top);
        }
    }

    fn build_tray(&self, always_on_top: bool) -> Result<(TrayIcon, CheckMenuItem), String> {
        let menu = Menu::new();

        let item_front = MenuItem::with_id(
            self.id_bring_front.clone(),
            "Bring Widget to Front",
            true,
            None,
        );
        let item_reset = MenuItem::with_id(
            self.id_reset_pos.clone(),
            "Reset Position (Center Screen)",
            true,
            None,
        );
        let item_aot = CheckMenuItem::with_id(
            self.id_toggle_aot.clone(),
            "Always on Top",
            true,
            always_on_top,
            None,
        );
        let item_bg = MenuItem::with_id(
            self.id_toggle_bg.clone(),
            "Toggle Background (Glass / Clear)",
            true,
            None,
        );
        let item_options = MenuItem::with_id(self.id_options.clone(), "Options...", true, None);
        let item_close = MenuItem::with_id(self.id_close.clone(), "Close", true, None);

        let _ = menu.append(&item_front);
        let _ = menu.append(&item_reset);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&item_aot);
        let _ = menu.append(&item_bg);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&item_options);
        let _ = menu.append(&item_close);

        let icon = generate_widget_tray_icon()?;

        let tray = TrayIconBuilder::new()
            .with_tooltip("Clock & Weather Widget")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(true)
            .build()
            .map_err(|e| format!("Failed to build system tray icon: {e}"))?;

        Ok((tray, item_aot))
    }

    pub fn poll_actions(&self) -> Vec<TrayAction> {
        let mut actions = Vec::new();

        // Drain menu events
        let menu_rx = MenuEvent::receiver();
        while let Ok(event) = menu_rx.try_recv() {
            if event.id == self.id_bring_front {
                actions.push(TrayAction::BringToFront);
            } else if event.id == self.id_reset_pos {
                actions.push(TrayAction::ResetPosition);
            } else if event.id == self.id_toggle_aot {
                actions.push(TrayAction::ToggleAlwaysOnTop);
            } else if event.id == self.id_toggle_bg {
                actions.push(TrayAction::ToggleBackground);
            } else if event.id == self.id_options {
                actions.push(TrayAction::OpenOptions);
            } else if event.id == self.id_close {
                actions.push(TrayAction::CloseApp);
            }
        }

        // Drain tray icon click events (double click brings widget to front immediately)
        let tray_rx = TrayIconEvent::receiver();
        while let Ok(event) = tray_rx.try_recv() {
            match event {
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => {
                    actions.push(TrayAction::BringToFront);
                }
                TrayIconEvent::Click {
                    button: MouseButton::Middle,
                    button_state: MouseButtonState::Up,
                    ..
                } => {
                    actions.push(TrayAction::ToggleAlwaysOnTop);
                }
                _ => {}
            }
        }

        actions
    }
}

/// Generates a crisp 32x32 RGBA iOS-style weather & clock icon in memory.
pub fn generate_widget_tray_icon() -> Result<Icon, String> {
    let (rgba, w, h) = generate_icon_rgba();
    Icon::from_rgba(rgba, w, h).map_err(|e| e.to_string())
}

pub fn generate_icon_rgba() -> (Vec<u8>, u32, u32) {
    let w = 32u32;
    let h = 32u32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];

    for y in 0..h {
        for x in 0..w {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;

            // Rounded rect SDF for iOS app icon tile
            let half_w = 14.0;
            let half_h = 14.0;
            let radius = 6.5;
            let dx = (fx - 16.0).abs() - (half_w - radius);
            let dy = (fy - 16.0).abs() - (half_h - radius);
            let dist = (dx.max(0.0).hypot(dy.max(0.0))) + dx.min(0.0).max(dy.min(0.0)) - radius;

            if dist <= 0.8 {
                let edge_alpha = (0.8 - dist).clamp(0.0, 1.0);
                // Sky-blue to deep-indigo gradient
                let t = (fy / 32.0).clamp(0.0, 1.0);
                let mut r = (36.0 * (1.0 - t) + 18.0 * t) as f32;
                let mut g = (116.0 * (1.0 - t) + 46.0 * t) as f32;
                let mut b = (228.0 * (1.0 - t) + 125.0 * t) as f32;

                // Golden sun circle in upper-left (cx=12.0, cy=12.0, r=5.2)
                let sun_d = (fx - 12.0).hypot(fy - 12.0);
                if sun_d < 5.8 {
                    let s_alpha = (5.8 - sun_d).clamp(0.0, 1.0);
                    r = r * (1.0 - s_alpha) + 255.0 * s_alpha;
                    g = g * (1.0 - s_alpha) + 206.0 * s_alpha;
                    b = b * (1.0 - s_alpha) + 58.0 * s_alpha;
                }

                // Soft white cloud in lower-right
                let c1 = (fx - 16.5).hypot(fy - 19.0) - 4.5;
                let c2 = (fx - 21.0).hypot(fy - 20.0) - 3.6;
                let c3 = (fx - 13.0).hypot(fy - 20.3) - 3.4;
                let cloud_d = c1.min(c2).min(c3);
                if cloud_d < 0.8 {
                    let c_alpha = ((0.8 - cloud_d).clamp(0.0, 1.0)) * 0.95;
                    r = r * (1.0 - c_alpha) + 248.0 * c_alpha;
                    g = g * (1.0 - c_alpha) + 251.0 * c_alpha;
                    b = b * (1.0 - c_alpha) + 255.0 * c_alpha;
                }

                let idx = ((y * w + x) * 4) as usize;
                rgba[idx] = r.clamp(0.0, 255.0) as u8;
                rgba[idx + 1] = g.clamp(0.0, 255.0) as u8;
                rgba[idx + 2] = b.clamp(0.0, 255.0) as u8;
                rgba[idx + 3] = (edge_alpha * 255.0) as u8;
            }
        }
    }

    (rgba, w, h)
}
