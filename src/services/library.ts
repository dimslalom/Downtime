import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Library } from "../types";

/** Delivers the current library, then every change to the flashcards folder. */
export async function subscribeToLibrary(
  onChange: (library: Library) => void,
): Promise<UnlistenFn> {
  const unlisten = await listen<Library>("decks-changed", (event) => onChange(event.payload));
  onChange(await invoke<Library>("load_decks"));
  return unlisten;
}
