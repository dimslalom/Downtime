export type OutlineShape = "button" | "panel" | "checkbox";

interface Point {
  x: number;
  y: number;
}

const OUTLINES = {
  checkbox: { inset: 3, corner: 3, wobble: 0.7, bow: 0.8, spacing: 12, tail: 0 },
  button: { inset: 2, corner: 10, wobble: 0.7, bow: 1, spacing: 64, tail: 0 },
  panel: { inset: 3, corner: 4, wobble: 1.1, bow: 2.4, spacing: 110, tail: 9 },
};

/** Small shapes need a stronger hand before a hover redraw reads at all. */
const HOVER_LOOSENESS = { checkbox: 3, button: 2, panel: 2 };

/** Mulberry32, so an outline is identical for a given seed and size. */
function seededRandom(seed: number): () => number {
  let state = seed;
  return () => {
    state = (state + 0x6d2b79f5) | 0;
    let mixed = Math.imul(state ^ (state >>> 15), 1 | state);
    mixed = (mixed + Math.imul(mixed ^ (mixed >>> 7), 61 | mixed)) ^ mixed;
    return ((mixed ^ (mixed >>> 14)) >>> 0) / 4294967296;
  };
}

/** Points along one side, spaced by distance so long sides are not stretched short ones. */
function sidePoints(
  from: Point,
  to: Point,
  { spacing, wobble, bow }: typeof OUTLINES.button,
  random: () => number,
): Point[] {
  const length = Math.hypot(to.x - from.x, to.y - from.y);
  // At least one point between the corners, or a short side comes out dead straight.
  const count = Math.max(2, Math.round(length / spacing));
  const along = { x: (to.x - from.x) / length, y: (to.y - from.y) / length };
  const normal = { x: -along.y, y: along.x };
  const sideBow = (random() - 0.5) * 2 * Math.min(bow, length / 60);

  const points = [from];
  for (let index = 1; index < count; index++) {
    const t = (index + (random() - 0.5) * 0.3) / count;
    const offset = Math.sin(Math.PI * t) * sideBow + (random() - 0.5) * 2 * wobble;
    points.push({
      x: from.x + along.x * length * t + normal.x * offset,
      y: from.y + along.y * length * t + normal.y * offset,
    });
  }
  points.push(to);
  return points;
}

function direction(from: Point, to: Point): Point {
  const length = Math.hypot(to.x - from.x, to.y - from.y) || 1;
  return { x: (to.x - from.x) / length, y: (to.y - from.y) / length };
}

const round = (value: number) => Math.round(value * 10) / 10;

/**
 * A smooth curve through the points, with handles scaled to each segment so corners
 * can't loop. A closed path repeats its first point last, and wraps its tangents
 * around that seam so the join doesn't kink.
 */
function smoothPath(points: Point[], closed: boolean): string {
  const first = points[0]!;
  const last = points.length - 1;
  let path = `M${round(first.x)} ${round(first.y)}`;
  for (let index = 0; index < last; index++) {
    const start = points[index]!;
    const end = points[index + 1]!;
    const previous = points[index - 1] ?? (closed ? points[last - 1]! : start);
    const next = points[index + 2] ?? (closed ? points[1]! : end);
    const handle = Math.hypot(end.x - start.x, end.y - start.y) / 3;
    const startTangent = direction(previous, end);
    const endTangent = direction(start, next);
    path +=
      `C${round(start.x + startTangent.x * handle)} ${round(start.y + startTangent.y * handle)}` +
      ` ${round(end.x - endTangent.x * handle)} ${round(end.y - endTangent.y * handle)}` +
      ` ${round(end.x)} ${round(end.y)}`;
  }
  return path;
}

/**
 * An imperfect pen outline for a box of the given size. Panels are drawn as one
 * stroke that starts and ends at the lower left, overshooting where the pen meets
 * its own start. A loose outline is the hover redraw: the same box in a freer hand,
 * so the change shows even on small controls.
 */
export function outlinePath(
  width: number,
  height: number,
  seed: number,
  shape: OutlineShape,
  loose = false,
) {
  const base = OUTLINES[shape];
  const options = loose
    ? { ...base, wobble: base.wobble * HOVER_LOOSENESS[shape], bow: base.bow * HOVER_LOOSENESS[shape] }
    : base;
  const { inset, corner, tail } = options;
  const left = inset;
  const top = inset;
  const right = width - inset;
  const bottom = height - inset;
  if (right - left <= corner * 2 || bottom - top <= corner * 2) return "";

  const random = seededRandom(seed);
  const sides: [Point, Point][] = [
    [{ x: left, y: bottom - corner }, { x: left, y: top + corner }],
    [{ x: left + corner, y: top }, { x: right - corner, y: top }],
    [{ x: right, y: top + corner }, { x: right, y: bottom - corner }],
    [{ x: right - corner, y: bottom }, { x: left + corner, y: bottom }],
  ];
  const points = sides.flatMap(([from, to]) => sidePoints(from, to, options, random));

  if (tail === 0) return `${smoothPath([...points, points[0]!], true)}Z`;

  points.unshift({ x: left + 1, y: bottom + tail * 0.4 });
  points.push({ x: left - tail, y: bottom + 1 + random() });
  return smoothPath(points, false);
}
