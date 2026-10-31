import { useId, useRef, useState, type ReactNode } from "react";
import { HandButton } from "./HandDrawn";
import { TEXT_FIELD } from "../styles";

export type LibraryIconName = "folder" | "folderPlus" | "deckPlus" | "chevron" | "more" | "check" | "close";

/** Each icon as drawn at rest, then as redrawn on hover. */
const LIBRARY_DRAWINGS: Record<LibraryIconName, [ReactNode, ReactNode]> = {
  folder: [
    <path d="M3 7V4.5l6-.2 2.2 2.6 9.6.3-.4 12-17.2.3L3 7Z" />,
    <path d="m3.200 7.400-.3-2.600 6.200-.5 2.100 2.900 9.500-.1.100 12.100-17.600.1.300-11.800Z" />,
  ],
  folderPlus: [
    <path d="M3 7V4.5l6-.2 2.2 2.6 9.6.3-.4 12-17.2.3L3 7ZM9 13h6M12 10v6" />,
    <path d="m3.200 7.400-.3-2.600 6.200-.5 2.100 2.900 9.500-.1.100 12.100-17.600.1.300-11.800ZM8.800 13.300c2-.4 4-.4 6.400-.2M12.200 9.800c-.4 2-.4 4.200 0 6.400" />,
  ],
  deckPlus: [
    <path d="m7 3 13 .4-.3 16-13-.2L7 3ZM3.500 6.500l-.2 15 12.500.1M10 11.300h7M13.500 7.800v7" />,
    <path d="m7.300 2.700 12.800.8-.6 15.900-12.900-.5.700-16.200ZM3.300 6.900l.1 14.700 12.700-.4M10.200 11c2.200-.3 4.600-.3 6.900.4M13.200 7.600c.5 2.300.5 4.700.2 7.300" />,
  ],
  chevron: [
    <path d="m9 5 6.200 7L9 19" />,
    <path d="M8.700 4.700c2.300 2.300 4.300 4.800 6.800 7.600-2.200 2.300-4.300 4.600-6.300 7" />,
  ],
  more: [
    <><circle cx="5" cy="12" r=".9" /><circle cx="12" cy="12" r=".9" /><circle cx="19" cy="12" r=".9" /></>,
    <><circle cx="5" cy="11.400" r="1" /><circle cx="12" cy="12.600" r=".8" /><circle cx="19" cy="11.700" r="1.100" /></>,
  ],
  check: [
    <path d="m5 12 4.500 5L19 6" />,
    <path d="M4.600 12.600c1.800 1.400 3.300 3 4.700 4.900C12 13.600 15.500 9.600 19.600 5.700" />,
  ],
  close: [
    <path d="m6 6 12 12M18 6 6 18" />,
    <path d="M5.600 6.400c4.200 3.600 8.600 7.600 12.800 11.200M18.400 5.600c-4.100 4-8.400 8.600-12.600 12.900" />,
  ],
};

export function LibraryIcon({ name, className = "size-5" }: { name: LibraryIconName; className?: string }) {
  const [rest, hover] = LIBRARY_DRAWINGS[name];
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" className={`${className} shrink-0 fill-none stroke-current`}
      strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
      <g className="hand-rest">{rest}</g>
      <g className="hand-hover">{hover}</g>
    </svg>
  );
}

export const ICON_BUTTON = "inline-flex size-10 shrink-0 cursor-pointer items-center justify-center rounded-lg text-neutral-600 outline-offset-2 hover:bg-neutral-100 hover:text-ink focus-visible:outline-2 focus-visible:outline-emerald-700 disabled:cursor-default disabled:opacity-40";

/** Native popovers provide Escape and outside-click dismissal without a UI library. */
export function LibraryActions({ label, disabled, children }: {
  label: string;
  disabled?: boolean;
  children: (close: () => void) => ReactNode;
}) {
  const id = useId();
  const panel = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ top: 0, left: 0 });
  const close = () => {
    document.getElementById(id)?.hidePopover();
    document.getElementById(`${id}-trigger`)?.focus();
  };
  return (
    <>
      <button id={`${id}-trigger`} type="button" popoverTarget={id} aria-label={label} title={label}
        disabled={disabled} className={ICON_BUTTON} onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setPosition({ top: Math.max(8, Math.min(rect.bottom + 4, window.innerHeight - 252)),
            left: Math.max(8, Math.min(rect.right - 224, window.innerWidth - 232)) });
        }}><LibraryIcon name="more" /></button>
      <div ref={panel} id={id} popover="auto" aria-label={label} style={position}
        className="fixed m-0 w-56 rounded-lg border border-neutral-300 bg-paper p-1.5 text-ink"
        onToggle={(event) => {
          if (event.newState === "open") panel.current?.querySelector<HTMLButtonElement>("button")?.focus();
        }}>{children(close)}</div>
    </>
  );
}

export const ACTION_ITEM = "block w-full cursor-pointer rounded-md px-3 py-2 text-left text-sm outline-emerald-700 hover:bg-neutral-100 focus-visible:outline-2 disabled:cursor-default disabled:opacity-40";

export function FolderNameEditor({ initialName = "", busy, onSave, onCancel }: {
  initialName?: string;
  busy: boolean;
  onSave: (name: string) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(initialName);
  return (
    <form className="flex min-w-0 flex-1 items-center gap-2" onSubmit={(event) => {
      event.preventDefault();
      if (!busy && name.trim()) onSave(name.trim());
    }} onKeyDown={(event) => {
      if (event.key === "Escape" && !busy) { event.preventDefault(); onCancel(); }
    }}>
      <LibraryIcon name="folder" className="size-6 text-neutral-500" />
      <input autoFocus aria-label={initialName ? "Rename folder" : "New folder name"}
        placeholder="Type a name…" value={name} maxLength={100} readOnly={busy}
        className={`${TEXT_FIELD} min-w-0 flex-1`} onChange={(event) => setName(event.target.value)} />
      <HandButton seed={68} type="submit" className="px-3!" disabled={busy || !name.trim()}
        aria-label="Save folder name" title="Save (Enter)"><LibraryIcon name="check" /></HandButton>
      <button type="button" disabled={busy} className={ICON_BUTTON} onClick={onCancel}
        aria-label="Cancel naming folder" title="Cancel (Escape)"><LibraryIcon name="close" /></button>
    </form>
  );
}
