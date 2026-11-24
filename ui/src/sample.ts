// A sample module's sample (Veneer, Splinter): the waveform (16 slice marks on a Veneer, the grain
// read point and spray on a Splinter), the file name with its clear control, choosing a file and
// dropping one onto the panel. Every change is a `set_sample` command; the waveform is the
// engine's own decoded buffer.
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { app, notify, patch, send } from './store';
import type { Module, ModuleDesc, SamplePeaks } from './types';
import { baseName, esc, messageOf } from './util';
import { initWebSampleDrop } from './web/sample-drop';

const HEIGHT = 64;
const AUDIO = /\.(wav|aiff?|flac|mp3|ogg|oga)$/i;

export const sliceIndex = (value: number, slices: number): number =>
  Math.min(slices - 1, Math.max(0, Math.floor(value * slices)));

const clamp01 = (x: number): number => Math.min(1, Math.max(0, x));

/** The stretch of the sample (0..1) a Splinter's grains start in, and its read point. */
export function grainRegion(position: number, spray: number): { from: number; to: number; at: number } {
  const at = clamp01(position);
  return { from: clamp01(at - spray / 2), to: clamp01(at + spray / 2), at };
}

function peaksPath(peaks: [number, number][]): string {
  const mid = HEIGHT / 2;
  return peaks
    .map(([lo, hi], x) => {
      const top = (mid - Math.min(1, hi) * (mid - 2)).toFixed(1);
      const bottom = (mid - Math.max(-1, lo) * (mid - 2)).toFixed(1);
      return `M${x + 0.5} ${top}V${Math.max(Number(bottom), Number(top) + 0.8).toFixed(1)}`;
    })
    .join('');
}

/** The waveform as SVG: min/max peaks, a mark between slices and, unless `selected` is null, that slice shaded. */
export function waveSvg(peaks: [number, number][], slices: number, selected: number | null): string {
  const w = Math.max(peaks.length, 1);
  const mid = HEIGHT / 2;
  const marks = Array.from({ length: slices - 1 }, (_, i) => `M${(((i + 1) * w) / slices).toFixed(2)} 0V${HEIGHT}`).join('');
  return `<svg viewBox="0 0 ${w} ${HEIGHT}" preserveAspectRatio="none" role="img" aria-label="Sample waveform in ${slices} slices">
    ${selected === null ? '' : `<rect class="wv-sel" x="${((selected * w) / slices).toFixed(2)}" y="0" width="${(w / slices).toFixed(2)}" height="${HEIGHT}"/>`}
    <path class="wv-zero" d="M0 ${mid}H${w}"/><path class="wv-marks" d="${marks}"/><path class="wv-peaks" d="${peaksPath(peaks)}"/></svg>`;
}

/** The waveform of a Splinter: the read point as a line, and the spray around it shaded. */
export function grainSvg(peaks: [number, number][], position: number, spray: number): string {
  const w = Math.max(peaks.length, 1);
  const mid = HEIGHT / 2;
  const r = grainRegion(position, spray);
  return `<svg viewBox="0 0 ${w} ${HEIGHT}" preserveAspectRatio="none" role="img" aria-label="Sample waveform with the grain read point" data-position="${position}" data-spray="${spray}">
    <rect class="wv-spray" x="${(r.from * w).toFixed(2)}" y="0" width="${((r.to - r.from) * w).toFixed(2)}" height="${HEIGHT}"/>
    <path class="wv-zero" d="M0 ${mid}H${w}"/><path class="wv-peaks" d="${peaksPath(peaks)}"/><path class="wv-pos" d="M${(r.at * w).toFixed(2)} 0V${HEIGHT}"/></svg>`;
}

type Entry = SamplePeaks | { error: string };
/** Decoded peaks by panel, sample and file; the oldest go first so a long session does not keep every sample it ever showed. */
const CACHE_MAX = 32;
const cache = new Map<string, Entry>();
const asked = new Set<string>();
/** Why the last read of a sample failed. Not cached: the next redraw asks again. */
const failed = new Map<string, string>();

function remember(key: string, peaks: SamplePeaks): void {
  cache.delete(key);
  cache.set(key, peaks);
  for (const old of cache.keys()) {
    if (cache.size <= CACHE_MAX) break;
    cache.delete(old);
    asked.delete(old);
  }
}
/** The demo sample's waveform, fetched once and shared by every read-only sampler card. */
let demo: Entry | null = null;
let demoAsked = false;

const keyOf = (m: Module): string => `${m.id}|${app.state?.path ?? ''}|${m.sample ?? ''}`;
const paramOf = (m: Module, key: string): number => m.params.find((p) => p.key === key)?.value ?? 0;
const sliceOf = (m: Module): number => sliceIndex(paramOf(m, 'slice'), 16);

function inner(m: Module): string {
  const key = keyOf(m);
  const e: Entry | undefined = cache.get(key) ?? (failed.has(key) ? { error: failed.get(key) ?? '' } : undefined);
  const name = m.sample ? esc(baseName(m.sample)) : 'Demo sample';
  const path = m.sample ? ` title="${esc(m.sample)}"` : '';
  const clear = m.sample
    ? '<button class="sample-clear" data-sample-clear data-tip-title="Remove sample" data-tip="Go back to the built-in demo sample.">×</button>'
    : '';
  const warn = e && 'warning' in e && e.warning ? `<div class="sample-warn">${esc(e.warning)}</div>` : '';
  const err = e && 'error' in e ? `<div class="sample-warn">${esc(e.error)}</div>` : '';
  const grains = m.kind === 'Splinter';
  const wave =
    e && 'peaks' in e
      ? grains
        ? grainSvg(e.peaks, paramOf(m, 'position'), paramOf(m, 'spray'))
        : waveSvg(e.peaks, e.slices, sliceOf(m))
      : '<span class="wave-wait">reading the sample…</span>';
  const secs = e && 'seconds' in e ? ` · ${e.seconds.toFixed(1)} s` : '';
  const use = grains ? 'Grains are read from it.' : 'It is cut into 16 slices.';
  return `<div class="wave" data-wave>${wave}</div>
    <div class="sample-row">
      <button class="sample-choose" data-sample-choose data-tip-title="Choose a sample" data-tip="Pick an audio file (WAV, AIFF, FLAC, MP3 or Ogg) or drop one on this panel. ${use}">Choose…</button>
      <span class="sample-name"${path}>${name}${secs}</span>${clear}
    </div>${warn}${err}`;
}

/** The sample section of a Veneer or Splinter panel, drawn from what is cached; fetches the rest. */
export function sampleBlock(m: Module): string {
  const key = keyOf(m);
  if (!cache.has(key) && !asked.has(key)) {
    asked.add(key);
    failed.delete(key);
    void app.api
      .samplePeaks(m.id)
      .then((p) => remember(key, p))
      .catch((e: unknown) => {
        failed.set(key, messageOf(e));
        asked.delete(key);
      })
      .finally(() => paint(m.id));
  }
  return `<div class="sample" data-sample>${inner(m)}</div>`;
}

function demoInner(kind: string, position: number, spray: number): string {
  const wave =
    demo && 'peaks' in demo
      ? kind === 'Splinter'
        ? grainSvg(demo.peaks, position, spray)
        : waveSvg(demo.peaks, demo.slices, null)
      : '<span class="wave-wait">reading the sample…</span>';
  const secs = demo && 'seconds' in demo ? ` · ${demo.seconds.toFixed(1)} s` : '';
  const err = demo && 'error' in demo ? `<div class="sample-warn">${esc(demo.error)}</div>` : '';
  return `<div class="wave">${wave}</div>
    <div class="sample-row"><span class="sample-name">Demo sample${secs}</span></div>${err}`;
}

/** The read-only sample section of a catalogue sampler card: the demo sample's waveform with its slice marks, or the default read point and spray. */
export function demoSampleBlock(d: ModuleDesc): string {
  const start = (key: string): number => d.params.find((p) => p.key === key)?.default ?? 0;
  const [position, spray] = [start('position'), start('spray')];
  if (!demo && !demoAsked) {
    demoAsked = true;
    void app.api
      .demoSamplePeaks()
      .then((p) => (demo = p))
      .catch((e: unknown) => (demo = { error: messageOf(e) }))
      .finally(() => {
        for (const box of document.querySelectorAll<HTMLElement>('[data-sample-demo]')) {
          box.innerHTML = demoInner(box.dataset.kind ?? '', Number(box.dataset.position), Number(box.dataset.spray));
        }
      });
  }
  return `<div class="sample" data-sample-demo data-kind="${esc(d.kind)}" data-position="${position}" data-spray="${spray}">${demoInner(d.kind, position, spray)}</div>`;
}

/** Redraws one panel's sample section in place, so the bench is not rebuilt. */
function paint(id: string): void {
  const m = patch().modules.find((x) => x.id === id);
  const box = document.querySelector<HTMLElement>(`.mod[data-uid="${CSS.escape(id)}"] [data-sample]`);
  if (m && box) box.innerHTML = inner(m);
}

/** Moves the selected-slice shading while the Slice knob turns. */
export function showSlice(panel: HTMLElement, value: number): void {
  const sel = panel.querySelector<SVGRectElement>('.wv-sel');
  const svg = sel?.ownerSVGElement;
  if (!sel || !svg) return;
  const w = svg.viewBox.baseVal.width;
  sel.setAttribute('x', String((sliceIndex(value, 16) * w) / 16));
}

/** Moves a Splinter's read point and spray shading while the Position or Spray knob turns. */
export function showGrains(panel: HTMLElement, param: 'position' | 'spray', value: number): void {
  const svg = panel.querySelector<SVGSVGElement>('.wave svg[data-spray]');
  const rect = svg?.querySelector<SVGRectElement>('.wv-spray');
  const mark = svg?.querySelector<SVGPathElement>('.wv-pos');
  if (!svg || !rect || !mark) return;
  svg.dataset[param] = String(value);
  const w = svg.viewBox.baseVal.width;
  const r = grainRegion(Number(svg.dataset.position), Number(svg.dataset.spray));
  rect.setAttribute('x', String(r.from * w));
  rect.setAttribute('width', String((r.to - r.from) * w));
  mark.setAttribute('d', `M${r.at * w} 0V${HEIGHT}`);
}

async function chooseFor(id: string): Promise<void> {
  const picked = await app.api.pickPath('open-sample', '');
  if (picked) await setSample(id, picked);
}

export async function setSample(id: string, path: string | null): Promise<void> {
  if (path !== null && !AUDIO.test(path)) {
    notify(`${baseName(path)} is not an audio file patchplay reads (WAV, AIFF, FLAC, MP3 or Ogg).`, true);
    return;
  }
  await send({ op: 'set_sample', module: id, path });
}

/** Handles clicks on the choose and clear buttons; true when it was one of them. */
export function onSampleClick(t: HTMLElement): boolean {
  const id = t.closest<HTMLElement>('.mod')?.dataset.uid;
  if (!id) return false;
  if (t.closest('[data-sample-choose]')) void chooseFor(id);
  else if (t.closest('[data-sample-clear]')) void setSample(id, null);
  else return false;
  return true;
}

const panelAt = (x: number, y: number): HTMLElement | null => {
  const p = document.elementFromPoint(x, y)?.closest<HTMLElement>('#bench .mod');
  return p?.querySelector('[data-sample]') ? p : null;
};

const highlight = (panel: HTMLElement | null): void => {
  document.querySelectorAll('.mod.drop-hover').forEach((el) => el !== panel && el.classList.remove('drop-hover'));
  panel?.classList.add('drop-hover');
};

/** Audio files dropped on a Veneer or Splinter panel become its sample: by path on the desktop, decoded in the page on the web. */
export async function initSampleDrop(): Promise<void> {
  if (__WEB__) return initWebSampleDrop();
  if (!('__TAURI_INTERNALS__' in window)) return;
  await getCurrentWebview().onDragDropEvent((event) => {
    const p = event.payload;
    if (p.type === 'leave') {
      highlight(null);
      return;
    }
    const at = 'position' in p ? panelAt(p.position.x / devicePixelRatio, p.position.y / devicePixelRatio) : null;
    if (p.type !== 'drop') {
      highlight(at);
      return;
    }
    highlight(null);
    const file = p.paths.find((x) => AUDIO.test(x));
    if (!at) {
      if (p.paths.length > 0) notify('Drop the file on a Veneer or Splinter panel.', true);
    } else if (file) void setSample(at.dataset.uid ?? '', file);
    else notify('patchplay reads WAV, AIFF, FLAC, MP3 and Ogg files.', true);
  });
}
