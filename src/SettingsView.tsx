import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { HandButton, HandOutline } from "./components/HandDrawn";
import Lettered from "./components/Lettered";
import { formatHotkey, shortcutFromEvent, type Hotkeys, type Placement } from "./services/hotkey";
import { KEYCAP, PAGE, PLACEMENT_ALIGN, TEXT_FIELD } from "./styles";
import type { HomeSnapshot, StudySettings } from "./types";

type SetCommand = "set_open_hotkey" | "set_hide_key";

/** Laid out like the screen. The gap in the middle is the crosshair. */
const PLACEMENT_CELLS: ({ placement: Placement; label: string } | null)[] = [
  { placement: "top-left", label: "Top left" },
  { placement: "top", label: "Top" },
  { placement: "top-right", label: "Top right" },
  { placement: "left", label: "Left" },
  null,
  { placement: "right", label: "Right" },
  { placement: "bottom-left", label: "Bottom left" },
  { placement: "bottom", label: "Bottom" },
  { placement: "bottom-right", label: "Bottom right" },
];

function PlacementPicker({ value, onChange }: { value: Placement; onChange: (placement: Placement) => void }) {
  return (
    <div role="group" aria-label="Card placement" className="relative isolate grid aspect-video w-64 shrink-0 grid-cols-3 grid-rows-3 p-2">
      <HandOutline seed={37} shape="panel" className="fill-paper stroke-ink stroke-[1.5]" />
      {PLACEMENT_CELLS.map((cell) =>
        cell ? (
          <button
            key={cell.placement}
            type="button"
            aria-label={cell.label}
            aria-pressed={cell.placement === value}
            onClick={() => onChange(cell.placement)}
            className={`group flex cursor-pointer rounded-sm p-1 outline-emerald-700 focus-visible:outline-2 ${PLACEMENT_ALIGN[cell.placement]}`}
          >
            <span className={`h-5 w-9 rounded-xs ${cell.placement === value ? "bg-ink" : "bg-neutral-300 group-hover:bg-neutral-500"}`} />
          </button>
        ) : (
          <span key="crosshair" aria-hidden className="place-self-center text-lg leading-none text-neutral-400">+</span>
        ),
      )}
    </div>
  );
}

interface KeyRowProps {
  title: string;
  description: string;
  value: string | undefined;
  capturing: boolean;
  disabled: boolean;
  seed: number;
  onChange: () => void;
  onCancel: () => void;
}

function KeyRow({
  title,
  description,
  value,
  capturing,
  disabled,
  seed,
  onChange,
  onCancel,
}: KeyRowProps) {
  return (
    <div className="flex items-center justify-between gap-8 border-b border-neutral-200 py-5">
      <div className="flex flex-col gap-1">
        <h2 className="text-xl font-semibold text-ink">{title}</h2>
        <p className={`text-base ${capturing ? "font-medium text-ink" : "text-neutral-600"}`}>
          {capturing ? "Press the key or combination you want." : description}
        </p>
      </div>
      {capturing ? (
        <HandButton seed={seed} onClick={onCancel}>
          Cancel
        </HandButton>
      ) : (
        <div className="flex shrink-0 items-center gap-4">
          {value && <kbd className={KEYCAP}>{formatHotkey(value)}</kbd>}
          <HandButton seed={seed} disabled={disabled} onClick={onChange}>
            Change
          </HandButton>
        </div>
      )}
    </div>
  );
}

function SchedulingForm({ settings }: { settings: StudySettings }) {
  const [retention, setRetention] = useState(Math.round(settings.desired_retention * 100).toString());
  const [newPerDay, setNewPerDay] = useState(settings.new_per_day.toString());
  const [dailyMinutes, setDailyMinutes] = useState(settings.daily_minutes.toString());
  const [status, setStatus] = useState<string | null>(null);

  const save = () => {
    const next: StudySettings = {
      desired_retention: Number(retention) / 100,
      new_per_day: Number(newPerDay),
      daily_minutes: Number(dailyMinutes),
    };
    invoke("set_study_settings", { settings: next })
      .then(() => setStatus("Saved."))
      .catch((message: unknown) => setStatus(String(message)));
  };

  return (
    <div className="flex flex-col gap-3 border-b border-neutral-200 py-5">
      <h2 className="text-xl font-semibold text-ink">Study schedule</h2>
      <div className="flex flex-wrap items-end gap-4">
        <label className="flex flex-col gap-1 text-sm text-neutral-600">
          Desired retention (%)
          <input
            type="number"
            min={70}
            max={97}
            value={retention}
            onChange={(event) => setRetention(event.target.value)}
            className={`${TEXT_FIELD} w-40`}
          />
        </label>
        <label className="flex flex-col gap-1 text-sm text-neutral-600">
          New cards a day, per deck
          <input
            type="number"
            min={0}
            value={newPerDay}
            onChange={(event) => setNewPerDay(event.target.value)}
            className={`${TEXT_FIELD} w-48`}
          />
        </label>
        <label className="flex flex-col gap-1 text-sm text-neutral-600">
          Daily study time, all decks (min)
          <input
            type="number"
            min={1}
            value={dailyMinutes}
            onChange={(event) => setDailyMinutes(event.target.value)}
            className={`${TEXT_FIELD} w-56`}
          />
        </label>
        <HandButton seed={33} onClick={save}>
          Save
        </HandButton>
      </div>
      <p className="text-sm text-neutral-600">
        Decks without a deadline use standard FSRS at this retention. Exam plans share the daily study
        time and do not change it.
      </p>
      {status && <p className="text-base text-ink">{status}</p>}
    </div>
  );
}

function ModelPanel({ home }: { home: HomeSnapshot }) {
  const { model, fitting } = home;
  const fit = () => {
    invoke("fit_memory_model").catch(console.error);
  };

  return (
    <div className="flex items-start justify-between gap-8 border-b border-neutral-200 py-5">
      <div className="flex flex-col gap-1">
        <h2 className="text-xl font-semibold text-ink">Memory model</h2>
        <p className="text-base text-neutral-600">
          {model.personalized
            ? `Personalized from ${model.fitItems} delayed reviews.`
            : "Using the standard FSRS parameters."}{" "}
          {model.modelVersion}. {model.reviewCount} reviews recorded, {model.availableItems} usable for
          fitting.
        </p>
        {model.limitedData && (
          <p className="text-base text-amber-800">
            This fit rests on limited data, so treat it as provisional.
          </p>
        )}
        {model.lastFitError && (
          <p role="alert" className="text-base text-rose-700">
            {model.lastFitError}
          </p>
        )}
      </div>
      <HandButton seed={34} disabled={fitting} onClick={fit}>
        {fitting ? "Fitting..." : "Personalize"}
      </HandButton>
    </div>
  );
}

interface SettingsViewProps {
  hotkeys: Hotkeys | null;
  home: HomeSnapshot | null;
  onBack: () => void;
}

export default function SettingsView({ hotkeys, home, onBack }: SettingsViewProps) {
  const [capturing, setCapturing] = useState<SetCommand | null>(null);
  const [error, setError] = useState<string | null>(null);
  const peek = hotkeys?.peek ?? true;

  const setPeek = (next: boolean) => {
    invoke("set_peek", { peek: next }).catch((message: unknown) => setError(String(message)));
  };

  const setPlacement = (placement: Placement) => {
    invoke("set_placement", { placement }).catch((message: unknown) => setError(String(message)));
  };

  useEffect(() => {
    if (!capturing) return;

    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      const shortcut = shortcutFromEvent(event);
      if (!shortcut) return;
      setCapturing(null);
      invoke(capturing, { shortcut }).catch((message: unknown) => setError(String(message)));
    };
    const cancel = () => {
      setCapturing(null);
      invoke("suspend_open_hotkey", { suspended: false }).catch(console.error);
    };

    invoke("suspend_open_hotkey", { suspended: true }).catch(console.error);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("blur", cancel);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("blur", cancel);
    };
  }, [capturing]);

  const startCapturing = (command: SetCommand) => {
    setError(null);
    setCapturing(command);
  };

  const cancelCapturing = () => {
    setCapturing(null);
    invoke("suspend_open_hotkey", { suspended: false }).catch(console.error);
  };

  return (
    <main className={PAGE}>
      <header className="flex items-start justify-between gap-6">
        <Lettered word="settings" as="h1" className="h-16 text-ink" />
        <HandButton seed={12} onClick={onBack} disabled={capturing !== null}>
          Back
        </HandButton>
      </header>

      <section className="flex min-h-0 flex-1 flex-col overflow-y-auto border-t border-neutral-200 pr-2">
        <div className="flex items-center justify-between gap-8 border-b border-neutral-200 py-5">
          <div className="flex flex-col gap-1">
            <h2 className="text-xl font-semibold text-ink">Overlay style</h2>
            <p className="text-base text-neutral-600">
              {peek
                ? "Peek: one small card at the side, with your game still in full view."
                : "Full screen: a large card over a dimmed game."}
            </p>
          </div>
          <div role="group" aria-label="Overlay style" className="flex shrink-0 items-center gap-2">
            <HandButton seed={35} variant={peek ? "primary" : "secondary"} aria-pressed={peek} onClick={() => setPeek(true)}>
              Peek
            </HandButton>
            <HandButton seed={36} variant={peek ? "secondary" : "primary"} aria-pressed={!peek} onClick={() => setPeek(false)}>
              Full screen
            </HandButton>
          </div>
        </div>
        {peek && (
          <div className="flex items-center justify-between gap-8 border-b border-neutral-200 py-5">
            <div className="flex flex-col gap-1">
              <h2 className="text-xl font-semibold text-ink">Card placement</h2>
              <p className="text-base text-neutral-600">Pick a spot your game leaves free.</p>
            </div>
            <PlacementPicker value={hotkeys?.placement ?? "right"} onChange={setPlacement} />
          </div>
        )}
        <KeyRow
          title="Open overlay"
          seed={31}
          description="Opens and closes the overlay from anywhere. Other apps won't receive this key."
          value={hotkeys?.openHotkey}
          capturing={capturing === "set_open_hotkey"}
          disabled={capturing !== null}
          onChange={() => startCapturing("set_open_hotkey")}
          onCancel={cancelCapturing}
        />
        <KeyRow
          title="Hide overlay"
          seed={32}
          description="Also closes the overlay. It only listens while the overlay is open, so your game keeps this key."
          value={hotkeys?.hideKey}
          capturing={capturing === "set_hide_key"}
          disabled={capturing !== null}
          onChange={() => startCapturing("set_hide_key")}
          onCancel={cancelCapturing}
        />
        {error && (
          <p role="alert" className="pt-4 text-base text-rose-700">
            {error}
          </p>
        )}
        {home && <SchedulingForm settings={home.settings} />}
        {home && <ModelPanel home={home} />}
      </section>
    </main>
  );
}
