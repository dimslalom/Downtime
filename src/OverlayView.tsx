import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useReducer, type ReactNode } from "react";
import { HandPanel } from "./components/HandDrawn";
import Lettered from "./components/Lettered";
import { PEN_DRAWINGS, PenIcon, PenStat } from "./components/PenIcon";
import { useHotkeys, useLibrary, useOverlayVisibility, useStudyKeys, useStudyView } from "./hooks";
import { mockHotkeySource } from "./services/gameEvents";
import { formatHotkey, type Placement } from "./services/hotkey";
import { initialStudyState, studyReducer, type PeekRest, type StudyState } from "./study";
import { PLACEMENT_ALIGN } from "./styles";
import type { CardType, CardView, DeckError, Mode, SessionView } from "./types";

const FRONT_TEXT_SIZES = [
  { upTo: 60, className: "text-[clamp(2rem,3.5vw,3.5rem)]" },
  { upTo: 140, className: "text-[clamp(1.75rem,3vw,3rem)]" },
  { upTo: 280, className: "text-[clamp(1.5rem,2.5vw,2.5rem)]" },
  { upTo: 500, className: "text-[clamp(1.25rem,2vw,2rem)]" },
];

const PEEK_TEXT_SIZES = [
  { upTo: 60, className: "text-[clamp(1.5rem,1.7vw,2.25rem)]" },
  { upTo: 140, className: "text-[clamp(1.25rem,1.4vw,1.875rem)]" },
  { upTo: 280, className: "text-[clamp(1.125rem,1.2vw,1.5rem)]" },
];

const FLIPPED_OUTLINE = "stroke-emerald-600 stroke-3";

/** Long enough to take one more, short enough not to hold up a respawn. */
const PEEK_REST_MS = 3000;

function textSizeFor(text: string): string {
  return FRONT_TEXT_SIZES.find(({ upTo }) => text.length <= upTo)?.className ?? "text-[clamp(1.125rem,1.75vw,1.75rem)]";
}

function peekTextSizeFor(text: string): string {
  return PEEK_TEXT_SIZES.find(({ upTo }) => text.length <= upTo)?.className ?? "text-base";
}

function PaperKey({ label, hint }: { label: string; hint: string }) {
  return (
    <span className="flex items-center gap-2">
      <kbd className="rounded border border-neutral-300 bg-neutral-50 px-2 py-0.5 font-sans text-sm text-ink">
        {label}
      </kbd>
      <span className="text-sm text-neutral-600">{hint}</span>
    </span>
  );
}

function Key({ label, hint, dimmed }: { label: string; hint: string; dimmed?: boolean }) {
  return (
    <span className={`flex items-center gap-3 ${dimmed ? "opacity-30" : "opacity-70"}`}>
      <kbd className="rounded border border-white/35 px-2 py-0.5 font-sans text-sm text-paper">
        {label}
      </kbd>
      <span className="text-sm text-neutral-200">{hint}</span>
    </span>
  );
}

interface KeyHintsProps {
  session: SessionView | null;
  deckCount: number;
  hideKey: string | undefined;
}

/** Only the next move: Peek is read at a glance, and the other keys still work. */
function PeekHints({ session, hideKey }: Omit<KeyHintsProps, "deckCount">) {
  const card = session?.card;
  return (
    <footer aria-label="Keyboard controls" className="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1">
      {session && (!card || !session.flipped) && <PaperKey label="Space" hint={card ? "Flip" : "Keep going"} />}
      {card && session?.flipped && <PaperKey label="←" hint="Failed" />}
      {card && session?.flipped && <PaperKey label="→" hint="Passed" />}
      {hideKey && <PaperKey label={formatHotkey(hideKey)} hint="Hide" />}
    </footer>
  );
}

function KeyHints({ session, deckCount, hideKey }: KeyHintsProps) {
  const card = session?.card;
  const canGrade = session?.flipped ?? false;

  return (
    <footer aria-label="Keyboard controls" className="flex shrink-0 flex-wrap items-center gap-x-5 gap-y-2 pt-1">
      {session && <Key label="Space" hint={card ? "Flip" : "Keep going"} />}
      {card && <Key label="←" hint="Failed" dimmed={!canGrade} />}
      {card && <Key label="→" hint="Passed" dimmed={!canGrade} />}
      {card && <Key label="1–4" hint="Again, Hard, Good, Easy" dimmed={!canGrade} />}
      {card && <Key label="N" hint={card.flagged ? "Remove flag" : "Needs explanation"} />}
      {card && (card.source || card.importanceReason) && <Key label="I" hint="Source" />}
      {deckCount > 1 && <Key label="Tab" hint="Next deck" />}
      {hideKey && <Key label={formatHotkey(hideKey)} hint="Hide" />}
    </footer>
  );
}

interface SheetProps {
  seed: number;
  className: string;
  outlineClassName?: string;
  children: ReactNode;
}

/** A white sheet with the pen frame inset, because ink on its edge would vanish into the backdrop. */
function Sheet({ seed, className, outlineClassName = "stroke-ink stroke-2", children }: SheetProps) {
  return (
    <div className="flex min-h-0 w-full flex-col rounded-sm bg-paper p-3">
      <HandPanel seed={seed} outlineClassName={`fill-paper ${outlineClassName}`} className={`min-h-0 ${className}`}>
        {children}
      </HandPanel>
    </div>
  );
}

function CardInfo({ card }: { card: CardView }) {
  return (
    <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1 border-t border-neutral-300 pt-4 text-2xl text-neutral-700">
      {card.source && (
        <>
          <dt className="font-semibold">Source</dt>
          <dd className="wrap-anywhere">{card.source}</dd>
        </>
      )}
      <dt className="font-semibold">Importance</dt>
      <dd>
        {card.importance} of 5{card.importanceReason ? `: ${card.importanceReason}` : ""}
      </dd>
    </dl>
  );
}

const CARD_TYPE_DRAWINGS: Record<CardType, ReactNode> = {
  Definition: PEN_DRAWINGS.book,
  Relationship: <>
    <path d="m9.2 14.8 5.6-5.6M7.1 12.5l-2 2c-3.6 3.8 1.6 8.9 5.3 5.2l3.4-3.4c1.8-1.9 1.7-4.1.1-5.8M16.9 11.5l2-2c3.6-3.8-1.6-8.9-5.3-5.2l-3.4 3.4c-1.8 1.9-1.7 4.1-.1 5.8" />
  </>,
  Exception: <>
    <path d="M3 6h3.5v3.5H3zM10 6h3.5v3.5H10zM17 6h3.5v3.5H17zM3 15h3.5v3.5H3zM10 15h3.5v3.5H10z" />
    <circle cx="18.7" cy="16.8" r="2.7" />
  </>,
  "Method Choice": <>
    <path d="M11.9 21v-7c0-4-6.5-3.5-6.5-7V3.5M2.4 6.5l3-3.2 3 3M12 14c0-4 6.5-3.5 6.5-7V3.5M15.5 6.5l3-3.2 3 3" />
  </>,
};

function CardCues({ card, flipped }: { card: CardView; flipped: boolean }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="flex items-center gap-3">
        {flipped ? (
          <PenIcon label="Answer" className="text-emerald-700">{PEN_DRAWINGS.answer}</PenIcon>
        ) : (
          <PenIcon label="Question">{PEN_DRAWINGS.question}</PenIcon>
        )}
        {card.cardType && <PenIcon label={card.cardType}>{CARD_TYPE_DRAWINGS[card.cardType]}</PenIcon>}
      </div>
      <div className="flex items-center gap-3">
        {card.flagged && <PenIcon label="Flagged" className="text-amber-700">{PEN_DRAWINGS.flag}</PenIcon>}
        {card.retry && (
          <PenIcon label="Retry: missed last time" className="rounded-full bg-amber-50 p-1 text-amber-800">
            {PEN_DRAWINGS.retry}
          </PenIcon>
        )}
      </div>
    </div>
  );
}

function Progress({ session, size }: { session: SessionView; size?: string }) {
  return (
    <span className="flex shrink-0 items-center gap-4">
      <PenStat drawing="tick" label={`${session.passed} passed`} value={session.passed} size={size} />
      <PenStat drawing="cards" label={`${session.remaining} left`} value={session.remaining} size={size} />
    </span>
  );
}

function StudyCard({ session, showInfo, peek }: { session: SessionView; showInfo: boolean; peek: boolean }) {
  const card = session.card;
  if (!card) return null;

  const text = session.flipped ? card.back : card.front;
  const content = (
    <div
      key={`${card.attemptId}-${session.flipped}`}
      className={`animate-card-in flex flex-col motion-reduce:animate-none ${peek ? "gap-3" : "gap-6"}`}
    >
      <CardCues card={card} flipped={session.flipped} />
      <p
        className={`${peek ? peekTextSizeFor(text) : textSizeFor(text)} leading-tight font-semibold wrap-break-word whitespace-pre-wrap text-ink`}
      >
        {text}
      </p>
      {showInfo && <CardInfo card={card} />}
    </div>
  );
  if (peek) return content;
  return (
    <Sheet
      seed={58}
      outlineClassName={session.flipped ? FLIPPED_OUTLINE : undefined}
      className="flex min-h-[18rem] flex-col justify-center px-6 py-7 sm:px-9 short:min-h-0 short:py-5"
    >
      {content}
    </Sheet>
  );
}

function nextDueText(nextDueMs: number | null): string | null {
  if (nextDueMs === null) return null;
  const minutes = Math.ceil((nextDueMs - Date.now()) / 60_000);
  if (minutes < 60) return `Next card in ${minutes} ${minutes === 1 ? "minute" : "minutes"}.`;
  const hours = Math.round(minutes / 60);
  if (hours < 36) return `Next card in about ${hours} ${hours === 1 ? "hour" : "hours"}.`;
  return `Next card in about ${Math.round(hours / 24)} days.`;
}

function FinishedView({ session, peek }: { session: SessionView; peek: boolean }) {
  const reviewed = session.passed + session.failed;
  const plan = session.mode === "deadline";
  if (peek) {
    return (
      <div className="flex flex-col gap-2">
        <p className="text-2xl font-semibold text-ink">{plan ? "Plan done" : "All caught up"}</p>
        <p className="text-base text-neutral-600">
          {nextDueText(session.nextDueMs) ?? "Press Space for extra practice."}
        </p>
      </div>
    );
  }
  return (
    <Sheet seed={58} className="flex min-h-[18rem] flex-col justify-center gap-6 px-8 py-8">
      <Lettered word="deckComplete" as="h2" className="h-24 text-ink" />
      <p className="text-2xl text-neutral-600">{plan ? "Today's plan is done." : "Nothing else is due."}</p>
      <div className="flex items-center gap-8 text-2xl text-neutral-600">
        <PenStat drawing="cards" size="size-7" value={reviewed}
          label={`${reviewed} ${reviewed === 1 ? "review" : "reviews"}`} />
        <PenStat drawing="retry" size="size-7" value={session.failed}
          label={`${session.failed} ${session.failed === 1 ? "miss" : "misses"}`} />
      </div>
      <p className="text-xl text-neutral-600">
        {nextDueText(session.nextDueMs) ?? "Press Space to keep going with extra practice."}
      </p>
    </Sheet>
  );
}

const MODE_LABELS: Record<Mode, string | null> = {
  ordinary: null,
  deadline: "Exam plan",
  expired: "Exam passed: ordinary review",
};

function EmptyView({ folder, peek }: { folder: string | undefined; peek: boolean }) {
  if (peek) {
    return (
      <div className="flex flex-col gap-2">
        <p className="text-2xl font-semibold text-ink">No decks selected</p>
        <p className="text-sm break-all text-neutral-600">{folder ?? "Loading..."}</p>
      </div>
    );
  }
  return (
    <Sheet seed={63} className="flex flex-col gap-6 px-8 py-8">
      <p className="text-4xl font-semibold text-ink">No decks selected</p>
      <p className="text-xl text-neutral-600">Select a folder or deck for the overlay in Downtime, or add a deck to:</p>
      <p className="text-lg break-all text-ink">{folder ?? "Loading..."}</p>
    </Sheet>
  );
}

function SkippedFiles({ errors, className }: { errors: DeckError[]; className: string }) {
  if (errors.length === 0) return null;
  return (
    <p role="status" className={`text-sm ${className}`}>
      Skipped {errors.length === 1 ? "a file" : `${errors.length} files`} with errors:{" "}
      {errors.map(({ file }) => file).join(", ")}
    </p>
  );
}

function RestChip({ rest, hideKey }: { rest: PeekRest; hideKey: string | undefined }) {
  return (
    <div role="status" className="flex items-center gap-4">
      {rest === "passed" ? (
        <PenIcon label="Passed" className="text-emerald-700">{PEN_DRAWINGS.tick}</PenIcon>
      ) : (
        <PenIcon label="Coming back soon" className="text-amber-800">{PEN_DRAWINGS.retry}</PenIcon>
      )}
      <PaperKey label="Space" hint="One more" />
      {hideKey && <PaperKey label={formatHotkey(hideKey)} hint="Hide" />}
    </div>
  );
}

function SaveNotice({ notice, className }: { notice: string; className: string }) {
  return (
    <p role="alert" className={className}>
      Could not save that. Your card is still here. {notice}
    </p>
  );
}

/** One white sheet holds everything, so nothing needs a dark box to read over the game. */
function PeekSheet({ study, hideKey }: Pick<OverlayContentProps, "study" | "hideKey">) {
  const session = study.view?.session ?? null;
  const flipped = !study.rest && session?.card && session.flipped;

  return (
    <Sheet
      seed={71}
      outlineClassName={flipped ? FLIPPED_OUTLINE : undefined}
      className="flex flex-col gap-3 px-5 py-4"
    >
      <SkippedFiles errors={study.library?.errors ?? []} className="text-amber-800" />
      {study.rest ? (
        <RestChip rest={study.rest} hideKey={hideKey} />
      ) : (
        <>
          {session && (
            <header className="flex shrink-0 items-start justify-between gap-4 text-sm text-neutral-600">
              <span className="min-w-0 font-medium wrap-anywhere">{session.deckName}</span>
              <Progress session={session} />
            </header>
          )}
          <div
            key={session?.card?.attemptId ?? `${session?.deckId}-empty`}
            className={`overlay-card-area min-h-0 overflow-y-auto pb-1 ${study.cardMotion === "fail" ? "overlay-fail" : "overlay-advance"}`}
          >
            {!session && <EmptyView folder={study.library?.folder} peek />}
            {session?.card && <StudyCard session={session} showInfo={study.showInfo} peek />}
            {session && !session.card && <FinishedView session={session} peek />}
          </div>
          <PeekHints session={session} hideKey={hideKey} />
        </>
      )}
      {study.notice && <SaveNotice notice={study.notice} className="text-sm text-amber-800" />}
    </Sheet>
  );
}

interface OverlayContentProps {
  study: StudyState;
  hideKey?: string;
  peek?: boolean;
  placement?: Placement;
}

export function OverlayContent({ study, hideKey, peek = false, placement = "right" }: OverlayContentProps) {
  const session = study.view?.session ?? null;
  const modeLabel = session ? MODE_LABELS[session.mode] : null;

  if (peek) {
    return (
      <main
        onContextMenu={(event) => event.preventDefault()}
        className={`overlay-peek-stage flex h-dvh select-none ${PLACEMENT_ALIGN[placement]}`}
      >
        <div className="overlay-peek flex min-h-0 flex-col">
          <PeekSheet study={study} hideKey={hideKey} />
        </div>
      </main>
    );
  }

  return (
    <main
      onContextMenu={(event) => event.preventDefault()}
      className="overlay-stage flex h-dvh items-center bg-black/60 select-none"
    >
      <div className="overlay-stack flex min-h-0 flex-col gap-4">
        <SkippedFiles errors={study.library?.errors ?? []} className="text-amber-200" />
        {session && (
          <header className="flex shrink-0 flex-wrap items-center justify-between gap-x-5 gap-y-1 text-lg font-semibold text-neutral-100">
            <span className="min-w-0 wrap-anywhere">
              {session.deckName}
              {modeLabel && <span className="ml-3 text-sm font-normal text-neutral-300">{modeLabel}</span>}
            </span>
            <span className="text-sm font-normal text-neutral-300"><Progress session={session} /></span>
          </header>
        )}
        <div
          key={session?.card?.attemptId ?? `${session?.deckId}-empty`}
          className={`overlay-card-area min-h-0 overflow-y-auto ${study.cardMotion === "fail" ? "overlay-fail" : "overlay-advance"}`}
        >
          {!session && <EmptyView folder={study.library?.folder} peek={false} />}
          {session?.card && <StudyCard session={session} showInfo={study.showInfo} peek={false} />}
          {session && !session.card && <FinishedView session={session} peek={false} />}
        </div>
        {study.notice && <SaveNotice notice={study.notice} className="text-base text-amber-200" />}
        <KeyHints session={session} deckCount={study.view?.deckCount ?? 0} hideKey={hideKey} />
      </div>
    </main>
  );
}

export default function OverlayView() {
  const [study, dispatch] = useReducer(studyReducer, initialStudyState);
  const endRest = useCallback(() => dispatch({ type: "endRest" }), []);
  const visible = useOverlayVisibility(mockHotkeySource, endRest);
  useLibrary(dispatch);
  useStudyView(dispatch);
  const hotkeys = useHotkeys();
  const peek = hotkeys?.peek ?? true;
  useStudyKeys(visible, hotkeys?.hideKey, peek, study, dispatch);

  useEffect(() => {
    if (!visible || !peek || !study.rest) return;
    const timer = setTimeout(() => {
      invoke("fire_mock_event", { event: "respawn" }).catch(console.error);
    }, PEEK_REST_MS);
    return () => clearTimeout(timer);
  }, [visible, peek, study.rest]);

  return <OverlayContent study={study} hideKey={hotkeys?.hideKey} peek={peek} placement={hotkeys?.placement} />;
}
