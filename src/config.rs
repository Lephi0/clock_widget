use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::weather::WeatherData;

const CONFIG_FILE_NAME: &str = "clock_widget_config.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackgroundMode {
    FrostedGlass,
    Transparent,
}

impl BackgroundMode {
    pub fn label(self) -> &'static str {
        match self {
            BackgroundMode::FrostedGlass => "Frosted Glass",
            BackgroundMode::Transparent => "Transparent Mode",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            BackgroundMode::FrostedGlass => BackgroundMode::Transparent,
            BackgroundMode::Transparent => BackgroundMode::FrostedGlass,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GlassTintPreset {
    #[serde(alias = "MidnightDark", alias = "CharcoalSlate", alias = "OceanBlue")]
    Dark,
    #[serde(alias = "FrostLight", alias = "Light")]
    Light,
    #[serde(alias = "FollowWindows", alias = "Auto")]
    System,
}

impl GlassTintPreset {
    pub fn label(self) -> &'static str {
        match self {
            GlassTintPreset::Dark => "Dark",
            GlassTintPreset::Light => "Light",
            GlassTintPreset::System => "Follow System",
        }
    }

    pub fn is_light_theme(self, windows_is_light: bool) -> bool {
        match self {
            GlassTintPreset::Dark => false,
            GlassTintPreset::Light => true,
            GlassTintPreset::System => windows_is_light,
        }
    }

    pub fn base_rgb(self, windows_is_light: bool) -> [u8; 3] {
        if self.is_light_theme(windows_is_light) {
            [240, 244, 250]
        } else {
            [20, 22, 28]
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TempUnit {
    Celsius,
    Fahrenheit,
}

impl TempUnit {
    pub fn symbol(self) -> &'static str {
        match self {
            TempUnit::Celsius => "°C",
            TempUnit::Fahrenheit => "°F",
        }
    }

    pub fn format_temp(self, temp_c: f64) -> String {
        let val = match self {
            TempUnit::Celsius => temp_c,
            TempUnit::Fahrenheit => temp_c * 9.0 / 5.0 + 32.0,
        };
        format!("{:.0}°", val.round())
    }

    pub fn format_temp_with_unit(self, temp_c: f64) -> String {
        let val = match self {
            TempUnit::Celsius => temp_c,
            TempUnit::Fahrenheit => temp_c * 9.0 / 5.0 + 32.0,
        };
        format!("{:.0}{}", val.round(), self.symbol())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WeatherProvider {
    GoogleMapsWeather,
    OpenMeteo,
}

impl WeatherProvider {
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            WeatherProvider::GoogleMapsWeather => "Google Maps Platform Weather API",
            WeatherProvider::OpenMeteo => "Open-Meteo (Keyless)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetConfig {
    pub window_pos: Option<[f32; 2]>,
    pub options_window_pos: Option<[f32; 2]>,
    pub always_on_top: bool,
    pub toggle_aot_hotkey: String,
    pub background_mode: BackgroundMode,
    pub compact_mode: bool,
    pub clock_font_size: f32,
    pub date_font_size: f32,
    pub glass_tint_opacity: f32,
    pub glass_tint_preset: GlassTintPreset,
    pub show_tray_icon: bool,
    pub show_seconds: bool,
    pub use_24_hour: bool,
    pub location_name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub temp_unit: TempUnit,
    pub weather_provider: WeatherProvider,
    pub google_api_key: String,
    pub cached_weather: Option<WeatherData>,
}

impl Default for WidgetConfig {
    fn default() -> Self {
        Self {
            window_pos: None,
            options_window_pos: None,
            always_on_top: false,
            toggle_aot_hotkey: "Alt+Shift+T".to_string(),
            background_mode: BackgroundMode::FrostedGlass,
            compact_mode: false,
            clock_font_size: 48.0,
            date_font_size: 13.0,
            glass_tint_opacity: 0.52,
            glass_tint_preset: GlassTintPreset::System,
            show_tray_icon: true,
            show_seconds: false,
            use_24_hour: false,
            location_name: "Cupertino".to_string(),
            latitude: 37.3230,
            longitude: -122.0322,
            temp_unit: TempUnit::Celsius,
            weather_provider: WeatherProvider::GoogleMapsWeather,
            google_api_key: String::new(),
            cached_weather: None,
        }
    }
}

impl WidgetConfig {
    pub fn config_path() -> PathBuf {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(dir) = exe_path.parent() {
                return dir.join(CONFIG_FILE_NAME);
            }
        }
        PathBuf::from(CONFIG_FILE_NAME)
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(contents) = fs::read_to_string(&path) {
            if let Ok(mut cfg) = serde_json::from_str::<WidgetConfig>(&contents) {
                cfg.glass_tint_opacity = cfg.glass_tint_opacity.clamp(0.05, 0.98);
                cfg.clock_font_size = cfg.clock_font_size.clamp(10.0, 80.0);
                cfg.date_font_size = cfg.date_font_size.clamp(8.0, 48.0);
                return cfg;
            }
        }
        let default_cfg = Self::default();
        let _ = default_cfg.save();
        default_cfg
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }
}
