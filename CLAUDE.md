# Downtime

Desktop overlay for studying flashcards during video game downtime (queue, spectating, death timers) in Apex Legends, Valorant, Overwatch and Deadlock.

Stack: Tauri v2 (Rust backend), React (TypeScript), Tailwind CSS. Windows is the target platform.

## Strict rules

These apply to the entire project.

### UI

- Show, don't tell. Preserve useful information and express it through drawings, layout, and visual states instead of explanatory headings. For flashcards, use visual cues for question/answer, card type, retry, and flags rather than deleting those cues or spelling them out. Keep accessible names for icons.
- Library folders are created and renamed inline. Use pen icons, indented deck rows, and contextual actions instead of permanent naming forms or per-deck folder selectors. Enter saves names; Escape cancels. Importing from a folder places the deck there.
- DO NOT use any UI component libraries (Shadcn, MUI, Radix or similar). Use raw Tailwind classes for absolute control over rendering performance.
- Light theme: white `bg-paper` surfaces with near-black `text-ink`. The overlay keeps a semi-transparent `bg-black/60` backing, so visual respawn cues in the game stay visible around the card. By default the overlay is Peek: one compact card at the side, only the next move as hints, and no full-screen dim. A grade shows a 3-second rest where Space takes one more, then the overlay hides. Players can switch to the dimmed full-screen layout in Settings. Both layouts run in the same `overlay` window.
- Hand-drawn details are sparing: pen outlines from `src/components/HandDrawn.tsx` on primary panels and buttons, and hand-lettered SVG paths for a few short headings. No handwriting fonts.
- Clean, high-contrast, large sans-serif type that reads from a distance. No web fonts.
- The UI is driven entirely by the keyboard. Space flips, Left Arrow is Failed (Again), Right Arrow is Passed (Good), 1 to 4 are Again, Hard, Good and Easy, N flags a card as needing explanation, and I shows its source. Hard always means a successful recall. Never require the mouse. Keep the subtle keybind hints at the bottom of the screen.
- Animations are instant or strictly under 100ms. No heavy effects that cost the game frames: no `backdrop-blur`, no 3D flips, no large shadows.

### Tauri windows

The `overlay` window must be configured strictly as `transparent: true`, `alwaysOnTop: true`, `decorations: false` and `fullscreen: true`.

The `home` window is an ordinary app window that opens on launch, for setup and managing decks outside a game. The keyboard-only and overlay animation rules above apply to the overlay, not to home. Home follows the same theme and no-component-library rules. Closing it hides it to the tray, because the overlay has to keep running during a game.

Home uses a custom white title bar without an app-name label, with hand-drawn minimize, maximize/restore, and close icons. Keep it mounted across home pages, reserve its height in the layout, and keep its window-control permissions scoped to `home`.

### State preservation

Hiding the overlay must never touch React state. The component tree stays mounted for the app's whole lifetime, and showing the overlay again displays the exact same card in the exact same flip state. Deck files changing on disk must not disturb a pass in progress: a session runs on its own snapshot of the cards and picks up changes when it restarts.

Only an explicit grade changes learning history. Viewing, revealing, hiding, losing focus, flagging and quitting never do, and time while hidden or unfocused never counts as study time.

### Verification

Run these after every edit, and never stop without all of them passing:

```bash
npm run tsc
npm run lint
cargo check --manifest-path src-tauri/Cargo.toml
```

Tests: `npm test` and `cargo test --manifest-path src-tauri/Cargo.toml --lib`.

## Architecture

- `src-tauri/src/overlay.rs`: the `toggle_overlay(visible)` command. Shows and focuses the window, or hides it and hands the foreground back to the game.
- `src-tauri/src/mock_trigger.rs`: the manual stand-in for Overwolf events. A global open hotkey (default F9) toggles the overlay, the tray emits `mock-game-event`, and a hide key (default Esc) is read only by the focused overlay, so it never takes a key from the game. Both, and the Peek setting, are saved in `settings.json` in AppData. Bare F12 cannot be the open hotkey: Windows reserves it for debuggers and refuses to register it.
- `src-tauri/src/flashcards.rs`: parses and validates `flashcards/*.json` in AppData, watches the folder and emits `decks-changed`.
- `src-tauri/src/study/`: the study system, with no Tauri dependency except `commands.rs`. `memory` wraps the pinned `fsrs` crate, `store` persists to `study/` in AppData (`reviews.jsonl` is the source of truth), `catalog` handles deck identity and reimport, `scheduler` builds queues, `planner` is the deadline planner, `session` tracks attempts, and `engine` ties them together with an injected clock. Planning and fitting run on background threads. See `docs/deadline-mode.md`.
- `overwolf-helper/`: a windowless ow-electron app, because Overwolf's game events are not available to Tauri. It maps each game's events in `games.js`, logs them raw to `events.log`, and sends `death` or `respawn` to `src-tauri/src/game_bridge.rs` on loopback. It needs `OW_DEV_KEY` in the shell to receive events; never commit that key. Test with `npm test` inside that folder.
- `src/services/gameEvents.ts`: `GameEventSource` interface and the mock hotkey source. A future Overwolf source implements the same interface.
- `src/study.ts`: pure reducer over the views the backend sends. It keeps flips instant and blocks a second grade.
- `src-tauri/src/lib.rs`: both windows use `"create": false` and are built at the end of the setup hook. Tauri would otherwise build them before the hook runs, and a fast page load calls commands before their state is managed. Keep any new managed state above that loop.
- `src/main.tsx`: both windows load the same bundle and render by window label: `src/OverlayView.tsx` or `src/HomeView.tsx`.

Deck file schema: `{"deck_name": "Name", "cards": [{"front": "Q", "back": "A"}]}`. Cards may also have `id`, `card_type`, `source`, `importance`, `importance_reason` and `assessment_evidence`.
