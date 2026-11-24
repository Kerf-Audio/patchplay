// Cable geometry and references, pure.

export type Point = [number, number];

/** A hanging cable: both ends sag below their jacks by a distance-based amount. */
export function cablePath([x1, y1]: Point, [x2, y2]: Point): string {
  const sag = Math.min(140, Math.hypot(x2 - x1, y2 - y1) * 0.3) + 25;
  return `M${x1} ${y1} C${x1} ${y1 + sag} ${x2} ${y2 + sag} ${x2} ${y2}`;
}

/** `"mix.in1"` is module `mix`, jack `in1`. */
export function splitRef(ref: string): [string, string] {
  const i = ref.indexOf('.');
  return i < 0 ? [ref, ''] : [ref.slice(0, i), ref.slice(i + 1)];
}

/** An output jack goes first, whichever way the cable was dragged. */
export function orient(
  a: { ref: string; dir: string },
  b: { ref: string; dir: string },
): { from: string; to: string } | null {
  if (a.dir === b.dir) return null;
  return a.dir === 'out' ? { from: a.ref, to: b.ref } : { from: b.ref, to: a.ref };
}
