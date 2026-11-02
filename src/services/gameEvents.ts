import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type GameEvent = "death" | "respawn";

export type GameEventListener = (event: GameEvent) => void;

/** Where death and respawn events come from. Only the manual keys for now. */
export interface GameEventSource {
  start(listener: GameEventListener): Promise<UnlistenFn>;
}

/** Death comes from the open hotkey, respawn from the overlay's hide key. */
export const mockHotkeySource: GameEventSource = {
  start(listener) {
    return listen<GameEvent>("mock-game-event", (event) => listener(event.payload));
  },
};
