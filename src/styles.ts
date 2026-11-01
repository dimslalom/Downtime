import type { Placement } from "./services/hotkey";

export const PAGE =
  "flex h-full min-h-0 flex-col gap-6 bg-paper px-[max(2.5rem,calc((100%-56rem)/2))] py-8 text-ink short:gap-4 short:py-5";

export const TEXT_FIELD =
  "rounded-md border border-neutral-300 bg-paper px-3 py-2 text-base text-ink outline-offset-1 outline-emerald-700 focus-visible:outline-2";

export const KEYCAP =
  "rounded-md border border-neutral-300 bg-neutral-50 px-2 py-0.5 font-sans text-base text-ink";

/** Shared by the overlay and the settings picker, so the picker is a true miniature. */
export const PLACEMENT_ALIGN: Record<Placement, string> = {
  "top-left": "items-start justify-start",
  top: "items-start justify-center",
  "top-right": "items-start justify-end",
  left: "items-center justify-start",
  right: "items-center justify-end",
  "bottom-left": "items-end justify-start",
  bottom: "items-end justify-center",
  "bottom-right": "items-end justify-end",
};
