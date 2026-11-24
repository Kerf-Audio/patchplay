// The workbench: a row per part, module panels, and the cables drawn over them.
import { cablePath, splitRef, type Point } from './cables';
import { explainModule, explainPart, trace } from './explain';
import { ICON } from './icons';
import { faderHtml } from './part-fader';
import { renderPanel } from './panel';
import { sidebarExplain } from './sidebar';
import { app, patch } from './store';
import { $, $$, cableStroke, esc } from './util';

const RACK_COLOUR = '#6b645a';

const bench = (): HTMLElement => $('#bench');
const jackEl = (ref: string): HTMLElement | null =>
  bench().querySelector<HTMLElement>(`.jack[data-ref="${CSS.escape(ref)}"]`);

/** Centre of a jack's hole in bench coordinates, or null when the module is not on the bench. */
export function holeCentre(ref: string): Point | null {
  const hole = jackEl(ref)?.querySelector<HTMLElement>('.hole');
  if (!hole) return null;
  const b = bench().getBoundingClientRect();
  const r = hole.getBoundingClientRect();
  return [r.left - b.left + r.width / 2, r.top - b.top + r.height / 2];
}

function partLabel(id: string, name: string, count: number, locked: boolean, muted: boolean, tools: boolean, level = 1): string {
  return `<div class="part-label" data-part="${esc(id)}">
    <div class="part-name"><b>${esc(name)}</b><span class="count">${count} module${count === 1 ? '' : 's'}</span></div>
    ${
      tools
        ? `<div class="tools">
      <button data-part-lock="${esc(id)}" class="${locked ? 'on' : ''}" aria-label="Lock part" aria-pressed="${locked}" data-tip-title="Lock part" data-tip="Evolve and Breed leave every module in this part alone.">${locked ? ICON.lock : ICON.unlock}</button>
      <button data-part-mute="${esc(id)}" class="${muted ? 'on' : ''}" aria-label="Mute part" aria-pressed="${muted}" data-tip-title="Mute part" data-tip="Silences this part while the rest plays.">${ICON.mute}</button></div>
      ${faderHtml(esc(id), level)}`
        : ''
    }
  </div>`;
}

export function renderBench(): void {
  const p = patch();
  const panels = (partId: string | null, colour: string): string =>
    p.modules
      .filter((m) => m.part === partId)
      .map((m) => {
        const d = app.descs.get(m.kind);
        return d ? renderPanel(d, m, colour) : '';
      })
      .join('');
  const rows = p.parts.map((pt) => {
    const count = p.modules.filter((m) => m.part === pt.id).length;
    return `<div class="part-row${pt.muted ? ' muted' : ''}" data-part-row="${esc(pt.id)}" style="--part:${esc(pt.colour)}">
      ${partLabel(pt.id, pt.name, count, pt.locked, pt.muted, true, pt.level ?? 1)}${panels(pt.id, pt.colour)}</div>`;
  });
  const loose = p.modules.filter((m) => m.part === null).length;
  if (loose > 0) {
    rows.push(`<div class="part-row" data-part-row="" style="--part:${RACK_COLOUR}">${partLabel('', 'Rack', loose, false, false, false)}${panels(null, RACK_COLOUR)}</div>`);
  }
  const empty =
    p.modules.length === 0
      ? '<div class="empty"><b>An empty rack.</b> Open a starting point, open a .kerf file, add a module, or press Random.</div>'
      : '';
  bench().innerHTML = `${empty}${rows.join('')}<svg id="cables"></svg>`;
  requestAnimationFrame(drawCables);
}

export function drawCables(): void {
  const svg = bench().querySelector<SVGElement>('#cables');
  if (!svg) return;
  svg.setAttribute('width', String(bench().scrollWidth));
  svg.setAttribute('height', String(bench().scrollHeight));
  const p = patch();
  let html = '';
  p.cables.forEach((c, i) => {
    const a = holeCentre(c.from);
    const b = holeCentre(c.to);
    const from = jackEl(c.from);
    if (!a || !b || !from) return;
    const owner = p.modules.find((m) => m.id === splitRef(c.from)[0]);
    const muted = p.parts.find((x) => x.id === owner?.part)?.muted;
    const d = cablePath(a, b);
    html += `<path class="shadow" d="${d}"/><path d="${d}" data-i="${i}" class="${muted ? 'faint' : ''}" style="${cableStroke(from.dataset.kind ?? '')}"
      data-tip-title="Cable" data-tip="${esc(c.from)} → ${esc(c.to)}. Click to remove."/><path class="sheen" d="${d}"/>`;
  });
  svg.innerHTML = html;
}

/** Dims everything unrelated to `id` and writes the trace into the sidebar. */
export function showExplainModule(id: string): void {
  const p = patch();
  const t = trace(p.cables, id);
  const lit = new Set([id, ...t.upstream, ...t.downstream]);
  $$('#bench .mod').forEach((el) => {
    el.classList.toggle('dim', !lit.has(el.dataset.uid ?? ''));
    el.classList.toggle('focus', el.dataset.uid === id);
  });
  $$('#cables path[data-i]').forEach((el) => {
    const c = p.cables[Number(el.dataset.i)];
    const on = !!c && lit.has(splitRef(c.from)[0]) && lit.has(splitRef(c.to)[0]);
    el.classList.toggle('hot', on);
    el.classList.toggle('faint', !on);
  });
  sidebarExplain({ module: explainModule(app.descs, p, id) });
}

export function showExplainPart(partId: string): void {
  const p = patch();
  const part = p.parts.find((x) => x.id === partId);
  const lines = explainPart(app.descs, p, partId || null);
  $$('#bench .mod').forEach((el) => {
    const m = p.modules.find((x) => x.id === el.dataset.uid);
    el.classList.toggle('dim', m?.part !== (partId || null));
  });
  sidebarExplain({ part: { name: part?.name ?? 'Rack', lines } });
}

export function clearExplain(): void {
  $$('#bench .mod').forEach((el) => el.classList.remove('dim', 'focus'));
  drawCables();
}
