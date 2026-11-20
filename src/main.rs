#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod dwm;
mod glass;
mod hotkey;
mod icons;
mod settings_ui;
mod tray;
mod weather;

use chrono::{Local, Timelike};
use config::{BackgroundMode, GlassTintPreset, TempUnit, WidgetConfig, WeatherProvider};
use dwm::{
    bring_hwnd_to_front, clamp_popup_pos_to_monitor_px, configure_popup_window,
    configure_settings_window, extract_hwnd, find_own_window_by_title, get_cursor_pos_px,
    get_monitor_work_area_for_hwnd, get_window_pos_px, hide_popup_window, is_windows_light_theme,
    position_popup_window_px, primary_monitor_center_pos_px, resize_window_px,
    resolve_window_pos_in_work_area, set_window_pos_px_checked, trim_process_working_set,
};
use egui::{
    Align, Align2, Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId,
    Frame, Id, Key, Margin, Painter, PointerButton, Pos2, Rect, RichText, Sense, Stroke, Vec2,
    ViewportBuilder, ViewportClass, ViewportCommand, ViewportId, WindowLevel,
};
use glass::{
    apply_glass_visuals, draw_glass_background, draw_glass_surface,
    draw_glass_surface_with_alpha, GlassPalette, GlassStyle, GlassWindowBackdrop,
};
use hotkey::{capture_egui_shortcut, HotkeyController};
use icons::draw_weather_icon;
use settings_ui::{
    draw_settings_tab_bar, draw_settings_titlebar, draw_status_pill, glass_button, glass_card,
    glass_location_result_row, glass_primary_button, glass_recording_button,
    glass_segmented_control, glass_slider_row, glass_toggle_row, OptionsWindowState, SettingsTab,
    OPTIONS_WINDOW_HEIGHT, OPTIONS_WINDOW_TITLE, OPTIONS_WINDOW_WIDTH,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tray::{TrayAction, TrayController};
use weather::{
    GeoLocationResult, WeatherConditionKind, WeatherData, WeatherFetchRequest, WeatherManager,
    WeatherWorkerMessage, WEATHER_REFRESH_INTERVAL,
};

const WIDGET_WIDTH: f32 = 348.0;
const WIDGET_HEIGHT: f32 = 176.0;
const CONTEXT_MENU_WINDOW_TITLE: &str = "Clock & Weather Context Menu";
const CONTEXT_MENU_WIDTH: f32 = 236.0;
const CONTEXT_MENU_HEIGHT: f32 = 178.0;
const CONTEXT_MENU_FADE_IN_SECS: f32 = 0.15;
const CONTEXT_MENU_FADE_OUT_SECS: f32 = 0.12;
const CARD_MARGIN: f32 = 0.0;

fn base_widget_size(config: &WidgetConfig) -> Vec2 {
    if config.compact_mode {
        let clock_sz = config.clock_font_size.clamp(10.0, 80.0);
        let date_sz = config.date_font_size.clamp(8.0, 48.0);
        let max_font = clock_sz.max(date_sz);
        let h = (max_font * 1.25 + 6.0).clamp(20.0, 130.0).round();

        let time_chars = 5.0
            + if config.show_seconds { 2.8 } else { 0.0 }
            + if !config.use_24_hour { 2.4 } else { 0.0 };
        let est_time_w = clock_sz * (time_chars * 0.56);
        let est_date_w = date_sz * 11.2;
        let w = (est_time_w + est_date_w + 18.0).clamp(100.0, 960.0).round();
        Vec2::new(w, h)
    } else {
        let extra_clock = (config.clock_font_size - 50.0).max(0.0);
        let extra_date = (config.date_font_size - 14.0).max(0.0);
        let w = (WIDGET_WIDTH + extra_clock * 3.2 + extra_date * 5.0).round();
        let h = (WIDGET_HEIGHT + extra_clock * 1.2 + extra_date * 1.5).round();
        Vec2::new(w, h)
    }
}

fn main() -> eframe::Result<()> {
    let initial_config = WidgetConfig::load();
    let initial_size = base_widget_size(&initial_config);

    let (icon_rgba, icon_w, icon_h) = tray::generate_icon_rgba();
    let app_icon = egui::IconData {
        rgba: icon_rgba,
        width: icon_w,
        height: icon_h,
    };

    let viewport = ViewportBuilder::default()
        .with_title("Clock & Weather Widget")
        .with_inner_size([initial_size.x, initial_size.y])
        .with_min_inner_size([initial_size.x, initial_size.y])
        .with_max_inner_size([initial_size.x, initial_size.y])
        .with_decorations(false)
        .with_minimize_button(false)
        .with_maximize_button(false)
        .with_close_button(false)
        .with_transparent(true)
        .with_taskbar(false)
        .with_resizable(false)
        .with_icon(app_icon.clone())
        .with_window_level(if initial_config.always_on_top {
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        });

    let native_options = eframe::NativeOptions {
        viewport,
        depth_buffer: 0,
        stencil_buffer: 0,
        multisampling: 0,
        vsync: true,
        ..Default::default()
    };

    eframe::run_native(
        "Clock & Weather Widget",
        native_options,
        Box::new(|cc| Ok(Box::new(ClockWeatherWidgetApp::new(cc, initial_config, app_icon)))),
    )
}

fn install_system_font(ctx: &Context) {
    let candidates = [
        r"C:\Windows\Fonts\segoeui.ttf",
        r"C:\Windows\Fonts\arial.ttf",
        r"C:\Windows\Fonts\tahoma.ttf",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = FontDefinitions::empty();
            fonts
                .font_data
                .insert("sys_ui".to_owned(), Arc::new(FontData::from_owned(bytes)));
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .push("sys_ui".to_owned());
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .push("sys_ui".to_owned());
            ctx.set_fonts(fonts);
            return;
        }
    }
}

struct ContextMenuState {
    open: bool,
    closing: bool,
    revealed: bool,
    activated: bool,
    locked_pos_px: [i32; 2],
    locked_size_px: [i32; 2],
    locked_logical_pos: Pos2,
    embedded_pos: Pos2,
    hwnd: Option<isize>,
    glass_backdrop: GlassWindowBackdrop,
    needs_reposition: bool,
    had_focus: bool,
    opened_at: Option<Instant>,
    fade_started_at: Option<Instant>,
    fade_from_alpha: f32,
    current_alpha: f32,
}

impl Default for ContextMenuState {
    fn default() -> Self {
        Self {
            open: false,
            closing: false,
            revealed: false,
            activated: false,
            locked_pos_px: [0, 0],
            locked_size_px: [0, 0],
            locked_logical_pos: Pos2::ZERO,
            embedded_pos: Pos2::ZERO,
            hwnd: None,
            glass_backdrop: GlassWindowBackdrop::new(),
            needs_reposition: false,
            had_focus: false,
            opened_at: None,
            fade_started_at: None,
            fade_from_alpha: 0.0,
            current_alpha: 0.0,
        }
    }
}

impl ContextMenuState {
    fn is_animating(&self) -> bool {
        self.open
            && (!self.revealed || !self.activated || self.closing || self.current_alpha < 0.999)
    }
}

struct ClockWeatherWidgetApp {
    config: WidgetConfig,
    app_icon: egui::IconData,
    hwnd: Option<isize>,
    windows_is_light: bool,
    widget_glass_backdrop: GlassWindowBackdrop,
    last_applied_size: Option<(Vec2, [i32; 2])>,
    measured_compact_size: Option<Vec2>,
    startup_frames: u8,

    // Controllers
    hotkey_controller: HotkeyController,
    tray_controller: TrayController,
    weather_manager: WeatherManager,

    // Weather state
    weather: Option<WeatherData>,
    last_weather_fetch: Option<Instant>,

    // Window position persistence (physical screen coordinates)
    last_observed_pos_px: Option<[i32; 2]>,
    pos_dirty_since: Option<Instant>,

    // Context menu & Options window state
    context_menu: ContextMenuState,
    options_window: OptionsWindowState,

    // Options window editable buffers
    city_search_query: String,
    city_search_results: Vec<GeoLocationResult>,
    city_search_error: Option<String>,
    lat_input: String,
    lon_input: String,
    location_name_input: String,
    hotkey_input: String,
    recording_hotkey: bool,
    hotkey_feedback: Option<(String, bool)>,
    show_api_key: bool,
}

impl ClockWeatherWidgetApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        mut config: WidgetConfig,
        app_icon: egui::IconData,
    ) -> Self {
        install_system_font(&cc.egui_ctx);

        let hwnd = extract_hwnd(cc);
        let windows_is_light = is_windows_light_theme();
        let hotkey_controller = HotkeyController::new(&config.toggle_aot_hotkey);
        let tray_controller = TrayController::new(config.show_tray_icon, config.always_on_top);
        let weather_manager = WeatherManager::new();

        // Restore physical screen position or center on primary monitor if off-screen
        let initial_size = base_widget_size(&config);
        let ppp = cc.egui_ctx.pixels_per_point().max(1.0);
        let width_px = (initial_size.x * ppp).round() as i32;
        let height_px = (initial_size.y * ppp).round() as i32;
        let saved_px = config
            .window_pos
            .map(|[x, y]| [x.round() as i32, y.round() as i32]);
        let actual_px = set_window_pos_px_checked(hwnd, saved_px, width_px, height_px);
        config.window_pos = Some([actual_px[0] as f32, actual_px[1] as f32]);
        let _ = config.save();

        // In Compact Mode, don't hold weather data in RAM
        let initial_weather = if config.compact_mode {
            None
        } else {
            let mut w = config.cached_weather.clone();
            if let Some(ref mut data) = w {
                data.is_offline_cache = true;
            }
            w
        };

        let lat_input = format!("{:.4}", config.latitude);
        let lon_input = format!("{:.4}", config.longitude);
        let location_name_input = config.location_name.clone();
        let hotkey_input = config.toggle_aot_hotkey.clone();

        let mut app = Self {
            config,
            app_icon,
            hwnd,
            windows_is_light,
            widget_glass_backdrop: GlassWindowBackdrop::new(),
            last_applied_size: None,
            measured_compact_size: None,
            startup_frames: 0,
            hotkey_controller,
            tray_controller,
            weather_manager,
            weather: initial_weather,
            last_weather_fetch: None,
            last_observed_pos_px: Some(actual_px),
            pos_dirty_since: None,
            context_menu: ContextMenuState::default(),
            options_window: OptionsWindowState::default(),
            city_search_query: String::new(),
            city_search_results: Vec::new(),
            city_search_error: None,
            lat_input,
            lon_input,
            location_name_input,
            hotkey_input,
            recording_hotkey: false,
            hotkey_feedback: None,
            show_api_key: false,
        };

        // Trigger initial background weather fetch if not in compact mode
        if !app.config.compact_mode {
            app.request_weather_refresh(&cc.egui_ctx);
        }

        // Apply initial DWM blur state
        app.ensure_dwm_synced();

        app
    }

    fn effective_background_mode(&self) -> BackgroundMode {
        if self.config.compact_mode {
            BackgroundMode::Transparent
        } else {
            self.config.background_mode
        }
    }

    fn current_content_size(&self) -> Vec2 {
        if self.config.compact_mode {
            self.measured_compact_size
                .unwrap_or_else(|| base_widget_size(&self.config))
        } else {
            base_widget_size(&self.config)
        }
    }

    fn desired_window_size(&self) -> Vec2 {
        self.current_content_size()
    }

    fn current_glass_style(&self) -> GlassStyle {
        GlassStyle::from_config(&self.config, self.windows_is_light)
    }

    fn ensure_window_size_synced(&mut self, ctx: &Context) {
        let target = self.desired_window_size();
        let ppp = ctx.pixels_per_point().max(1.0);
        let target_px = [
            (target.x * ppp).round() as i32,
            (target.y * ppp).round() as i32,
        ];
        if self.last_applied_size != Some((target, target_px)) {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::MinInnerSize(target));
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::MaxInnerSize(target));
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::InnerSize(target));
            resize_window_px(self.hwnd, target_px[0], target_px[1]);
            self.last_applied_size = Some((target, target_px));
            self.widget_glass_backdrop.invalidate();
        }
    }

    fn request_weather_refresh(&mut self, ctx: &Context) {
        let req = WeatherFetchRequest {
            latitude: self.config.latitude,
            longitude: self.config.longitude,
            location_name: self.config.location_name.clone(),
            provider: self.config.weather_provider,
            google_api_key: self.config.google_api_key.clone(),
        };
        self.weather_manager.trigger_fetch(req, ctx.clone());
        self.last_weather_fetch = Some(Instant::now());
    }

    fn ensure_dwm_synced(&mut self) {
        if self.config.glass_tint_preset == GlassTintPreset::System {
            self.windows_is_light = is_windows_light_theme();
        }
        let target_mode = self.effective_background_mode();
        let is_light = self.current_glass_style().is_light();
        self.widget_glass_backdrop
            .sync(self.hwnd, target_mode, is_light);
    }

    fn set_always_on_top(&mut self, ctx: &Context, enabled: bool) {
        self.config.always_on_top = enabled;
        let level = if enabled {
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        };
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::WindowLevel(level));
        self.widget_glass_backdrop.invalidate();
        self.tray_controller.sync_always_on_top(enabled);
        let _ = self.config.save();
    }

    fn toggle_always_on_top(&mut self, ctx: &Context) {
        let next = !self.config.always_on_top;
        self.set_always_on_top(ctx, next);
    }

    fn set_background_mode(&mut self, ctx: &Context, mode: BackgroundMode) {
        self.config.background_mode = mode;
        if self.config.compact_mode {
            self.config.compact_mode = false;
            if self.weather.is_none() {
                self.weather = self.config.cached_weather.clone();
            }
            if self.last_weather_fetch.is_none() {
                self.request_weather_refresh(ctx);
            }
        }
        self.widget_glass_backdrop.invalidate();
        self.ensure_window_size_synced(ctx);
        self.ensure_dwm_synced();
        let _ = self.config.save();
        ctx.request_repaint();
    }

    fn set_compact_mode(&mut self, ctx: &Context, compact: bool) {
        self.config.compact_mode = compact;
        self.measured_compact_size = None;
        self.widget_glass_backdrop.invalidate();
        self.ensure_window_size_synced(ctx);
        self.ensure_dwm_synced();
        let _ = self.config.save();
        if compact {
            // Release in-memory weather structs and trim working set
            self.weather = None;
            self.city_search_results.clear();
            trim_process_working_set();
        } else {
            if self.weather.is_none() {
                self.weather = self.config.cached_weather.clone();
            }
            if self.last_weather_fetch.is_none() {
                self.request_weather_refresh(ctx);
            }
        }
        ctx.request_repaint();
    }

    fn reset_position_to_center(&mut self, ctx: &Context) {
        let size = base_widget_size(&self.config);
        let ppp = ctx.pixels_per_point().max(1.0);
        let width_px = (size.x * ppp).round() as i32;
        let height_px = (size.y * ppp).round() as i32;
        let center_px = primary_monitor_center_pos_px(width_px, height_px);
        let actual_px = set_window_pos_px_checked(self.hwnd, Some(center_px), width_px, height_px);
        self.last_observed_pos_px = Some(actual_px);
        self.last_applied_size = None;
        self.widget_glass_backdrop.invalidate();
        self.config.window_pos = Some([actual_px[0] as f32, actual_px[1] as f32]);
        let _ = self.config.save();
        bring_hwnd_to_front(self.hwnd);
    }

    fn handle_background_events(&mut self, ctx: &Context) {
        // 1. Poll Global Hotkey
        if self.hotkey_controller.poll_triggered() {
            self.toggle_always_on_top(ctx);
        }

        // 2. Poll System Tray Actions
        for action in self.tray_controller.poll_actions() {
            match action {
                TrayAction::BringToFront => {
                    bring_hwnd_to_front(self.hwnd);
                    ctx.send_viewport_cmd(ViewportCommand::Focus);
                }
                TrayAction::ResetPosition => {
                    self.reset_position_to_center(ctx);
                }
                TrayAction::ToggleAlwaysOnTop => {
                    self.toggle_always_on_top(ctx);
                }
                TrayAction::ToggleBackground => {
                    let next = self.config.background_mode.toggled();
                    self.set_background_mode(ctx, next);
                }
                TrayAction::OpenOptions => {
                    self.begin_close_context_menu(ctx);
                    self.open_options_window(ctx);
                }
                TrayAction::CloseApp => {
                    self.finish_close_context_menu();
                    self.persist_options_window_pos();
                    hide_popup_window(self.options_window.hwnd);
                    let _ = self.config.save();
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        }

        // 3. Poll Weather & Geocoding Worker
        let mut received_weather_msg = false;
        for msg in self.weather_manager.poll() {
            received_weather_msg = true;
            match msg {
                WeatherWorkerMessage::WeatherFetched(Ok(data)) => {
                    self.config.cached_weather = Some(data.clone());
                    if !self.config.compact_mode {
                        self.weather = Some(data);
                    }
                    let _ = self.config.save();
                    trim_process_working_set();
                }
                WeatherWorkerMessage::WeatherFetched(Err(_err)) => {
                    if let Some(ref mut cached) = self.weather {
                        cached.is_offline_cache = true;
                    }
                    trim_process_working_set();
                }
                WeatherWorkerMessage::CitySearchResults(Ok(results)) => {
                    self.city_search_results = results;
                    self.city_search_error = None;
                    trim_process_working_set();
                }
                WeatherWorkerMessage::CitySearchResults(Err(err)) => {
                    self.city_search_results.clear();
                    self.city_search_error = Some(err);
                    trim_process_working_set();
                }
            }
        }
        if received_weather_msg && self.options_window.open {
            ctx.request_repaint_of(ViewportId::from_hash_of("clock_widget_options_viewport"));
        }

        // 4. Periodic 20-minute weather refresh (skipped in Compact Mode to save resources)
        if !self.config.compact_mode {
            let should_refresh = match self.last_weather_fetch {
                Some(last) => last.elapsed() >= WEATHER_REFRESH_INTERVAL,
                None => true,
            };
            if should_refresh && !self.weather_manager.is_fetching {
                self.request_weather_refresh(ctx);
            }
        }

        // 5. Track physical window position changes via Win32 GetWindowRect and save after drag settles
        if let Some(pos_px) = get_window_pos_px(self.hwnd) {
            let changed = match self.last_observed_pos_px {
                Some(prev) => (prev[0] - pos_px[0]).abs() > 1 || (prev[1] - pos_px[1]).abs() > 1,
                None => {
                    self.last_observed_pos_px = Some(pos_px);
                    false
                }
            };
            if changed {
                self.last_observed_pos_px = Some(pos_px);
                self.pos_dirty_since = Some(Instant::now());
            }
        }

        if let Some(dirty_time) = self.pos_dirty_since {
            let is_dragging = ctx.input(|i| i.pointer.primary_down());
            if !is_dragging && dirty_time.elapsed() >= Duration::from_millis(350) {
                if let Some(pos_px) = self.last_observed_pos_px {
                    self.config.window_pos = Some([pos_px[0] as f32, pos_px[1] as f32]);
                    let _ = self.config.save();
                }
                self.pos_dirty_since = None;
            }
        }
    }
}

impl eframe::App for ClockWeatherWidgetApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Must return full transparency so no solid rectangular box renders behind rounded corners
        [0.0, 0.0, 0.0, 0.0]
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(pos_px) = get_window_pos_px(self.hwnd).or(self.last_observed_pos_px) {
            self.config.window_pos = Some([pos_px[0] as f32, pos_px[1] as f32]);
        }
        self.persist_options_window_pos();
        let _ = self.config.save();
    }

    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.ensure_window_size_synced(ctx);
        self.ensure_dwm_synced();
        self.handle_background_events(ctx);

        let win_size = self.desired_window_size();
        let full_rect = Rect::from_min_size(Pos2::ZERO, win_size);
        let card_rect = full_rect.shrink(CARD_MARGIN);

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::TRANSPARENT))
            .show(ctx, |ui| {
                let painter = ui.painter().clone();

                // 1. Draw Widget Card Background (skipped in Transparent mode and Compact mode)
                self.draw_card_background(&painter, card_rect);

                // 2. Draw Clock & Weather Content (or single-line Time & Date in Compact mode)
                self.draw_widget_contents(ctx, &painter, card_rect);

                // 3. Handle Dragging & Right-Click Context Menu
                self.handle_widget_interactions(ctx, ui, card_rect);
            });

        // 4. Render Right-Click Context Menu Window in its own native glass viewport when open
        if self.context_menu.open {
            self.show_context_menu_viewport(ctx);
        }

        // 5. Render Options Window in its own native glass viewport when open
        if self.options_window.open {
            self.show_options_viewport(ctx);
        }

        // Reclaim one-time OpenGL driver / shader compiler startup buffers after initial frames render
        if self.startup_frames < 5 {
            if self.startup_frames == 1 {
                self.widget_glass_backdrop.invalidate();
                self.ensure_dwm_synced();
            }
            self.startup_frames += 1;
            if self.startup_frames == 4 {
                trim_process_working_set();
            }
        }

        // 6. Schedule low-CPU repaint:
        // When seconds are shown, wake on the next second tick; when seconds are hidden, wake on minute tick
        // (capped at 2.5s so background hotkey/tray events remain responsive with minimal wakeups).
        let now = Local::now();
        let nanos = now.nanosecond();
        let ms_until_next_second = ((1_000_000_000u32.saturating_sub(nanos)) / 1_000_000).max(16);

        let mut next_repaint = if self.config.show_seconds {
            Duration::from_millis(ms_until_next_second as u64)
        } else {
            let secs_left = 59u64.saturating_sub(now.second() as u64);
            let ms_until_next_minute = secs_left * 1000 + ms_until_next_second as u64;
            Duration::from_millis(ms_until_next_minute.min(2500))
        };

        if self.context_menu.is_animating()
            || (self.options_window.open && !self.options_window.activated)
        {
            next_repaint = next_repaint.min(Duration::from_millis(16));
        } else if self.context_menu.open {
            next_repaint = next_repaint.min(Duration::from_millis(100));
        } else if self.pos_dirty_since.is_some() || self.options_window.pos_dirty_since.is_some() {
            next_repaint = next_repaint.min(Duration::from_millis(200));
        } else if self.options_window.open {
            next_repaint = next_repaint.min(Duration::from_millis(250));
        }

        ctx.request_repaint_after(next_repaint);
    }
}

impl ClockWeatherWidgetApp {
    fn draw_card_background(&self, painter: &Painter, card_rect: Rect) {
        let style = self.current_glass_style();
        draw_glass_background(painter, card_rect, self.effective_background_mode(), &style);
    }

    fn draw_widget_contents(&mut self, ctx: &Context, painter: &Painter, card_rect: Rect) {
        let is_clear_mode = self.effective_background_mode() == BackgroundMode::Transparent;
        let is_light_glass = !is_clear_mode && self.current_glass_style().is_light();
        let palette = GlassPalette::for_theme(is_light_glass);
        let draw_shadow = is_clear_mode || !palette.is_light;

        let primary_text = palette.primary_text;
        let secondary_text = palette.secondary_text;
        let accent_date_color = palette.accent_text;

        let clock_sz = self.config.clock_font_size.clamp(10.0, 80.0);
        let date_sz = self.config.date_font_size.clamp(8.0, 48.0);

        let now = Local::now();
        let weekday_str = now.format("%A").to_string().to_uppercase();
        let month_day_str = now.format("%b %-d").to_string().to_uppercase();

        let (hour_min_str, am_pm_opt) = if self.config.use_24_hour {
            (now.format("%H:%M").to_string(), None)
        } else {
            let h12 = match now.hour() % 12 {
                0 => 12,
                h => h,
            };
            let am_pm = if now.hour() < 12 { "AM" } else { "PM" };
            (format!("{h12}:{:02}", now.minute()), Some(am_pm))
        };
        let sec_str = format!(":{:02}", now.second());

        // --- COMPACT MODE: SINGLE-LINE CLOCK & DATE TEXT (FITS INSIDE A TITLEBAR) ---
        if self.config.compact_mode {
            let clock_font = FontId::new(clock_sz, FontFamily::Proportional);
            let sec_font = FontId::new((clock_sz * 0.80).max(8.0), FontFamily::Proportional);
            let ampm_font = FontId::new((clock_sz * 0.72).max(8.0), FontFamily::Proportional);
            let date_font = FontId::new(date_sz, FontFamily::Proportional);

            let hm_galley =
                painter.layout_no_wrap(hour_min_str.clone(), clock_font.clone(), primary_text);
            let ref_hm_galley =
                painter.layout_no_wrap("88:88".to_string(), clock_font.clone(), primary_text);

            let (sec_w, ref_sec_w) = if self.config.show_seconds {
                let sg = painter.layout_no_wrap(sec_str.clone(), sec_font.clone(), secondary_text);
                let rsg =
                    painter.layout_no_wrap(":88".to_string(), sec_font.clone(), secondary_text);
                (sg.size().x, rsg.size().x)
            } else {
                (0.0, 0.0)
            };

            let (ampm_txt, ampm_w) = if let Some(am_pm) = am_pm_opt {
                let txt = format!(" {am_pm}");
                let ag = painter.layout_no_wrap(txt.clone(), ampm_font.clone(), secondary_text);
                (Some(txt), ag.size().x)
            } else {
                (None, 0.0)
            };

            let sep_str = "  |  ";
            let sep_galley =
                painter.layout_no_wrap(sep_str.to_string(), date_font.clone(), secondary_text);
            let w_galley =
                painter.layout_no_wrap(weekday_str.clone(), date_font.clone(), accent_date_color);
            let m_str = format!("  {month_day_str}");
            let m_galley =
                painter.layout_no_wrap(m_str.clone(), date_font.clone(), secondary_text);

            let actual_total_w = hm_galley.size().x
                + sec_w
                + ampm_w
                + sep_galley.size().x
                + w_galley.size().x
                + m_galley.size().x;
            let ref_total_w = ref_hm_galley.size().x
                + ref_sec_w
                + ampm_w
                + sep_galley.size().x
                + w_galley.size().x
                + m_galley.size().x;
            let max_h = hm_galley.size().y.max(w_galley.size().y);

            let target_compact_size = Vec2::new(
                (ref_total_w + 14.0).ceil(),
                (max_h + 4.0).max(18.0).ceil(),
            );
            if self.measured_compact_size != Some(target_compact_size) {
                self.measured_compact_size = Some(target_compact_size);
                self.ensure_window_size_synced(ctx);
            }

            let center_y = target_compact_size.y * 0.5;
            let mut cur_x = ((target_compact_size.x - actual_total_w) * 0.5).max(6.0);

            draw_shadowed_text(
                painter,
                Pos2::new(cur_x, center_y),
                Align2::LEFT_CENTER,
                &hour_min_str,
                clock_font,
                primary_text,
                true,
            );
            cur_x += hm_galley.size().x;

            if self.config.show_seconds {
                draw_shadowed_text(
                    painter,
                    Pos2::new(cur_x, center_y),
                    Align2::LEFT_CENTER,
                    &sec_str,
                    sec_font,
                    secondary_text,
                    true,
                );
                cur_x += sec_w;
            }

            if let Some(ref txt) = ampm_txt {
                draw_shadowed_text(
                    painter,
                    Pos2::new(cur_x, center_y),
                    Align2::LEFT_CENTER,
                    txt,
                    ampm_font,
                    secondary_text,
                    true,
                );
                cur_x += ampm_w;
            }

            draw_shadowed_text(
                painter,
                Pos2::new(cur_x, center_y),
                Align2::LEFT_CENTER,
                sep_str,
                date_font.clone(),
                secondary_text,
                true,
            );
            cur_x += sep_galley.size().x;

            draw_shadowed_text(
                painter,
                Pos2::new(cur_x, center_y),
                Align2::LEFT_CENTER,
                &weekday_str,
                date_font.clone(),
                accent_date_color,
                true,
            );
            cur_x += w_galley.size().x;

            draw_shadowed_text(
                painter,
                Pos2::new(cur_x, center_y),
                Align2::LEFT_CENTER,
                &m_str,
                date_font,
                secondary_text,
                true,
            );
            return;
        }

        // --- FULL WIDGET MODE: CLOCK + WEATHER ---
        let inner = card_rect.shrink2(Vec2::new(20.0, 16.0));
        let divider_x = inner.min.x + inner.width() * 0.56;

        // Vertical divider in Frosted Glass mode
        if !is_clear_mode {
            painter.line_segment(
                [
                    Pos2::new(divider_x, inner.min.y + 6.0),
                    Pos2::new(divider_x, inner.max.y - 6.0),
                ],
                Stroke::new(1.0_f32, palette.divider),
            );
        }

        // --- LEFT COLUMN: CLOCK & DATE ---
        let date_y = inner.min.y + 4.0;
        let date_font = FontId::new(date_sz, FontFamily::Proportional);
        draw_shadowed_text(
            painter,
            Pos2::new(inner.min.x, date_y),
            Align2::LEFT_TOP,
            &weekday_str,
            date_font.clone(),
            accent_date_color,
            draw_shadow,
        );

        let weekday_galley =
            painter.layout_no_wrap(weekday_str.clone(), date_font.clone(), accent_date_color);
        draw_shadowed_text(
            painter,
            Pos2::new(inner.min.x + weekday_galley.size().x + 6.0, date_y),
            Align2::LEFT_TOP,
            &format!("•  {month_day_str}"),
            date_font,
            secondary_text,
            draw_shadow,
        );

        let clock_y = inner.center().y - 2.0;
        let clock_font = FontId::new(clock_sz, FontFamily::Proportional);
        let clock_galley =
            painter.layout_no_wrap(hour_min_str.clone(), clock_font.clone(), primary_text);

        draw_shadowed_text(
            painter,
            Pos2::new(inner.min.x - 1.0, clock_y),
            Align2::LEFT_CENTER,
            &hour_min_str,
            clock_font,
            primary_text,
            draw_shadow,
        );

        let right_of_hm = inner.min.x - 1.0 + clock_galley.size().x + 4.0;
        let ampm_sz = (clock_sz * 0.24).clamp(9.0, 22.0);
        let sec_sz = (clock_sz * 0.30).clamp(10.0, 26.0);
        if let Some(am_pm) = am_pm_opt {
            draw_shadowed_text(
                painter,
                Pos2::new(right_of_hm, clock_y - clock_sz * 0.32),
                Align2::LEFT_TOP,
                am_pm,
                FontId::new(ampm_sz, FontFamily::Proportional),
                secondary_text,
                draw_shadow,
            );
        }

        if self.config.show_seconds {
            let sec_y_offset = if am_pm_opt.is_some() { 2.0 } else { 4.0 };
            draw_shadowed_text(
                painter,
                Pos2::new(right_of_hm, clock_y + sec_y_offset),
                Align2::LEFT_TOP,
                &sec_str,
                FontId::new(sec_sz, FontFamily::Proportional),
                secondary_text,
                draw_shadow,
            );
        }

        // Bottom-left: Location name & Always-on-Top pin badge
        let bottom_y = inner.max.y - 2.0;
        let loc_display = if self.config.location_name.trim().is_empty() {
            format!("{:.2}, {:.2}", self.config.latitude, self.config.longitude)
        } else {
            self.config.location_name.clone()
        };
        let mut footer_left = truncate_str(&loc_display, 18);
        if self.config.always_on_top {
            footer_left.push_str("  •  Pinned");
        }

        draw_shadowed_text(
            painter,
            Pos2::new(inner.min.x, bottom_y),
            Align2::LEFT_BOTTOM,
            &footer_left,
            FontId::new(12.5, FontFamily::Proportional),
            secondary_text,
            draw_shadow,
        );

        // --- RIGHT COLUMN: WEATHER ---
        let right_x = divider_x + 14.0;
        let right_width = inner.max.x - right_x;

        if let Some(ref w) = self.weather {
            // Weather Icon + Current Temperature row
            let icon_rect = Rect::from_min_size(
                Pos2::new(right_x - 2.0, inner.min.y + 2.0),
                Vec2::new(44.0, 44.0),
            );
            draw_weather_icon(painter, icon_rect, w.condition_kind, draw_shadow);

            let temp_str = self.config.temp_unit.format_temp(w.temperature_c);
            draw_shadowed_text(
                painter,
                Pos2::new(inner.max.x, inner.min.y + 4.0),
                Align2::RIGHT_TOP,
                &temp_str,
                FontId::new(34.0, FontFamily::Proportional),
                primary_text,
                draw_shadow,
            );

            // Condition Label
            let cond_str = truncate_str(&w.condition_label, 16);
            draw_shadowed_text(
                painter,
                Pos2::new(right_x, inner.center().y + 8.0),
                Align2::LEFT_CENTER,
                &cond_str,
                FontId::new(13.5, FontFamily::Proportional),
                primary_text,
                draw_shadow,
            );

            // High / Low
            let hi_str = self.config.temp_unit.format_temp(w.high_c);
            let lo_str = self.config.temp_unit.format_temp(w.low_c);
            let hl_line = format!("H:{hi_str}  L:{lo_str}");
            draw_shadowed_text(
                painter,
                Pos2::new(right_x, inner.max.y - 18.0),
                Align2::LEFT_BOTTOM,
                &hl_line,
                FontId::new(12.0, FontFamily::Proportional),
                secondary_text,
                draw_shadow,
            );

            // Humidity or Offline cache badge on bottom line
            let meta_line = if w.is_offline_cache {
                "Cached (Offline)".to_string()
            } else if let Some(hum) = w.humidity {
                format!("Humidity: {hum}%")
            } else {
                w.provider_used.clone()
            };
            draw_shadowed_text(
                painter,
                Pos2::new(right_x, bottom_y),
                Align2::LEFT_BOTTOM,
                &truncate_str(&meta_line, 18),
                FontId::new(11.0, FontFamily::Proportional),
                secondary_text,
                draw_shadow,
            );
        } else {
            // Initial loading state
            let icon_rect = Rect::from_min_size(
                Pos2::new(right_x, inner.min.y + 6.0),
                Vec2::new(40.0, 40.0),
            );
            draw_weather_icon(
                painter,
                icon_rect,
                WeatherConditionKind::PartlyCloudyDay,
                draw_shadow,
            );

            let status_txt = if self.weather_manager.is_fetching {
                "Updating..."
            } else {
                "Weather N/A"
            };
            draw_shadowed_text(
                painter,
                Pos2::new(right_x + right_width * 0.5, inner.center().y + 18.0),
                Align2::CENTER_CENTER,
                status_txt,
                FontId::new(13.0, FontFamily::Proportional),
                secondary_text,
                draw_shadow,
            );
        }
    }

    fn open_context_menu(&mut self, root_ctx: &Context, click_pos: Pos2) {
        let ppp = root_ctx.pixels_per_point().max(1.0);
        let menu_w_px = (CONTEXT_MENU_WIDTH * ppp).round() as i32;
        let menu_h_px = (CONTEXT_MENU_HEIGHT * ppp).round() as i32;
        let anchor_px = get_cursor_pos_px()
            .or_else(|| {
                get_window_pos_px(self.hwnd).map(|[wx, wy]| {
                    [
                        wx + (click_pos.x * ppp).round() as i32,
                        wy + (click_pos.y * ppp).round() as i32,
                    ]
                })
            })
            .unwrap_or([240, 180]);
        let pos_px = clamp_popup_pos_to_monitor_px(anchor_px, menu_w_px, menu_h_px);
        let logical_pos = Pos2::new(pos_px[0] as f32 / ppp, pos_px[1] as f32 / ppp);

        self.context_menu.locked_pos_px = pos_px;
        self.context_menu.locked_size_px = [menu_w_px, menu_h_px];
        self.context_menu.locked_logical_pos = logical_pos;
        self.context_menu.embedded_pos = click_pos;

        if self.context_menu.open {
            self.context_menu.closing = false;
            self.context_menu.needs_reposition = true;
            self.context_menu.had_focus = false;
            self.context_menu.opened_at = Some(Instant::now());
            self.context_menu.fade_started_at = Some(Instant::now());
            self.context_menu.fade_from_alpha = 0.0;
            self.context_menu.current_alpha = 0.0;
        } else {
            self.context_menu.open = true;
            self.context_menu.closing = false;
            self.context_menu.revealed = false;
            self.context_menu.activated = false;
            self.context_menu.needs_reposition = true;
            self.context_menu.had_focus = false;
            self.context_menu.opened_at = None;
            self.context_menu.fade_started_at = None;
            self.context_menu.fade_from_alpha = 0.0;
            self.context_menu.current_alpha = 0.0;
        }
        root_ctx.request_repaint();
    }

    fn begin_close_context_menu(&mut self, root_ctx: &Context) {
        if self.context_menu.open && !self.context_menu.closing {
            self.context_menu.closing = true;
            self.context_menu.fade_from_alpha = self.context_menu.current_alpha;
            self.context_menu.fade_started_at = Some(Instant::now());
            root_ctx.request_repaint();
        }
    }

    fn finish_close_context_menu(&mut self) {
        if self.context_menu.open {
            hide_popup_window(self.context_menu.hwnd);
            self.context_menu = ContextMenuState::default();
            trim_process_working_set();
        }
    }

    fn handle_widget_interactions(&mut self, ctx: &Context, _ui: &mut egui::Ui, card_rect: Rect) {
        // Right-click anywhere on the widget opens the standalone glass context menu window
        let right_clicked_pos = ctx.input(|i| {
            if i.pointer.button_clicked(PointerButton::Secondary) {
                i.pointer.interact_pos()
            } else {
                None
            }
        });

        if let Some(pos) = right_clicked_pos {
            self.open_context_menu(ctx, pos);
            return;
        }

        if self.context_menu.open {
            // Dismiss context menu on Escape or primary click on the root widget (without starting a drag)
            if !self.context_menu.closing {
                let dismiss_from_widget = ctx.input(|i| {
                    i.key_pressed(Key::Escape) || i.pointer.button_pressed(PointerButton::Primary)
                });
                if dismiss_from_widget {
                    self.begin_close_context_menu(ctx);
                }
            }
        } else {
            // When context menu is not open, left-clicking and dragging anywhere moves the widget smoothly
            let start_drag = ctx.input(|i| {
                i.pointer.button_pressed(PointerButton::Primary)
                    && i.pointer
                        .interact_pos()
                        .map_or(false, |p| card_rect.contains(p))
            });
            if start_drag {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
        }
    }

    fn show_context_menu_viewport(&mut self, root_ctx: &Context) {
        let menu_size = Vec2::new(CONTEXT_MENU_WIDTH, CONTEXT_MENU_HEIGHT);
        let pos_px = self.context_menu.locked_pos_px;
        let size_px = self.context_menu.locked_size_px;
        let logical_pos = self.context_menu.locked_logical_pos;

        let viewport_id = ViewportId::from_hash_of("clock_widget_context_menu_viewport");
        let builder = ViewportBuilder::default()
            .with_title(CONTEXT_MENU_WINDOW_TITLE)
            .with_inner_size([menu_size.x, menu_size.y])
            .with_min_inner_size([menu_size.x, menu_size.y])
            .with_max_inner_size([menu_size.x, menu_size.y])
            .with_position(logical_pos)
            .with_visible(self.context_menu.revealed)
            .with_decorations(false)
            .with_minimize_button(false)
            .with_maximize_button(false)
            .with_close_button(false)
            .with_transparent(true)
            .with_taskbar(false)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop);

        root_ctx.show_viewport_immediate(viewport_id, builder, |menu_ctx, class| {
            if class == ViewportClass::Embedded {
                if !self.context_menu.revealed || !self.context_menu.activated {
                    self.context_menu.revealed = true;
                    self.context_menu.activated = true;
                    self.context_menu.fade_started_at = Some(Instant::now());
                    self.context_menu.fade_from_alpha = 0.0;
                }

                let bounds_rect = Rect::from_min_size(Pos2::ZERO, self.desired_window_size());
                let max_x = (bounds_rect.max.x - menu_size.x - 4.0).max(bounds_rect.min.x + 4.0);
                let max_y = (bounds_rect.max.y - menu_size.y - 4.0).max(bounds_rect.min.y + 4.0);
                let min_x = self
                    .context_menu
                    .embedded_pos
                    .x
                    .clamp(bounds_rect.min.x + 4.0, max_x);
                let min_y = self
                    .context_menu
                    .embedded_pos
                    .y
                    .clamp(bounds_rect.min.y + 4.0, max_y);
                let menu_rect = Rect::from_min_size(Pos2::new(min_x, min_y), menu_size);

                let elapsed_secs = self
                    .context_menu
                    .fade_started_at
                    .map_or(0.0, |t| t.elapsed().as_secs_f32());
                let (alpha, anim_done) = context_menu_fade_alpha(
                    self.context_menu.closing,
                    elapsed_secs,
                    self.context_menu.fade_from_alpha,
                );
                self.context_menu.current_alpha = alpha;

                if self.context_menu.closing && anim_done {
                    self.finish_close_context_menu();
                    root_ctx.request_repaint();
                    return;
                }

                if !self.context_menu.closing {
                    let close_outside = menu_ctx.input(|i| {
                        i.key_pressed(Key::Escape)
                            || (i.pointer.button_pressed(PointerButton::Primary)
                                && i.pointer
                                    .interact_pos()
                                    .map_or(false, |p| !menu_rect.contains(p)))
                    });
                    if close_outside {
                        self.begin_close_context_menu(root_ctx);
                    }
                }

                if !anim_done || self.context_menu.closing {
                    menu_ctx.request_repaint();
                    root_ctx.request_repaint();
                }

                egui::Area::new(Id::new("embedded_ctx_menu_area"))
                    .fixed_pos(menu_rect.min)
                    .order(egui::Order::Foreground)
                    .show(menu_ctx, |ui| {
                        let (rect, _) = ui.allocate_exact_size(menu_size, Sense::hover());
                        self.render_context_menu_surface(root_ctx, ui, rect, alpha);
                    });
                return;
            }

            // 1. Discover popup HWND owned by this process
            if self.context_menu.hwnd.is_none() {
                self.context_menu.hwnd = find_own_window_by_title(CONTEXT_MENU_WINDOW_TITLE);
                if self.context_menu.hwnd.is_none() {
                    root_ctx.request_repaint();
                    return;
                }
            }

            let glass_style = self.current_glass_style();

            // 2. Frame 1 (window still hidden): disable DWM transitions, set tool-window style,
            // sync Acrylic backdrop, and place at exact physical coordinates before ever showing.
            if !self.context_menu.revealed {
                configure_popup_window(self.context_menu.hwnd);
                self.context_menu.glass_backdrop.sync(
                    self.context_menu.hwnd,
                    BackgroundMode::FrostedGlass,
                    glass_style.is_light(),
                );
                position_popup_window_px(
                    self.context_menu.hwnd,
                    pos_px[0],
                    pos_px[1],
                    size_px[0],
                    size_px[1],
                    false,
                );

                egui::CentralPanel::default()
                    .frame(Frame::NONE.fill(Color32::TRANSPARENT))
                    .show(menu_ctx, |ui| {
                        let menu_rect = ui.max_rect();
                        self.render_context_menu_surface(root_ctx, ui, menu_rect, 0.0);
                    });

                self.context_menu.revealed = true;
                self.context_menu.needs_reposition = false;
                self.context_menu.current_alpha = 0.0;
                menu_ctx.request_repaint();
                root_ctx.request_repaint();
                return;
            }

            // 3. Frame 2+: keep DWM backdrop synced and reveal/reposition in place without SW_RESTORE
            self.context_menu.glass_backdrop.sync(
                self.context_menu.hwnd,
                BackgroundMode::FrostedGlass,
                glass_style.is_light(),
            );

            if !self.context_menu.activated || self.context_menu.needs_reposition {
                configure_popup_window(self.context_menu.hwnd);
                position_popup_window_px(
                    self.context_menu.hwnd,
                    pos_px[0],
                    pos_px[1],
                    size_px[0],
                    size_px[1],
                    true,
                );
                self.context_menu.glass_backdrop.invalidate();
                self.context_menu.glass_backdrop.sync(
                    self.context_menu.hwnd,
                    BackgroundMode::FrostedGlass,
                    glass_style.is_light(),
                );
                if !self.context_menu.activated {
                    self.context_menu.activated = true;
                    self.context_menu.opened_at = Some(Instant::now());
                    self.context_menu.fade_started_at = Some(Instant::now());
                    self.context_menu.fade_from_alpha = 0.0;
                }
                self.context_menu.needs_reposition = false;
            }

            // 4. Advance pure fade-in / fade-out curve
            let elapsed_secs = self
                .context_menu
                .fade_started_at
                .map_or(0.0, |t| t.elapsed().as_secs_f32());
            let (alpha, anim_done) = context_menu_fade_alpha(
                self.context_menu.closing,
                elapsed_secs,
                self.context_menu.fade_from_alpha,
            );
            self.context_menu.current_alpha = alpha;

            if self.context_menu.closing && anim_done {
                self.finish_close_context_menu();
                root_ctx.request_repaint();
                return;
            }

            // 5. Track focus and begin fade-out on close request, Escape, or focus loss
            if !self.context_menu.closing {
                let (close_req, esc_pressed, focused) = menu_ctx.input(|i| {
                    (
                        i.viewport().close_requested(),
                        i.key_pressed(Key::Escape),
                        i.viewport().focused,
                    )
                });
                if focused == Some(true) {
                    self.context_menu.had_focus = true;
                }
                let grace_elapsed = self
                    .context_menu
                    .opened_at
                    .map_or(false, |t| t.elapsed() >= Duration::from_millis(250));
                let lost_focus =
                    focused == Some(false) && (self.context_menu.had_focus || grace_elapsed);

                if close_req || esc_pressed || lost_focus {
                    self.begin_close_context_menu(root_ctx);
                }
            }

            if !anim_done || self.context_menu.closing {
                menu_ctx.request_repaint();
                root_ctx.request_repaint();
            }

            egui::CentralPanel::default()
                .frame(Frame::NONE.fill(Color32::TRANSPARENT))
                .show(menu_ctx, |ui| {
                    let menu_rect = ui.max_rect();
                    self.render_context_menu_surface(root_ctx, ui, menu_rect, alpha);
                });
        });
    }

    fn render_context_menu_surface(
        &mut self,
        root_ctx: &Context,
        ui: &mut egui::Ui,
        menu_rect: Rect,
        fade_alpha: f32,
    ) {
        let fade_alpha = fade_alpha.clamp(0.0, 1.0);
        let glass_style = self.current_glass_style();
        let palette = glass_style.palette();
        let painter = ui.painter();

        // Reusable frosted glass panel matching widget tint & Windows DWM Acrylic backdrop
        draw_glass_surface_with_alpha(painter, menu_rect, &glass_style, fade_alpha);

        let inner = menu_rect.shrink(6.0);
        let row_h = 23.0;
        let interactive = !self.context_menu.closing;

        let aot_shortcut = self.hotkey_controller.active_shortcut_str.clone();

        let rows: [(&str, bool, Option<&str>, bool); 6] = [
            ("Compact Mode", self.config.compact_mode, None, false),
            (
                "Transparent Mode",
                !self.config.compact_mode
                    && self.config.background_mode == BackgroundMode::Transparent,
                None,
                false,
            ),
            ("Show Seconds", self.config.show_seconds, None, false),
            (
                "Always on Top",
                self.config.always_on_top,
                Some(&aot_shortcut),
                false,
            ),
            ("Options...", false, None, false),
            ("Close", false, None, true),
        ];

        let mut cur_y = inner.min.y;
        for (idx, (label, is_checked, right_badge, is_danger)) in rows.iter().enumerate() {
            if idx == 4 || idx == 5 {
                let div_y = cur_y + 5.5;
                painter.line_segment(
                    [
                        Pos2::new(inner.min.x + 8.0, div_y),
                        Pos2::new(inner.max.x - 8.0, div_y),
                    ],
                    Stroke::new(1.0_f32, palette.divider.gamma_multiply(fade_alpha)),
                );
                cur_y += 11.0;
            }

            let item_rect =
                Rect::from_min_size(Pos2::new(inner.min.x, cur_y), Vec2::new(inner.width(), row_h));
            cur_y += if idx == 3 || idx == 4 {
                row_h
            } else {
                row_h + 2.0
            };

            let resp = ui.interact(
                item_rect,
                Id::new(("ctx_menu_row", idx)),
                Sense::click(),
            );
            let is_hovered = interactive && resp.hovered();

            if is_hovered {
                let hover_fill = if *is_danger {
                    palette.danger_hover_fill
                } else {
                    palette.item_hover_fill
                };
                painter.rect_filled(
                    item_rect,
                    CornerRadius::same(7),
                    hover_fill.gamma_multiply(fade_alpha),
                );
            }

            let text_color = if is_hovered {
                palette.item_hover_text
            } else if *is_danger {
                palette.danger_text
            } else {
                palette.primary_text
            }
            .gamma_multiply(fade_alpha);

            if *is_checked {
                let cx = item_rect.min.x + 13.0;
                let cy = item_rect.center().y;
                let stroke = Stroke::new(1.6_f32, text_color);
                painter.line_segment(
                    [Pos2::new(cx - 4.0, cy + 0.2), Pos2::new(cx - 1.2, cy + 3.0)],
                    stroke,
                );
                painter.line_segment(
                    [Pos2::new(cx - 1.2, cy + 3.0), Pos2::new(cx + 4.2, cy - 3.0)],
                    stroke,
                );
            }

            painter.text(
                Pos2::new(item_rect.min.x + 26.0, item_rect.center().y),
                Align2::LEFT_CENTER,
                *label,
                FontId::new(12.5, FontFamily::Proportional),
                text_color,
            );

            if let Some(badge) = right_badge {
                let badge_color = if is_hovered {
                    palette.item_hover_secondary_text
                } else {
                    palette.secondary_text
                }
                .gamma_multiply(fade_alpha);
                painter.text(
                    Pos2::new(item_rect.max.x - 8.0, item_rect.center().y),
                    Align2::RIGHT_CENTER,
                    *badge,
                    FontId::new(10.5, FontFamily::Proportional),
                    badge_color,
                );
            }

            if interactive && resp.clicked() {
                match idx {
                    0 => {
                        self.begin_close_context_menu(root_ctx);
                        let next = !self.config.compact_mode;
                        self.set_compact_mode(root_ctx, next);
                    }
                    1 => {
                        self.begin_close_context_menu(root_ctx);
                        let next = if self.config.compact_mode {
                            BackgroundMode::Transparent
                        } else {
                            self.config.background_mode.toggled()
                        };
                        self.set_background_mode(root_ctx, next);
                    }
                    2 => {
                        self.begin_close_context_menu(root_ctx);
                        self.config.show_seconds = !self.config.show_seconds;
                        self.measured_compact_size = None;
                        self.ensure_window_size_synced(root_ctx);
                        let _ = self.config.save();
                    }
                    3 => {
                        self.begin_close_context_menu(root_ctx);
                        self.toggle_always_on_top(root_ctx);
                    }
                    4 => {
                        self.begin_close_context_menu(root_ctx);
                        self.open_options_window(root_ctx);
                    }
                    5 => {
                        self.finish_close_context_menu();
                        self.persist_options_window_pos();
                        hide_popup_window(self.options_window.hwnd);
                        let _ = self.config.save();
                        root_ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
                    }
                    _ => {}
                }
                root_ctx.request_repaint();
            }
        }
    }

    fn open_options_window(&mut self, root_ctx: &Context) {
        if self.options_window.open {
            bring_hwnd_to_front(self.options_window.hwnd);
            return;
        }

        let work_rect = get_monitor_work_area_for_hwnd(self.hwnd, self.last_observed_pos_px);
        let ppp = root_ctx.pixels_per_point().max(1.0);
        let work_h_logical = ((work_rect[3] - work_rect[1]) as f32 / ppp).max(400.0);
        let logical_h = OPTIONS_WINDOW_HEIGHT
            .min((work_h_logical - 32.0).max(400.0))
            .round();
        let logical_size = Vec2::new(OPTIONS_WINDOW_WIDTH, logical_h);
        let size_px = [
            (logical_size.x * ppp).round() as i32,
            (logical_size.y * ppp).round() as i32,
        ];
        let saved_px = self
            .config
            .options_window_pos
            .map(|[x, y]| [x.round() as i32, y.round() as i32]);
        let pos_px = resolve_window_pos_in_work_area(saved_px, size_px, work_rect);
        let logical_pos = Pos2::new(pos_px[0] as f32 / ppp, pos_px[1] as f32 / ppp);

        self.options_window.reset_keeping_tab();
        self.options_window.open = true;
        self.options_window.logical_size = logical_size;
        self.options_window.locked_pos_px = pos_px;
        self.options_window.locked_size_px = size_px;
        self.options_window.locked_logical_pos = logical_pos;
        self.options_window.last_observed_pos_px = Some(pos_px);
        root_ctx.request_repaint();
    }

    fn persist_options_window_pos(&mut self) {
        if self.options_window.open && self.options_window.activated {
            if let Some(pos_px) = get_window_pos_px(self.options_window.hwnd)
                .or(self.options_window.last_observed_pos_px)
            {
                self.config.options_window_pos = Some([pos_px[0] as f32, pos_px[1] as f32]);
            }
        }
    }

    fn close_options_window(&mut self) {
        if !self.options_window.open {
            return;
        }
        self.persist_options_window_pos();
        let _ = self.config.save();
        hide_popup_window(self.options_window.hwnd);
        self.options_window.reset_keeping_tab();
        self.recording_hotkey = false;
        trim_process_working_set();
    }

    fn show_options_viewport(&mut self, root_ctx: &Context) {
        let logical_size = self.options_window.logical_size;
        let pos_px = self.options_window.locked_pos_px;
        let size_px = self.options_window.locked_size_px;

        let viewport_id = ViewportId::from_hash_of("clock_widget_options_viewport");
        let mut builder = ViewportBuilder::default()
            .with_title(OPTIONS_WINDOW_TITLE)
            .with_inner_size([logical_size.x, logical_size.y])
            .with_min_inner_size([logical_size.x, logical_size.y])
            .with_max_inner_size([logical_size.x, logical_size.y])
            .with_visible(self.options_window.revealed)
            .with_decorations(false)
            .with_minimize_button(false)
            .with_maximize_button(false)
            .with_close_button(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_icon(self.app_icon.clone());

        if !self.options_window.activated {
            builder = builder.with_position(self.options_window.locked_logical_pos);
        }

        root_ctx.show_viewport_immediate(viewport_id, builder, |opt_ctx, class| {
            let glass_style = self.current_glass_style();
            let palette = glass_style.palette();
            apply_glass_visuals(opt_ctx, &glass_style);

            if class == ViewportClass::Embedded {
                let mut open = self.options_window.open;
                let mut done_clicked = false;
                egui::Window::new(OPTIONS_WINDOW_TITLE)
                    .open(&mut open)
                    .show(opt_ctx, |ui| {
                        done_clicked = self.render_options_frame(
                            root_ctx,
                            opt_ctx,
                            ui,
                            &glass_style,
                            &palette,
                        );
                    });
                if !open || done_clicked {
                    self.close_options_window();
                    root_ctx.request_repaint();
                }
                return;
            }

            if self.options_window.hwnd.is_none() {
                self.options_window.hwnd = find_own_window_by_title(OPTIONS_WINDOW_TITLE);
                if self.options_window.hwnd.is_none() {
                    root_ctx.request_repaint();
                    return;
                }
            }

            // Frame 1 (hidden): suppress transitions, apply Acrylic glass backdrop, and place on target monitor.
            if !self.options_window.revealed {
                configure_settings_window(self.options_window.hwnd);
                self.options_window.glass_backdrop.sync(
                    self.options_window.hwnd,
                    BackgroundMode::FrostedGlass,
                    glass_style.is_light(),
                );
                position_popup_window_px(
                    self.options_window.hwnd,
                    pos_px[0],
                    pos_px[1],
                    size_px[0],
                    size_px[1],
                    false,
                );

                egui::CentralPanel::default()
                    .frame(Frame::NONE.fill(Color32::TRANSPARENT))
                    .show(opt_ctx, |ui| {
                        let _ = self.render_options_frame(
                            root_ctx,
                            opt_ctx,
                            ui,
                            &glass_style,
                            &palette,
                        );
                    });

                self.options_window.revealed = true;
                opt_ctx.request_repaint();
                root_ctx.request_repaint();
                return;
            }

            // Frame 2: reveal and focus cleanly without DWM transition flicker.
            if !self.options_window.activated {
                configure_settings_window(self.options_window.hwnd);
                position_popup_window_px(
                    self.options_window.hwnd,
                    pos_px[0],
                    pos_px[1],
                    size_px[0],
                    size_px[1],
                    true,
                );
                self.options_window.glass_backdrop.invalidate();
                self.options_window.glass_backdrop.sync(
                    self.options_window.hwnd,
                    BackgroundMode::FrostedGlass,
                    glass_style.is_light(),
                );
                self.options_window.activated = true;

                let actual = get_window_pos_px(self.options_window.hwnd).unwrap_or(pos_px);
                self.options_window.last_observed_pos_px = Some(actual);
                let actual_f = [actual[0] as f32, actual[1] as f32];
                if self.config.options_window_pos != Some(actual_f) {
                    self.config.options_window_pos = Some(actual_f);
                    let _ = self.config.save();
                }
            } else {
                self.options_window.glass_backdrop.sync(
                    self.options_window.hwnd,
                    BackgroundMode::FrostedGlass,
                    glass_style.is_light(),
                );
            }

            let (close_req, esc_pressed) = opt_ctx.input(|i| {
                (
                    i.viewport().close_requested(),
                    i.key_pressed(Key::Escape),
                )
            });
            let mut should_close = close_req
                || (esc_pressed && !self.recording_hotkey && !opt_ctx.wants_keyboard_input());

            // Track physical drag moves via Win32 GetWindowRect and debounce persistence
            if let Some(cur_px) = get_window_pos_px(self.options_window.hwnd) {
                let changed = match self.options_window.last_observed_pos_px {
                    Some(prev) => (prev[0] - cur_px[0]).abs() > 1 || (prev[1] - cur_px[1]).abs() > 1,
                    None => {
                        self.options_window.last_observed_pos_px = Some(cur_px);
                        false
                    }
                };
                if changed {
                    self.options_window.last_observed_pos_px = Some(cur_px);
                    self.options_window.pos_dirty_since = Some(Instant::now());
                }
            }
            if let Some(dirty_time) = self.options_window.pos_dirty_since {
                let is_dragging = opt_ctx.input(|i| i.pointer.primary_down());
                if !is_dragging && dirty_time.elapsed() >= Duration::from_millis(350) {
                    if let Some(cur_px) = self.options_window.last_observed_pos_px {
                        self.config.options_window_pos = Some([cur_px[0] as f32, cur_px[1] as f32]);
                        let _ = self.config.save();
                    }
                    self.options_window.pos_dirty_since = None;
                }
            }

            egui::CentralPanel::default()
                .frame(Frame::NONE.fill(Color32::TRANSPARENT))
                .show(opt_ctx, |ui| {
                    let frame_close = self.render_options_frame(
                        root_ctx,
                        opt_ctx,
                        ui,
                        &glass_style,
                        &palette,
                    );
                    should_close |= frame_close;
                });

            if should_close {
                self.close_options_window();
                root_ctx.request_repaint();
            }
        });
    }

    fn render_options_frame(
        &mut self,
        root_ctx: &Context,
        opt_ctx: &Context,
        ui: &mut egui::Ui,
        glass_style: &GlassStyle,
        palette: &GlassPalette,
    ) -> bool {
        let full_rect = ui.max_rect();
        draw_glass_surface(ui.painter(), full_rect, glass_style);

        ui.spacing_mut().item_spacing.y = 0.0;

        let close_top = draw_settings_titlebar(opt_ctx, ui, palette);
        ui.add_space(10.0);

        Frame::NONE.inner_margin(Margin::symmetric(16, 0)).show(ui, |ui| {
            draw_settings_tab_bar(ui, palette, &mut self.options_window.active_tab);
        });
        ui.add_space(10.0);

        let scroll_h = ui.available_height().max(120.0);
        ui.allocate_ui(Vec2::new(ui.available_width(), scroll_h), |ui| {
            ui.set_min_size(Vec2::new(ui.available_width(), scroll_h));
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 7.0;
                    Frame::NONE
                        .inner_margin(Margin {
                            left: 16,
                            right: 16,
                            top: 2,
                            bottom: 14,
                        })
                        .show(ui, |ui| {
                            self.render_options_contents(root_ctx, opt_ctx, ui, palette);
                        });
                });
        });

        close_top
    }

    fn render_options_contents(
        &mut self,
        root_ctx: &Context,
        opt_ctx: &Context,
        ui: &mut egui::Ui,
        palette: &GlassPalette,
    ) {
        if self.recording_hotkey {
            if let Some(captured) = capture_egui_shortcut(opt_ctx) {
                self.recording_hotkey = false;
                if captured != "ESCAPE_CANCEL" {
                    self.hotkey_input = captured.clone();
                    match self.hotkey_controller.set_shortcut(&captured) {
                        Ok(()) => {
                            self.config.toggle_aot_hotkey =
                                self.hotkey_controller.active_shortcut_str.clone();
                            self.hotkey_input = self.config.toggle_aot_hotkey.clone();
                            let _ = self.config.save();
                            self.hotkey_feedback = Some((
                                format!(
                                    "Registered global shortcut: {}",
                                    self.config.toggle_aot_hotkey
                                ),
                                true,
                            ));
                        }
                        Err(err) => {
                            self.hotkey_feedback = Some((err, false));
                        }
                    }
                }
            }
        }

        match self.options_window.active_tab {
            SettingsTab::Appearance => {
                glass_card(ui, palette, "Display & Glass", |ui| {
                    let mut compact = self.config.compact_mode;
                    if glass_toggle_row(
                        ui,
                        "opt_compact_mode",
                        palette,
                        "Compact Mode",
                        Some("Single-line clock & date that fits inside titlebars"),
                        &mut compact,
                    ) {
                        self.set_compact_mode(root_ctx, compact);
                        root_ctx.request_repaint();
                    }

                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Normal Mode Background")
                            .size(12.0)
                            .color(palette.secondary_text),
                    );
                    let current_bg = if self.config.compact_mode {
                        None
                    } else {
                        Some(self.config.background_mode)
                    };
                    if let Some(mode) = glass_segmented_control(
                        ui,
                        "opt_bg_mode",
                        palette,
                        &[
                            (BackgroundMode::FrostedGlass, BackgroundMode::FrostedGlass.label()),
                            (BackgroundMode::Transparent, BackgroundMode::Transparent.label()),
                        ],
                        current_bg,
                    ) {
                        self.set_background_mode(root_ctx, mode);
                        root_ctx.request_repaint();
                    }

                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Glass Theme")
                            .size(12.0)
                            .color(palette.secondary_text),
                    );
                    if let Some(preset) = glass_segmented_control(
                        ui,
                        "opt_glass_preset",
                        palette,
                        &[
                            (GlassTintPreset::Dark, GlassTintPreset::Dark.label()),
                            (GlassTintPreset::Light, GlassTintPreset::Light.label()),
                            (GlassTintPreset::System, GlassTintPreset::System.label()),
                        ],
                        Some(self.config.glass_tint_preset),
                    ) {
                        self.config.glass_tint_preset = preset;
                        self.ensure_dwm_synced();
                        let _ = self.config.save();
                        root_ctx.request_repaint();
                    }

                    ui.add_space(8.0);
                    let mut pct = (self.config.glass_tint_opacity * 100.0).round();
                    if glass_slider_row(
                        ui,
                        palette,
                        "Glass Tint Opacity",
                        &mut pct,
                        10.0..=95.0,
                        1.0,
                        "%",
                    ) {
                        self.config.glass_tint_opacity = (pct / 100.0).clamp(0.05, 0.95);
                        let _ = self.config.save();
                        root_ctx.request_repaint();
                    }

                    if self.config.compact_mode
                        || self.config.background_mode == BackgroundMode::Transparent
                    {
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(
                                "Theme & opacity also style the context menu and this settings window.",
                            )
                            .size(11.0)
                            .color(palette.muted_text),
                        );
                    }
                });

                ui.add_space(10.0);

                glass_card(ui, palette, "Clock & Typography", |ui| {
                    if glass_slider_row(
                        ui,
                        palette,
                        "Clock Text Size",
                        &mut self.config.clock_font_size,
                        10.0..=80.0,
                        1.0,
                        " px",
                    ) {
                        self.measured_compact_size = None;
                        self.ensure_window_size_synced(root_ctx);
                        let _ = self.config.save();
                        root_ctx.request_repaint();
                    }

                    ui.add_space(4.0);
                    if glass_slider_row(
                        ui,
                        palette,
                        "Date Text Size",
                        &mut self.config.date_font_size,
                        8.0..=48.0,
                        1.0,
                        " px",
                    ) {
                        self.measured_compact_size = None;
                        self.ensure_window_size_synced(root_ctx);
                        let _ = self.config.save();
                        root_ctx.request_repaint();
                    }

                    ui.add_space(4.0);
                    if glass_toggle_row(
                        ui,
                        "opt_24h",
                        palette,
                        "24-Hour Clock",
                        None,
                        &mut self.config.use_24_hour,
                    ) {
                        self.measured_compact_size = None;
                        self.ensure_window_size_synced(root_ctx);
                        let _ = self.config.save();
                        root_ctx.request_repaint();
                    }

                    if glass_toggle_row(
                        ui,
                        "opt_seconds",
                        palette,
                        "Show Seconds",
                        None,
                        &mut self.config.show_seconds,
                    ) {
                        self.measured_compact_size = None;
                        self.ensure_window_size_synced(root_ctx);
                        let _ = self.config.save();
                        root_ctx.request_repaint();
                    }
                });
            }

            SettingsTab::Weather => {
                glass_card(ui, palette, "Location & Units", |ui| {
                    ui.label(
                        RichText::new("City Lookup")
                            .size(12.0)
                            .color(palette.secondary_text),
                    );
                    ui.horizontal(|ui| {
                        let input_w = (ui.available_width() - 102.0).max(160.0);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.city_search_query)
                                .hint_text("e.g. Tokyo, London, New York...")
                                .desired_width(input_w),
                        );
                        let enter_pressed =
                            resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                        if (glass_primary_button(ui, palette, "Search").clicked() || enter_pressed)
                            && !self.city_search_query.trim().is_empty()
                        {
                            self.weather_manager.trigger_city_search(
                                self.city_search_query.clone(),
                                root_ctx.clone(),
                            );
                        }
                        if self.weather_manager.is_searching_city {
                            ui.spinner();
                        }
                    });

                    if let Some(ref err) = self.city_search_error {
                        ui.label(
                            RichText::new(err)
                                .size(11.5)
                                .color(palette.danger_text),
                        );
                    }

                    if !self.city_search_results.is_empty() {
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new("Select a matching location:")
                                .size(11.5)
                                .color(palette.muted_text),
                        );
                        let results = self.city_search_results.clone();
                        for (idx, loc) in results.into_iter().enumerate() {
                            if glass_location_result_row(
                                ui,
                                ("city_res", idx),
                                palette,
                                &loc.display_name,
                                loc.latitude,
                                loc.longitude,
                            ) {
                                self.config.location_name = loc.short_name.clone();
                                self.config.latitude = loc.latitude;
                                self.config.longitude = loc.longitude;
                                self.location_name_input = loc.short_name;
                                self.lat_input = format!("{:.4}", loc.latitude);
                                self.lon_input = format!("{:.4}", loc.longitude);
                                self.city_search_results.clear();
                                let _ = self.config.save();
                                self.request_weather_refresh(root_ctx);
                                root_ctx.request_repaint();
                            }
                        }
                    }

                    ui.add_space(6.0);
                    egui::Grid::new("loc_coords_grid")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("Display Label")
                                    .size(12.0)
                                    .color(palette.secondary_text),
                            );
                            ui.add(
                                egui::TextEdit::singleline(&mut self.location_name_input)
                                    .desired_width(236.0),
                            );
                            ui.end_row();

                            ui.label(
                                RichText::new("Coordinates")
                                    .size(12.0)
                                    .color(palette.secondary_text),
                            );
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.lat_input)
                                        .hint_text("Lat")
                                        .desired_width(80.0),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.lon_input)
                                        .hint_text("Lon")
                                        .desired_width(80.0),
                                );
                                if glass_button(ui, palette, "Apply").clicked() {
                                    if let (Ok(lat), Ok(lon)) = (
                                        self.lat_input.trim().parse::<f64>(),
                                        self.lon_input.trim().parse::<f64>(),
                                    ) {
                                        self.config.latitude = lat.clamp(-90.0, 90.0);
                                        self.config.longitude = lon.clamp(-180.0, 180.0);
                                        self.config.location_name =
                                            self.location_name_input.trim().to_string();
                                        let _ = self.config.save();
                                        self.request_weather_refresh(root_ctx);
                                        root_ctx.request_repaint();
                                    }
                                }
                            });
                            ui.end_row();
                        });

                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Temperature Unit")
                            .size(12.0)
                            .color(palette.secondary_text),
                    );
                    if let Some(unit) = glass_segmented_control(
                        ui,
                        "opt_temp_unit",
                        palette,
                        &[
                            (TempUnit::Celsius, "Celsius (°C)"),
                            (TempUnit::Fahrenheit, "Fahrenheit (°F)"),
                        ],
                        Some(self.config.temp_unit),
                    ) {
                        self.config.temp_unit = unit;
                        let _ = self.config.save();
                        root_ctx.request_repaint();
                    }
                });

                ui.add_space(10.0);

                glass_card(ui, palette, "Weather Provider", |ui| {
                    ui.label(
                        RichText::new("Data Source")
                            .size(12.0)
                            .color(palette.secondary_text),
                    );
                    if let Some(provider) = glass_segmented_control(
                        ui,
                        "opt_weather_provider",
                        palette,
                        &[
                            (WeatherProvider::GoogleMapsWeather, "Google Weather"),
                            (WeatherProvider::OpenMeteo, WeatherProvider::OpenMeteo.label()),
                        ],
                        Some(self.config.weather_provider),
                    ) {
                        self.config.weather_provider = provider;
                        let _ = self.config.save();
                        self.request_weather_refresh(root_ctx);
                        root_ctx.request_repaint();
                    }

                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Google Maps Weather API Key (Optional)")
                            .size(12.0)
                            .color(palette.secondary_text),
                    );
                    ui.horizontal(|ui| {
                        let key_w = (ui.available_width() - 134.0).max(150.0);
                        let key_resp = ui.add(
                            egui::TextEdit::singleline(&mut self.config.google_api_key)
                                .password(!self.show_api_key)
                                .hint_text("Blank = keyless Open-Meteo fallback")
                                .desired_width(key_w),
                        );
                        if key_resp.changed() {
                            let _ = self.config.save();
                        }
                        let eye_label = if self.show_api_key { "Hide" } else { "Show" };
                        if glass_button(ui, palette, eye_label).clicked() {
                            self.show_api_key = !self.show_api_key;
                        }
                        if glass_button(ui, palette, "Refresh").clicked() {
                            let _ = self.config.save();
                            self.request_weather_refresh(root_ctx);
                        }
                    });

                    if let Some(ref w) = self.weather {
                        ui.add_space(2.0);
                        let dot_color = if w.is_offline_cache {
                            palette.accent_text
                        } else {
                            palette.success_text
                        };
                        let cache_suffix = if w.is_offline_cache { " (Cached)" } else { "" };
                        let status_text = format!(
                            "Active: {}{}  •  {}",
                            w.provider_used,
                            cache_suffix,
                            self.config.temp_unit.format_temp_with_unit(w.temperature_c)
                        );
                        draw_status_pill(ui, palette, dot_color, &status_text);
                    }
                });
            }

            SettingsTab::System => {
                glass_card(ui, palette, "Always on Top & Global Hotkey", |ui| {
                    let mut aot = self.config.always_on_top;
                    if glass_toggle_row(
                        ui,
                        "opt_aot",
                        palette,
                        "Pin Widget Always on Top",
                        Some("Keeps the clock widget above other windows"),
                        &mut aot,
                    ) {
                        self.set_always_on_top(root_ctx, aot);
                        root_ctx.request_repaint();
                    }

                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Global Toggle Shortcut")
                            .size(12.0)
                            .color(palette.secondary_text),
                    );
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.hotkey_input)
                                .desired_width(142.0)
                                .hint_text("e.g. Alt+Shift+T"),
                        );

                        if glass_button(ui, palette, "Apply").clicked() {
                            let candidate = self.hotkey_input.trim().to_string();
                            match self.hotkey_controller.set_shortcut(&candidate) {
                                Ok(()) => {
                                    self.config.toggle_aot_hotkey =
                                        self.hotkey_controller.active_shortcut_str.clone();
                                    self.hotkey_input = self.config.toggle_aot_hotkey.clone();
                                    let _ = self.config.save();
                                    self.hotkey_feedback = Some((
                                        format!("Registered: {}", self.config.toggle_aot_hotkey),
                                        true,
                                    ));
                                }
                                Err(err) => {
                                    self.hotkey_feedback = Some((err, false));
                                }
                            }
                        }

                        let rec_clicked = if self.recording_hotkey {
                            glass_recording_button(ui, palette, "Press Keys... (Esc)").clicked()
                        } else {
                            glass_button(ui, palette, "Record").clicked()
                        };
                        if rec_clicked {
                            self.recording_hotkey = !self.recording_hotkey;
                        }
                    });

                    if let Some((ref msg, ok)) = self.hotkey_feedback {
                        ui.add_space(2.0);
                        let dot_color = if ok {
                            palette.success_text
                        } else {
                            palette.danger_text
                        };
                        draw_status_pill(ui, palette, dot_color, msg);
                    }
                });

                ui.add_space(10.0);

                glass_card(ui, palette, "System Tray & Window", |ui| {
                    let mut show_tray = self.config.show_tray_icon;
                    if glass_toggle_row(
                        ui,
                        "opt_tray",
                        palette,
                        "Show System Tray Icon",
                        Some("Display quick controls in the Windows notification area"),
                        &mut show_tray,
                    ) {
                        self.config.show_tray_icon = show_tray;
                        self.tray_controller
                            .set_enabled(show_tray, self.config.always_on_top);
                        let _ = self.config.save();
                    }

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.label(
                                RichText::new("Widget Position")
                                    .size(12.5)
                                    .color(palette.primary_text),
                            );
                            ui.label(
                                RichText::new("Move the clock back to the primary monitor center")
                                    .size(11.0)
                                    .color(palette.muted_text),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            if glass_button(ui, palette, "Center Widget").clicked() {
                                self.reset_position_to_center(root_ctx);
                                root_ctx.request_repaint();
                            }
                        });
                    });
                });
            }
        }
    }
}

/// Computes the pure fade-in / fade-out alpha scale and completion flag for the right-click context menu.
fn context_menu_fade_alpha(is_closing: bool, elapsed_secs: f32, start_alpha: f32) -> (f32, bool) {
    if is_closing {
        let t = (elapsed_secs / CONTEXT_MENU_FADE_OUT_SECS).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t).powi(2);
        let alpha = (start_alpha * (1.0 - eased)).clamp(0.0, 1.0);
        (alpha, t >= 1.0)
    } else {
        let t = (elapsed_secs / CONTEXT_MENU_FADE_IN_SECS).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        let alpha = (start_alpha + (1.0 - start_alpha) * eased).clamp(0.0, 1.0);
        (alpha, t >= 1.0)
    }
}

/// Renders crisp text with an optional multi-pass soft drop shadow for legibility in Clear and Glass modes.
fn draw_shadowed_text(
    painter: &Painter,
    pos: Pos2,
    anchor: Align2,
    text: &str,
    font: FontId,
    color: Color32,
    shadow: bool,
) {
    if shadow {
        painter.text(
            pos + Vec2::new(0.0, 2.0),
            anchor,
            text,
            font.clone(),
            Color32::from_black_alpha(115),
        );
        painter.text(
            pos + Vec2::new(0.8, 1.0),
            anchor,
            text,
            font.clone(),
            Color32::from_black_alpha(80),
        );
    }
    painter.text(pos, anchor, text, font, color);
}

fn truncate_str(input: &str, max_chars: usize) -> String {
    let count = input.chars().count();
    if count <= max_chars {
        input.to_string()
    } else {
        let mut s: String = input.chars().take(max_chars.saturating_sub(1)).collect();
        s.push('…');
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::{normalize_hotkey_label, parse_hotkey_string};

    #[test]
    fn test_temp_unit_conversion() {
        assert_eq!(TempUnit::Celsius.format_temp(20.0), "20°");
        assert_eq!(TempUnit::Fahrenheit.format_temp(20.0), "68°");
        assert_eq!(TempUnit::Fahrenheit.format_temp_with_unit(0.0), "32°F");
    }

    #[test]
    fn test_hotkey_parsing_and_normalization() {
        assert!(parse_hotkey_string("Alt+Shift+T").is_ok());
        assert!(parse_hotkey_string("ctrl + alt + space").is_ok());
        assert!(parse_hotkey_string("F9").is_ok());
        assert!(parse_hotkey_string("Ctrl+Shift").is_err());
        assert_eq!(normalize_hotkey_label("alt + shift + t"), "Alt+Shift+T");
    }

    #[test]
    fn test_config_roundtrip() {
        let mut cfg = WidgetConfig::default();
        cfg.background_mode = BackgroundMode::Transparent;
        cfg.always_on_top = true;
        cfg.window_pos = Some([420.0, 280.0]);
        cfg.options_window_pos = Some([1980.0, 210.0]);
        let json = serde_json::to_string(&cfg).expect("serialize config");
        let parsed: WidgetConfig = serde_json::from_str(&json).expect("deserialize config");
        assert_eq!(parsed.background_mode, BackgroundMode::Transparent);
        assert!(parsed.always_on_top);
        assert_eq!(parsed.window_pos, Some([420.0, 280.0]));
        assert_eq!(parsed.options_window_pos, Some([1980.0, 210.0]));

        // Legacy config JSON missing `options_window_pos` deserializes cleanly
        let legacy_json = r#"{"window_pos":[100.0,120.0],"always_on_top":false}"#;
        let legacy: WidgetConfig =
            serde_json::from_str(legacy_json).expect("deserialize legacy config");
        assert_eq!(legacy.window_pos, Some([100.0, 120.0]));
        assert_eq!(legacy.options_window_pos, None);
    }

    #[test]
    fn test_tray_icon_generation() {
        let (rgba, w, h) = tray::generate_icon_rgba();
        assert_eq!(w, 32);
        assert_eq!(h, 32);
        assert_eq!(rgba.len(), 32 * 32 * 4);
        assert!(tray::generate_widget_tray_icon().is_ok());
    }

    #[test]
    fn test_context_menu_fade_alpha() {
        let (start_in, done_start) = context_menu_fade_alpha(false, 0.0, 0.0);
        assert!((start_in - 0.0).abs() < 1e-6);
        assert!(!done_start);

        let (mid_in, done_mid) =
            context_menu_fade_alpha(false, CONTEXT_MENU_FADE_IN_SECS * 0.5, 0.0);
        assert!(mid_in > 0.5 && mid_in < 1.0);
        assert!(!done_mid);

        let (end_in, done_in) = context_menu_fade_alpha(false, CONTEXT_MENU_FADE_IN_SECS, 0.0);
        assert!((end_in - 1.0).abs() < 1e-6);
        assert!(done_in);

        let (start_out, done_out_start) = context_menu_fade_alpha(true, 0.0, 1.0);
        assert!((start_out - 1.0).abs() < 1e-6);
        assert!(!done_out_start);

        let (end_out, done_out) = context_menu_fade_alpha(true, CONTEXT_MENU_FADE_OUT_SECS, 1.0);
        assert!((end_out - 0.0).abs() < 1e-6);
        assert!(done_out);

        // Mid-fade reversal from partial alpha (0.6 -> 0.0)
        let (rev_start, rev_done_start) = context_menu_fade_alpha(true, 0.0, 0.6);
        assert!((rev_start - 0.6).abs() < 1e-6);
        assert!(!rev_done_start);

        let (rev_mid, _) = context_menu_fade_alpha(true, CONTEXT_MENU_FADE_OUT_SECS * 0.5, 0.6);
        assert!(rev_mid > 0.0 && rev_mid < 0.6);

        let (rev_end, rev_done_end) =
            context_menu_fade_alpha(true, CONTEXT_MENU_FADE_OUT_SECS, 0.6);
        assert!((rev_end - 0.0).abs() < 1e-6);
        assert!(rev_done_end);
    }
}
