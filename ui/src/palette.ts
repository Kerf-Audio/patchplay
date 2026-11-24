// The "+ Module" palette: a floating, searchable list of every module grouped
// by band. Click one to add it, or drag it onto the rack. It is not modal, so
// the rack stays in view and droppable.
import { addModule, autoPatchPreference, setAutoPatchPreference, suggestId } from './add-module';
import { startDrag } from './drag';
import { readStored, writeStored } from './storage';
import { app, patch, subscribe } from './store';
import type { ModuleDesc } from './types';
import { $, esc } from './util';

const NEW_PART = '__new';
const HINT_KEY = 'kerf.hint.beginner';

/** Modules whose kind, summary or band match every word of `query`. */
export function filterModules(mods: ModuleDesc[], query: string): ModuleDesc[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  return mods.filter((m) => {
    const hay = `${m.kind} ${m.summary} ${m.band}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

const el = (): HTMLElement => $('#palette');
const open = (): boolean => !el().classList.contains('hidden');

function renderList(): void {
  const desc = app.desc;
  if (!desc) return;
  const query = $<HTMLInputElement>('#pal-q').value;
  const shown = filterModules(desc.modules, query);
  $('.pal-list').innerHTML =
    desc.bands
      .map((b) => {
        const items = shown.filter((m) => m.band === b.id);
        if (!items.length) return '';
        return `<h4>${esc(b.name)}<small>${esc(b.summary)}</small></h4>${items
          .map((m) => `<div class="pal-item" data-kind="${esc(m.kind)}"><b>${esc(m.kind)}</b><span>${esc(m.summary)}</span></div>`)
          .join('')}`;
      })
      .join('') || '<p class="hint">No module matches.</p>';
}

function renderChoices(): void {
  const part = $<HTMLSelectElement>('#pal-part');
  const keep = part.value;
  const parts = patch().parts;
  part.innerHTML = `${parts.map((p) => `<option value="${esc(p.id)}">${esc(p.name)}</option>`).join('')}
    <option value="">Loose rack</option><option value="${NEW_PART}">New part…</option>`;
  part.value = [...part.options].some((o) => o.value === keep) ? keep : (parts[0]?.id ?? '');
  $<HTMLInputElement>('#pal-id').placeholder = 'id, e.g. chisel1';
}

function addFromPalette(kind: string, keepOpen: boolean): void {
  const part = $<HTMLSelectElement>('#pal-part').value;
  const id = $<HTMLInputElement>('#pal-id').value.trim();
  const fresh = part === NEW_PART;
  void addModule(kind, {
    part: fresh ? undefined : part || undefined,
    newPart: fresh ? suggestId(kind, patch().parts.map((p) => p.id)) : undefined,
    id: id || undefined,
    autoPatch: $<HTMLInputElement>('#pal-auto').checked,
  }).then((ok) => {
    if (ok) $<HTMLInputElement>('#pal-id').value = '';
  });
  if (!keepOpen) setOpen(false);
}

function setOpen(on: boolean): void {
  el().classList.toggle('hidden', !on);
  const add = $('#btn-add');
  add.classList.toggle('on', on);
  add.setAttribute('aria-expanded', String(on));
  if (on) {
    renderChoices();
    renderList();
    $<HTMLInputElement>('#pal-q').focus();
  } else if (el().contains(document.activeElement)) add.focus();
}

export function initPalette(): void {
  document.body.insertAdjacentHTML(
    'beforeend',
    `<div id="palette" class="palette hidden" role="dialog" aria-label="Add a module">
      <div class="pal-top"><input id="pal-q" type="search" placeholder="Search modules" aria-label="Search modules" autocomplete="off"><button class="btn ghost" data-pal-close aria-label="Close" data-tip="Close the palette.">×</button></div>
      <div class="pal-opts">
        <label>Part <select id="pal-part"></select></label>
        <label>Id <input id="pal-id" autocomplete="off" spellcheck="false"></label>
        <label class="chk"><input type="checkbox" id="pal-auto"> Patch it for me</label>
      </div>
      <p class="pal-hint hint"></p>
      <div class="pal-list"></div>
    </div>`,
  );
  const auto = $<HTMLInputElement>('#pal-auto');
  auto.checked = autoPatchPreference();
  auto.addEventListener('change', () => setAutoPatchPreference(auto.checked));
  const hint = $('.pal-hint');
  const paintHint = (): void => {
    const seen = readStored(HINT_KEY) === 'seen';
    hint.classList.toggle('hidden', seen);
    hint.innerHTML = `Click a module to add it, or drag it onto the rack. With <b>Beginner mode</b> on, dragging it over another module shows the cables that would suit; drop to wire them. <button data-pal-hint>Got it</button>`;
  };
  paintHint();
  $('#btn-add').onclick = () => setOpen(!open());
  $('#pal-q').addEventListener('input', renderList);
  $('#pal-q').addEventListener('keydown', (e) => {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    const first = filterModules(app.desc?.modules ?? [], (e.target as HTMLInputElement).value)[0];
    if (first) addFromPalette(first.kind, e.shiftKey);
  });
  el().addEventListener('click', (e) => {
    const t = e.target as HTMLElement;
    if (t.closest('[data-pal-close]')) setOpen(false);
    else if (t.closest('[data-pal-hint]')) {
      writeStored(HINT_KEY, 'seen');
      paintHint();
    }
  });
  el().addEventListener('pointerdown', (e) => {
    const kind = (e.target as HTMLElement).closest<HTMLElement>('.pal-item')?.dataset.kind;
    if (kind) startDrag(e, { kind }, () => addFromPalette(kind, e.shiftKey));
  });
  el().addEventListener('keydown', (e) => {
    if (e.key === 'Escape') setOpen(false);
  });
  subscribe(() => {
    if (open()) renderChoices();
  });
}
