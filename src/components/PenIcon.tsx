import type { ReactNode } from "react";

export const PEN_DRAWINGS = {
  answer: <path d="M8.6 16c0-2.4-3-3.6-3-7a6.4 6.4 0 0 1 12.8 0c0 3.4-3 4.6-3 7ZM9 19h6M10.5 22h3M12 1V0M3.5 3.5 2 2M20.5 3.5 22 2M2 10H.5M22 10h1.5" />,
  book: <path d="M12 5.4C9.3 3.8 5.8 3.7 2.8 4.5l.4 14c3.1-.8 6-.5 8.8 1.1 2.8-1.6 5.8-1.9 8.9-1.1l.3-14c-3-.8-6.5-.7-9.2.9ZM12 5.4v14.2M6 8l3 .5M6 11.4l3 .5M15 8.5l3-.5M15 11.9l3-.5" />,
  calendar: <path d="M4 6.5c5.3-.4 10.7-.4 16 0 .2 4.5.2 9 0 13.5-5.3.4-10.7.4-16 0-.2-4.5-.2-9 0-13.5ZM4 10.5h16M8.5 3.5v4M15.5 3.5v4" />,
  cards: <path d="M8 4.5c4-.4 8-.6 12-.6.3 5 .5 10 .5 15-4 .3-8 .5-12 .6-.2-5-.4-10-.5-15ZM8.1 8.5 4 9c.1 3.7.3 7.3.6 11 3.3-.1 6.5-.2 9.7-.6" />,
  clock: <path d="M12 3.2c5-.2 8.9 3.7 8.8 8.8-.1 5-3.9 8.7-8.9 8.7-5 0-8.8-3.8-8.7-8.8.1-5 3.8-8.6 8.8-8.7ZM12 7.5v4.9l3.2 2" />,
  flag: <path d="m5.5 21 .2-18c4-2 7.5 3 12.7.5l-.3 9c-5 2.3-8.7-2.3-12.5-.4" />,
  ghost: <path d="M5 21c0-6-.4-11 1.3-14 2.800-4.800 9.300-4.600 11.500.2 1.300 3 1 8 1.100 13.800-1.300 1-2.600-.8-3.500-.8-1 0-1.800 1.500-3.200 1.500-1.300 0-2.100-1.500-3.200-1.500-1.100 0-2.300 1.700-3.500.8ZM9.500 9.800v2M14.500 9.800v2" />,
  pencil: <path d="m4 20 1-4.600L16.400 4c1-1 2.600-1 3.500 0 1 1 1 2.500 0 3.500L8.500 19ZM14 6.500l3.500 3.500" />,
  question: <>
    <path d="M6 18.2 3.500 21l.5-5.100C2.900 14.500 2.500 12.800 2.600 10.600 2.700 5.100 6.100 2.700 12 2.800c6 .1 9.400 2.700 9.300 7.800-.1 5.200-3.600 7.800-9.300 7.700-2.400 0-4.300-.1-6-.1Z" />
    <path d="M9.700 7.500c.4-2.100 4.800-2.200 4.800.4 0 1.800-2.500 1.800-2.500 3.900M12 14.500v.2" />
  </>,
  retry: <path d="M5.100 8.100C7.200 3.600 13.400 2.700 17.200 6.200c3.700 3.300 3.200 9.500-.8 12.200-3.800 2.500-8.900 1.100-10.800-2.500M4.700 3.800l.2 5.100 5-.6" />,
  sparkle: <path d="M12 3c.6 5 2.500 8 9 9-6.500 1-8.400 4-9 9-.6-5-2.500-8-9-9 6.500-1 8.400-4 9-9Z" />,
  star: <path d="m12 3.500 2.600 5.600 6 .7-4.500 4.100 1.300 6-5.400-3.100-5.300 3.100 1.200-6L3.400 9.800l6-.7Z" />,
  tick: <path d="M4 13.500 9.500 19 20.500 5" />,
};

interface PenIconProps {
  /** Announced to assistive tech and shown as a tooltip. */
  label: string;
  children: ReactNode;
  className?: string;
  size?: string;
}

export function PenIcon({ label, children, className = "text-neutral-500", size = "size-8" }: PenIconProps) {
  return (
    <svg role="img" aria-label={label} viewBox="0 0 24 24"
      className={`${size} shrink-0 fill-none stroke-current ${className}`}
      strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
      <title>{label}</title>
      {children}
    </svg>
  );
}

interface PenStatProps {
  /** The full meaning, such as "21 cards left". */
  label: string;
  drawing: keyof typeof PEN_DRAWINGS;
  value?: ReactNode;
  className?: string;
  size?: string;
}

/** One fact as one object. Lay several out side by side rather than joining them into a string. */
export function PenStat({ label, drawing, value, className = "", size = "size-5" }: PenStatProps) {
  return (
    <span role="img" aria-label={label} title={label} className={`inline-flex min-w-0 items-center gap-1.5 tabular-nums ${className}`}>
      <svg aria-hidden viewBox="0 0 24 24" className={`${size} shrink-0 fill-none stroke-current`}
        strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
        {PEN_DRAWINGS[drawing]}
      </svg>
      {value !== undefined && <span aria-hidden className="min-w-0 wrap-anywhere">{value}</span>}
    </span>
  );
}
