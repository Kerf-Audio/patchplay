import { describe, expect, it } from 'vitest';
import { buildCommands, filterCommands, freshId, snippetsFromDocs } from './command-palette';
import { appendChanges } from './kerf-syntax';
import { renderMarkdown } from './markdown';
import type { Completions, DocSection } from './types';

const completions: Completions = {
  keywords: [],
  kinds: [
    { kind: 'Mortise', band: 'voice', summary: 'A drum voice', params: [{ key: 'decay', label: 'Decay', default: 0.3, help: '' }, { key: 'tone', label: 'Tone', default: 0.5, help: '' }], inputs: [], outputs: [] },
    { kind: 'Plane', band: 'tone', summary: 'A filter', params: [], inputs: [], outputs: [] },
  ],
  styles: [],
  keys: [],
  scales: [],
} as unknown as Completions;

const docs: DocSection[] = [
  { id: 'file-anatomy', title: 'File anatomy', markdown: '### Not a snippet\n\n```kerf snippet\nx\n```' },
  {
    id: 'snippets-drums',
    title: 'Snippets: Drums',
    markdown: 'Blurb.\n\n### Kick\n\nA kick.\n\n```kerf snippet\n# Kick\n\npart kick "Kick"\n  module kick_drum Mortise\n```\n\n### Hats\n\nHats.\n\n```kerf snippet\npart hats "Hats"\n```',
  },
];

describe('snippetsFromDocs', () => {
  it('reads titles, descriptions and code from the snippet sections only', () => {
    const found = snippetsFromDocs(docs);
    expect(found.map((s) => [s.category, s.title, s.about])).toEqual([
      ['Drums', 'Kick', 'A kick.'],
      ['Drums', 'Hats', 'Hats.'],
    ]);
    expect(found[0]?.code).toContain('module kick_drum Mortise');
  });
});

describe('buildCommands', () => {
  const draft = 'kerf 1\n\npart part "x"\n  module mortise Mortise\n';
  const cmds = buildCommands(completions, snippetsFromDocs(docs), draft);

  it('lists the new-patch skeleton, statements, modules, then snippets', () => {
    expect(cmds.map((c) => c.group)).toEqual(['Patch', 'Statement', 'Statement', 'Statement', 'Statement', 'Module', 'Module', 'Drums', 'Drums']);
    expect(cmds[0]?.mode).toBe('replace');
    expect(cmds[0]?.text).toMatch(/Tally[\s\S]*Router[\s\S]*Finish/);
  });

  it('gives each module a fresh id and its default params', () => {
    const mortise = cmds.find((c) => c.label === 'add module Mortise');
    expect(mortise?.text).toBe('module mortise2 Mortise decay=0.3 tone=0.5');
    expect(cmds.find((c) => c.label === 'add module Plane')?.text).toBe('module plane Plane');
  });

  it('never repeats an id, and avoids part ids too', () => {
    const ids = cmds.filter((c) => c.group === 'Module').map((c) => c.text.split(' ')[1]);
    expect(new Set(ids).size).toBe(ids.length);
    expect(cmds.find((c) => c.label === 'part')?.text).toMatch(/^part part2 /);
  });

  it('appends snippets and inserts statements at the cursor', () => {
    expect(cmds.filter((c) => c.group === 'Drums').every((c) => c.mode === 'append')).toBe(true);
    expect(cmds.find((c) => c.label === 'tempo')?.mode).toBe('cursor');
    expect(cmds.find((c) => c.label === 'connect')?.complete).toBe(true);
  });
});

describe('snippets in a draft without a clock or output', () => {
  const snippets = [
    { category: 'Start', title: 'Clock and output', about: '', code: 'part master "Master"\n  module clock Tally\n  module out Finish\n' },
    { category: 'Drums', title: 'Kick', about: '', code: 'part kick "Kick"\n  module k Mortise\ncable clock.clock -> k.clock\ncable k.out -> out.l\n' },
  ];
  const kick = (draft: string): string => buildCommands(null, snippets, draft).find((c) => c.label === 'Kick')?.text ?? '';
  it('brings the clock and output it cables to', () => {
    expect(kick('kerf 1\n')).toMatch(/^part master "Master"\n  module clock Tally\n  module out Finish\n\npart kick/);
  });
  it('brings only what is missing', () => {
    expect(kick('kerf 1\npart m "M"\n  module clock Tally\n')).toMatch(/^part master "Master"\n  module out Finish\n\npart kick/);
    expect(kick('kerf 1\npart m "M"\n  module clock Tally\n  module out Finish\n')).toMatch(/^part kick/);
  });
});

describe('freshId', () => {
  it('counts up from the base', () => {
    const taken = new Set(['osc', 'osc2']);
    expect(freshId('osc', taken)).toBe('osc3');
    expect(freshId('osc', taken)).toBe('osc4');
  });
});

describe('filterCommands', () => {
  const cmds = buildCommands(completions, snippetsFromDocs(docs), '');
  it('returns everything for an empty query, in menu order', () => {
    expect(filterCommands(cmds, '  ')).toEqual(cmds);
  });
  it('needs every word to match', () => {
    expect(filterCommands(cmds, 'add plane').map((c) => c.label)).toEqual(['add module Plane']);
    expect(filterCommands(cmds, 'zzz')).toEqual([]);
  });
  it('puts label-prefix matches before detail matches', () => {
    const labels = filterCommands(cmds, 'kick').map((c) => c.label);
    expect(labels[0]).toBe('Kick');
  });
  it('keeps menu order within one rank', () => {
    expect(filterCommands(cmds, 'drums').map((c) => c.label)).toEqual(['Kick', 'Hats']);
  });
});

describe('appendChanges', () => {
  const apply = (draft: string, fragment: string): string => {
    let out = draft;
    for (const c of [...appendChanges(draft, fragment)].sort((a, b) => b.from - a.from)) out = out.slice(0, c.from) + c.insert + out.slice(c.to);
    return out;
  };
  it('appends after a blank line', () => {
    expect(apply('kerf 1\ntempo 120', 'part a "A"\n')).toBe('kerf 1\ntempo 120\n\npart a "A"\n');
    expect(apply('kerf 1\n', 'part a "A"')).toBe('kerf 1\n\npart a "A"\n');
    expect(apply('kerf 1\n\n', 'part a "A"')).toBe('kerf 1\n\npart a "A"\n');
  });
  it('starts a blank draft with the kerf header', () => {
    expect(apply('', 'part a "A"')).toBe('kerf 1\n\npart a "A"\n');
    expect(apply('  \n', 'part a "A"')).toBe('kerf 1\n\npart a "A"\n');
  });
  it('adds the header to a draft that has none', () => {
    expect(apply('# notes\n', 'part a "A"')).toBe('kerf 1\n\n# notes\n\npart a "A"\n');
  });
});

describe('snippet code blocks', () => {
  it('get an Add to patch button that appends', () => {
    const html = renderMarkdown('```kerf snippet\npart a "A"\n```');
    expect(html).toContain('data-code-insert="append"');
    expect(html).toContain('Add to patch');
    expect(html).not.toContain('Apply as patch');
  });
});
