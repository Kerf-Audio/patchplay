// Pointer interaction on the bench: turning knobs, dragging cables, locks,
// mutes and the explain hover. Everything ends in one `Command`.
import { autoPatchModule, removeModule } from './add-module';
import { cablePath, orient, type Point } from './cables';
import { clearExplain, drawCables, holeCentre, showExplainModule, showExplainPart } from './bench';
import { startDrag } from './drag';
import { dragValue, keyValue, setKnob, wheelValue } from './knob';
import { onSampleClick, showGrains, showSlice } from './sample';
import { sidebarExplain } from './sidebar';
import { app, patch, send, tryOut } from './store';
import { refreshTip } from './tooltip';
import { $, $$, cableStroke } from './util';

interface KnobDrag {
  type: 'knob';
  el: HTMLElement;
  module: string;
  param: string;
  y: number;
  start: number;
  value: number;
}

interface JackDrag {
  type: 'jack';
  ref: string;
  dir: string;
  kind: string;
  from: Point;
}

let drag: KnobDrag | JackDrag | null = null;

const bench = (): HTMLElement => $('#bench');
const jackAt = (x: number, y: number): HTMLElement | null =>
  document.elementFromPoint(x, y)?.closest<HTMLElement>('#bench .jack') ?? null;

function startKnob(e: PointerEvent, el: HTMLElement): void {
  const module = el.closest<HTMLElement>('.mod')?.dataset.uid ?? '';
  const param = el.dataset.k ?? '';
  const p = patch().modules.find((m) => m.id === module)?.params.find((q) => q.key === param);
  if (!p) return;
  if (e.altKey) {
    void send({ op: 'lock', target: { kind: 'param', module, param }, locked: !p.locked });
    return;
  }
  drag = { type: 'knob', el, module, param, y: e.clientY, start: p.value, value: p.value };
  el.focus({ preventScroll: true });
  el.setPointerCapture(e.pointerId);
  e.preventDefault();
}

function startJack(e: PointerEvent, el: HTMLElement): void {
  const ref = el.dataset.ref ?? '';
  if (e.altKey) {
    void unplug(ref);
    e.preventDefault();
    return;
  }
  const from = holeCentre(ref);
  if (!from) return;
  drag = { type: 'jack', ref, dir: el.dataset.dir ?? '', kind: el.dataset.kind ?? '', from };
  $('#cables').insertAdjacentHTML('beforeend', `<path class="temp" id="temp" style="${cableStroke(drag.kind)}"/>`);
  e.preventDefault();
}

function moveKnob(d: KnobDrag, e: PointerEvent): void {
  d.value = dragValue(d.start, d.y - e.clientY, e.shiftKey);
  setKnob(d.el, d.value);
  refreshTip(d.el);
  const panel = d.el.closest<HTMLElement>('.mod') ?? d.el;
  if (d.param === 'slice') showSlice(panel, d.value);
  else if (d.param === 'position' || d.param === 'spray') showGrains(panel, d.param, d.value);
  tryOut({ op: 'set_param', module: d.module, param: d.param, value: d.value });
}

function moveJack(d: JackDrag, e: PointerEvent): void {
  const b = bench().getBoundingClientRect();
  $('#temp').setAttribute('d', cablePath(d.from, [e.clientX - b.left, e.clientY - b.top]));
  $$('.jack.target').forEach((j) => j.classList.remove('target'));
  const over = jackAt(e.clientX, e.clientY);
  if (over && over.dataset.dir !== d.dir) over.classList.add('target');
}

function endJack(d: JackDrag, e: PointerEvent): void {
  document.getElementById('temp')?.remove();
  $$('.jack.target').forEach((j) => j.classList.remove('target'));
  const over = jackAt(e.clientX, e.clientY);
  const ends = over && orient({ ref: d.ref, dir: d.dir }, { ref: over.dataset.ref ?? '', dir: over.dataset.dir ?? '' });
  if (ends) void send({ op: 'connect', ...ends });
}

async function unplug(ref: string): Promise<void> {
  for (const c of patch().cables.filter((x) => x.from === ref || x.to === ref)) {
    if (!(await send({ op: 'disconnect', from: c.from, to: c.to }))) return;
  }
}

function onBenchClick(e: MouseEvent): void {
  const t = e.target as HTMLElement;
  if (onSampleClick(t)) return;
  const mod = t.closest<HTMLElement>('.mod')?.dataset.uid ?? '';
  if (t.closest('[data-autopatch]')) {
    void autoPatchModule(mod);
    return;
  }
  if (t.closest('[data-remove]')) {
    void removeModule(mod);
    return;
  }
  const cable = t.closest<SVGElement>('#cables path[data-i]');
  const cableAt = cable && patch().cables[Number(cable.dataset.i)];
  if (cableAt) {
    void send({ op: 'disconnect', from: cableAt.from, to: cableAt.to });
    return;
  }
  const lock = t.closest<HTMLElement>('[data-lock]');
  if (lock) {
    const id = lock.closest<HTMLElement>('.mod')?.dataset.uid ?? '';
    const m = patch().modules.find((x) => x.id === id);
    if (m) void send({ op: 'lock', target: { kind: 'module', module: id }, locked: !m.locked });
    return;
  }
  const partLock = t.closest<HTMLElement>('[data-part-lock]')?.dataset.partLock;
  if (partLock !== undefined) {
    const part = patch().parts.find((x) => x.id === partLock);
    if (part) void send({ op: 'lock', target: { kind: 'part', part: partLock }, locked: !part.locked });
    return;
  }
  const partMute = t.closest<HTMLElement>('[data-part-mute]')?.dataset.partMute;
  if (partMute !== undefined) {
    const part = patch().parts.find((x) => x.id === partMute);
    if (part) void send({ op: 'mute', part: partMute, muted: !part.muted });
  }
}

function onExplainHover(e: MouseEvent): void {
  if (!app.explain) return;
  const t = e.target as HTMLElement;
  const mod = t.closest<HTMLElement>('.mod');
  const label = t.closest<HTMLElement>('.part-label');
  if (mod?.dataset.uid) showExplainModule(mod.dataset.uid);
  else if (label?.dataset.part !== undefined) showExplainPart(label.dataset.part);
}

interface KnobRef {
  el: HTMLElement;
  module: string;
  param: string;
  value: number;
}

function knobRef(el: HTMLElement): KnobRef | null {
  const module = el.closest<HTMLElement>('.mod')?.dataset.uid ?? '';
  const param = el.dataset.k ?? '';
  const p = patch().modules.find((m) => m.id === module)?.params.find((q) => q.key === param);
  return p ? { el, module, param, value: Number(el.dataset.live ?? p.value) } : null;
}

interface Pending {
  module: string;
  param: string;
  value: number;
}
let pending: Pending | null = null;
let settle: number | undefined;

/** Commits a keyboard or wheel change as one undo step, and gives the knob its focus back after the redraw. */
async function flush(): Promise<void> {
  clearTimeout(settle);
  const p = pending;
  pending = null;
  if (!p) return;
  await send({ op: 'set_param', ...p });
  document.querySelector<HTMLElement>(`.mod[data-uid="${CSS.escape(p.module)}"] .knob[data-k="${CSS.escape(p.param)}"]`)?.focus({ preventScroll: true });
}

/** Shows the new value at once and commits it once the keys or wheel pause. */
function nudge(k: KnobRef, value: number): void {
  if (pending && (pending.module !== k.module || pending.param !== k.param)) void flush();
  setKnob(k.el, value);
  refreshTip(k.el);
  tryOut({ op: 'set_param', module: k.module, param: k.param, value });
  pending = { module: k.module, param: k.param, value };
  clearTimeout(settle);
  settle = window.setTimeout(() => void flush(), 350);
}

function resetKnob(k: KnobRef): void {
  const kind = k.el.closest<HTMLElement>('.mod')?.dataset.kind;
  const def = app.desc?.modules.find((m) => m.kind === kind)?.params.find((q) => q.key === k.param)?.default;
  if (def !== undefined) void send({ op: 'set_param', module: k.module, param: k.param, value: def });
}

const ARROWS: Record<string, 1 | -1> = { ArrowUp: 1, ArrowRight: 1, ArrowDown: -1, ArrowLeft: -1 };

function onKnobKey(e: KeyboardEvent): void {
  const el = (e.target as HTMLElement).closest<HTMLElement>('.knob');
  const k = el && knobRef(el);
  if (!k) return;
  const dir = ARROWS[e.key];
  if (dir) nudge(k, keyValue(k.value, dir, e.shiftKey));
  else if (e.key === 'Home' || e.key === 'End') nudge(k, e.key === 'Home' ? 0 : 1);
  else if (e.key === 'Enter') resetKnob(k);
  else return;
  e.preventDefault();
}

/** The wheel only turns a knob that has focus, so scrolling the rack never changes a value by accident. */
function onKnobWheel(e: WheelEvent): void {
  const el = (e.target as HTMLElement).closest<HTMLElement>('.knob');
  if (!el || el !== document.activeElement) return;
  const k = knobRef(el);
  if (!k) return;
  nudge(k, wheelValue(k.value, e.deltaY, e.shiftKey));
  e.preventDefault();
}

export function initInteract(): void {
  bench().addEventListener('pointerdown', (e) => {
    const t = e.target as HTMLElement;
    const knob = t.closest<HTMLElement>('.knob');
    const jack = t.closest<HTMLElement>('.jack');
    if (knob) startKnob(e, knob);
    else if (jack) startJack(e, jack);
    else if (t.closest('.mod-head') && !t.closest('button')) {
      const m = t.closest<HTMLElement>('.mod');
      if (m?.dataset.uid && m.dataset.kind) startDrag(e, { kind: m.dataset.kind, module: m.dataset.uid });
    }
  });
  document.addEventListener('pointermove', (e) => {
    if (drag?.type === 'knob') moveKnob(drag, e);
    else if (drag?.type === 'jack') moveJack(drag, e);
  });
  document.addEventListener('pointerup', (e) => {
    const d = drag;
    drag = null;
    if (d?.type === 'knob') {
      if (Math.abs(d.value - d.start) > 0.001) void send({ op: 'set_param', module: d.module, param: d.param, value: d.value });
    } else if (d?.type === 'jack') endJack(d, e);
  });
  bench().addEventListener('dblclick', (e) => {
    const t = e.target as HTMLElement;
    const ref = t.closest<HTMLElement>('.jack')?.dataset.ref;
    if (ref) void unplug(ref);
    const knob = t.closest<HTMLElement>('.knob');
    const k = knob && knobRef(knob);
    if (k) resetKnob(k);
  });
  bench().addEventListener('keydown', onKnobKey);
  bench().addEventListener('wheel', onKnobWheel, { passive: false });
  bench().addEventListener('click', onBenchClick);
  bench().addEventListener('mouseover', onExplainHover);
  bench().addEventListener('mouseleave', () => {
    if (!app.explain) return;
    clearExplain();
    sidebarExplain(null);
  });
  addEventListener('resize', () => requestAnimationFrame(drawCables));
}
