// What the editor suggests and explains, as pure functions of the text before
// the cursor, the modules named in the document and the backend's `completions`.
import { DIRECTIVES, lexLine, scanModules, wordsOf, type Span } from './kerf-syntax';
import type { Completions, KindDoc, KindJackDoc } from './types';
import { takesSample } from './util';

export interface Option {
  label: string;
  detail?: string;
  info?: string;
  type: 'keyword' | 'class' | 'property' | 'variable' | 'constant' | 'text';
  /** A CodeMirror snippet template; applied instead of `label` when present. */
  snippet?: string;
  boost?: number;
  /** A name the docs know (a module kind), for the "open docs" link in the completion detail. */
  term?: string;
}

export interface Suggestions {
  /** Offset in the line where the replaced word starts. */
  from: number;
  options: Option[];
}

const kindIndex = new WeakMap<Completions, Map<string, KindDoc>>();

export function kindOf(c: Completions, kind: string): KindDoc | undefined {
  let index = kindIndex.get(c);
  if (!index) {
    index = new Map(c.kinds.map((k) => [k.kind, k]));
    kindIndex.set(c, index);
  }
  return index.get(kind);
}

const num = (n: number): string => String(Number(n.toFixed(3)));

function jackOptions(modules: Map<string, string>, c: Completions, dir: 'out' | 'in', wantKind?: string): Option[] {
  const out: Option[] = [];
  for (const [id, kind] of modules) {
    const doc = kindOf(c, kind);
    for (const j of (dir === 'out' ? doc?.outputs : doc?.inputs) ?? []) {
      out.push({
        label: `${id}.${j.key}`,
        detail: `${j.kind} · ${kind}`,
        info: j.help,
        type: 'property',
        term: kind,
        boost: wantKind && j.kind === wantKind ? 2 : 0,
      });
    }
  }
  return out;
}

function directiveOptions(c: Completions, indented: boolean): Option[] {
  const allowed: readonly string[] = indented ? ['module'] : DIRECTIVES;
  return c.keywords
    .filter((k) => allowed.includes(k.word))
    .map((k) => ({
      label: k.word,
      detail: 'directive',
      info: k.doc,
      type: 'keyword' as const,
      ...(k.snippet ? { snippet: k.snippet } : {}),
    }));
}

function moduleOptions(words: string[], cur: string, curFrom: number, c: Completions): Suggestions | null {
  const idx = words.length;
  if (idx === 1) return null;
  if (idx === 2) {
    return {
      from: curFrom,
      options: c.kinds.map((k) => ({ label: k.kind, detail: k.band, info: k.summary, type: 'class' as const, term: k.kind })),
    };
  }
  const kind = kindOf(c, words[2] ?? '');
  const eq = cur.indexOf('=');
  const key = cur.slice(0, eq);
  if (eq >= 0) {
    const param = kind?.params.find((p) => p.key === key);
    if (!param) return null;
    return {
      from: curFrom + eq + 1,
      options: [{ label: num(param.default), detail: 'default', info: param.help, type: 'constant' }],
    };
  }
  const used = new Set(words.slice(3).map((w) => w.split('=')[0]));
  const options: Option[] = (kind?.params ?? [])
    .filter((p) => !used.has(p.key))
    .map((p) => ({
      label: `${p.key}=`,
      detail: `${p.label} · default ${num(p.default)}`,
      info: p.help,
      type: 'property' as const,
      term: kind?.kind,
    }));
  if (!used.has('lock')) {
    options.push({ label: 'lock', detail: 'keep this module through Evolve and Breed', type: 'keyword' });
  }
  if (kind && takesSample(kind.kind) && !used.has('sample')) {
    options.push({ label: 'sample=', detail: 'audio file', type: 'property', snippet: 'sample="${1:path/to/file.wav}"' });
  }
  return { from: curFrom, options };
}

function cableOptions(words: string[], curFrom: number, c: Completions, modules: Map<string, string>): Suggestions | null {
  const idx = words.length;
  if (idx === 1) return { from: curFrom, options: jackOptions(modules, c, 'out') };
  if (idx === 2) return { from: curFrom, options: [{ label: '->', detail: 'to', type: 'keyword' }] };
  if (idx === 3) {
    const [fromModule, fromJack] = (words[1] ?? '').split('.');
    const source = kindOf(c, modules.get(fromModule ?? '') ?? '')?.outputs.find((j) => j.key === fromJack);
    return { from: curFrom, options: jackOptions(modules, c, 'in', source?.kind) };
  }
  return null;
}

function sceneOptions(words: string[], curFrom: number, c: Completions, modules: Map<string, string>): Suggestions | null {
  if (words.length === 1) {
    return { from: curFrom, options: ['A', 'B', 'C', 'D'].map((s) => ({ label: s, detail: 'slot', type: 'constant' as const })) };
  }
  const options: Option[] = [];
  for (const [id, kind] of modules) {
    for (const p of kindOf(c, kind)?.params ?? []) {
      options.push({ label: `${id}.${p.key}=`, detail: `${p.label} · ${kind}`, info: p.help, type: 'property' });
    }
  }
  return { from: curFrom, options };
}

/** Suggestions for the word being typed at the end of `before`, or null when none apply. */
export function suggest(before: string, text: string, c: Completions): Suggestions | null {
  if ((before.match(/"/g)?.length ?? 0) % 2 === 1) return null;
  if (before.trimStart().startsWith('#')) return null;
  const all = wordsOf(before);
  const typing = before.length > 0 && !/\s$/.test(before);
  const last = typing ? all.pop() : undefined;
  const cur = last?.text ?? '';
  const curFrom = last?.from ?? before.length;
  const words = all.map((w) => w.text);
  const modules = scanModules(text);
  const idx = words.length;
  if (idx === 0) return { from: curFrom, options: directiveOptions(c, /^\s/.test(before)) };
  switch (words[0]) {
    case 'module':
      return moduleOptions(words, cur, curFrom, c);
    case 'cable':
      return cableOptions(words, curFrom, c, modules);
    case 'scene':
      return sceneOptions(words, curFrom, c, modules);
    case 'style':
      if (idx === 1) {
        return { from: curFrom, options: c.styles.map((s) => ({ label: s.name, detail: s.title, info: s.summary, type: 'constant' as const })) };
      }
      return idx === 2 ? { from: curFrom, options: [{ label: 'seed', detail: 'the generator seed', type: 'keyword' }] } : null;
    case 'key':
      if (idx === 1) return { from: curFrom, options: c.keys.map((k) => ({ label: k, detail: 'tonic', type: 'constant' as const })) };
      return idx === 2 ? { from: curFrom, options: c.scales.map((k) => ({ label: k, detail: 'scale', type: 'constant' as const })) } : null;
    case 'part':
      if (idx === 3) return { from: curFrom, options: [{ label: 'colour', detail: '#rrggbb', type: 'keyword' }] };
      return idx >= 5
        ? { from: curFrom, options: ['lock', 'mute'].filter((f) => !words.includes(f)).map((f) => ({ label: f, type: 'keyword' as const })) }
        : null;
    default:
      return null;
  }
}

export interface Hover {
  from: number;
  to: number;
  title: string;
  body: string;
  /** A name the docs know (a module kind or a directive), for the "open docs" link. */
  term?: string;
}

function jackHover(s: Span, c: Completions, modules: Map<string, string>): Hover | null {
  const kind = kindOf(c, modules.get(s.module ?? '') ?? '');
  const find = (list: KindJackDoc[] | undefined): KindJackDoc | undefined => list?.find((j) => j.key === s.key);
  const jack = s.dir === 'in' ? find(kind?.inputs) : s.dir === 'out' ? find(kind?.outputs) : undefined;
  if (!jack || !kind) return null;
  return {
    from: s.from,
    to: s.to,
    title: `${s.module}.${jack.key}: ${jack.kind} ${s.dir === 'in' ? 'input' : 'output'}`,
    body: jack.help,
    term: kind.kind,
  };
}

/** The explanation for the word at column `ch` (0-based) of `line`, or null. */
export function hoverAt(line: string, ch: number, text: string, c: Completions): Hover | null {
  const s = lexLine(line).find((x) => ch >= x.from && ch <= x.to && x.to > x.from);
  if (!s) return null;
  const modules = scanModules(text);
  const at = { from: s.from, to: s.to };
  switch (s.role) {
    case 'directive': {
      const word = line.slice(s.from, s.to);
      const k = c.keywords.find((x) => x.word === word);
      return k ? { ...at, title: k.word, body: k.doc, term: k.word } : null;
    }
    case 'kind':
    case 'id':
    case 'ref-module': {
      const kind = kindOf(c, s.role === 'kind' ? (s.kind ?? '') : (modules.get(s.module ?? '') ?? ''));
      if (!kind) return null;
      const head = s.role === 'kind' ? kind.kind : `${s.module}: ${kind.kind}`;
      return {
        ...at,
        title: head,
        body: `${kind.summary}\n${kind.params.length} params, ${kind.inputs.length} inputs, ${kind.outputs.length} outputs.`,
        term: kind.kind,
      };
    }
    case 'param': {
      const kind = kindOf(c, s.kind ?? modules.get(s.module ?? '') ?? '');
      const param = kind?.params.find((p) => p.key === s.key);
      if (!kind || !param) return null;
      return { ...at, title: `${param.label} (${param.key}), default ${num(param.default)}`, body: param.help, term: kind.kind };
    }
    case 'ref-jack':
      return jackHover(s, c, modules);
    default:
      return null;
  }
}
