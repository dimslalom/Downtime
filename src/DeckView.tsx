import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { HandButton } from "./components/HandDrawn";
import { PenStat } from "./components/PenIcon";
import { DEFAULT_EXAM_TIME, examTimeFrom, formatExam, localTimeZone } from "./deadline";
import { PAGE, TEXT_FIELD } from "./styles";
import type { CardRow, Deadline, DeckSummary, Fit, Measured, PlanView } from "./types";

function percent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

function minutes(value: number): string {
  return value >= 90 ? `${(value / 60).toFixed(1)} hours` : `${Math.round(value)} min`;
}

const FIT_TEXT: Record<Fit, string> = {
  fits: "The estimate meets your target within the time available.",
  tight: "All cards fit, but the estimate falls short of your target.",
  over: "Your cards do not all fit in the time available.",
};

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col">
      <dt className="text-sm text-neutral-600">{label}</dt>
      <dd className="text-2xl font-semibold tabular-nums">{value}</dd>
    </div>
  );
}

function PlanPanel({ plan, deck, measured }: { plan: PlanView; deck: DeckSummary; measured: Measured }) {
  const { estimate } = plan;
  return (
    <section className="flex flex-col gap-3 border-b border-neutral-200 pb-4">
      <div className="flex flex-col">
        <h2 className="text-lg font-semibold">Exam plan</h2>
        <p className="text-sm text-neutral-600">
          Model estimates of recall, not guarantees and not predicted marks. They assume the forgetting
          model fits you and that your study time turns up.
        </p>
      </div>
      <p className={`text-base font-medium ${plan.fit === "over" ? "text-rose-700" : "text-ink"}`}>
        {FIT_TEXT[plan.fit]}
      </p>
      <dl className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        <Stat label="Estimated exam-day recall" value={percent(estimate.examRecall)} />
        <Stat label="Weighted by importance" value={percent(estimate.examRecallWeighted)} />
        <Stat label="If you stopped today" value={percent(estimate.noStudyRecallWeighted)} />
        <Stat label="Your target" value={percent(plan.targetRecall)} />
        <Stat label="Unseen now" value={`${deck.unseen} of ${deck.cardCount}`} />
        <Stat label="Estimated unseen at exam" value={`${Math.round(plan.expectedUnseenAtExam)}`} />
        <Stat label="Expected workload" value={minutes(estimate.expectedMinutes)} />
        <Stat label="Estimated time available" value={minutes(plan.minutesAvailable)} />
      </dl>
      {plan.minutesToCoverUnseen > plan.minutesAvailable && (
        <p className="text-base text-rose-700">
          Seeing every unseen card once needs about {minutes(plan.minutesToCoverUnseen)}, more than the
          time estimated.
        </p>
      )}
      {deck.flagged > 0 && (
        <p className="text-base text-neutral-700">
          {deck.flagged} flagged {deck.flagged === 1 ? "card is" : "cards are"} out of rapid review and
          still counted in these estimates.
        </p>
      )}
      <p className="text-sm text-neutral-600">
        Planned today: {minutes(estimate.todayMinutes)}. The plan expects about {percent(plan.reliability)}{" "}
        of your daily time to turn up, and replans as it does.
      </p>
      <p className="text-sm text-neutral-600">
        Measured today: {measured.reviewsToday} reviews in {minutes(measured.minutesToday)}. Review time is{" "}
        {measured.costsAreMeasured
          ? `from your ${measured.costReviews} timed reviews`
          : "a default until 20 clean reviews are timed"}
        .
      </p>
    </section>
  );
}

export function DeadlineForm({ deck, onSaved, folderId }: {
  deck: Pick<DeckSummary, "id" | "deadline" | "mode">;
  onSaved: () => void;
  folderId?: string;
}) {
  const saved = deck.deadline;
  const [savedDate, savedTime] = saved?.exam.local.split("T") ?? ["", ""];
  const [date, setDate] = useState(savedDate ?? "");
  const [time, setTime] = useState(saved?.exam.dateOnly ? "" : (savedTime ?? ""));
  const [dailyMinutes, setDailyMinutes] = useState(saved?.dailyMinutes?.toString() ?? "");
  const [reliability, setReliability] = useState(saved?.advanced.reliability?.toString() ?? "");
  const [targetRecall, setTargetRecall] = useState(saved?.advanced.targetRecall?.toString() ?? "");
  const [maxNew, setMaxNew] = useState(saved?.advanced.maxNewPerDay?.toString() ?? "");
  const [error, setError] = useState<string | null>(null);
  const timeZone = saved?.exam.timeZone ?? localTimeZone();

  const optional = (text: string) => (text.trim() === "" ? null : Number(text));

  const save = () => {
    const exam = examTimeFrom(date, time, timeZone);
    if (!exam) {
      setError("Choose an exam date.");
      return;
    }
    const deadline: Deadline = {
      enabled: true,
      exam,
      dailyMinutes: optional(dailyMinutes),
      advanced: {
        reliability: optional(reliability),
        targetRecall: optional(targetRecall),
        maxNewPerDay: optional(maxNew),
      },
    };
    setError(null);
    invoke(folderId ? "organize_library" : "set_deck_deadline", folderId
      ? { action: { kind: "folderDeadline", id: folderId, deadline } }
      : { deckId: deck.id, deadline })
      .then(onSaved)
      .catch((message: unknown) => setError(String(message)));
  };

  const disable = () => {
    invoke(folderId ? "organize_library" : "disable_deck_deadline", folderId
      ? { action: { kind: "disableFolderDeadline", id: folderId } }
      : { deckId: deck.id }).then(onSaved).catch((message: unknown) => setError(String(message)));
  };

  return (
    <section className="flex flex-col gap-3 border-b border-neutral-200 pb-4">
      <h2 className="text-lg font-semibold">Exam deadline</h2>
      <div className="flex flex-wrap items-end gap-4">
        <label className="flex flex-col gap-1 text-sm text-neutral-600">
          Exam date
          <input type="date" value={date} onChange={(event) => setDate(event.target.value)} className={TEXT_FIELD} />
        </label>
        <label className="flex flex-col gap-1 text-sm text-neutral-600">
          Time
          <input type="time" value={time} onChange={(event) => setTime(event.target.value)} className={TEXT_FIELD} />
        </label>
        <label className="flex flex-col gap-1 text-sm text-neutral-600">
          Minutes a day for this {folderId ? "folder (shared across decks)" : "deck"}
          <input
            type="number"
            min={1}
            value={dailyMinutes}
            placeholder="Shared"
            onChange={(event) => setDailyMinutes(event.target.value)}
            className={`${TEXT_FIELD} w-36`}
          />
        </label>
      </div>
      <p className="text-sm text-neutral-600">
        {time ? "Exam time" : `No time chosen, so this uses ${DEFAULT_EXAM_TIME}`} in {timeZone}. Leave
        minutes blank to share your daily study time with other decks.
      </p>
      <details>
        <summary className="cursor-pointer text-base">Advanced planning</summary>
        <div className="flex flex-wrap gap-4 pt-3">
          <label className="flex flex-col gap-1 text-sm text-neutral-600">
            Share of daily time you expect (0.1 to 1)
            <input
              type="number"
              step={0.05}
              value={reliability}
              placeholder="Learned from use"
              onChange={(event) => setReliability(event.target.value)}
              className={`${TEXT_FIELD} w-44`}
            />
          </label>
          <label className="flex flex-col gap-1 text-sm text-neutral-600">
            Target recall (0.5 to 0.99)
            <input
              type="number"
              step={0.01}
              value={targetRecall}
              placeholder="Desired retention"
              onChange={(event) => setTargetRecall(event.target.value)}
              className={`${TEXT_FIELD} w-44`}
            />
          </label>
          {!folderId && <label className="flex flex-col gap-1 text-sm text-neutral-600">
            Most new cards a day
            <input
              type="number"
              min={0}
              value={maxNew}
              placeholder="No limit"
              onChange={(event) => setMaxNew(event.target.value)}
              className={`${TEXT_FIELD} w-44`}
            />
          </label>}
        </div>
      </details>
      {error && (
        <p role="alert" className="text-base text-rose-700">
          {error}
        </p>
      )}
      <div className="flex gap-3">
        <HandButton seed={71} variant="primary" onClick={save}>
          {deck.mode === "deadline" ? "Update deadline" : "Plan for this exam"}
        </HandButton>
        {deck.mode === "deadline" && (
          <HandButton seed={72} onClick={disable}>
            Turn off deadline
          </HandButton>
        )}
      </div>
    </section>
  );
}

function ExpiredPanel({ deck, onResume }: { deck: DeckSummary; onResume: () => void }) {
  return (
    <section className="flex flex-col items-start gap-2 border-b border-neutral-200 pb-4">
      <h2 className="text-lg font-semibold">This exam has passed</h2>
      <p className="text-base text-neutral-600">
        {deck.deadline && formatExam(deck.deadline.exam)}. Your cards and history are unchanged, and this
        deck is using ordinary review. Set a new date below to reschedule.
      </p>
      <HandButton seed={73} onClick={onResume}>
        Resume ordinary review
      </HandButton>
    </section>
  );
}

function CardItem({ card, onChanged }: { card: CardRow; onChanged: () => void }) {
  const act = (command: string, args: Record<string, unknown>) => {
    invoke(command, { key: card.key, ...args }).then(onChanged).catch(console.error);
  };

  return (
    <li className="border-b border-neutral-200 py-2">
      <details>
        <summary className="flex cursor-pointer items-baseline justify-between gap-4">
          <span className="text-base font-medium wrap-anywhere">{card.front}</span>
          <span className="flex shrink-0 items-center gap-3 self-center text-sm text-neutral-600">
            {card.flagged && <PenStat drawing="flag" label="Flagged" className="text-amber-700" />}
            {card.changed && <PenStat drawing="pencil" label="Changed" />}
            {card.reviews === 0 ? (
              <PenStat drawing="sparkle" label="Unseen" />
            ) : (
              <PenStat drawing="answer" value={percent(card.recallNow ?? 0)}
                label={`Recall now ${percent(card.recallNow ?? 0)}`} />
            )}
            <PenStat drawing="star" value={card.importance} label={`Importance ${card.importance}`} />
          </span>
        </summary>
        <div className="flex flex-col gap-2 py-2 text-base">
          <p className="wrap-anywhere">{card.back}</p>
          {card.cardType && <p className="text-sm text-neutral-600">{card.cardType}</p>}
          <p className="text-sm text-neutral-600">Source: {card.source ?? "None given"}</p>
          {card.importanceReason && (
            <p className="text-sm text-neutral-600">Why this importance: {card.importanceReason}</p>
          )}
          {card.assessmentEvidence && (
            <p className="text-sm text-neutral-600">Assessment evidence: {card.assessmentEvidence}</p>
          )}
          <div className="flex items-center gap-4 text-sm text-neutral-600">
            <PenStat drawing="cards" value={card.reviews}
              label={`${card.reviews} ${card.reviews === 1 ? "review" : "reviews"}`} />
            <PenStat drawing="retry" value={card.lapses}
              label={`${card.lapses} ${card.lapses === 1 ? "lapse" : "lapses"}`} />
          </div>
          <label className="flex items-center gap-3 text-sm text-neutral-600">
            Importance, a study priority you can edit
            <select
              value={card.importance}
              onChange={(event) => act("set_card_importance", { importance: Number(event.target.value) })}
              className={TEXT_FIELD}
            >
              {[1, 2, 3, 4, 5].map((value) => (
                <option key={value} value={value}>
                  {value}
                  {value === card.suggestedImportance ? " (suggested)" : ""}
                </option>
              ))}
            </select>
          </label>
          <div className="flex flex-wrap gap-3">
            <HandButton seed={81} onClick={() => act("set_card_flag", { flagged: !card.flagged })}>
              {card.flagged ? "Return to review" : "Needs explanation"}
            </HandButton>
            {card.changed && (
              <HandButton seed={82} onClick={() => act("keep_card_history", {})}>
                Keep history
              </HandButton>
            )}
            {card.reviews > 0 && (
              <HandButton seed={83} onClick={() => act("reset_card_learning", {})}>
                Reset learning state
              </HandButton>
            )}
          </div>
        </div>
      </details>
    </li>
  );
}

interface DeckViewProps {
  deck: DeckSummary;
  measured: Measured;
  onBack: () => void;
}

export default function DeckView({ deck, measured, onBack }: DeckViewProps) {
  const [cards, setCards] = useState<CardRow[]>([]);
  const [version, setVersion] = useState(0);
  const refresh = () => setVersion((value) => value + 1);

  useEffect(() => {
    let current = true;
    invoke<CardRow[]>("deck_cards", { deckId: deck.id })
      .then((rows) => {
        if (current) setCards(rows);
      })
      .catch(console.error);
    return () => {
      current = false;
    };
  }, [deck.id, deck.seen, deck.flagged, deck.changed, version]);

  const changed = cards.filter((card) => card.changed);
  const resumeOrdinary = () => {
    invoke("disable_deck_deadline", { deckId: deck.id }).then(refresh).catch(console.error);
  };

  return (
    <main className={PAGE}>
      <header className="flex items-start justify-between gap-6">
        <div className="flex flex-col gap-1">
          <h1 className="text-4xl font-bold wrap-anywhere">{deck.name}</h1>
          <p className="text-base text-neutral-600">
            {deck.cardCount} cards, {deck.unseen} unseen, {deck.due} due.{" "}
            {deck.mode === "ordinary" ? "Ordinary FSRS review." : ""}
          </p>
        </div>
        <HandButton seed={12} onClick={onBack}>
          Back
        </HandButton>
      </header>

      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto pr-2">
        {deck.orphaned > 0 && (
          <p className="text-base text-neutral-700">
            {deck.orphaned === 1
              ? "1 reviewed card is no longer in this deck. Its history is kept and returns if it does."
              : `${deck.orphaned} reviewed cards are no longer in this deck. Their history is kept and returns if they do.`}
          </p>
        )}
        {deck.mode === "expired" && <ExpiredPanel deck={deck} onResume={resumeOrdinary} />}
        {deck.plan && <PlanPanel plan={deck.plan} deck={deck} measured={measured} />}
        {deck.mode === "deadline" && !deck.plan && (
          <p className="text-base text-neutral-600">Working out your plan...</p>
        )}
        {deck.folderId && (
          <div className="flex flex-col items-start gap-2">
            <p className="text-sm text-neutral-600">
              {deck.deadlineInherited ? "Using the folder deadline. Saving here creates a deck override." : "This deck can use its own deadline or inherit its folder deadline."}
            </p>
            {!deck.deadlineInherited && <HandButton seed={74} onClick={() => {
              invoke("organize_library", { action: { kind: "inheritDeadline", deck: deck.id } }).then(refresh).catch(console.error);
            }}>Use folder deadline</HandButton>}
          </div>
        )}
        <DeadlineForm key={`${deck.id}-${deck.deadline?.exam.utcMs ?? 0}-${deck.deadlineInherited}-${deck.deadline?.enabled}`} deck={deck} onSaved={refresh} />

        {changed.length > 0 && (
          <p className="text-base font-medium text-amber-800">
            {changed.length} reviewed {changed.length === 1 ? "card has" : "cards have"} new wording. Open
            each one below to keep its history or reset it if it now tests something different.
          </p>
        )}
        <section className="flex flex-col">
          <h2 className="text-lg font-semibold">Cards</h2>
          <ul className="flex flex-col border-t border-neutral-200">
            {cards.map((card) => (
              <CardItem key={card.key} card={card} onChanged={refresh} />
            ))}
          </ul>
        </section>
      </div>
    </main>
  );
}
