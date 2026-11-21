use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use crate::config::WeatherProvider;

pub const WEATHER_REFRESH_INTERVAL: Duration = Duration::from_secs(20 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WeatherConditionKind {
    ClearDay,
    ClearNight,
    PartlyCloudyDay,
    PartlyCloudyNight,
    Cloudy,
    Fog,
    Drizzle,
    Rain,
    Thunderstorm,
    Snow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherData {
    pub temperature_c: f64,
    pub high_c: f64,
    pub low_c: f64,
    pub condition_label: String,
    pub condition_kind: WeatherConditionKind,
    pub humidity: Option<u8>,
    pub wind_kph: Option<f64>,
    pub provider_used: String,
    pub updated_at_epoch: i64,
    pub location_label: String,
    #[serde(default)]
    pub is_offline_cache: bool,
}

#[derive(Debug, Clone)]
pub struct GeoLocationResult {
    pub display_name: String,
    pub short_name: String,
    pub latitude: f64,
    pub longitude: f64,
}

pub enum WeatherWorkerMessage {
    WeatherFetched(Result<WeatherData, String>),
    CitySearchResults(Result<Vec<GeoLocationResult>, String>),
}

#[derive(Debug, Clone)]
pub struct WeatherFetchRequest {
    pub latitude: f64,
    pub longitude: f64,
    pub location_name: String,
    pub provider: WeatherProvider,
    pub google_api_key: String,
}

pub struct WeatherManager {
    tx: Sender<WeatherWorkerMessage>,
    rx: Receiver<WeatherWorkerMessage>,
    pub is_fetching: bool,
    pub is_searching_city: bool,
    pub last_error: Option<String>,
}

impl WeatherManager {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            is_fetching: false,
            is_searching_city: false,
            last_error: None,
        }
    }

    pub fn trigger_fetch(&mut self, req: WeatherFetchRequest, ctx: egui::Context) {
        if self.is_fetching {
            return;
        }
        self.is_fetching = true;
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = fetch_weather_blocking(&req);
            let _ = tx.send(WeatherWorkerMessage::WeatherFetched(result));
            ctx.request_repaint();
            egui_glass_ui::dwm::trim_process_working_set();
        });
    }

    pub fn trigger_city_search(&mut self, query: String, ctx: egui::Context) {
        let trimmed = query.trim().to_string();
        if trimmed.is_empty() || self.is_searching_city {
            return;
        }
        self.is_searching_city = true;
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = search_city_blocking(&trimmed);
            let _ = tx.send(WeatherWorkerMessage::CitySearchResults(result));
            ctx.request_repaint();
            egui_glass_ui::dwm::trim_process_working_set();
        });
    }

    pub fn poll(&mut self) -> Vec<WeatherWorkerMessage> {
        let mut msgs = Vec::new();
        while let Ok(msg) = self.rx.try_recv() {
            match &msg {
                WeatherWorkerMessage::WeatherFetched(res) => {
                    self.is_fetching = false;
                    if let Err(err) = res {
                        self.last_error = Some(err.clone());
                    } else {
                        self.last_error = None;
                    }
                }
                WeatherWorkerMessage::CitySearchResults(_) => {
                    self.is_searching_city = false;
                }
            }
            msgs.push(msg);
        }
        msgs
    }
}

fn fetch_weather_blocking(req: &WeatherFetchRequest) -> Result<WeatherData, String> {
    let key = req.google_api_key.trim();
    match req.provider {
        WeatherProvider::GoogleMapsWeather => {
            if !key.is_empty() {
                match fetch_google_weather(req.latitude, req.longitude, &req.location_name, key) {
                    Ok(data) => return Ok(data),
                    Err(google_err) => {
                        // Fallback to Open-Meteo if Google API fails, noting fallback in provider_used
                        if let Ok(mut fallback) =
                            fetch_open_meteo(req.latitude, req.longitude, &req.location_name)
                        {
                            fallback.provider_used =
                                "Open-Meteo (Google fallback)".to_string();
                            return Ok(fallback);
                        }
                        return Err(format!("Google Weather error: {google_err}"));
                    }
                }
            } else {
                // Keyless Open-Meteo fallback when no API key is set
                let mut data = fetch_open_meteo(req.latitude, req.longitude, &req.location_name)?;
                data.provider_used = "Open-Meteo (Keyless Fallback)".to_string();
                Ok(data)
            }
        }
        WeatherProvider::OpenMeteo => {
            fetch_open_meteo(req.latitude, req.longitude, &req.location_name)
        }
    }
}

fn fetch_google_weather(
    lat: f64,
    lon: f64,
    location_name: &str,
    api_key: &str,
) -> Result<WeatherData, String> {
    let current_url = format!(
        "https://weather.googleapis.com/v1/currentConditions:lookup?key={}&location.latitude={:.4}&location.longitude={:.4}&unitsSystem=METRIC",
        url_encode(api_key),
        lat,
        lon
    );

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(8))
        .build();

    let resp: serde_json::Value = agent
        .get(&current_url)
        .call()
        .map_err(|e| format!("HTTP request failed: {e}"))?
        .into_json()
        .map_err(|e| format!("JSON parse failed: {e}"))?;

    let temp_c = resp
        .pointer("/temperature/degrees")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "Missing temperature.degrees in Google Weather response".to_string())?;

    let is_day = resp
        .pointer("/isDaytime")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let cond_type = resp
        .pointer("/weatherCondition/type")
        .and_then(|v| v.as_str())
        .unwrap_or("CLEAR");

    let cond_text = resp
        .pointer("/weatherCondition/description/text")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format_google_condition_label(cond_type));

    let condition_kind = map_google_condition_kind(cond_type, is_day);

    // High / Low from currentConditionsHistory or forecast endpoint
    let mut high_c = resp
        .pointer("/currentConditionsHistory/maxTemperature/degrees")
        .and_then(|v| v.as_f64())
        .unwrap_or(temp_c + 2.0);
    let mut low_c = resp
        .pointer("/currentConditionsHistory/minTemperature/degrees")
        .and_then(|v| v.as_f64())
        .unwrap_or(temp_c - 2.0);

    if resp
        .pointer("/currentConditionsHistory/maxTemperature/degrees")
        .is_none()
    {
        let forecast_url = format!(
            "https://weather.googleapis.com/v1/forecast/days:lookup?key={}&location.latitude={:.4}&location.longitude={:.4}&days=1&unitsSystem=METRIC",
            url_encode(api_key),
            lat,
            lon
        );
        if let Ok(f_resp) = agent.get(&forecast_url).call() {
            if let Ok(f_json) = f_resp.into_json::<serde_json::Value>() {
                if let Some(h) = f_json
                    .pointer("/forecastDays/0/maxTemperature/degrees")
                    .and_then(|v| v.as_f64())
                {
                    high_c = h;
                }
                if let Some(l) = f_json
                    .pointer("/forecastDays/0/minTemperature/degrees")
                    .and_then(|v| v.as_f64())
                {
                    low_c = l;
                }
            }
        }
    }

    let humidity = resp
        .pointer("/relativeHumidity")
        .and_then(|v| v.as_u64())
        .map(|v| v.min(100) as u8);

    let wind_kph = resp
        .pointer("/wind/speed/value")
        .and_then(|v| v.as_f64());

    Ok(WeatherData {
        temperature_c: temp_c,
        high_c: high_c.max(temp_c),
        low_c: low_c.min(temp_c),
        condition_label: cond_text,
        condition_kind,
        humidity,
        wind_kph,
        provider_used: "Google Maps Weather".to_string(),
        updated_at_epoch: Utc::now().timestamp(),
        location_label: location_name.to_string(),
        is_offline_cache: false,
    })
}

fn fetch_open_meteo(lat: f64, lon: f64, location_name: &str) -> Result<WeatherData, String> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={:.4}&longitude={:.4}&current=temperature_2m,relative_humidity_2m,is_day,weather_code,wind_speed_10m&daily=temperature_2m_max,temperature_2m_min&timezone=auto&forecast_days=1",
        lat, lon
    );

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(8))
        .build();

    let resp: serde_json::Value = agent
        .get(&url)
        .call()
        .map_err(|e| format!("Open-Meteo HTTP error: {e}"))?
        .into_json()
        .map_err(|e| format!("Open-Meteo JSON error: {e}"))?;

    let temp_c = resp
        .pointer("/current/temperature_2m")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "Missing current.temperature_2m in Open-Meteo response".to_string())?;

    let is_day = resp
        .pointer("/current/is_day")
        .and_then(|v| v.as_i64())
        .unwrap_or(1)
        != 0;

    let code = resp
        .pointer("/current/weather_code")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let high_c = resp
        .pointer("/daily/temperature_2m_max/0")
        .and_then(|v| v.as_f64())
        .unwrap_or(temp_c);

    let low_c = resp
        .pointer("/daily/temperature_2m_min/0")
        .and_then(|v| v.as_f64())
        .unwrap_or(temp_c);

    let humidity = resp
        .pointer("/current/relative_humidity_2m")
        .and_then(|v| v.as_u64())
        .map(|v| v.min(100) as u8);

    let wind_kph = resp
        .pointer("/current/wind_speed_10m")
        .and_then(|v| v.as_f64());

    let (condition_kind, condition_label) = map_wmo_code(code, is_day);

    Ok(WeatherData {
        temperature_c: temp_c,
        high_c: high_c.max(temp_c),
        low_c: low_c.min(temp_c),
        condition_label: condition_label.to_string(),
        condition_kind,
        humidity,
        wind_kph,
        provider_used: "Open-Meteo".to_string(),
        updated_at_epoch: Utc::now().timestamp(),
        location_label: location_name.to_string(),
        is_offline_cache: false,
    })
}

fn search_city_blocking(query: &str) -> Result<Vec<GeoLocationResult>, String> {
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=5&language=en&format=json",
        url_encode(query)
    );

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(8))
        .build();

    let resp: serde_json::Value = agent
        .get(&url)
        .call()
        .map_err(|e| format!("Geocoding request failed: {e}"))?
        .into_json()
        .map_err(|e| format!("Geocoding parse failed: {e}"))?;

    let mut out = Vec::new();
    if let Some(results) = resp.get("results").and_then(|v| v.as_array()) {
        for item in results {
            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() {
                continue;
            }
            let admin1 = item.get("admin1").and_then(|v| v.as_str()).unwrap_or("");
            let country = item.get("country").and_then(|v| v.as_str()).unwrap_or("");
            let lat = item.get("latitude").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let lon = item.get("longitude").and_then(|v| v.as_f64()).unwrap_or(0.0);

            let display_name = match (admin1.is_empty(), country.is_empty()) {
                (false, false) => format!("{name}, {admin1}, {country}"),
                (true, false) => format!("{name}, {country}"),
                (false, true) => format!("{name}, {admin1}"),
                (true, true) => name.to_string(),
            };

            out.push(GeoLocationResult {
                display_name,
                short_name: name.to_string(),
                latitude: lat,
                longitude: lon,
            });
        }
    }

    if out.is_empty() {
        return Err(format!("No locations found matching \"{query}\""));
    }

    Ok(out)
}

fn map_wmo_code(code: i64, is_day: bool) -> (WeatherConditionKind, &'static str) {
    match code {
        0 => (
            if is_day {
                WeatherConditionKind::ClearDay
            } else {
                WeatherConditionKind::ClearNight
            },
            if is_day { "Sunny" } else { "Clear" },
        ),
        1 => (
            if is_day {
                WeatherConditionKind::ClearDay
            } else {
                WeatherConditionKind::ClearNight
            },
            if is_day { "Mostly Sunny" } else { "Mostly Clear" },
        ),
        2 => (
            if is_day {
                WeatherConditionKind::PartlyCloudyDay
            } else {
                WeatherConditionKind::PartlyCloudyNight
            },
            "Partly Cloudy",
        ),
        3 => (WeatherConditionKind::Cloudy, "Overcast"),
        45 | 48 => (WeatherConditionKind::Fog, "Foggy"),
        51 | 53 | 55 | 56 | 57 => (WeatherConditionKind::Drizzle, "Drizzle"),
        61 | 63 | 80 | 81 => (WeatherConditionKind::Rain, "Rain"),
        65 | 66 | 67 | 82 => (WeatherConditionKind::Rain, "Heavy Rain"),
        71 | 73 | 75 | 77 | 85 | 86 => (WeatherConditionKind::Snow, "Snow"),
        95 | 96 | 99 => (WeatherConditionKind::Thunderstorm, "Thunderstorm"),
        _ => (
            if is_day {
                WeatherConditionKind::PartlyCloudyDay
            } else {
                WeatherConditionKind::PartlyCloudyNight
            },
            "Fair",
        ),
    }
}

fn map_google_condition_kind(cond_type: &str, is_day: bool) -> WeatherConditionKind {
    let upper = cond_type.to_ascii_uppercase();
    if upper.contains("THUNDER") || upper.contains("STORM") {
        WeatherConditionKind::Thunderstorm
    } else if upper.contains("SNOW")
        || upper.contains("FLURR")
        || upper.contains("ICE")
        || upper.contains("SLEET")
        || upper.contains("BLIZZARD")
    {
        WeatherConditionKind::Snow
    } else if upper.contains("DRIZZLE") {
        WeatherConditionKind::Drizzle
    } else if upper.contains("RAIN") || upper.contains("SHOWER") {
        WeatherConditionKind::Rain
    } else if upper.contains("FOG") || upper.contains("HAZE") || upper.contains("MIST") {
        WeatherConditionKind::Fog
    } else if upper == "CLOUDY" || upper == "MOSTLY_CLOUDY" || upper == "OVERCAST" {
        WeatherConditionKind::Cloudy
    } else if upper.contains("PARTLY") || upper.contains("SCATTERED") {
        if is_day {
            WeatherConditionKind::PartlyCloudyDay
        } else {
            WeatherConditionKind::PartlyCloudyNight
        }
    } else if is_day {
        WeatherConditionKind::ClearDay
    } else {
        WeatherConditionKind::ClearNight
    }
}

fn format_google_condition_label(cond_type: &str) -> String {
    cond_type
        .split('_')
        .filter(|s| !s.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn url_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            b' ' => encoded.push_str("%20"),
            _ => encoded.push_str(&format!("%{:02X}", b)),
        }
    }
    encoded
}
