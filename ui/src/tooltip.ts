// The hover tooltip. Anything with `data-tip` gets one; knobs and jacks add
// a live meta line (value, locks, cable count).
import { readout } from './roll';
import { app, patch } from './store';
import { $, esc } from './util';

const tip = (): HTMLElement => $('#tip');

function body(el: HTMLElement): string {
  const kind = el.dataset.tipKind;
  const label = app.desc?.jack_kinds.find((k) => k.id === kind)?.label ?? kind;
  let html = `<b>${esc(el.dataset.tipTitle ?? '')}${kind ? `<span class="kindtag k-${esc(kind)}">${esc(label ?? '')}</span>` : ''}</b>${esc(el.dataset.tip ?? '')}`;
  if (el.hasAttribute('data-tip-knob') && app.state) {
    const modId = el.closest<HTMLElement>('.mod')?.dataset.uid ?? '';
    const key = el.dataset.k ?? '';
    const p = patch().modules.find((m) => m.id === modId)?.params.find((q) => q.key === key);
    if (p) {
      const live = el.dataset.live ?? p.value.toFixed(2);
      const inRack = !modId.startsWith('cat-');
      const reading = readout(el.closest<HTMLElement>('.mod')?.dataset.kind ?? '', key, Number(live));
      html += `<div class="meta">value ${esc(live)}${reading ? ` (${esc(reading)})` : ''}${p.locked ? ' · locked' : ''}${inRack ? ` · drag to change · Alt-click to ${p.locked ? 'unlock' : 'lock'}` : ''}</div>`;
    }
  }
  const ref = el.dataset.ref;
  if (el.classList.contains('jack') && ref && app.state) {
    const n = patch().cables.filter((c) => c.from === ref || c.to === ref).length;
    html += `<div class="meta">${n ? `${n} cable${n > 1 ? 's' : ''} · double-click to unplug` : 'drag to patch'}</div>`;
  }
  return html;
}

function place(e: MouseEvent): void {
  const t = tip();
  const x = Math.min(e.clientX + 14, innerWidth - 280);
  const y = e.clientY + 18 + t.offsetHeight > innerHeight ? e.clientY - t.offsetHeight - 10 : e.clientY + 18;
  t.style.left = `${x}px`;
  t.style.top = `${y}px`;
}

/** Refreshes an open tooltip, for a knob that is changing under the pointer. */
export function refreshTip(el: HTMLElement): void {
  if (!tip().classList.contains('hidden')) tip().innerHTML = body(el);
}

export function initTooltip(): void {
  document.addEventListener('mouseover', (e) => {
    const el = (e.target as HTMLElement).closest<HTMLElement>('[data-tip]');
    if (!el) {
      tip().classList.add('hidden');
      return;
    }
    tip().innerHTML = body(el);
    tip().classList.remove('hidden');
    place(e);
  });
  document.addEventListener('mousemove', (e) => {
    if (!tip().classList.contains('hidden')) place(e);
  });
}
