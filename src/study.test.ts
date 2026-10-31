import assert from "node:assert/strict";
import { test } from "node:test";
import { canGrade, canRestart, initialStudyState, studyReducer, type StudyState } from "./study.ts";
import type { SessionView, StudyView } from "./types.ts";

function view(session: Partial<SessionView> = {}, attemptId = "a1"): StudyView {
  return {
    deckCount: 1,
    session: {
      deckId: "bio",
      deckName: "Bio",
      mode: "ordinary",
      extra: false,
      passed: 0,
      failed: 0,
      remaining: 3,
      flipped: false,
      nextDueMs: null,
      planPending: false,
      card: {
        attemptId,
        key: "bio::c1",
        cardType: null,
        front: "Q",
        back: "A",
        source: "Lecture 1",
        importance: 3,
        importanceReason: null,
        flagged: false,
        retry: false,
      },
      ...session,
    },
  };
}

function withView(next: StudyView): StudyState {
  return studyReducer(initialStudyState, { type: "view", view: next });
}

test("a card cannot be graded before its answer is shown", () => {
  const state = withView(view());
  assert.equal(canGrade(state), false);
  assert.equal(canGrade(studyReducer(state, { type: "flip" })), true);
});

test("a second grade is blocked until the first has landed", () => {
  const flipped = studyReducer(withView(view()), { type: "flip" });
  const sent = studyReducer(flipped, { type: "gradeSent" });
  assert.equal(sent.pending, true);
  assert.equal(canGrade(sent), false);
  assert.equal(studyReducer(sent, { type: "gradeSent" }), sent);
  assert.equal(studyReducer(sent, { type: "flip" }), sent);
});

test("a Peek grade rests until the player takes one more", () => {
  const flipped = studyReducer(withView(view()), { type: "flip" });
  const resting = studyReducer(flipped, { type: "gradeSent", rest: "passed" });
  assert.equal(resting.rest, "passed");
  assert.equal(canGrade(resting), false);
  assert.equal(studyReducer(resting, { type: "flip" }), resting);
  assert.equal(studyReducer(resting, { type: "endRest" }).rest, null);
});

test("a failed Peek grade brings the same card back", () => {
  const flipped = studyReducer(withView(view()), { type: "flip" });
  const resting = studyReducer(flipped, { type: "gradeSent", rest: "failed" });
  const failed = studyReducer(resting, { type: "failed", message: "Could not save." });
  assert.equal(failed.rest, null);
  assert.equal(canGrade(failed), true);
});

test("a failed grade unblocks the same card and says so", () => {
  const sent = studyReducer(studyReducer(withView(view()), { type: "flip" }), { type: "gradeSent" });
  const failed = studyReducer(sent, { type: "failed", message: "Could not save." });
  assert.equal(failed.pending, false);
  assert.equal(failed.notice, "Could not save.");
  assert.equal(canGrade(failed), true);
});

test("a deck change leaves the card and flip state alone", () => {
  const flipped = studyReducer(withView(view()), { type: "flip" });
  const library = { folder: "f", decks: [], errors: [], warnings: [] };
  const next = studyReducer(flipped, { type: "libraryChanged", library });
  assert.equal(next.view, flipped.view);
});

test("source details close when the card changes, not when it refreshes", () => {
  const open = studyReducer(withView(view()), { type: "toggleInfo" });
  assert.equal(studyReducer(open, { type: "view", view: view() }).showInfo, true);
  assert.equal(studyReducer(open, { type: "view", view: view({}, "a2") }).showInfo, false);
});

test("restarting is only offered when no card is showing", () => {
  assert.equal(canRestart(withView(view())), false);
  assert.equal(canRestart(withView(view({ card: null }))), true);
  assert.equal(canRestart(initialStudyState), false);
});

test("confirmed failures animate differently from passes and survive view refreshes", () => {
  const state = withView(view());
  const failed = studyReducer(state, { type: "view", view: view({ failed: 1 }, "a2") });
  assert.equal(failed.cardMotion, "fail");
  assert.equal(studyReducer(failed, { type: "view", view: view({ failed: 1 }, "a2") }).cardMotion, "fail");
  assert.equal(studyReducer(failed, { type: "view", view: view({ failed: 1, passed: 1 }, "a3") }).cardMotion, "advance");
});

test("save errors and deck switches do not produce failure feedback", () => {
  const state = withView(view());
  assert.notEqual(studyReducer(state, { type: "failed", message: "Offline" }).cardMotion, "fail");
  assert.equal(studyReducer(state, { type: "view", view: view({ deckId: "other", failed: 5 }, "a2") }).cardMotion, "advance");
});
