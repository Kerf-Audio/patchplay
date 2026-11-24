// The Modules catalogue and the Starting points pages.
import { addModule, autoPatchPreference } from './add-module';
import { startDrag } from './drag';
import { renderPanel } from './panel';
import { app, notify, run, subscribe } from './store';
import { collectTags, extraTags, filterByTag, groupByCategory } from './tag-groups';
import type { BandId, PresetInfo } from './types';
import { $, esc } from './util';

const BAND_COLOUR: Record<BandId, string> = {
  time: '#fbbf24',
  motion: '#6a96ff',
  voices: '#34d399',
  shape: '#c58bff',
  space: '#6ec1e4',
  out: '#a2a4aa',
};

export function renderCatalogue(): void {
  const desc = app.desc;
  if (!desc) return;
  const legend = desc.jack_kinds
    .map((k) => `<span class="k-${esc(k.id)}" data-tip-title="${esc(k.label)}" data-tip="${esc(k.tooltip)}"><i></i>${esc(k.label)}</span>`)
    .join('');
  const bands = desc.bands
    .map((b) => {
      const panels = desc.modules
        .filter((m) => m.band === b.id)
        .map((m) => renderPanel(m, null, BAND_COLOUR[b.id]))
        .join('');
      return `<h2 style="--band:${BAND_COLOUR[b.id]}">${esc(b.name)}<small>${esc(b.summary)}</small></h2><div class="cat-row">${panels}</div>`;
    })
    .join('');
  $('#catalogue').innerHTML = `<p class="intro">${desc.modules.length} modules in ${desc.bands.length} bands, all named after joinery and woodwork. Hover any knob or jack for help. Every jack has a kind and colour; the colours are a guide, and any output can go to any input.</p>
    <div class="legend">${legend}</div>${bands}`;
}

let activeTag = 'all';
/** The starting point last opened from this page, so its play button survives a redraw. */
let openedPath: string | null = null;

function spRow(p: PresetInfo, heading: string | null): string {
  const detail = [`${p.modules} modules`, `${Math.round(p.tempo)} bpm`, p.key, p.style].filter(Boolean).map((t) => esc(String(t)));
  const tags = extraTags(p, heading).map(esc).join(' · ');
  const on = p.path === openedPath;
  const playing = on && (app.state?.playing ?? false);
  return `<li class="sp-row${on ? ' on' : ''}" data-start="${esc(p.path)}" data-name="${esc(p.name)}" tabindex="0" role="button">
    <b>${esc(p.name)}</b><span class="sp-meta">${detail.join(' · ')}${tags ? `<span class="sp-tags">${tags}</span>` : ''}</span>
    <span class="sp-actions"><button class="btn" data-play aria-pressed="${playing}" aria-label="${playing ? 'Stop' : 'Play'} ${esc(p.name)}" title="${playing ? 'Stop' : 'Play'}">${playing ? STOP_ICON : PLAY_ICON}</button><button class="btn" data-open aria-label="Open ${esc(p.name)} in the workbench">Open</button></span></li>`;
}

export function renderStartingPoints(): void {
  const tags = collectTags(app.presets);
  if (activeTag !== 'all' && !tags.includes(activeTag)) activeTag = 'all';
  const chips = ['all', ...tags]
    .map((t) => `<button class="chip${t === activeTag ? ' on' : ''}" data-tag="${esc(t)}" aria-pressed="${t === activeTag}">${esc(t)}</button>`)
    .join('');
  const shown = filterByTag(app.presets, activeTag);
  const body =
    activeTag === 'all'
      ? groupByCategory(shown)
          .map((g) => `<h3 class="sp-group">${esc(g.category)}</h3><ul class="sp-list">${g.items.map((p) => spRow(p, g.category)).join('')}</ul>`)
          .join('')
      : `<ul class="sp-list">${shown.map((p) => spRow(p, activeTag)).join('')}</ul>`;
  $('#starting-points').innerHTML = `<h2>starting points<small>Open one as a new session, then change anything</small></h2>
    ${app.presets.length ? `<div class="chips" role="group" aria-label="filter by tag">${chips}</div>${body}` : '<p class="intro">No starting points are installed.</p>'}`;
}

const PLAY_ICON = '<svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M4.5 2.6v10.8L13 8z" fill="currentColor"/></svg>';
const STOP_ICON = '<svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><rect x="3.5" y="3.5" width="9" height="9" rx="1.5" fill="currentColor"/></svg>';

export function initPages(showBench: () => void): void {
  const list = $('#starting-points');
  const syncPlay = (): void => {
    const playing = app.state?.playing ?? false;
    list.querySelectorAll<HTMLElement>('.sp-row').forEach((r) => {
      const btn = r.querySelector<HTMLElement>('[data-play]');
      const on = playing && r.classList.contains('on');
      if (btn) {
        btn.innerHTML = on ? STOP_ICON : PLAY_ICON;
        btn.title = on ? 'Stop' : 'Play';
        btn.setAttribute('aria-label', `${on ? 'Stop' : 'Play'} ${r.dataset.name ?? ''}`.trim());
        btn.setAttribute('aria-pressed', String(on));
      }
    });
  };
  subscribe(syncPlay);
  const load = async (row: HTMLElement): Promise<boolean> => {
    const { start, name } = row.dataset;
    if (!start) return false;
    if (row.classList.contains('on')) return true;
    if (!(await run('open', app.api.openFile(start)))) return false;
    notify(`Opened starting point ${name ?? ''}`.trim());
    list.querySelectorAll('.sp-row.on').forEach((r) => r.classList.remove('on'));
    row.classList.add('on');
    openedPath = start;
    return true;
  };
  const play = async (row: HTMLElement): Promise<void> => {
    if (row.classList.contains('on') && app.state?.playing) await run('stop', app.api.stop());
    else if (await load(row)) await run('play', app.api.play());
    syncPlay();
  };
  const open = async (row: HTMLElement): Promise<void> => {
    if (await load(row)) showBench();
  };
  list.addEventListener('click', (e) => {
    const t = e.target as HTMLElement;
    const tag = t.closest<HTMLElement>('[data-tag]')?.dataset.tag;
    if (tag) {
      activeTag = tag;
      renderStartingPoints();
      return;
    }
    const row = t.closest<HTMLElement>('[data-start]');
    if (!row) return;
    if (t.closest('[data-open]')) void open(row);
    else void play(row);
  });
  list.addEventListener('keydown', (e) => {
    const row = (e.target as HTMLElement).closest<HTMLElement>('[data-start]');
    if (row && e.target === row && (e.key === 'Enter' || e.key === ' ')) {
      e.preventDefault();
      void play(row);
    }
  });
  const cat = $('#catalogue');
  cat.addEventListener('click', (e) => {
    const kind = (e.target as HTMLElement).closest<HTMLElement>('[data-add]')?.dataset.add;
    if (kind) void addModule(kind, { autoPatch: autoPatchPreference() });
  });
  cat.addEventListener('pointerdown', (e) => {
    const t = e.target as HTMLElement;
    const mod = t.closest<HTMLElement>('.mod');
    if (mod?.dataset.kind && t.closest('.mod-head') && !t.closest('button')) startDrag(e, { kind: mod.dataset.kind });
  });
}
