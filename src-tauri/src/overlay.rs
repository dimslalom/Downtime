use std::sync::Mutex;

use tauri::{State, WebviewWindow};

/// The window that had the foreground when the overlay opened, usually the game.
#[derive(Default)]
pub struct OverlayState {
    game_window: Mutex<Option<isize>>,
}

#[cfg(windows)]
mod foreground {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, IsWindow, SetForegroundWindow,
    };

    pub fn current() -> isize {
        unsafe { GetForegroundWindow() as isize }
    }

    pub fn restore(window: isize) {
        let hwnd = window as _;
        unsafe {
            if IsWindow(hwnd) != 0 {
                SetForegroundWindow(hwnd);
            }
        }
    }
}

#[cfg(not(windows))]
mod foreground {
    pub fn current() -> isize {
        0
    }

    pub fn restore(_window: isize) {}
}

#[cfg(windows)]
fn own_handle(window: &WebviewWindow) -> tauri::Result<isize> {
    Ok(window.hwnd()?.0 as isize)
}

#[cfg(not(windows))]
fn own_handle(_window: &WebviewWindow) -> tauri::Result<isize> {
    Ok(0)
}

/// Shows the overlay in front of everything with keyboard focus, or hides it
/// completely. A hidden window receives no input, so handing the foreground
/// back to the game returns the keyboard and mouse to it.
#[tauri::command]
pub fn toggle_overlay(
    window: WebviewWindow,
    state: State<OverlayState>,
    visible: bool,
) -> Result<(), String> {
    apply(&window, &state, visible).map_err(|error| error.to_string())
}

fn apply(window: &WebviewWindow, state: &OverlayState, visible: bool) -> tauri::Result<()> {
    if window.is_visible()? == visible {
        return Ok(());
    }

    let own = own_handle(window)?;
    let mut game_window = state.game_window.lock().expect("overlay state poisoned");

    if visible {
        let previous = foreground::current();
        *game_window = (previous != own).then_some(previous);
        window.show()?;
        window.set_always_on_top(true)?;
        window.set_focus()?;
    } else {
        // Only take the foreground back if the player is still on the overlay;
        // if they alt-tabbed to something else, leave them there.
        if let Some(game) = game_window.take().filter(|_| foreground::current() == own) {
            foreground::restore(game);
        }
        window.hide()?;
    }
    Ok(())
}
