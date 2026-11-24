// The header (files, tempo, key, play and the level meter), the generate bar
// and the keyboard shortcuts.
import { clearExplain } from './bench';
import { breedMate } from './breed';
import { ICON } from './icons';
import { openPopout } from './popout';
import { shortcutFor, type KeyTarget, type Shortcut } from './shortcuts';
import { sidebarExplain } from './sidebar';
import { writeStored } from './storage';
import { app, changed, notify, patch, run, send } from './store';
import type { Meter } from './types';
import { $, $$, baseName, esc, messageOf, newSeed } from './util';

const EXPORT_SECONDS = 16;
const METER_MS = 66;
const FLOOR_DB = 48;

const input = (sel: string): HTMLInputElement => $<HTMLInputElement>(sel);

export function renderToolbar(): void {
  const s = app.state;
  if (!s) return;
  const p = patch();
  const bpm = input('#bpm');
  if (document.activeElement !== bpm) bpm.value = String(Math.round(p.tempo));
  const key = $('#key');
  key.textContent = p.key ?? '—';
  key.title = p.key ?? '';
  const style = $<HTMLSelectElement>('#style');
  if (p.style && [...style.options].some((o) => o.value === p.style?.name)) style.value = p.style.name;
  $<HTMLButtonElement>('#btn-undo').disabled = !s.can_undo;
  $<HTMLButtonElement>('#btn-redo').disabled = !s.can_redo;
  const mate = breedMate(s.lineage, app.mate);
  const breed = $<HTMLButtonElement>('#btn-breed');
  breed.disabled = mate === null;
  breed.dataset.tip =
    mate === null
      ? 'Make another patch first with random or evolve, then breed crosses the two. Each knob comes from one parent.'
      : `Crosses this patch with #${mate}${app.mate === mate ? ' (hearted)' : ', the latest other patch; heart one in the lineage to choose'}. Each knob comes from one parent.`;
  $('#btn-explain').classList.toggle('on', app.explain);
  $('#btn-beginner').classList.toggle('on', app.beginner);
  $('#btn-code').classList.toggle('on', app.popout);
  const play = $('#play');
  play.classList.toggle('on', s.playing);
  play.innerHTML = s.playing ? ICON.stop : ICON.play;
  play.setAttribute('aria-label', s.playing ? 'Stop' : 'Play');
  play.setAttribute('aria-pressed', String(s.playing));
  const status = $('#check');
  status.classList.toggle('bad', app.notice?.error ?? false);
  const text = app.notice?.text ?? `${p.modules.length} modules · ${p.cables.length} cables · ${s.path ? baseName(s.path) : 'unsaved'}`;
  status.innerHTML = `${ICON.check}<span>${esc(text)}</span>`;
  // The status line also carries the module count, which changes on every edit, so only notices are announced.
  const announce = $('#announce');
  const said = app.notice?.text ?? '';
  if (announce.textContent !== said) announce.textContent = said;
}

async function pickAndRun(kind: 'open-kerf' | 'save-kerf', suggested: string): Promise<void> {
  try {
    const path = await app.api.pickPath(kind, suggested);
    if (!path) return;
    const call = kind === 'open-kerf' ? app.api.openFile(path) : app.api.saveFile(path);
    if (await run(kind === 'open-kerf' ? 'open' : 'save', call)) notify(`${kind === 'open-kerf' ? 'Opened' : 'Saved'} ${baseName(path)}`);
  } catch (e) {
    notify(messageOf(e), true);
  }
}

async function save(asNew: boolean): Promise<void> {
  const path = app.state?.path;
  if (!path || asNew) return pickAndRun('save-kerf', path ?? 'patch.kerf');
  if (await run('save', app.api.saveFile())) notify(`Saved ${baseName(path)}`);
}

async function exportWav(): Promise<void> {
  try {
    const path = await app.api.pickPath('save-wav', 'kerf.wav');
    if (!path) return;
    notify(`Rendering ${EXPORT_SECONDS} s…`);
    const r = await app.api.renderWav(path, EXPORT_SECONDS);
    const silent = r.silent_kinds.length ? ` · silent: ${r.silent_kinds.join(', ')}` : '';
    notify(`Wrote ${baseName(r.path)} · peak ${r.peak_db.toFixed(1)} dB · rms ${r.rms_db.toFixed(1)} dB${silent}`);
  } catch (e) {
    notify(messageOf(e), true);
  }
}

export function togglePlay(): Promise<boolean> {
  return app.state?.playing ? run('stop', app.api.stop()) : run('play', app.api.play());
}

export function toggleExplain(): void {
  app.explain = !app.explain;
  if (!app.explain) {
    clearExplain();
    sidebarExplain(null);
  }
  changed();
}

const percent = (peak: number): number => Math.max(0, Math.min(100, 100 * (1 + (20 * Math.log10(peak || 1e-6)) / FLOOR_DB)));

let metering = false;

async function pollMeter(): Promise<void> {
  if (metering) return;
  if (!app.state?.playing) {
    if (app.meter.peak_l + app.meter.peak_r + app.meter.load > 0) setLevels({ peak_l: 0, peak_r: 0, load: 0 });
    return;
  }
  metering = true;
  try {
    setLevels(await app.api.meter());
  } catch {
    // The next poll tries again; a failed poll is not worth a notice.
  } finally {
    metering = false;
  }
}

function setLevels(m: Meter): void {
  // The load jumps from block to block; rise fast and fall slowly so the figure stays readable.
  const load = m.load > app.meter.load ? (m.load + app.meter.load) / 2 : app.meter.load * 0.85 + m.load * 0.15;
  app.meter = { ...m, load: load < 0.001 ? 0 : load };
  const heights = [percent(m.peak_l), percent(m.peak_r)];
  $('#lvl-l').style.height = `${heights[0]}%`;
  $('#lvl-r').style.height = `${heights[1]}%`;
  $$('[data-meter] i').forEach((el, i) => (el.style.height = `${heights[i % 2]}%`));
  const pct = Math.round(app.meter.load * 100);
  $('#cpu-bar').style.width = `${Math.min(100, pct)}%`;
  $('#cpu-val').textContent = `${pct}%`;
  $('#cpu').classList.toggle('warn', pct >= 60 && pct < 85);
  $('#cpu').classList.toggle('bad', pct >= 85);
}

const SHORTCUTS: Record<Shortcut, () => void> = {
  undo: () => void send({ op: 'undo' }),
  redo: () => void send({ op: 'redo' }),
  save: () => void save(false),
  'save-as': () => void save(true),
  open: () => void pickAndRun('open-kerf', ''),
  play: () => void togglePlay(),
  random: () => $('#btn-random').click(),
  evolve: () => $('#btn-evolve').click(),
  explain: toggleExplain,
};

/** Typing fields and buttons (Space presses them) each treat keys their own way. */
function keyTarget(el: Element | null): KeyTarget {
  if (el?.closest('input, select, textarea, [contenteditable]:not([contenteditable="false"])')) return 'field';
  return el?.closest('button, a[href], [role="button"]') ? 'button' : 'page';
}

function onKey(e: KeyboardEvent): void {
  const shortcut = shortcutFor(e, keyTarget(e.target instanceof Element ? e.target : null), app.view === 'bench');
  if (!shortcut) return;
  e.preventDefault();
  SHORTCUTS[shortcut]();
}

export function initToolbar(): void {
  $$('[data-ico]').forEach((b) => b.insertAdjacentHTML('afterbegin', ICON[b.dataset.ico as keyof typeof ICON]));
  const style = $<HTMLSelectElement>('#style');
  style.innerHTML = app.styles.map((s) => `<option value="${esc(s.name)}">${esc(s.title)}</option>`).join('');
  $('#btn-random').onclick = () => void send({ op: 'random', style: style.value, seed: newSeed() });
  $('#btn-evolve').onclick = () => void send({ op: 'evolve', strength: Number(input('#strength').value), seed: newSeed() });
  $('#btn-breed').onclick = () => {
    const mate = app.state && breedMate(app.state.lineage, app.mate);
    if (mate !== null && mate !== undefined) void send({ op: 'breed', with: { node: mate }, seed: newSeed() });
  };
  $('#btn-undo').onclick = () => void send({ op: 'undo' });
  $('#btn-redo').onclick = () => void send({ op: 'redo' });
  $('#btn-explain').onclick = toggleExplain;
  $('#btn-beginner').onclick = () => {
    app.beginner = !app.beginner;
    writeStored('kerf.beginner', app.beginner ? 'on' : 'off');
    notify(app.beginner ? 'Beginner mode on: drop a module on another to patch them together' : 'Beginner mode off: dropped modules are only placed');
  };
  $('#btn-open').onclick = () => void pickAndRun('open-kerf', '');
  $('#btn-save').onclick = (e) => void save(e.shiftKey);
  $('#btn-wav').onclick = () => void exportWav();
  $('#btn-code').onclick = () => void openPopout();
  $('#play').onclick = () => void togglePlay();
  input('#bpm').addEventListener('change', (e) => {
    const bpm = parseFloat((e.target as HTMLInputElement).value);
    if (Number.isFinite(bpm)) void send({ op: 'set_tempo', bpm });
    else notify('Tempo must be a number', true);
  });
  document.addEventListener('keydown', onKey);
  setInterval(() => void pollMeter(), METER_MS);
}
