// Pure geometry for dragging a module over the rack: which module is under
// the pointer, where a dropped module would land, and the dashed "ghost"
// cables Beginner mode previews before anything is wired.
import { cablePath, type Point } from './cables';
import type { CableSuggestion } from './types';

export interface Box {
  id: string;
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export const NEW_MODULE = 'new';

/** The box the point is in, or within `margin` of; the nearest wins. */
export function hitModule(x: number, y: number, boxes: Box[], margin = 28): string | null {
  let best: string | null = null;
  let bestDist = Infinity;
  for (const b of boxes) {
    const dx = Math.max(b.left - x, 0, x - b.right);
    const dy = Math.max(b.top - y, 0, y - b.bottom);
    const dist = Math.hypot(dx, dy);
    if (dist <= margin && dist < bestDist) {
      best = b.id;
      bestDist = dist;
    }
  }
  return best;
}

/** Position among siblings (left to right) a module dropped at `x` would take. */
export const insertIndex = (x: number, siblings: Box[]): number => siblings.filter((b) => (b.left + b.right) / 2 < x).length;

export interface Ghost {
  /** The real jack the dragged module would connect to. */
  ref: string;
  d: string;
  label: string;
  mid: Point;
  score: number;
}

/**
 * Lays out suggested cables. `chip` is where the dragged module is drawn;
 * the `new.<jack>` end of each cable runs to it, the other end to the real jack.
 * Cables whose real jack is not on screen are dropped.
 */
export function ghostLayout(cables: CableSuggestion[], chip: Point, jackAt: (ref: string) => Point | null): Ghost[] {
  const out: Ghost[] = [];
  for (const c of cables) {
    const fromNew = c.from.startsWith(`${NEW_MODULE}.`);
    const toNew = c.to.startsWith(`${NEW_MODULE}.`);
    if (fromNew === toNew) continue;
    const ref = fromNew ? c.to : c.from;
    const there = jackAt(ref);
    if (!there) continue;
    const [a, b] = fromNew ? [chip, there] : [there, chip];
    out.push({ ref, d: cablePath(a, b), label: c.reason, mid: [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2], score: c.score });
  }
  return out.sort((x, y) => y.score - x.score);
}
