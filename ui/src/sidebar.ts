// The right-hand sidebar: the lineage tree and the Explain text.
import type { ModuleExplanation } from './explain';
import { ICON } from './icons';
import { app, changed, send } from './store';
import type { LineageNode } from './types';
import { $, esc } from './util';

const HINT = '<p class="hint">Turn on Explain and point at a module or a part.</p>';
const FALLBACK_COLOUR = '#6b645a';

function label(n: LineageNode): { name: string; detail: string } {
  const o = n.origin;
  switch (o.kind) {
    case 'root':
      return { name: 'start', detail: '' };
    case 'edited':
      return { name: 'edited', detail: '' };
    case 'imported':
      return { name: 'imported', detail: '' };
    case 'random':
      return { name: `random ${o.style}`, detail: `seed ${o.seed}` };
    case 'evolve':
      return { name: `evolve ${o.strength.toFixed(2)}`, detail: `seed ${o.seed}` };
    case 'breed':
      return { name: `breed ${n.parents.map((p) => `#${p}`).join('×')}`, detail: `seed ${o.seed}` };
  }
}

/** The first part's colour, read from the node's `.kerf` text. */
const dotColour = (n: LineageNode): string => /^part .* colour (#[0-9a-fA-F]{6})/m.exec(n.patch)?.[1] ?? FALLBACK_COLOUR;

export function renderLineage(): void {
  const lineage = app.state?.lineage;
  if (!lineage) return;
  const kids = new Map<number, LineageNode[]>();
  for (const n of lineage.nodes) {
    const parent = n.parents[0];
    if (parent !== undefined) kids.set(parent, [...(kids.get(parent) ?? []), n]);
  }
  const rows: string[] = [];
  const walk = (parent: number, depth: number, guides: boolean[]): void => {
    const list = kids.get(parent) ?? [];
    list.forEach((n, i) => {
      const last = i === list.length - 1;
      const tree =
        depth > 0
          ? guides.map((g) => `<i class="g${g ? ' v' : ''}"></i>`).join('') + `<i class="g ${last ? 'l' : 't'}"></i>`
          : '';
      rows.push(nodeRow(n, tree));
      walk(n.id, depth + 1, depth > 0 ? [...guides, !last] : guides);
    });
  };
  const root = lineage.nodes[0];
  if (root) {
    rows.push(nodeRow(root, ''));
    walk(root.id, 1, []);
  }
  $('#lineage').innerHTML = rows.join('');
}

function nodeRow(n: LineageNode, tree: string): string {
  const cur = n.id === app.state?.lineage.current;
  const isMate = n.id === app.mate;
  const { name, detail } = label(n);
  return `<div class="node${cur ? ' cur' : ''}${isMate ? ' mate' : ''}" data-node="${n.id}">
    <span class="tree">${tree}</span><span class="dot" style="background:${esc(dotColour(n))}"></span>
    <span class="nlabel"><b>#${n.id}</b> ${esc(name)}</span><small>${esc(detail)}</small>
    <button class="mate-btn${isMate ? ' on' : ''}" aria-label="Pick as mate" aria-pressed="${isMate}" data-mate="${n.id}" data-tip-title="Pick as mate" data-tip="Breed crosses the current patch with this one.">${isMate ? ICON.heartOn : ICON.heart}</button></div>`;
}

export interface ExplainView {
  module?: ModuleExplanation | null;
  part?: { name: string; lines: string[] };
}

export function sidebarExplain(view: ExplainView | null): void {
  const el = $('#explain-text');
  const m = view?.module;
  if (m) {
    const list = (items: { label: string; tip: string }[]): string =>
      items.map((i) => `<li><b>${esc(i.label)}</b> ${esc(i.tip)}</li>`).join('');
    el.innerHTML = `<p class="lead">${esc(m.title)}</p><p>${esc(m.summary)}</p>
      ${m.flow.map((f) => `<p>${esc(f)}</p>`).join('')}
      <h4>Knobs</h4><ul>${list(m.controls)}</ul><h4>Jacks</h4><ul>${list(m.jacks)}</ul>`;
  } else if (view?.part) {
    el.innerHTML = `<p class="lead">${esc(view.part.name)}</p>${view.part.lines.map((l) => `<p>${esc(l)}</p>`).join('')}`;
  } else {
    el.innerHTML = HINT;
  }
}

export function initSidebar(): void {
  $('#lineage').addEventListener('click', (e) => {
    const target = e.target as HTMLElement;
    const heart = target.closest<HTMLElement>('[data-mate]');
    if (heart) {
      const id = Number(heart.dataset.mate);
      app.mate = app.mate === id ? null : id;
      changed();
      return;
    }
    const node = target.closest<HTMLElement>('[data-node]');
    if (node) void send({ op: 'checkout', node: Number(node.dataset.node) });
  });
  sidebarExplain(null);
}
