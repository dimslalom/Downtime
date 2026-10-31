use std::io::{BufRead, BufReader};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::thread;

use tauri::{AppHandle, Runtime};

use crate::mock_trigger::{self, GameEvent};

/// overwolf-helper/main.js connects here. Loopback only: the worst another local
/// process can do is show or hide the overlay.
const BRIDGE_PORT: u16 = 47631;

fn parse_event(line: &str) -> Option<GameEvent> {
    match line.trim() {
        "death" => Some(GameEvent::Death),
        "respawn" => Some(GameEvent::Respawn),
        _ => None,
    }
}

fn handle_connection<R: Runtime>(app: AppHandle<R>, stream: TcpStream) {
    for line in BufReader::new(stream).lines().map_while(Result::ok) {
        match parse_event(&line) {
            Some(event) => mock_trigger::fire(&app, event),
            None => eprintln!("ignoring unknown game event: {line}"),
        }
    }
}

/// Receives death and respawn events from the Overwolf helper, which Tauri cannot
/// host itself because Overwolf's game events only run inside ow-electron.
/// Not fatal: without the helper the manual keys and tray still work.
pub fn start<R: Runtime>(app: AppHandle<R>) {
    let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, BRIDGE_PORT)) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("failed to start game bridge: {error}");
            return;
        }
    };

    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let app = app.clone();
            thread::spawn(move || handle_connection(app, stream));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_known_events() {
        assert!(matches!(parse_event("death\n"), Some(GameEvent::Death)));
        assert!(matches!(parse_event(" respawn "), Some(GameEvent::Respawn)));
        assert!(parse_event("shutdown").is_none());
        assert!(parse_event("").is_none());
    }
}
