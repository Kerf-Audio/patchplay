// The footer's small oscilloscope and spectrum analyser for the master output. It polls the
// backend's `scope` (a short run of the latest output samples) only while the sound is playing,
// the workbench is showing and the tab is visible, and draws to a canvas scaled for the screen.
// Stopped, it shows a flat line, dimmed. The sums are in `scope-math.ts`.
import { readStored, writeStored } from './storage';
import { app } from './store';
import { onTheme } from './theme';
import { columnLevels, FFT_SIZE, FLOOR_DB, findTrigger, hann, MAX_HZ, MIN_HZ, smoothLevels, spectrumDb } from './scope-math';
import type { Scope } from './types';
import { $, $$ } from './util';

type Mode = 'wave' | 'spectrum';

const KEY = 'patchplay.scope';
const WIDTH = 160;
const HEIGHT = 56;
/** Samples the oscilloscope draws: about 21 ms at 48 kHz, so a bass note shows a cycle or two. */
const WAVE_SAMPLES = 1024;
const FRAME_MS = 33;
const SLOW_FRAME_MS = 100;

const LABEL: Record<Mode, { name: string; tip: string }> = {
  wave: {
    name: 'Oscilloscope of the master output',
    tip: 'The shape of the sound leaving the speakers, as it plays. The line holds still on a steady note. Flat when stopped.',
  },
  spectrum: {
    name: 'Spectrum analyser of the master output',
    tip: 'How loud the sound is at each pitch, low on the left to high on the right (20 Hz to 20 kHz, 0 to -90 dB). Flat when stopped.',
  },
};

const hannWindow = hann(FFT_SIZE);
const shown = new Float32Array(WIDTH).fill(FLOOR_DB);

let canvas: HTMLCanvasElement;
/** False until `initScope` has a 2D context; without one the footer box is gone and there is nothing to sync. */
let ready = false;
let reduced: MediaQueryList;
let g: CanvasRenderingContext2D;
let mode: Mode = readStored(KEY) === 'spectrum' ? 'spectrum' : 'wave';
let frame = 0;
let busy = false;
let lastAsk = -Infinity;
/** The last answer, kept to redraw on a theme change. */
let last: Scope | null = null;
let ink = { trace: '#0a8359', grid: '#0000002e' };

const shouldRun = (): boolean => !!app.state?.playing && app.view === 'bench' && !document.hidden;

function readInk(): void {
  const css = getComputedStyle(document.documentElement);
  ink = { trace: css.getPropertyValue('--audio').trim() || ink.trace, grid: css.getPropertyValue('--wave-zero').trim() || ink.grid };
}

/** Sizes the backing store for the screen's pixel density; the canvas stays 160 by 56 CSS pixels. */
function fit(): void {
  const dpr = Math.max(1, devicePixelRatio || 1);
  canvas.width = Math.round(WIDTH * dpr);
  canvas.height = Math.round(HEIGHT * dpr);
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  draw(last);
}

const lineAt = (x0: number, y0: number, x1: number, y1: number): void => {
  g.moveTo(x0, y0);
  g.lineTo(x1, y1);
};

function graticule(): void {
  g.strokeStyle = ink.grid;
  g.lineWidth = 1;
  g.globalAlpha = 0.4;
  g.beginPath();
  if (mode === 'wave') {
    for (const f of [0.25, 0.75]) lineAt(0, Math.round(HEIGHT * f) + 0.5, WIDTH, Math.round(HEIGHT * f) + 0.5);
    for (const f of [0.25, 0.5, 0.75]) lineAt(Math.round(WIDTH * f) + 0.5, 0, Math.round(WIDTH * f) + 0.5, HEIGHT);
  } else {
    for (const db of [-30, -60]) lineAt(0, Math.round((HEIGHT * db) / FLOOR_DB) + 0.5, WIDTH, Math.round((HEIGHT * db) / FLOOR_DB) + 0.5);
    for (const hz of [100, 1000, 10000]) {
      const x = Math.round((WIDTH * Math.log(hz / MIN_HZ)) / Math.log(MAX_HZ / MIN_HZ)) + 0.5;
      lineAt(x, 0, x, HEIGHT);
    }
  }
  g.stroke();
  if (mode === 'wave') {
    g.globalAlpha = 1;
    g.beginPath();
    lineAt(0, HEIGHT / 2 + 0.5, WIDTH, HEIGHT / 2 + 0.5);
    g.stroke();
  }
  g.globalAlpha = 1;
}

function drawWave(samples: ArrayLike<number>): void {
  const n = samples.length;
  const count = Math.min(WAVE_SAMPLES, n);
  // Search the first part for a rising edge so the rest of the frame is the wave after it.
  const start = n >= WAVE_SAMPLES ? findTrigger(samples, n - WAVE_SAMPLES) : 0;
  const mid = HEIGHT / 2;
  const half = mid - 2;
  g.strokeStyle = ink.trace;
  g.lineWidth = 1.25;
  g.lineJoin = 'round';
  g.beginPath();
  if (count < 2) {
    lineAt(0, mid, WIDTH, mid);
  } else {
    for (let i = 0; i < count; i++) {
      const s = Math.max(-1, Math.min(1, samples[start + i] ?? 0));
      const x = (i * (WIDTH - 1)) / (count - 1);
      if (i === 0) g.moveTo(x, mid - s * half);
      else g.lineTo(x, mid - s * half);
    }
  }
  g.stroke();
}

function drawSpectrum(samples: ArrayLike<number>, rate: number): void {
  if (samples.length >= FFT_SIZE && rate > 0) smoothLevels(shown, columnLevels(spectrumDb(samples, hannWindow), rate, WIDTH));
  else shown.fill(FLOOR_DB);
  g.fillStyle = ink.trace;
  g.strokeStyle = ink.trace;
  g.lineWidth = 1.25;
  g.lineJoin = 'round';
  g.beginPath();
  const y = (db: number): number => HEIGHT - 1 - ((db - FLOOR_DB) / -FLOOR_DB) * (HEIGHT - 3);
  for (let x = 0; x < WIDTH; x++) {
    const level = y(shown[x] ?? FLOOR_DB);
    if (x === 0) g.moveTo(x, level);
    else g.lineTo(x, level);
  }
  g.stroke();
  g.lineTo(WIDTH - 1, HEIGHT);
  g.lineTo(0, HEIGHT);
  g.closePath();
  g.globalAlpha = 0.22;
  g.fill();
  g.globalAlpha = 1;
}

function draw(scope: Scope | null): void {
  g.clearRect(0, 0, WIDTH, HEIGHT);
  graticule();
  if (mode === 'wave') drawWave(scope?.samples ?? []);
  else drawSpectrum(scope?.samples ?? [], scope?.rate ?? 0);
}

function ask(now: number): void {
  if (busy || now - lastAsk < (reduced.matches ? SLOW_FRAME_MS : FRAME_MS)) return;
  busy = true;
  lastAsk = now;
  app.api
    .scope()
    .then((s) => {
      if (!shouldRun()) return;
      last = s;
      draw(s);
    })
    .catch(() => undefined)
    .finally(() => {
      busy = false;
    });
}

function tick(now: number): void {
  frame = 0;
  if (!shouldRun()) {
    stop();
    return;
  }
  frame = requestAnimationFrame(tick);
  ask(now);
}

function stop(): void {
  cancelAnimationFrame(frame);
  frame = 0;
  last = null;
  shown.fill(FLOOR_DB);
  canvas.classList.add('idle');
  draw(null);
}

/** Call after anything that can change whether the scope should run: state, view, visibility. */
export function syncScope(): void {
  if (!ready) return;
  $('#scope-box').classList.toggle('hidden', app.view !== 'bench');
  if (shouldRun()) {
    canvas.classList.remove('idle');
    if (!frame) frame = requestAnimationFrame(tick);
  } else if (frame || !canvas.classList.contains('idle')) {
    stop();
  }
}

function setMode(next: Mode): void {
  mode = next;
  writeStored(KEY, next);
  canvas.setAttribute('aria-label', LABEL[next].name);
  canvas.dataset.tipTitle = next === 'wave' ? 'oscilloscope' : 'spectrum';
  canvas.dataset.tip = LABEL[next].tip;
  $$<HTMLButtonElement>('[data-scope-mode]').forEach((b) => {
    const on = b.dataset.scopeMode === next;
    b.classList.toggle('on', on);
    b.setAttribute('aria-pressed', String(on));
  });
  shown.fill(FLOOR_DB);
  draw(last);
}

export function initScope(): void {
  canvas = $<HTMLCanvasElement>('#scope');
  reduced = matchMedia('(prefers-reduced-motion: reduce)');
  const ctx = canvas.getContext('2d');
  if (!ctx) {
    $('#scope-box').remove();
    return;
  }
  g = ctx;
  ready = true;
  readInk();
  fit();
  setMode(mode);
  $$<HTMLButtonElement>('[data-scope-mode]').forEach((b) => b.addEventListener('click', () => setMode(b.dataset.scopeMode as Mode)));
  onTheme(() => {
    readInk();
    draw(last);
  });
  addEventListener('resize', fit);
  document.addEventListener('visibilitychange', syncScope);
  syncScope();
}
