import type { Library, StudyView } from "./types";

/** The moment after a Peek grade, while the player can take one more or leave. */
export type PeekRest = "passed" | "failed";

/**
 * What the overlay is showing. The backend owns cards, memory and scheduling and
 * sends a `StudyView` after each change. This only keeps the screen instant:
 * flips apply at once, and a grade blocks further input until it has landed.
 */
export interface StudyState {
  library: Library | null;
  view: StudyView | null;
  pending: boolean;
  showInfo: boolean;
  notice: string | null;
  cardMotion: "advance" | "fail" | null;
  rest: PeekRest | null;
}

export type StudyAction =
  | { type: "libraryChanged"; library: Library }
  | { type: "view"; view: StudyView }
  | { type: "flip" }
  | { type: "gradeSent"; rest?: PeekRest }
  | { type: "endRest" }
  | { type: "toggleInfo" }
  | { type: "failed"; message: string };

export const initialStudyState: StudyState = {
  library: null,
  view: null,
  pending: false,
  showInfo: false,
  notice: null,
  cardMotion: null,
  rest: null,
};

export function currentAttemptId(state: StudyState): string | null {
  return state.view?.session?.card?.attemptId ?? null;
}

/** Grading needs the answer to have been seen, and one grade at a time. */
export function canGrade(state: StudyState): boolean {
  const session = state.view?.session;
  return Boolean(session?.card && session.flipped && !state.pending && !state.rest);
}

export function canRestart(state: StudyState): boolean {
  return Boolean(state.view?.session && !state.view.session.card && !state.pending && !state.rest);
}

export function studyReducer(state: StudyState, action: StudyAction): StudyState {
  switch (action.type) {
    case "libraryChanged":
      return { ...state, library: action.library };

    case "view": {
      const isSameCard = currentAttemptId(state) === action.view.session?.card?.attemptId;
      const previous = state.view?.session;
      const next = action.view.session;
      const sameDeck = previous && next && previous.deckId === next.deckId;
      return {
        ...state,
        view: action.view,
        pending: false,
        showInfo: isSameCard && state.showInfo,
        notice: null,
        cardMotion: isSameCard ? state.cardMotion : sameDeck && next.failed > previous.failed
          ? "fail" : "advance",
      };
    }

    case "flip": {
      const session = state.view?.session;
      if (!state.view || !session?.card || state.pending || state.rest) return state;
      return {
        ...state,
        view: { ...state.view, session: { ...session, flipped: !session.flipped } },
      };
    }

    case "gradeSent":
      return canGrade(state) ? { ...state, pending: true, rest: action.rest ?? null } : state;

    case "endRest":
      return state.rest ? { ...state, rest: null } : state;

    case "toggleInfo":
      return { ...state, showInfo: !state.showInfo };

    case "failed":
      return { ...state, pending: false, notice: action.message, rest: null };
  }
}
