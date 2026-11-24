// The `.kerf` grammar as the editor sees it: one pure pass over a line gives
// spans with a role. Highlighting, hover and the docs' code samples all use it,
// so they agree. It mirrors kerf-core's parser (text/parse.rs) but never fails:
// half-typed lines still get sensible spans.

export type Role =
  | 'comment'
  | 'directive'
  | 'bad-directive'
  | 'id'
  | 'kind'
  | 'param'
  | 'value'
  | 'string'
  | 'flag'
  | 'ref-module'
  | 'ref-jack'
  | 'arrow'
  | 'colour'
  | 'word';

export interface Span {
  from: number;
  to: number;
  role: Role;
  /** The module id: on its own `module` line, or the module half of a `module.jack` reference. */
  module?: string;
  /** The module kind a param belongs to (on a `module` line). */
  kind?: string;
  /** The param or jack key. */
  key?: string;
  /** For jack references: an output on the left of `->`, an input on the right. */
  dir?: 'out' | 'in';
}

export const DIRECTIVES = ['kerf', 'tempo', 'key', 'style', 'tags', 'module', 'part', 'cable', 'scene'] as const;

/** CodeMirror's legacy-mode token names, one per role. */
export const TAG: Record<Role, string> = {
  comment: 'comment',
  directive: 'keyword',
  'bad-directive': 'invalid',
  id: 'variableName',
  kind: 'typeName',
  param: 'propertyName',
  value: 'number',
  string: 'string',
  flag: 'atom',
  'ref-module': 'variableName',
  'ref-jack': 'propertyName',
  arrow: 'operator',
  colour: 'string',
  word: 'name',
};

/** `key="a b"`, a quoted string, or a plain word: the parser's tokenisation. */
export const WORD = /[^\s"]*="[^"]*"?|"[^"]*"?|\S+/g;

export interface Word {
  text: string;
  from: number;
  to: number;
}

export function wordsOf(line: string): Word[] {
  return [...line.matchAll(WORD)].map((m) => ({ text: m[0], from: m.index, to: m.index + m[0].length }));
}

/** `kick.out` as two spans, or one when the dot has not been typed yet. */
function ref(w: Word, dir: 'out' | 'in' | null, jackRole: Role): Span[] {
  const dot = w.text.indexOf('.');
  const module = dot < 0 ? w.text : w.text.slice(0, dot);
  const spans: Span[] = [{ from: w.from, to: w.from + module.length, role: 'ref-module', module }];
  if (dot >= 0 && dot + 1 < w.text.length) {
    spans.push({
      from: w.from + dot + 1,
      to: w.to,
      role: jackRole,
      module,
      key: w.text.slice(dot + 1),
      ...(dir ? { dir } : {}),
    });
  }
  return spans;
}

/** `key=value`: the key as a param, the value as a number or string. */
function pair(w: Word, extra: Partial<Span>): Span[] {
  const eq = w.text.indexOf('=');
  if (eq < 0) return [{ from: w.from, to: w.to, role: 'param', key: w.text, ...extra }];
  const key = w.text.slice(0, eq);
  const spans: Span[] = [{ from: w.from, to: w.from + eq, role: 'param', key, ...extra }];
  if (eq + 1 < w.text.length) {
    spans.push({ from: w.from + eq + 1, to: w.to, role: w.text[eq + 1] === '"' ? 'string' : 'value' });
  }
  return spans;
}

function moduleLine(words: Word[]): Span[] {
  const out: Span[] = [];
  const kind = words[2]?.text;
  words.slice(1).forEach((w, n) => {
    const i = n + 1;
    if (i === 1) out.push({ from: w.from, to: w.to, role: 'id', module: w.text });
    else if (i === 2) out.push({ from: w.from, to: w.to, role: 'kind', kind: w.text, module: words[1]?.text });
    else if (w.text === 'lock' || w.text.startsWith('lock=')) out.push({ from: w.from, to: w.to, role: 'flag' });
    else out.push(...pair(w, { kind, module: words[1]?.text }));
  });
  return out;
}

function partLine(words: Word[]): Span[] {
  return words.slice(1).map((w, n) => {
    const base = { from: w.from, to: w.to };
    if (n === 0) return { ...base, role: 'id' as const };
    if (n === 1) return { ...base, role: 'string' as const };
    if (n === 2) return { ...base, role: 'flag' as const };
    if (n === 3) return { ...base, role: 'colour' as const };
    return { ...base, role: 'flag' as const };
  });
}

function sceneLine(words: Word[]): Span[] {
  const out: Span[] = [];
  words.slice(1).forEach((w, n) => {
    if (n === 0) {
      out.push({ from: w.from, to: w.to, role: 'word' });
      return;
    }
    const eq = w.text.indexOf('=');
    const target = eq < 0 ? w : { text: w.text.slice(0, eq), from: w.from, to: w.from + eq };
    out.push(...ref(target, null, 'param'));
    if (eq >= 0 && eq + 1 < w.text.length) out.push({ from: w.from + eq + 1, to: w.to, role: 'value' });
  });
  return out;
}

export function lexLine(line: string): Span[] {
  const trimmed = line.trimStart();
  if (trimmed.startsWith('#')) return [{ from: line.length - trimmed.length, to: line.length, role: 'comment' }];
  const words = wordsOf(line);
  const head = words[0];
  if (!head) return [];
  const known = (DIRECTIVES as readonly string[]).includes(head.text);
  const out: Span[] = [{ from: head.from, to: head.to, role: known ? 'directive' : 'bad-directive' }];
  switch (head.text) {
    case 'module':
      return out.concat(moduleLine(words));
    case 'part':
      return out.concat(partLine(words));
    case 'cable':
      words.slice(1).forEach((w, n) => {
        if (n === 0) out.push(...ref(w, 'out', 'ref-jack'));
        else if (n === 1) out.push({ from: w.from, to: w.to, role: 'arrow' });
        else out.push(...ref(w, 'in', 'ref-jack'));
      });
      return out;
    case 'scene':
      return out.concat(sceneLine(words));
    case 'style':
      words.slice(1).forEach((w, n) =>
        out.push({ from: w.from, to: w.to, role: n === 0 ? 'id' : n === 1 ? 'flag' : 'value' }),
      );
      return out;
    case 'key':
    case 'tags':
      return out.concat(words.slice(1).map((w) => ({ from: w.from, to: w.to, role: 'word' as const })));
    case 'tempo':
    case 'kerf':
      return out.concat(words.slice(1).map((w) => ({ from: w.from, to: w.to, role: 'value' as const })));
    default:
      return out;
  }
}

const MODULE_LINE = /^\s*module\s+(\S+)\s+([A-Za-z0-9]+)/;

/** Module id → kind, for every `module` line in the text. */
export function scanModules(text: string): Map<string, string> {
  const found = new Map<string, string>();
  for (const line of text.split('\n')) {
    const m = MODULE_LINE.exec(line);
    if (m?.[1] && m[2]) found.set(m[1], m[2]);
  }
  return found;
}

/** Whether `text` is a whole patch (it starts with the `kerf` header) rather than a fragment. */
export const isCompletePatch = (text: string): boolean =>
  text.split('\n').find((l) => l.trim() && !l.trim().startsWith('#'))?.trim().startsWith('kerf ') ?? false;

/** Where inserted text goes: at the cursor, over the whole draft, or at the end of it. */
export type InsertMode = 'cursor' | 'replace' | 'append';

/**
 * The edits that append a fragment (parts and cables) to a draft, separated by a blank line.
 * A draft with no `kerf` header gets `kerf 1` first, because the format needs it. Nothing is applied.
 */
export function appendChanges(draft: string, fragment: string): { from: number; to: number; insert: string }[] {
  const body = `${fragment.trimEnd()}\n`;
  if (!draft.trim()) return [{ from: 0, to: draft.length, insert: `kerf 1\n\n${body}` }];
  const gap = draft.endsWith('\n\n') ? '' : draft.endsWith('\n') ? '\n' : '\n\n';
  const edits = [{ from: draft.length, to: draft.length, insert: gap + body }];
  if (!isCompletePatch(draft)) edits.unshift({ from: 0, to: 0, insert: 'kerf 1\n\n' });
  return edits;
}
