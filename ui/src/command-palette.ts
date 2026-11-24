// The editor's command list: statements, modules and snippets that insert text
// into the draft. The module list is the completion data the editor already
// has; the snippets are read out of the docs, so there is no second copy.
import { scanModules, type InsertMode } from './kerf-syntax';
import type { Completions, DocSection } from './types';

export interface Command {
  group: string;
  label: string;
  detail: string;
  text: string;
  mode: InsertMode;
  /** Open the editor's completion list after inserting (for a half-written line). */
  complete?: boolean;
}

export interface SnippetEntry {
  category: string;
  title: string;
  about: string;
  code: string;
}

const NEW_PATCH = `kerf 1
tempo 120
key A minor

part master "Master" colour #8a8f98
  module clock Tally
  module mix Router
  module out Finish

cable mix.mix_l -> out.l
cable mix.mix_r -> out.r
`;

/** `base`, or `base2`, `base3`, ... : the first id not in `taken`. The id is added to `taken`. */
export function freshId(base: string, taken: Set<string>): string {
  let id = base;
  for (let n = 2; taken.has(id); n++) id = `${base}${n}`;
  taken.add(id);
  return id;
}

/** Every module and part id already written in the draft. */
export function idsIn(draft: string): Set<string> {
  const ids = new Set(scanModules(draft).keys());
  for (const m of draft.matchAll(/^part\s+([a-z][a-z0-9_]*)/gm)) if (m[1]) ids.add(m[1]);
  return ids;
}

/** The `### Title` + about + ```kerf snippet blocks of the `Snippets: <Category>` doc sections. */
export function snippetsFromDocs(sections: DocSection[]): SnippetEntry[] {
  const found: SnippetEntry[] = [];
  for (const s of sections) {
    if (!s.id.startsWith('snippets-')) continue;
    const category = s.title.replace(/^Snippets:\s*/, '');
    for (const part of s.markdown.split(/^### /m).slice(1)) {
      const title = part.split('\n', 1)[0]?.trim() ?? '';
      const code = /```kerf snippet\n([\s\S]*?)```/.exec(part)?.[1];
      const about = part.slice(title.length, code === undefined ? undefined : part.indexOf('```kerf snippet')).trim();
      if (title && code !== undefined) found.push({ category, title, about, code });
    }
  }
  return found;
}

/** Every command, in menu order. Module ids are made unique against the draft and each other. */
export function buildCommands(c: Completions | null, snippets: SnippetEntry[], draft: string): Command[] {
  const taken = idsIn(draft);
  const cmds: Command[] = [
    { group: 'Patch', label: 'New patch', detail: 'Replace the draft with a clock, mixer and master output', text: NEW_PATCH, mode: 'replace' },
    { group: 'Statement', label: 'tempo', detail: 'tempo 120', text: 'tempo 120', mode: 'cursor' },
    { group: 'Statement', label: 'key', detail: 'key A minor', text: 'key A minor', mode: 'cursor' },
    {
      group: 'Statement',
      label: 'part',
      detail: 'Group modules under a name and colour',
      text: `part ${freshId('part', taken)} "Part" colour #8a8f98\n  `,
      mode: 'cursor',
    },
    { group: 'Statement', label: 'connect', detail: 'cable module.jack -> module.jack', text: 'cable ', mode: 'cursor', complete: true },
  ];
  for (const k of c?.kinds ?? []) {
    const params = k.params.map((p) => ` ${p.key}=${p.default}`).join('');
    const id = freshId(k.kind.toLowerCase(), taken);
    cmds.push({ group: 'Module', label: `add module ${k.kind}`, detail: k.summary, text: `module ${id} ${k.kind}${params}`, mode: 'cursor' });
  }
  const have = idsIn(draft);
  const start = snippets.find((s) => s.category === 'Start');
  for (const s of snippets) {
    const text = s === start || !start ? s.code : withMissingModules(s.code, start.code, have);
    cmds.push({ group: s.category, label: s.title, detail: s.about, text, mode: 'append' });
  }
  return cmds;
}

/** `code` preceded by the modules of `start` it cables to that the draft lacks, so a snippet works in an empty draft. */
function withMissingModules(code: string, start: string, have: Set<string>): string {
  const lines = start.split('\n');
  const needed = lines.filter((l) => {
    const id = /^\s+module\s+([a-z][a-z0-9_]*)/.exec(l)?.[1];
    return id !== undefined && !have.has(id) && new RegExp(`\\b${id}\\.`).test(code);
  });
  if (needed.length === 0) return code;
  const part = lines.find((l) => l.startsWith('part ')) ?? '';
  return `${part}\n${needed.join('\n')}\n\n${code}`;
}

/** Keeps commands that contain every word of `query`; label matches first, then earlier groups, in menu order. */
export function filterCommands(cmds: Command[], query: string): Command[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return cmds;
  const scored: { cmd: Command; rank: number; at: number }[] = [];
  cmds.forEach((cmd, at) => {
    const label = cmd.label.toLowerCase();
    const all = `${cmd.group} ${cmd.label} ${cmd.detail}`.toLowerCase();
    if (!words.every((w) => all.includes(w))) return;
    const rank = label.startsWith(words[0] ?? '') ? 0 : words.every((w) => label.includes(w)) ? 1 : 2;
    scored.push({ cmd, rank, at });
  });
  return scored.sort((a, b) => a.rank - b.rank || a.at - b.at).map((s) => s.cmd);
}
