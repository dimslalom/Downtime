pub mod flashcards;
mod game_bridge;
mod mock_trigger;
mod overlay;
pub mod study;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, Runtime, WebviewWindowBuilder, WindowEvent};

use mock_trigger::GameEvent;

fn show_home<R: Runtime>(app: &AppHandle<R>) {
    let Some(home) = app.get_webview_window("home") else {
        return;
    };
    let shown = home
        .show()
        .and_then(|_| home.unminimize())
        .and_then(|_| home.set_focus());
    if let Err(error) = shown {
        eprintln!("failed to show home window: {error}");
    }
}

/// Closing the home window keeps the app in the tray, so the hotkey and overlay
/// keep working during a game. The tray is the way to reopen it or quit.
fn build_tray(app: &App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Downtime", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "Open overlay", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "Hide overlay", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Downtime", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &show, &hide, &quit])?;

    let mut tray = TrayIconBuilder::new()
        .tooltip("Downtime")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_home(app),
            "show" => mock_trigger::fire(app, GameEvent::Death),
            "hide" => mock_trigger::fire(app, GameEvent::Respawn),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_home(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_home(app);
        }))
        .manage(overlay::OverlayState::default())
        .setup(|app| {
            mock_trigger::register(app.handle())?;
            study::commands::setup(app.handle())?;
            flashcards::setup(app.handle())?;
            // Created here, not by Tauri, because Tauri builds windows before this hook
            // and a fast page load would call commands before their state is managed.
            for config in app.config().app.windows.clone() {
                WebviewWindowBuilder::from_config(app.handle(), &config)?.build()?;
            }
            build_tray(app)?;
            game_bridge::start(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::Focused(focused) = event {
                if window.label() == "overlay" {
                    // Hiding also delivers a focus gain, which must not restart the clock.
                    let studying = *focused && window.is_visible().unwrap_or(false);
                    study::commands::overlay_focus_changed(window.app_handle(), studying);
                }
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                // The overlay only ever hides: its webview holds the study session.
                api.prevent_close();
                if window.label() == "home" {
                    if let Err(error) = window.hide() {
                        eprintln!("failed to hide home window: {error}");
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            overlay::toggle_overlay,
            mock_trigger::fire_mock_event,
            mock_trigger::get_hotkeys,
            mock_trigger::set_open_hotkey,
            mock_trigger::set_hide_key,
            mock_trigger::set_peek,
            mock_trigger::suspend_open_hotkey,
            flashcards::open_flashcards_folder,
            study::commands::load_decks,
            study::commands::study_view,
            study::commands::study_flip,
            study::commands::study_grade,
            study::commands::study_end_rest,
            study::commands::study_flag,
            study::commands::study_next_deck,
            study::commands::study_restart,
            study::commands::home_snapshot,
            study::commands::organize_library,
            study::commands::deck_cards,
            study::commands::set_card_flag,
            study::commands::set_card_importance,
            study::commands::keep_card_history,
            study::commands::reset_card_learning,
            study::commands::set_study_settings,
            study::commands::set_deck_deadline,
            study::commands::disable_deck_deadline,
            study::commands::preview_deck_import,
            study::commands::commit_deck_import,
            study::commands::fit_memory_model
        ])
        .run(tauri::generate_context!())
        .expect("error while running Downtime");
}
