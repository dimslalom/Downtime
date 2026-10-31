import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { HomeSnapshot, StudyView } from "../types";

let tail: Promise<unknown> = Promise.resolve();

/** Commands run in the order keys were pressed, so a flip cannot overtake a grade. */
function inOrder<T>(task: () => Promise<T>): Promise<T> {
  const run = tail.then(task, task);
  tail = run.catch(() => undefined);
  return run;
}

export function studyCommand(command: string, args?: Record<string, unknown>): Promise<StudyView> {
  return inOrder(() => invoke<StudyView>(command, args));
}

/** Delivers the current view, then every change made anywhere else. */
export async function subscribeToStudy(onChange: (view: StudyView) => void): Promise<UnlistenFn> {
  const refresh = () => studyCommand("study_view").then(onChange);
  const unlisten = await listen("study-changed", () => {
    refresh().catch(console.error);
  });
  await refresh();
  return unlisten;
}

/** Delivers home's data, then again after any change to study state or decks. */
export async function subscribeToHome(onChange: (home: HomeSnapshot) => void): Promise<UnlistenFn> {
  const refresh = () => invoke<HomeSnapshot>("home_snapshot").then(onChange);
  const report = () => {
    refresh().catch(console.error);
  };
  const stopStudy = await listen("study-changed", report);
  const stopDecks = await listen("decks-changed", report);
  await refresh();
  return () => {
    stopStudy();
    stopDecks();
  };
}
