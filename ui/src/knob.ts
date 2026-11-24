// Knob drawing and the value mappings, kept pure where they can be tested.
// A knob is a ring (track and value arc, in SVG) around a machined cap (CSS) with a pointer line.
// Turning it only changes a dash length and one custom property, so nothing is rebuilt or re-laid-out.

/** Pixels of vertical drag for a full sweep of a knob. */
export const DRAG_PIXELS = 160;
/** Holding shift divides the sweep by this much. */
export const FINE = 0.2;
/** Arrow-key step, and the same with shift held. */
export const KEY_STEP = 0.02;
/** Wheel sensitivity per pixel of delta, fine with shift held. */
const WHEEL = 0.002;

export const clamp01 = (v: number): number => Math.min(1, Math.max(0, v));

/** The value after dragging `dy` pixels upwards from `start`. */
export function dragValue(start: number, dy: number, fine: boolean): number {
  return clamp01(start + (dy / DRAG_PIXELS) * (fine ? FINE : 1));
}

/** The value after an arrow key: `dir` is +1 or -1. */
export const keyValue = (v: number, dir: 1 | -1, fine: boolean): number => clamp01(v + dir * KEY_STEP * (fine ? FINE : 1));

/** The value after a wheel tick of `deltaY` pixels (scrolling up raises it). */
export const wheelValue = (v: number, deltaY: number, fine: boolean): number => clamp01(v - deltaY * WHEEL * (fine ? FINE : 1));

/** Pointer angle in degrees: -135 at 0, +135 at 1. */
export const angle = (v: number): number => -135 + clamp01(v) * 270;

const point = (deg: number, r: number): [number, number] => {
  const t = (deg * Math.PI) / 180;
  return [20 + r * Math.sin(t), 20 - r * Math.cos(t)];
};
const fmt = ([x, y]: [number, number]): string => `${x.toFixed(2)} ${y.toFixed(2)}`;

export const arc = (a0: number, a1: number, r: number): string =>
  `M ${fmt(point(a0, r))} A ${r} ${r} 0 ${a1 - a0 > 180 ? 1 : 0} 1 ${fmt(point(a1, r))}`;

const RING = arc(-135, 135, 16.6);
const dash = (v: number): string => `${(clamp01(v) * 100).toFixed(2)} 100`;

/** The knob's inner markup: the ring, then the cap with its pointer. */
export function knobHtml(v: number): string {
  return (
    `<svg class="ring" viewBox="0 0 40 40" aria-hidden="true"><path class="track" d="${RING}" pathLength="100"/>` +
    `<path class="val" d="${RING}" pathLength="100" stroke-dasharray="${dash(v)}"/></svg>` +
    `<i class="cap" style="--a:${angle(v).toFixed(1)}deg"><i class="ptr"></i></i>`
  );
}

/** Moves an existing knob to `v` without rebuilding it. */
export function setKnob(el: HTMLElement, v: number): void {
  el.querySelector('.val')?.setAttribute('stroke-dasharray', dash(v));
  el.querySelector<HTMLElement>('.cap')?.style.setProperty('--a', `${angle(v).toFixed(1)}deg`);
  el.setAttribute('aria-valuenow', clamp01(v).toFixed(2));
  el.dataset.live = clamp01(v).toFixed(2);
}
