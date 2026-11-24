// Explain mode: what feeds a module and where it goes, worked out from the
// cables alone, plus the plain-English text for the sidebar.
import { splitRef } from './cables';
import type { Cable, ModuleDesc, Patch } from './types';
import { baseName } from './util';

export interface Trace {
  upstream: Set<string>;
  downstream: Set<string>;
}

function reach(edges: Map<string, string[]>, start: string): Set<string> {
  const seen = new Set<string>();
  const todo = [start];
  for (let id = todo.pop(); id !== undefined; id = todo.pop()) {
    for (const next of edges.get(id) ?? []) {
      if (next !== start && !seen.has(next)) {
        seen.add(next);
        todo.push(next);
      }
    }
  }
  return seen;
}

/** Every module that feeds `id`, and every module it feeds, through any number of cables. */
export function trace(cables: Cable[], id: string): Trace {
  const forward = new Map<string, string[]>();
  const backward = new Map<string, string[]>();
  for (const c of cables) {
    const [from] = splitRef(c.from);
    const [to] = splitRef(c.to);
    forward.set(from, [...(forward.get(from) ?? []), to]);
    backward.set(to, [...(backward.get(to) ?? []), from]);
  }
  return { upstream: reach(backward, id), downstream: reach(forward, id) };
}

export interface ModuleExplanation {
  title: string;
  summary: string;
  flow: string[];
  controls: { label: string; tip: string }[];
  jacks: { label: string; tip: string }[];
}

const label = (descs: Map<string, ModuleDesc>, patch: Patch, ref: string): string => {
  const [id, jack] = splitRef(ref);
  const kind = patch.modules.find((m) => m.id === id)?.kind ?? '';
  const d = descs.get(kind);
  const j = [...(d?.inputs ?? []), ...(d?.outputs ?? [])].find((x) => x.key === jack);
  return `${kind} ${id}'s ${j?.label ?? jack}`;
};

export function explainModule(
  descs: Map<string, ModuleDesc>,
  patch: Patch,
  id: string,
): ModuleExplanation | null {
  const mod = patch.modules.find((m) => m.id === id);
  const d = mod && descs.get(mod.kind);
  if (!mod || !d) return null;
  const flow: string[] = [];
  for (const c of patch.cables) {
    if (splitRef(c.to)[0] === id) {
      flow.push(`${label(descs, patch, c.to)} listens to ${label(descs, patch, c.from)}.`);
    }
  }
  for (const c of patch.cables) {
    if (splitRef(c.from)[0] === id) {
      flow.push(`${label(descs, patch, c.from)} goes to ${label(descs, patch, c.to)}.`);
    }
  }
  if (flow.length === 0) flow.push('Nothing is patched to or from it.');
  if (mod.kind === 'Veneer') {
    flow.push(
      mod.sample
        ? `Plays ${baseName(mod.sample)}, cut into 16 equal slices; the shaded slice is the one the Slice knob picks.`
        : 'Plays the built-in demo sample. Drop an audio file on the panel, or press Choose, to play your own, cut into 16 slices.',
    );
  } else if (mod.kind === 'Splinter') {
    flow.push(
      mod.sample
        ? `Grains are read from ${baseName(mod.sample)}; the line is the Position knob and the shaded stretch is the Spray around it.`
        : 'Grains are read from the built-in demo sample. Drop an audio file on the panel, or press Choose, to use your own.',
    );
  }
  return {
    title: `${d.kind} ${id}`,
    summary: d.summary,
    flow,
    controls: d.params.map((p) => ({ label: p.label, tip: p.tooltip })),
    jacks: [...d.inputs, ...d.outputs].map((j) => ({ label: j.label, tip: j.tooltip })),
  };
}

/** One line per module of a part, for pointing at the part's label. */
export function explainPart(
  descs: Map<string, ModuleDesc>,
  patch: Patch,
  partId: string | null,
): string[] {
  return patch.modules
    .filter((m) => m.part === partId)
    .map((m) => `${m.kind} ${m.id}: ${descs.get(m.kind)?.summary ?? ''}`);
}
