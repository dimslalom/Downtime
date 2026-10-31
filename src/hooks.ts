import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState, type Dispatch } from "react";
import type { GameEventSource } from "./services/gameEvents";
import { shortcutFromEvent, subscribeToHotkeys, type Hotkeys } from "./services/hotkey";
import { subscribeToLibrary } from "./services/library";
import { studyCommand, subscribeToHome, subscribeToStudy } from "./services/study";
import { canGrade, canRestart, currentAttemptId, type StudyAction, type StudyState } from "./study";
import type { HomeSnapshot, Library, Rating } from "./types";

/** Runs an async subscription, cleaning up even if unmounted before it resolves. */
function useSubscription(subscribe: () => Promise<() => void>) {
  useEffect(() => {
    let cancelled = false;
    let unsubscribe: (() => void) | undefined;

    subscribe()
      .then((stop) => {
        if (cancelled) stop();
        else unsubscribe = stop;
      })
      .catch(console.error);

    return () => {
      cancelled = true;
      unsubscribe?.();
    };
  }, [subscribe]);
}

/** Shows the overlay on death and hides it on respawn. */
export function useOverlayVisibility(source: GameEventSource, onHide: () => void): boolean {
  const [visible, setVisible] = useState(false);

  const subscribe = useCallback(
    () =>
      source.start((event) => {
        const next = event === "death";
        setVisible(next);
        if (!next) onHide();
        invoke("toggle_overlay", { visible: next }).catch(console.error);
      }),
    [source, onHide],
  );
  useSubscription(subscribe);

  return visible;
}

export function useLibrary(dispatch: Dispatch<StudyAction>) {
  const subscribe = useCallback(
    () => subscribeToLibrary((library) => dispatch({ type: "libraryChanged", library })),
    [dispatch],
  );
  useSubscription(subscribe);
}

export function useStudyView(dispatch: Dispatch<StudyAction>) {
  const subscribe = useCallback(
    () => subscribeToStudy((view) => dispatch({ type: "view", view })),
    [dispatch],
  );
  useSubscription(subscribe);
}

export function useHotkeys(): Hotkeys | null {
  const [hotkeys, setHotkeys] = useState<Hotkeys | null>(null);
  const subscribe = useCallback(() => subscribeToHotkeys(setHotkeys), []);
  useSubscription(subscribe);
  return hotkeys;
}

/** Failed is Again and Passed is Good. Neither is ever inferred from speed. */
const RATING_KEYS: Record<string, Rating> = {
  ArrowLeft: "again",
  ArrowRight: "good",
  Digit1: "again",
  Digit2: "hard",
  Digit3: "good",
  Digit4: "easy",
};

export function useStudyKeys(
  enabled: boolean,
  hideKey: string | undefined,
  peek: boolean,
  state: StudyState,
  dispatch: Dispatch<StudyAction>,
) {
  const latest = useRef(state);
  useEffect(() => {
    latest.current = state;
  }, [state]);

  useEffect(() => {
    if (!enabled) return;

    const send = (command: string, args?: Record<string, unknown>) => {
      studyCommand(command, args)
        .then((view) => dispatch({ type: "view", view }))
        .catch((message: unknown) => dispatch({ type: "failed", message: String(message) }));
    };

    const act = (code: string) => {
      const current = latest.current;
      const attemptId = currentAttemptId(current);
      const rating = RATING_KEYS[code];

      if (current.rest) {
        // Wait for the grade to land, so the next attempt starts after it.
        if (code === "Space" && !current.pending) {
          dispatch({ type: "endRest" });
          send("study_end_rest");
        }
        return;
      }

      if (rating) {
        if (!canGrade(current)) return;
        // Applied here so a second key before the next render is already blocked.
        const rest = peek ? (rating === "again" ? "failed" : "passed") : undefined;
        latest.current = { ...current, pending: true, rest: rest ?? null };
        dispatch({ type: "gradeSent", rest });
        send("study_grade", { attemptId, rating, rest: rest !== undefined });
      } else if (code === "Space") {
        if (attemptId) {
          dispatch({ type: "flip" });
          send("study_flip", { attemptId });
        } else if (canRestart(current)) {
          send("study_restart");
        }
      } else if (code === "KeyN") {
        if (attemptId && !current.pending) send("study_flag", { attemptId });
      } else if (code === "KeyI") {
        dispatch({ type: "toggleInfo" });
      } else if (code === "Tab") {
        send("study_next_deck");
      }
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (shortcutFromEvent(event) === hideKey) {
        event.preventDefault();
        if (!event.repeat) invoke("fire_mock_event", { event: "respawn" }).catch(console.error);
        return;
      }
      // A browser reload would wipe what is on screen.
      if (event.key === "F5" || (event.ctrlKey && event.key.toLowerCase() === "r")) {
        event.preventDefault();
        return;
      }
      const isStudyKey =
        event.code in RATING_KEYS || ["Space", "KeyN", "KeyI", "Tab"].includes(event.code);
      if (!isStudyKey) return;
      event.preventDefault();
      if (!event.repeat) act(event.code);
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [enabled, hideKey, peek, dispatch]);
}

export function useLibraryValue(): Library | null {
  const [library, setLibrary] = useState<Library | null>(null);
  const subscribe = useCallback(() => subscribeToLibrary(setLibrary), []);
  useSubscription(subscribe);
  return library;
}

export function useHome(): HomeSnapshot | null {
  const [home, setHome] = useState<HomeSnapshot | null>(null);
  const subscribe = useCallback(() => subscribeToHome(setHome), []);
  useSubscription(subscribe);
  return home;
}
