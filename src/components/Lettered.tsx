import { LETTERING } from "./lettering";

interface LetteredProps {
  word: keyof typeof LETTERING;
  as: "h1" | "h2";
  /** Sets the height; the width follows the drawing. */
  className: string;
}

/** A heading whose visible text is artwork. Screen readers get the words once, as text. */
export default function Lettered({ word, as: Heading, className }: LetteredProps) {
  const { text, width, height, strokes } = LETTERING[word];
  return (
    <Heading className={className}>
      <span className="sr-only">{text}</span>
      <svg
        aria-hidden
        viewBox={`0 0 ${width} ${height}`}
        className="block h-full w-auto max-w-full overflow-visible"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        {strokes.map(([d, pen]) => (
          <path key={d} d={d} strokeWidth={pen} />
        ))}
      </svg>
    </Heading>
  );
}
