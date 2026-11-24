// Dragging modules with pointer events (so it works in Tauri's webview too):
// from the palette or the catalogue into the rack, and existing modules to
// reorder them or move them between parts. In Beginner mode, hovering a rack
// module asks `suggest_cables` and shows the answers as dashed ghost cables.
import { addModule } from './add-module';
import { holeCentre } from './bench';
import { ghostLayout, hitModule, insertIndex, type Box } from './ghost';
import { app, send } from './store';
import type { CableSuggestion } from './types';
import { $, $$, esc } from './util';

export interface DragSource {
  kind: string;
  /** Set when an existing module is being moved. */
  module?: string;
}

const THRESHOLD = 5;
const SUGGEST_DELAY_MS = 120;
const NS = 'http://www.w3.org/2000/svg';

const boxOf = (id: string, el: Element): Box => {
  const r = el.getBoundingClientRect();
  return { id, left: r.left, top: r.top, right: r.right, bottom: r.bottom };
};

const rackBoxes = (skip?: string): Box[] =>
  $$('#bench .mod[data-uid]')
    .filter((m) => m.dataset.uid !== skip)
    .map((m) => boxOf(m.dataset.uid ?? '', m));

const rowAt = (x: number, y: number): HTMLElement | null =>
  document.elementsFromPoint(x, y).find((e): e is HTMLElement => e instanceof HTMLElement && e.matches('#bench .part-row')) ?? null;

interface Hover {
  target: string | null;
  row: HTMLElement | null;
}

function clearMarks(): void {
  $$('.drop-target, .nomatch, .match, .drop-row, .drop-hot').forEach((e) => e.classList.remove('drop-target', 'nomatch', 'match', 'drop-row', 'drop-hot'));
  document.querySelector('#cables .ghost-layer')?.remove();
}

function drawGhosts(cables: CableSuggestion[], x: number, y: number): void {
  const svg = document.querySelector('#cables');
  const b = document.querySelector('#bench')?.getBoundingClientRect();
  if (!svg || !b) return;
  svg.querySelector('.ghost-layer')?.remove();
  const ghosts = ghostLayout(cables, [x - b.left, y - b.top], holeCentre);
  const layer = document.createElementNS(NS, 'g');
  layer.setAttribute('class', 'ghost-layer');
  layer.innerHTML = ghosts
    .map((g) => `<path class="ghost" d="${g.d}"/><text class="ghost-tag" x="${g.mid[0]}" y="${g.mid[1] - 6}" text-anchor="middle">${esc(g.label)}</text>`)
    .join('');
  svg.appendChild(layer);
  $$('.jack.match').forEach((j) => j.classList.remove('match'));
  ghosts.forEach((g) => document.querySelector(`.jack[data-ref="${CSS.escape(g.ref)}"]`)?.classList.add('match'));
}

/**
 * Starts a drag from `e`. A release before the pointer moved far is a click and
 * calls `onClick`; otherwise the module is dropped where the pointer is.
 */
export function startDrag(e: PointerEvent, src: DragSource, onClick?: () => void): void {
  const sx = e.clientX;
  const sy = e.clientY;
  let chip: HTMLElement | null = null;
  let hover: Hover = { target: null, row: null };
  let cables: CableSuggestion[] = [];
  let token = 0;
  let timer = 0;

  const suggest = (near: string): void => {
    window.clearTimeout(timer);
    const mine = ++token;
    timer = window.setTimeout(() => {
      app.api
        .suggestCables(src.module ? { module: src.module, near } : { kind: src.kind, near })
        .then((r) => {
          if (mine !== token || hover.target !== near) return;
          cables = r.cables;
          if (cables.length === 0) document.querySelector(`.mod[data-uid="${CSS.escape(near)}"]`)?.classList.add('nomatch');
          drawGhosts(cables, lastX, lastY);
        })
        .catch(() => undefined);
    }, SUGGEST_DELAY_MS);
  };

  let lastX = sx;
  let lastY = sy;
  const onMove = (ev: PointerEvent): void => {
    lastX = ev.clientX;
    lastY = ev.clientY;
    if (!chip) {
      if (Math.hypot(lastX - sx, lastY - sy) < THRESHOLD) return;
      chip = document.createElement('div');
      chip.className = 'drag-chip';
      chip.textContent = src.module ?? src.kind;
      document.body.appendChild(chip);
      document.body.classList.add('dragging-module');
    }
    chip.style.transform = `translate(${lastX + 14}px, ${lastY + 10}px)`;
    const benchShown = app.view === 'bench';
    const target = benchShown ? hitModule(lastX, lastY, rackBoxes(src.module)) : null;
    const row = benchShown ? rowAt(lastX, lastY) : null;
    if (target !== hover.target || row !== hover.row) {
      clearMarks();
      cables = [];
      token++;
      hover = { target, row };
      if (target) document.querySelector(`.mod[data-uid="${CSS.escape(target)}"]`)?.classList.add('drop-target');
      else row?.classList.add('drop-row');
      if (target && app.beginner && !src.module) suggest(target);
    } else if (cables.length) {
      drawGhosts(cables, lastX, lastY);
    }
    if (!benchShown) {
      document.querySelector('nav [data-view=bench]')?.classList.toggle('drop-hot', document.elementsFromPoint(lastX, lastY).some((n) => n.matches('nav [data-view=bench]')));
    }
  };

  const finish = (): void => {
    window.clearTimeout(timer);
    token++;
    document.removeEventListener('pointermove', onMove);
    document.removeEventListener('pointerup', onUp);
    document.removeEventListener('keydown', onKey);
    clearMarks();
    chip?.remove();
    document.body.classList.remove('dragging-module');
  };

  const onKey = (ev: KeyboardEvent): void => {
    if (ev.key === 'Escape') finish();
  };

  const onUp = (ev: PointerEvent): void => {
    const wasDrag = chip !== null;
    const { target, row } = hover;
    const overBenchTab = document.elementsFromPoint(ev.clientX, ev.clientY).some((n) => n.matches('nav [data-view=bench]'));
    finish();
    if (!wasDrag) onClick?.();
    else if (app.view !== 'bench') {
      if (overBenchTab && !src.module) {
        $('nav [data-view=bench]').click();
        void addModule(src.kind, { autoPatch: app.beginner });
      }
    } else void drop(src, target, row, ev.clientX);
  };

  document.addEventListener('pointermove', onMove);
  document.addEventListener('pointerup', onUp);
  document.addEventListener('keydown', onKey);
  e.preventDefault();
}

async function drop(src: DragSource, target: string | null, row: HTMLElement | null, x: number): Promise<void> {
  const home = row ?? (target ? document.querySelector<HTMLElement>(`.mod[data-uid="${CSS.escape(target)}"]`)?.closest<HTMLElement>('.part-row') ?? null : null);
  const part = home?.dataset.partRow || undefined;
  if (src.module) {
    if (!home) return;
    const siblings = $$('.mod[data-uid]', home)
      .filter((m) => m.dataset.uid !== src.module)
      .map((m) => boxOf(m.dataset.uid ?? '', m));
    await send({ op: 'move_module', module: src.module, part: part ?? null, index: insertIndex(x, siblings) });
    return;
  }
  const wire = app.beginner;
  await addModule(src.kind, { part, near: wire && target ? target : undefined, autoPatch: wire });
}
