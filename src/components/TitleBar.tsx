import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";

interface TitleBarControlsProps {
  maximized: boolean;
  onMinimize: () => void;
  onToggleMaximize: () => void;
  onClose: () => void;
}

const CONTROL = "inline-flex h-full w-12 shrink-0 cursor-default items-center justify-center text-ink outline-offset-[-3px] hover:bg-neutral-100 focus-visible:outline-2 focus-visible:outline-emerald-700 active:bg-neutral-200";

const MINIMIZE_DRAWINGS = [
  "M4.500 14.600c2.800-1.500 5-.7 7.400-.3 2.600.3 5 .1 7.800-1.500",
  "M4.300 13.200c2.600 1.300 5.400.9 7.700.6 2.600-.4 5.200.2 7.700 1.200",
] as const;
const MAXIMIZE_DRAWINGS = [
  "M4.600 20.800C4.200 15 4.300 9.500 4.700 5.200 9.500 4.400 14.800 4.800 19.300 4.500c.5 4.800.4 9.700.1 14.800-5 .5-10.400.3-17.100.4",
  "M5 20.400C4.700 15.500 4.900 9.700 4.300 4.700 9.300 5.200 14.600 4.300 19.700 5c-.2 4.800.1 9.900-.4 14.900-5.500-.3-11 .3-17.300-.2",
] as const;
const RESTORE_DRAWINGS = [
  "M8 7.200c0-1 .1-1.900.2-2.700 4-.4 8.100-.2 12-.3.3 4 .2 8-.1 12-.9.1-1.700.1-2.400.1M4 20.900c-.2-3.900-.2-8.200.1-12.200 3.900-.4 8-.3 12-.4.3 4.100.3 8.200 0 12.300-4.200.4-9 .2-14.100.3",
  "M7.800 6.900c.2-.9.2-1.800.5-2.900 4-.2 7.900.2 12.200-.1.1 4.200.4 8.100-.4 12.300-.8-.2-1.600 0-2.300.1M4.300 20.600c.1-4-.5-8 .2-12.300 3.600.2 8-.5 11.700.1.2 4-.1 8.100.3 12.300-4.400 0-9.200.4-14.300-.2",
] as const;
const CLOSE_DRAWINGS = [
  "M5.200 4.800c3.600 4.800 9.200 9.700 14 15.600M19.400 4.200c-5 4.600-9.600 9.800-14.700 15.500-.3.300-.6.600-1 .8",
  "M4.900 5.300c4 4.300 8.800 10 14.600 14.600M18.900 4.600c-4.500 5-9.600 9.300-14 15.400-.2.300-.4.500-.7.700",
] as const;

/** The icon as drawn at rest, plus the line index.css swaps in on hover. */
function PenGlyph({ drawings: [rest, hover] }: { drawings: readonly [string, string] }) {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" className="size-6 fill-none stroke-current"
      strokeWidth="2.1" strokeLinecap="round" strokeLinejoin="round">
      <path className="hand-rest" d={rest} />
      <path className="hand-hover" d={hover} />
    </svg>
  );
}

export function TitleBarControls({ maximized, onMinimize, onToggleMaximize, onClose }: TitleBarControlsProps) {
  return (
    <header className="flex h-10 shrink-0 items-stretch bg-paper select-none" aria-label="Window controls">
      <div data-tauri-drag-region className="h-full min-w-0 flex-1" />
      <button type="button" className={`${CONTROL} hover:text-yellow-500`} onClick={onMinimize}
        aria-label="Minimize window" title="Minimize">
        <PenGlyph drawings={MINIMIZE_DRAWINGS} />
      </button>
      <button type="button" className={`${CONTROL} hover:text-green-600`} onClick={onToggleMaximize}
        aria-label={maximized ? "Restore window" : "Maximize window"} title={maximized ? "Restore" : "Maximize"}>
        <PenGlyph drawings={maximized ? RESTORE_DRAWINGS : MAXIMIZE_DRAWINGS} />
      </button>
      <button type="button" className={`${CONTROL} hover:text-red-500`} onClick={onClose}
        aria-label="Close window" title="Close">
        <PenGlyph drawings={CLOSE_DRAWINGS} />
      </button>
    </header>
  );
}

export default function TitleBar() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const update = () => appWindow.isMaximized().then((value) => {
      if (!cancelled) setMaximized(value);
    }).catch(console.error);
    void update();
    void appWindow.onResized(() => { void update(); }).then((stop) => {
      if (cancelled) stop(); else unlisten = stop;
    }).catch(console.error);
    return () => { cancelled = true; unlisten?.(); };
  }, []);

  return <TitleBarControls maximized={maximized}
    onMinimize={() => { void getCurrentWindow().minimize().catch(console.error); }}
    onToggleMaximize={() => { void getCurrentWindow().toggleMaximize().catch(console.error); }}
    onClose={() => { void getCurrentWindow().close().catch(console.error); }} />;
}
