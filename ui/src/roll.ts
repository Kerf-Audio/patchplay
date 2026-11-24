// Inlay's piano roll: 16 steps by the 12 notes of an octave, drawn on a keyboard strip. The roll is
// only a view of the module's 16 `stepN` params, so drawing is `set_params`: undo, locks, the code
// editor, the CLI and agents all see the same numbers. A step param holds one of 25 levels, `round(value * 25)`:
// 0 is a rest, 1..12 a note (C to B) and 13..24 the same note accented. That is also how the engine reads it.
import { patch, send } from './store';
import type { Module, ModuleDesc } from './types';
import { esc } from './util';

export const STEPS = 16;
export const ROWS = 12;
const LEVELS = 25;

export const NOTE_NAMES = ['C', 'C♯', 'D', 'D♯', 'E', 'F', 'F♯', 'G', 'G♯', 'A', 'A♯', 'B'] as const;
const BLACK = new Set([1, 3, 6, 8, 10]);
/** 0 V is C4, so row 0 with no transposition sounds as C4. */
const BASE_OCTAVE = 4;

/** One drawn step: the row (0 is C) and whether it is accented. */
export interface Cell {
  row: number;
  accent: boolean;
}

export const isStepParam = (key: string): boolean => /^step([1-9]|1[0-6])$/.test(key);
export const stepKey = (step: number): string => `step${step + 1}`;
export const isBlack = (row: number): boolean => BLACK.has(row);
export const noteName = (row: number): string => NOTE_NAMES[row] ?? '';

/** The note a step param draws, or null for a rest. */
export function cellOf(value: number): Cell | null {
  const level = Math.min(2 * ROWS, Math.round(Math.min(1, Math.max(0, value)) * LEVELS));
  return level === 0 ? null : { row: (level - 1) % ROWS, accent: level > ROWS };
}

/** The param value that draws a cell; whole hundredths, so the `.kerf` text stays short. */
export const valueOf = (cell: Cell | null): number =>
  cell ? ((cell.row + 1 + (cell.accent ? ROWS : 0)) * 4) / 100 : 0;

/**
 * What a step becomes when cell `row` is pressed: a different row (or a rest) draws that note, the
 * same row clears it. Shift draws accented, and on the note already there it flips the accent instead.
 */
export function clickValue(current: number, row: number, shift: boolean): number {
  const was = cellOf(current);
  if (was?.row === row) return valueOf(shift ? { row, accent: !was.accent } : null);
  return valueOf({ row, accent: shift });
}

const sign = (n: number): string => (n > 0 ? `+${n}` : String(n));

export const lengthOf = (value: number): number => 1 + Math.round(Math.min(1, Math.max(0, value)) * (STEPS - 1));
export const semitonesOf = (value: number): number => Math.round(Math.min(1, Math.max(0, value)) * 24) - 12;
export const octavesOf = (value: number): number => Math.round(Math.min(1, Math.max(0, value)) * 4) - 2;

/** The note a row sounds as after transposing, with its octave number: row 0 at 0 and 0 is C4. */
export function soundName(row: number, semitones: number, octaves: number): string {
  const n = row + semitones + 12 * octaves;
  return `${noteName(((n % 12) + 12) % 12)}${BASE_OCTAVE + Math.floor(n / 12)}`;
}

/** The transposition in words, for the line under the roll. */
export function transposeLabel(semitones: number, octaves: number): string {
  if (semitones === 0 && octaves === 0) return 'Not transposed';
  const parts = [];
  if (semitones !== 0) parts.push(`${sign(semitones)} st`);
  if (octaves !== 0) parts.push(`${sign(octaves)} oct`);
  return `Transposed ${parts.join(', ')}`;
}

const RATES = ['1/4', '1/8', '1/8 triplet', '1/16', '1/16 triplet', '1/32'];

/** What an Inlay knob's value means, for its tooltip; null for a knob that is just a level. */
export function readout(kind: string, key: string, value: number): string | null {
  if (kind !== 'Inlay') return null;
  const v = Math.min(1, Math.max(0, value));
  switch (key) {
    case 'length': {
      const n = lengthOf(v);
      return `${n} ${n === 1 ? 'step' : 'steps'}`;
    }
    case 'transpose': {
      const n = semitonesOf(v);
      return n === 0 ? 'no transposition' : `${sign(n)} semitones`;
    }
    case 'octave': {
      const n = octavesOf(v);
      return n === 0 ? 'no shift' : `${sign(n)} ${Math.abs(n) === 1 ? 'octave' : 'octaves'}`;
    }
    case 'rate':
      return `${RATES[Math.round(v * (RATES.length - 1))]} notes`;
    case 'gate':
      return `${Math.round((0.05 + 0.9 * v) * 100)}% of the step`;
    default:
      return null;
  }
}

const num = (m: Module | null, key: string, fallback: number): number =>
  m?.params.find((p) => p.key === key)?.value ?? fallback;

const stepValues = (m: Module): number[] => Array.from({ length: STEPS }, (_, i) => num(m, stepKey(i), 0));

/** What the catalogue card draws: a short phrase with a tie, a rest and two accents. */
const EXAMPLE_PICKS: ([number, boolean] | null)[] = [
  [0, true], [0, false], null, [4, false], [7, true], [7, false], null, [9, false],
  [7, false], null, [4, false], [2, false], [0, true], null, [7, false], null,
];
const EXAMPLE: (Cell | null)[] = EXAMPLE_PICKS.map((p) => (p ? { row: p[0], accent: p[1] } : null));

function infoText(semitones: number, octaves: number): string {
  const base = transposeLabel(semitones, octaves);
  return semitones === 0 && octaves === 0 ? base : `${base} · C plays ${soundName(0, semitones, octaves)}`;
}

/** The roll as HTML. A module gives buttons to draw with; the catalogue card (`m` null) is a picture. */
export function rollBlock(d: ModuleDesc, m: Module | null): string {
  const values = m ? stepValues(m) : EXAMPLE.map(valueOf);
  const length = lengthOf(num(m, 'length', d.params.find((p) => p.key === 'length')?.default ?? 1));
  const semitones = semitonesOf(num(m, 'transpose', 0.5));
  const octaves = octavesOf(num(m, 'octave', 0.5));
  const picks = values.map(cellOf);
  const focus = picks.findIndex((c) => c) < 0 ? 0 : picks.findIndex((c) => c);
  const keys: string[] = [];
  const cells: string[] = [];
  for (let row = ROWS - 1; row >= 0; row--) {
    keys.push(`<span class="rk${isBlack(row) ? ' black' : ''}">${noteName(row)}</span>`);
    for (let step = 0; step < STEPS; step++) {
      const c = picks[step];
      const on = c?.row === row;
      const cls = `rc${isBlack(row) ? ' blk' : ''}${on ? ' on' : ''}${on && c.accent ? ' acc' : ''}${step >= length ? ' out' : ''}${step % 4 === 0 ? ' beat' : ''}`;
      if (!m) {
        cells.push(`<i class="${cls}"></i>`);
        continue;
      }
      const state = on ? (c.accent ? 'accented note' : 'note') : 'empty';
      const tab = row === (picks[focus]?.row ?? 0) && step === focus ? 0 : -1;
      cells.push(
        `<button type="button" class="${cls}" data-step="${step}" data-row="${row}" tabindex="${tab}" role="gridcell" aria-label="Step ${step + 1}, ${esc(noteName(row))}, ${state}"></button>`,
      );
    }
  }
  const grid = m
    ? `<div class="roll-grid" role="grid" aria-label="Note pattern, 16 steps by 12 notes. Arrow keys move, Enter or Space draws or clears, Shift adds an accent.">${cells.join('')}</div>`
    : `<div class="roll-grid" aria-hidden="true">${cells.join('')}</div>`;
  return `<div class="roll-block"${m ? '' : ' role="img" aria-label="Example note pattern"'}>
    <div class="roll"><div class="roll-keys" aria-hidden="true">${keys.join('')}</div>${grid}</div>
    <div class="roll-info" data-roll-info>${esc(infoText(semitones, octaves))}</div>
  </div>`;
}

const rollOf = (el: Element): HTMLElement | null => el.closest<HTMLElement>('.roll-grid');
const moduleOf = (el: Element): string => el.closest<HTMLElement>('.mod')?.dataset.uid ?? '';
const cellAt = (grid: ParentNode, step: number, row: number): HTMLElement | null =>
  grid.querySelector<HTMLElement>(`.rc[data-step="${step}"][data-row="${row}"]`);

/** Shows a step's new value at once, before the command comes back and redraws the panel. */
function paintStep(grid: ParentNode, step: number, value: number): void {
  const c = cellOf(value);
  grid.querySelectorAll<HTMLElement>(`.rc[data-step="${step}"]`).forEach((el) => {
    const on = c?.row === Number(el.dataset.row);
    el.classList.toggle('on', on);
    el.classList.toggle('acc', on && !!c?.accent);
  });
}

/** Moves the length shading and the transposition line while the Length, Transpose or Octave knob turns. */
export function showRollKnob(panel: HTMLElement, param: string, value: number): void {
  if (param === 'length') {
    const n = lengthOf(value);
    panel.querySelectorAll<HTMLElement>('.rc').forEach((el) => el.classList.toggle('out', Number(el.dataset.step) >= n));
  } else if (param === 'transpose' || param === 'octave') {
    const read = (key: string, now: number): number => (key === param ? now : num(moduleById(moduleOf(panel)), key, 0.5));
    const info = panel.querySelector('[data-roll-info]');
    if (info) info.textContent = infoText(semitonesOf(read('transpose', value)), octavesOf(read('octave', value)));
  }
}

function moduleById(id: string): Module | null {
  return patch().modules.find((m) => m.id === id) ?? null;
}

interface Stroke {
  module: string;
  grid: HTMLElement;
  before: number[];
  edits: Map<number, number>;
  /** Drawing sets the step under the pointer; erasing (the stroke began on a drawn note) clears it. */
  erase: boolean;
  accent: boolean;
  last: string;
}

let stroke: Stroke | null = null;

function touch(s: Stroke, el: HTMLElement): void {
  const step = Number(el.dataset.step);
  const row = Number(el.dataset.row);
  const at = `${step}.${row}`;
  if (at === s.last) return;
  s.last = at;
  if (s.erase && s.edits.has(step)) return;
  const value = s.erase ? 0 : valueOf({ row, accent: s.accent });
  s.edits.set(step, value);
  paintStep(s.grid, step, value);
}

function startStroke(e: PointerEvent, el: HTMLElement, grid: HTMLElement): void {
  const module = moduleOf(el);
  const m = moduleById(module);
  if (!m || e.button !== 0 || e.altKey) return;
  const before = stepValues(m);
  const step = Number(el.dataset.step);
  const row = Number(el.dataset.row);
  const erase = cellOf(before[step] ?? 0)?.row === row && !e.shiftKey;
  stroke = { module, grid, before, edits: new Map(), erase, accent: e.shiftKey, last: '' };
  // A shift press on a drawn note flips its accent, which is not a stroke that carries on.
  const flip = e.shiftKey && cellOf(before[step] ?? 0)?.row === row;
  if (flip) {
    const value = clickValue(before[step] ?? 0, row, true);
    stroke.edits.set(step, value);
    paintStep(grid, step, value);
    stroke.last = `${step}.${row}`;
    stroke.accent = cellOf(value)?.accent ?? true;
  } else {
    touch(stroke, el);
  }
  e.preventDefault();
}

/** The params a stroke changed: only steps whose value really moved. */
export function strokeParams(before: number[], edits: Map<number, number>): { param: string; value: number }[] {
  return [...edits]
    .filter(([step, value]) => Math.abs(value - (before[step] ?? 0)) >= 0.001)
    .map(([step, value]) => ({ param: stepKey(step), value }));
}

async function commit(s: Stroke): Promise<void> {
  const params = strokeParams(s.before, s.edits);
  // One command, so one drag is one undo step.
  if (params.length) await send({ op: 'set_params', module: s.module, params });
}

/** Puts keyboard focus back on a cell after the panel was redrawn. */
function refocus(module: string, step: number, row: number): void {
  const grid = document.querySelector<HTMLElement>(`.mod[data-uid="${CSS.escape(module)}"] .roll-grid`);
  const next = grid && cellAt(grid, step, row);
  if (!next) return;
  grid.querySelectorAll<HTMLElement>('.rc[tabindex="0"]').forEach((el) => el.setAttribute('tabindex', '-1'));
  next.setAttribute('tabindex', '0');
  next.focus({ preventScroll: true });
}

const MOVES: Record<string, [number, number]> = {
  ArrowLeft: [-1, 0],
  ArrowRight: [1, 0],
  ArrowUp: [0, 1],
  ArrowDown: [0, -1],
};

function onKey(e: KeyboardEvent): void {
  const el = (e.target as HTMLElement).closest<HTMLElement>('.rc');
  const grid = el && rollOf(el);
  if (!el || !grid) return;
  const step = Number(el.dataset.step);
  const row = Number(el.dataset.row);
  const move = MOVES[e.key];
  const jump = e.key === 'Home' ? 0 : e.key === 'End' ? STEPS - 1 : null;
  if (move || jump !== null) {
    const nextStep = jump ?? Math.min(STEPS - 1, Math.max(0, step + (move?.[0] ?? 0)));
    const nextRow = Math.min(ROWS - 1, Math.max(0, row + (move?.[1] ?? 0)));
    const next = cellAt(grid, nextStep, nextRow);
    if (next) {
      el.setAttribute('tabindex', '-1');
      next.setAttribute('tabindex', '0');
      next.focus();
    }
  } else if (e.key === 'Enter' || e.key === ' ' || e.key === 'Delete' || e.key === 'Backspace') {
    const m = moduleById(moduleOf(el));
    if (!m) return;
    const current = stepValues(m)[step] ?? 0;
    const value = e.key === 'Delete' || e.key === 'Backspace' ? 0 : clickValue(current, row, e.shiftKey);
    const module = moduleOf(el);
    paintStep(grid, step, value);
    void send({ op: 'set_param', module, param: stepKey(step), value }).then(() => refocus(module, step, row));
  } else return;
  e.preventDefault();
}

/** Tells a cell's tooltip what it would do, just before the tooltip reads it. */
function onHover(e: MouseEvent): void {
  const el = (e.target as HTMLElement).closest<HTMLElement>('.rc');
  if (!el) return;
  const m = moduleById(moduleOf(el));
  if (!m) return;
  const step = Number(el.dataset.step);
  const row = Number(el.dataset.row);
  const semitones = semitonesOf(num(m, 'transpose', 0.5));
  const octaves = octavesOf(num(m, 'octave', 0.5));
  const there = cellOf(num(m, stepKey(step), 0));
  const sounds = semitones !== 0 || octaves !== 0 ? ` Sounds as ${soundName(row, semitones, octaves)}.` : '';
  el.dataset.tipTitle = `Step ${step + 1} · ${noteName(row)}`;
  el.dataset.tip =
    (there?.row === row ? 'Click to clear this step.' : 'Click to put this note on the step. Drag across steps to draw several.') +
    ' Shift adds an accent.' +
    sounds;
}

/** Keeps the roll's length shading and transposition line in step with the knob being turned (the one with focus). */
function followKnob(): void {
  const knob = document.activeElement?.closest<HTMLElement>('.knob');
  const panel = knob?.closest<HTMLElement>('.mod[data-kind="Inlay"]');
  if (knob?.dataset.k && knob.dataset.live && panel) showRollKnob(panel, knob.dataset.k, Number(knob.dataset.live));
}

export function initRoll(): void {
  const bench = document.getElementById('bench');
  if (!bench) return;
  bench.addEventListener('pointerdown', (e) => {
    const el = (e.target as HTMLElement).closest<HTMLElement>('.rc[data-step]');
    const grid = el && rollOf(el);
    if (el && grid) startStroke(e, el, grid);
  });
  document.addEventListener('pointermove', (e) => {
    if (!stroke) {
      if (e.buttons & 1) followKnob();
      return;
    }
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>('.rc[data-step]');
    if (el && rollOf(el) === stroke.grid) touch(stroke, el);
  });
  bench.addEventListener('keydown', followKnob);
  bench.addEventListener('wheel', followKnob, { passive: true });
  const end = (): void => {
    const s = stroke;
    stroke = null;
    if (s) void commit(s);
  };
  document.addEventListener('pointerup', end);
  document.addEventListener('pointercancel', () => {
    const s = stroke;
    stroke = null;
    if (!s) return;
    s.before.forEach((value, step) => paintStep(s.grid, step, value));
  });
  bench.addEventListener('keydown', onKey);
  bench.addEventListener('mouseover', onHover);
}
