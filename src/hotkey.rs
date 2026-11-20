use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};

pub struct HotkeyController {
    manager: Option<GlobalHotKeyManager>,
    active_hotkey: Option<HotKey>,
    pub active_shortcut_str: String,
    pub status_error: Option<String>,
}

impl HotkeyController {
    pub fn new(initial_shortcut: &str) -> Self {
        let manager = GlobalHotKeyManager::new().ok();
        let mut controller = Self {
            manager,
            active_hotkey: None,
            active_shortcut_str: initial_shortcut.to_string(),
            status_error: None,
        };
        let _ = controller.set_shortcut(initial_shortcut);
        controller
    }

    pub fn set_shortcut(&mut self, shortcut_str: &str) -> Result<(), String> {
        let trimmed = shortcut_str.trim();
        let new_hk = parse_hotkey_string(trimmed)?;

        if let Some(mgr) = &self.manager {
            if let Some(old) = self.active_hotkey.take() {
                let _ = mgr.unregister(old);
            }
            mgr.register(new_hk)
                .map_err(|e| format!("Could not register '{trimmed}': {e}"))?;
            self.active_hotkey = Some(new_hk);
            self.active_shortcut_str = normalize_hotkey_label(trimmed);
            self.status_error = None;
            Ok(())
        } else {
            let err = "GlobalHotKeyManager unavailable on this session".to_string();
            self.status_error = Some(err.clone());
            Err(err)
        }
    }

    pub fn poll_triggered(&self) -> bool {
        let Some(active) = self.active_hotkey else {
            return false;
        };
        let mut triggered = false;
        let rx = GlobalHotKeyEvent::receiver();
        while let Ok(event) = rx.try_recv() {
            if event.id == active.id() && event.state == HotKeyState::Pressed {
                triggered = true;
            }
        }
        triggered
    }
}

impl Drop for HotkeyController {
    fn drop(&mut self) {
        if let (Some(mgr), Some(hk)) = (&self.manager, self.active_hotkey.take()) {
            let _ = mgr.unregister(hk);
        }
    }
}

pub fn parse_hotkey_string(input: &str) -> Result<HotKey, String> {
    let parts: Vec<&str> = input
        .split('+')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if parts.is_empty() {
        return Err("Keybind cannot be empty".to_string());
    }

    let mut mods = Modifiers::empty();
    let mut key_code: Option<Code> = None;

    for part in parts {
        let upper = part.to_ascii_uppercase();
        match upper.as_str() {
            "CTRL" | "CONTROL" => mods |= Modifiers::CONTROL,
            "ALT" | "OPTION" => mods |= Modifiers::ALT,
            "SHIFT" => mods |= Modifiers::SHIFT,
            "WIN" | "SUPER" | "META" | "CMD" => mods |= Modifiers::SUPER,
            other => {
                if key_code.is_some() {
                    return Err(format!("Multiple non-modifier keys in '{input}'"));
                }
                key_code = Some(parse_single_code(other)?);
            }
        }
    }

    let code = key_code.ok_or_else(|| "Shortcut must include a main key (e.g. T, F9, Space)".to_string())?;
    let modifiers_opt = if mods.is_empty() { None } else { Some(mods) };
    Ok(HotKey::new(modifiers_opt, code))
}

fn parse_single_code(token: &str) -> Result<Code, String> {
    let code = match token {
        "A" => Code::KeyA,
        "B" => Code::KeyB,
        "C" => Code::KeyC,
        "D" => Code::KeyD,
        "E" => Code::KeyE,
        "F" => Code::KeyF,
        "G" => Code::KeyG,
        "H" => Code::KeyH,
        "I" => Code::KeyI,
        "J" => Code::KeyJ,
        "K" => Code::KeyK,
        "L" => Code::KeyL,
        "M" => Code::KeyM,
        "N" => Code::KeyN,
        "O" => Code::KeyO,
        "P" => Code::KeyP,
        "Q" => Code::KeyQ,
        "R" => Code::KeyR,
        "S" => Code::KeyS,
        "T" => Code::KeyT,
        "U" => Code::KeyU,
        "V" => Code::KeyV,
        "W" => Code::KeyW,
        "X" => Code::KeyX,
        "Y" => Code::KeyY,
        "Z" => Code::KeyZ,
        "0" | "DIGIT0" => Code::Digit0,
        "1" | "DIGIT1" => Code::Digit1,
        "2" | "DIGIT2" => Code::Digit2,
        "3" | "DIGIT3" => Code::Digit3,
        "4" | "DIGIT4" => Code::Digit4,
        "5" | "DIGIT5" => Code::Digit5,
        "6" | "DIGIT6" => Code::Digit6,
        "7" | "DIGIT7" => Code::Digit7,
        "8" | "DIGIT8" => Code::Digit8,
        "9" | "DIGIT9" => Code::Digit9,
        "F1" => Code::F1,
        "F2" => Code::F2,
        "F3" => Code::F3,
        "F4" => Code::F4,
        "F5" => Code::F5,
        "F6" => Code::F6,
        "F7" => Code::F7,
        "F8" => Code::F8,
        "F9" => Code::F9,
        "F10" => Code::F10,
        "F11" => Code::F11,
        "F12" => Code::F12,
        "SPACE" | "SPACEBAR" => Code::Space,
        "TAB" => Code::Tab,
        "HOME" => Code::Home,
        "END" => Code::End,
        "PAGEUP" | "PGUP" => Code::PageUp,
        "PAGEDOWN" | "PGDN" => Code::PageDown,
        "INSERT" | "INS" => Code::Insert,
        "DELETE" | "DEL" => Code::Delete,
        "UP" | "ARROWUP" => Code::ArrowUp,
        "DOWN" | "ARROWDOWN" => Code::ArrowDown,
        "LEFT" | "ARROWLEFT" => Code::ArrowLeft,
        "RIGHT" | "ARROWRIGHT" => Code::ArrowRight,
        "BACKQUOTE" | "GRAVE" | "`" | "~" => Code::Backquote,
        "MINUS" | "-" => Code::Minus,
        "EQUAL" | "=" => Code::Equal,
        _ => return Err(format!("Unsupported key '{token}'")),
    };
    Ok(code)
}

pub fn normalize_hotkey_label(input: &str) -> String {
    let mut mods = Vec::new();
    let mut main_key = String::new();
    for part in input.split('+').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        match part.to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" => {
                if !mods.contains(&"Ctrl") {
                    mods.push("Ctrl");
                }
            }
            "ALT" | "OPTION" => {
                if !mods.contains(&"Alt") {
                    mods.push("Alt");
                }
            }
            "SHIFT" => {
                if !mods.contains(&"Shift") {
                    mods.push("Shift");
                }
            }
            "WIN" | "SUPER" | "META" | "CMD" => {
                if !mods.contains(&"Win") {
                    mods.push("Win");
                }
            }
            other => {
                main_key = if other.len() == 1 {
                    other.to_string()
                } else {
                    let mut c = other.chars();
                    match c.next() {
                        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                        None => String::new(),
                    }
                };
            }
        }
    }
    if !main_key.is_empty() {
        mods.push(&main_key);
    }
    mods.join("+")
}

/// Captures a shortcut from egui input when the user is recording a keybind in the Options window.
pub fn capture_egui_shortcut(ctx: &egui::Context) -> Option<String> {
    ctx.input(|i| {
        for event in &i.events {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            {
                if *key == egui::Key::Escape {
                    return Some("ESCAPE_CANCEL".to_string());
                }
                if let Some(key_name) = egui_key_to_token(*key) {
                    let mut parts = Vec::new();
                    if modifiers.ctrl || modifiers.command {
                        parts.push("Ctrl".to_string());
                    }
                    if modifiers.alt {
                        parts.push("Alt".to_string());
                    }
                    if modifiers.shift {
                        parts.push("Shift".to_string());
                    }
                    parts.push(key_name.to_string());
                    return Some(parts.join("+"));
                }
            }
        }
        None
    })
}

fn egui_key_to_token(key: egui::Key) -> Option<&'static str> {
    use egui::Key;
    match key {
        Key::A => Some("A"),
        Key::B => Some("B"),
        Key::C => Some("C"),
        Key::D => Some("D"),
        Key::E => Some("E"),
        Key::F => Some("F"),
        Key::G => Some("G"),
        Key::H => Some("H"),
        Key::I => Some("I"),
        Key::J => Some("J"),
        Key::K => Some("K"),
        Key::L => Some("L"),
        Key::M => Some("M"),
        Key::N => Some("N"),
        Key::O => Some("O"),
        Key::P => Some("P"),
        Key::Q => Some("Q"),
        Key::R => Some("R"),
        Key::S => Some("S"),
        Key::T => Some("T"),
        Key::U => Some("U"),
        Key::V => Some("V"),
        Key::W => Some("W"),
        Key::X => Some("X"),
        Key::Y => Some("Y"),
        Key::Z => Some("Z"),
        Key::Num0 => Some("0"),
        Key::Num1 => Some("1"),
        Key::Num2 => Some("2"),
        Key::Num3 => Some("3"),
        Key::Num4 => Some("4"),
        Key::Num5 => Some("5"),
        Key::Num6 => Some("6"),
        Key::Num7 => Some("7"),
        Key::Num8 => Some("8"),
        Key::Num9 => Some("9"),
        Key::F1 => Some("F1"),
        Key::F2 => Some("F2"),
        Key::F3 => Some("F3"),
        Key::F4 => Some("F4"),
        Key::F5 => Some("F5"),
        Key::F6 => Some("F6"),
        Key::F7 => Some("F7"),
        Key::F8 => Some("F8"),
        Key::F9 => Some("F9"),
        Key::F10 => Some("F10"),
        Key::F11 => Some("F11"),
        Key::F12 => Some("F12"),
        Key::Space => Some("Space"),
        Key::Tab => Some("Tab"),
        Key::Home => Some("Home"),
        Key::End => Some("End"),
        Key::PageUp => Some("PageUp"),
        Key::PageDown => Some("PageDown"),
        Key::Insert => Some("Insert"),
        Key::Delete => Some("Delete"),
        Key::ArrowUp => Some("Up"),
        Key::ArrowDown => Some("Down"),
        Key::ArrowLeft => Some("Left"),
        Key::ArrowRight => Some("Right"),
        Key::Backtick => Some("Backquote"),
        Key::Minus => Some("Minus"),
        Key::Equals => Some("Equal"),
        _ => None,
    }
}
