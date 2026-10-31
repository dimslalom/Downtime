import { useLayoutEffect, useRef, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode } from "react";
import { outlinePath, type OutlineShape } from "../handPath";

interface HandOutlineProps {
  /** Fixes the drawing. Give each element its own so outlines differ from one another. */
  seed: number;
  shape: OutlineShape;
  className: string;
  /** Also draws the alternate line that index.css swaps in on hover. */
  redrawOnHover?: boolean;
}

/** Offsets the seed for a control's hover drawing, which stays the same on every hover. */
const HOVER_SEED = 1009;

/**
 * Traces the parent, which must be `relative isolate`. The path is written directly
 * rather than through state, so it is there before the first paint and rerenders
 * never touch it. Only a real size change redraws it.
 */
export function HandOutline({ seed, shape, className, redrawOnHover = false }: HandOutlineProps) {
  const pathRef = useRef<SVGPathElement>(null);
  const hoverPathRef = useRef<SVGPathElement>(null);

  useLayoutEffect(() => {
    const path = pathRef.current;
    const host = path?.ownerSVGElement?.parentElement;
    if (!path || !host) return;

    const draw = () => {
      const { offsetWidth: width, offsetHeight: height } = host;
      path.setAttribute("d", outlinePath(width, height, seed, shape));
      hoverPathRef.current?.setAttribute("d", outlinePath(width, height, seed + HOVER_SEED, shape, true));
    };
    draw();
    const observer = new ResizeObserver(draw);
    observer.observe(host);
    return () => observer.disconnect();
  }, [seed, shape]);

  return (
    <svg
      aria-hidden
      className="pointer-events-none absolute inset-0 -z-10 size-full overflow-visible"
    >
      <path ref={pathRef} className={`hand-rest ${className}`} strokeLinecap="round" strokeLinejoin="round" />
      {redrawOnHover && (
        <path ref={hoverPathRef} className={`hand-hover ${className}`} strokeLinecap="round" strokeLinejoin="round" />
      )}
    </svg>
  );
}

const BUTTON_VARIANTS = {
  primary: {
    label: "text-paper",
    outline:
      "fill-ink stroke-ink group-enabled:group-hover:fill-neutral-700 group-enabled:group-hover:stroke-neutral-700",
  },
  secondary: {
    label: "text-ink disabled:text-neutral-500",
    outline:
      "fill-paper stroke-ink group-enabled:group-hover:fill-neutral-100 group-enabled:group-active:fill-neutral-200 group-disabled:stroke-neutral-300",
  },
};

interface HandButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  seed: number;
  variant?: keyof typeof BUTTON_VARIANTS;
}

export function HandButton({ seed, variant = "secondary", className = "", ...props }: HandButtonProps) {
  const { label, outline } = BUTTON_VARIANTS[variant];
  return (
    <button
      type="button"
      {...props}
      className={`group relative isolate min-h-11 shrink-0 rounded-xl px-5 py-2 text-base font-medium outline-offset-3 outline-emerald-700 focus-visible:outline-2 enabled:cursor-pointer enabled:active:translate-y-px motion-reduce:enabled:active:translate-y-0 disabled:cursor-not-allowed ${label} ${className}`}
    >
      <HandOutline seed={seed} shape="button" redrawOnHover className={`${outline} stroke-[1.5]`} />
      {props.children}
    </button>
  );
}

interface HandPanelProps {
  seed: number;
  className?: string;
  outlineClassName?: string;
  children: ReactNode;
}

interface HandCheckboxProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "type"> {
  seed?: number;
}

/** A native keyboard-accessible checkbox with the same pen strokes as our panels. */
export function HandCheckbox({ seed = 91, className = "", ...props }: HandCheckboxProps) {
  return (
    <span className={`relative inline-flex size-8 shrink-0 align-middle ${className}`}>
      <input {...props} type="checkbox"
        className="peer absolute inset-0 z-10 m-0 size-full cursor-pointer opacity-0 disabled:cursor-not-allowed forced-colors:appearance-auto forced-colors:opacity-100" />
      <svg aria-hidden="true" viewBox="0 0 28 28"
        className="pointer-events-none size-full overflow-visible rounded-sm text-ink peer-enabled:peer-hover:text-neutral-600 peer-focus-visible:outline-2 peer-focus-visible:outline-offset-3 peer-focus-visible:outline-emerald-700 peer-disabled:opacity-35 peer-checked:[&_.hand-check]:opacity-100 forced-colors:hidden">
        <path className="hand-rest" d={outlinePath(28, 28, seed, "checkbox")} fill="white" stroke="currentColor"
          strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
        <path className="hand-hover" d={outlinePath(28, 28, seed + HOVER_SEED, "checkbox", true)} fill="white" stroke="currentColor"
          strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" />
        <path className="hand-check hand-rest opacity-0" d="M6.8 14.2 C8.1 15.2 9.9 17.8 11.5 19.2 C14.2 15.5 18.2 10.3 22 6.5"
          fill="none" stroke="currentColor" strokeWidth="2.3" strokeLinecap="round" strokeLinejoin="round" />
        <path className="hand-check hand-hover opacity-0" d="M6 15.200 C7.700 15.700 9.300 18.900 11 20.300 C13.800 15 18.800 10 23.200 5.300"
          fill="none" stroke="currentColor" strokeWidth="2.3" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    </span>
  );
}

export function HandPanel({
  seed,
  className = "",
  outlineClassName = "fill-paper stroke-ink stroke-[1.5]",
  children,
}: HandPanelProps) {
  return (
    <section className={`relative isolate ${className}`}>
      <HandOutline seed={seed} shape="panel" className={outlineClassName} />
      {children}
    </section>
  );
}
