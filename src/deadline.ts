import type { ExamTime } from "./types";

/** Used when only a date is chosen. The form always shows it. */
export const DEFAULT_EXAM_TIME = "09:00";

/**
 * The player's wall-clock choice as an instant in their own zone. The date and
 * time are read as local, so daylight saving is applied for that date, not today's.
 */
export function examTimeFrom(date: string, time: string, timeZone: string): ExamTime | null {
  const chosen = time || DEFAULT_EXAM_TIME;
  const [year, month, day] = date.split("-").map(Number);
  const [hours, minutes] = chosen.split(":").map(Number);
  if (!year || !month || !day || hours === undefined || minutes === undefined) return null;

  const instant = new Date(year, month - 1, day, hours, minutes);
  if (Number.isNaN(instant.getTime())) return null;
  return {
    utcMs: instant.getTime(),
    local: `${date}T${chosen}`,
    timeZone,
    dateOnly: !time,
  };
}

export function localTimeZone(): string {
  return Intl.DateTimeFormat().resolvedOptions().timeZone;
}

/** Shown in the exam's own zone, so travel cannot make it read as another time. */
export function formatExam(exam: ExamTime): string {
  const date = new Date(exam.utcMs).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
    timeZone: exam.timeZone,
  });
  return `${date} (${exam.timeZone})`;
}
