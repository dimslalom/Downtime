import assert from "node:assert/strict";
import { test } from "node:test";
import { DEFAULT_EXAM_TIME, examTimeFrom, formatExam } from "./deadline.ts";

test("a date alone uses the visible default time and says so", () => {
  const exam = examTimeFrom("2026-11-20", "", "Australia/Brisbane");
  assert.equal(exam?.local, `2026-11-20T${DEFAULT_EXAM_TIME}`);
  assert.equal(exam?.dateOnly, true);
});

test("a chosen time is kept as the local time", () => {
  const exam = examTimeFrom("2026-11-20", "14:30", "Australia/Brisbane");
  assert.equal(exam?.local, "2026-11-20T14:30");
  assert.equal(exam?.dateOnly, false);
  assert.equal(exam?.utcMs, new Date(2026, 10, 20, 14, 30).getTime());
});

test("an empty date is rejected", () => {
  assert.equal(examTimeFrom("", "09:00", "UTC"), null);
});

test("an exam displays in its stored zone", () => {
  const exam = {
    utcMs: Date.UTC(2026, 10, 19, 23, 0),
    local: "2026-11-20T09:00",
    timeZone: "Australia/Brisbane",
    dateOnly: true,
  };
  const shown = formatExam(exam);
  assert.match(shown, /Australia\/Brisbane/);
  assert.match(shown, /20/);
  assert.match(shown, /9.00/);
});
