// The level fader in each part's header card: a vertical slider that works like
// the knobs. Dragging previews the level; letting go applies it as one command;
// double-click resets to unity.
import { app, send, tryOut } from './store';
import { $ } from './util';

/** Level (0..1, 1 = unity) for a pointer at `y` on a track spanning `top`..`bottom`. */
export function levelFromY(top: number, bottom: number, y: number): number {
  if (bottom <= top) return 1;
  return Math.max(0, Math.min(1, (bottom - y) / (bottom - top)));
}

/** The readout: decibels, with silence at the very bottom. */
export function levelLabel(level: number): string {
  if (level <= 0.001) return '−∞ dB';
  const db = 20 * Math.log10(level);
  return Math.abs(db) < 0.05 ? '0 dB' : `${db.toFixed(1).replace('-', '−')} dB`;
}

export const faderHtml = (part: string, level: number): string =>
  `<div class="pfader" data-part-fader="${part}" data-level="${level}" data-tip-title="Part level"
    data-tip="Drag to set how loud this part is. Double-click for unity (0 dB)."><div class="ptrack"><i style="height:${Math.round(level * 100)}%"></i></div><span class="preadout">${levelLabel(level)}</span></div>`;

function paint(fader: HTMLElement, level: number): void {
  fader.dataset.level = String(level);
  $('i', fader).style.height = `${level * 100}%`;
  $('.preadout', fader).textContent = levelLabel(level);
}

export function initPartFader(): void {
  const bench = $('#bench');
  let active: { el: HTMLElement; part: string; level: number } | null = null;

  const track = (el: HTMLElement): DOMRect => $('.ptrack', el).getBoundingClientRect();
  bench.addEventListener('pointerdown', (e) => {
    const el = (e.target as HTMLElement).closest<HTMLElement>('[data-part-fader]');
    if (!el || e.detail > 1) return;
    const r = track(el);
    active = { el, part: el.dataset.partFader ?? '', level: levelFromY(r.top, r.bottom, e.clientY) };
    paint(el, active.level);
    tryOut({ op: 'set_part_level', part: active.part, level: active.level });
    e.preventDefault();
  });
  document.addEventListener('pointermove', (e) => {
    if (!active) return;
    const r = track(active.el);
    active.level = levelFromY(r.top, r.bottom, e.clientY);
    paint(active.el, active.level);
    tryOut({ op: 'set_part_level', part: active.part, level: active.level });
  });
  document.addEventListener('pointerup', () => {
    const d = active;
    active = null;
    if (!d) return;
    const had = app.state?.patch.parts.find((p) => p.id === d.part)?.level ?? 1;
    if (Math.abs(d.level - had) > 0.001) void send({ op: 'set_part_level', part: d.part, level: d.level });
  });
  bench.addEventListener('dblclick', (e) => {
    const el = (e.target as HTMLElement).closest<HTMLElement>('[data-part-fader]');
    if (!el) return;
    paint(el, 1);
    void send({ op: 'set_part_level', part: el.dataset.partFader ?? '', level: 1 });
  });
}
