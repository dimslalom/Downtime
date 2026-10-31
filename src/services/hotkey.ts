import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Mirrors `Hotkeys` in src-tauri/src/mock_trigger.rs. */
export interface Hotkeys {
  openHotkey: string;
  hideKey: string;
  openRegistered: boolean;
  peek: boolean;
}

/** Delivers the current hotkeys, then every change made on the settings page. */
export async function subscribeToHotkeys(
  onChange: (hotkeys: Hotkeys) => void,
): Promise<UnlistenFn> {
  const unlisten = await listen<Hotkeys>("hotkeys-changed", (event) => onChange(event.payload));
  onChange(await invoke<Hotkeys>("get_hotkeys"));
  return unlisten;
}

const KEY_LABELS: Record<string, string> = {
  Escape: "Esc",
  Control: "Ctrl",
  Super: "Win",
};

export function formatHotkey(shortcut: string): string {
  return shortcut
    .split("+")
    .map((part) => KEY_LABELS[part] ?? part.replace(/^(Key|Digit)/, ""))
    .join(" + ");
}

/** The shortcut for a keydown, or null while only a modifier is held. */
export function shortcutFromEvent(event: KeyboardEvent): string | null {
  if (/^(Control|Shift|Alt|Meta)(Left|Right)$/.test(event.code)) return null;
  const modifiers = [
    event.ctrlKey && "Control",
    event.altKey && "Alt",
    event.shiftKey && "Shift",
    event.metaKey && "Super",
  ].filter(Boolean);
  return [...modifiers, event.code].join("+");
}
