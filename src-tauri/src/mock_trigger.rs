use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

const GAME_EVENT: &str = "mock-game-event";
const HOTKEYS_EVENT: &str = "hotkeys-changed";
/// The overlay's own study keys, which neither hotkey may take over.
const RESERVED_KEYS: [&str; 10] = [
    "Space", "ArrowLeft", "ArrowRight", "Tab", "Digit1", "Digit2", "Digit3", "Digit4", "KeyN", "KeyI",
];

/// Stands in for death and respawn events from a game.
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GameEvent {
    Death,
    Respawn,
}

/// Where the Peek card sits. The centre is left out so it never covers the crosshair.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Placement {
    TopLeft,
    Top,
    TopRight,
    Left,
    #[default]
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

#[derive(Deserialize, Serialize)]
#[serde(default)]
struct Settings {
    open_hotkey: String,
    hide_key: String,
    peek: bool,
    placement: Placement,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            open_hotkey: "F9".into(),
            hide_key: "Escape".into(),
            peek: true,
            placement: Placement::default(),
        }
    }
}

/// `open_hotkey` is global, so it works from inside a game. `hide_key` is only
/// read by the overlay while it has focus, so it never takes a key from the game.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hotkeys {
    open_hotkey: String,
    hide_key: String,
    open_registered: bool,
    /// One compact card at the side, instead of the dimmed full-screen layout.
    peek: bool,
    placement: Placement,
}

pub struct HotkeyState {
    hotkeys: Mutex<Hotkeys>,
    settings_path: PathBuf,
}

impl HotkeyState {
    fn save(&self, hotkeys: &Hotkeys) -> Result<(), String> {
        let settings = Settings {
            open_hotkey: hotkeys.open_hotkey.clone(),
            hide_key: hotkeys.hide_key.clone(),
            peek: hotkeys.peek,
            placement: hotkeys.placement,
        };
        let contents =
            serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
        fs::write(&self.settings_path, contents).map_err(|error| error.to_string())
    }
}

fn parse_shortcut(shortcut: &str, other_key: &str) -> Result<Shortcut, String> {
    if RESERVED_KEYS.contains(&shortcut) {
        return Err("That key is used to study cards in the overlay. Pick another.".into());
    }
    if shortcut == other_key {
        return Err("The open and hide keys need to be different.".into());
    }
    shortcut
        .parse()
        .map_err(|_| "Downtime can't use that key. Pick another.".to_owned())
}

pub fn fire<R: Runtime>(app: &AppHandle<R>, event: GameEvent) {
    if let Err(error) = app.emit(GAME_EVENT, event) {
        eprintln!("failed to emit {GAME_EVENT}: {error}");
    }
}

/// The open hotkey works both ways, so it closes the overlay from inside a game.
fn toggle_overlay<R: Runtime>(app: &AppHandle<R>) {
    let open = app
        .get_webview_window("overlay")
        .is_some_and(|overlay| overlay.is_visible().unwrap_or(false));
    fire(app, if open { GameEvent::Respawn } else { GameEvent::Death });
}

#[tauri::command]
pub fn fire_mock_event(app: AppHandle, event: GameEvent) {
    fire(&app, event);
}

#[tauri::command]
pub fn get_hotkeys(state: State<HotkeyState>) -> Hotkeys {
    state.hotkeys.lock().expect("hotkey state poisoned").clone()
}

fn resume_open_hotkey(app: &AppHandle, hotkeys: &mut Hotkeys) {
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    hotkeys.open_registered = shortcuts.register(hotkeys.open_hotkey.as_str()).is_ok();
}

/// Releases the open hotkey while the settings page listens for a new key, so
/// that pressing the current one is captured instead of opening the overlay.
#[tauri::command]
pub fn suspend_open_hotkey(app: AppHandle, state: State<HotkeyState>, suspended: bool) {
    let mut hotkeys = state.hotkeys.lock().expect("hotkey state poisoned");
    if suspended {
        let _ = app.global_shortcut().unregister_all();
    } else {
        resume_open_hotkey(&app, &mut hotkeys);
    }
}

#[tauri::command]
pub fn set_open_hotkey(
    app: AppHandle,
    state: State<HotkeyState>,
    shortcut: String,
) -> Result<(), String> {
    let shortcuts = app.global_shortcut();
    let mut hotkeys = state.hotkeys.lock().expect("hotkey state poisoned");
    let _ = shortcuts.unregister_all();

    let registered = parse_shortcut(&shortcut, &hotkeys.hide_key).and_then(|parsed| {
        // Windows refuses bare F12, which it reserves for debuggers, and any
        // key another app already holds.
        shortcuts
            .register(parsed)
            .map_err(|_| "Windows or another app is using that key. Pick another.".to_owned())
    });
    if let Err(message) = registered {
        resume_open_hotkey(&app, &mut hotkeys);
        return Err(message);
    }

    hotkeys.open_hotkey = shortcut;
    hotkeys.open_registered = true;
    let _ = app.emit(HOTKEYS_EVENT, hotkeys.clone());
    state.save(&hotkeys)
}

#[tauri::command]
pub fn set_hide_key(
    app: AppHandle,
    state: State<HotkeyState>,
    shortcut: String,
) -> Result<(), String> {
    let mut hotkeys = state.hotkeys.lock().expect("hotkey state poisoned");
    let parsed = parse_shortcut(&shortcut, &hotkeys.open_hotkey);
    resume_open_hotkey(&app, &mut hotkeys);
    parsed?;

    hotkeys.hide_key = shortcut;
    let _ = app.emit(HOTKEYS_EVENT, hotkeys.clone());
    state.save(&hotkeys)
}

#[tauri::command]
pub fn set_peek(app: AppHandle, state: State<HotkeyState>, peek: bool) -> Result<(), String> {
    let mut hotkeys = state.hotkeys.lock().expect("hotkey state poisoned");
    hotkeys.peek = peek;
    let _ = app.emit(HOTKEYS_EVENT, hotkeys.clone());
    state.save(&hotkeys)
}

#[tauri::command]
pub fn set_placement(
    app: AppHandle,
    state: State<HotkeyState>,
    placement: Placement,
) -> Result<(), String> {
    let mut hotkeys = state.hotkeys.lock().expect("hotkey state poisoned");
    hotkeys.placement = placement;
    let _ = app.emit(HOTKEYS_EVENT, hotkeys.clone());
    state.save(&hotkeys)
}

pub fn register<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, _shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    toggle_overlay(app);
                }
            })
            .build(),
    )?;

    let dir = app.path().app_data_dir()?;
    fs::create_dir_all(&dir)?;
    let settings_path = dir.join("settings.json");
    let settings: Settings = fs::read_to_string(&settings_path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default();

    // Not fatal: another app may own the key, and home and the tray still work.
    let registered = app
        .global_shortcut()
        .register(settings.open_hotkey.as_str());
    if let Err(error) = &registered {
        eprintln!("failed to register {}: {error}", settings.open_hotkey);
    }
    app.manage(HotkeyState {
        hotkeys: Mutex::new(Hotkeys {
            open_hotkey: settings.open_hotkey,
            hide_key: settings.hide_key,
            open_registered: registered.is_ok(),
            peek: settings.peek,
            placement: settings.placement,
        }),
        settings_path,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_keys_captured_from_the_settings_page() {
        for shortcut in ["Escape", "F9", "F12", "KeyK", "Control+Shift+KeyK", "Alt+Digit1"] {
            assert!(parse_shortcut(shortcut, "F1").is_ok(), "{shortcut}");
        }
    }

    #[test]
    fn rejects_study_keys_unless_combined_with_a_modifier() {
        assert!(parse_shortcut("Space", "F1").is_err());
        assert!(parse_shortcut("ArrowLeft", "F1").is_err());
        assert!(parse_shortcut("Control+Space", "F1").is_ok());
    }

    #[test]
    fn rejects_the_same_key_for_open_and_hide() {
        assert!(parse_shortcut("Escape", "Escape").is_err());
    }

    #[test]
    fn missing_settings_fall_back_to_the_defaults() {
        let settings: Settings = serde_json::from_str(r#"{"hide_key": "KeyQ"}"#).unwrap();
        assert_eq!(settings.open_hotkey, "F9");
        assert_eq!(settings.hide_key, "KeyQ");
        assert!(settings.peek);
        assert_eq!(settings.placement, Placement::Right);
    }

    #[test]
    fn placement_uses_the_names_the_frontend_sends() {
        let settings: Settings = serde_json::from_str(r#"{"placement": "bottom-left"}"#).unwrap();
        assert_eq!(settings.placement, Placement::BottomLeft);
    }
}
