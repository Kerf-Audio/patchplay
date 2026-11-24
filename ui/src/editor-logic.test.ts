import { describe, expect, it } from 'vitest';
import { diagnosticSpan } from './diagnostics';
import { hoverAt, suggest } from './kerf-hints';
import { isCompletePatch, lexLine, scanModules } from './kerf-syntax';
import type { Completions } from './types';

const completions: Completions = {
  keywords: [
    { word: 'module', doc: 'Declare a module.' },
    { word: 'cable', doc: 'Connect an output to an input.' },
    { word: 'tempo', doc: 'Beats per minute.' },
  ],
  kinds: [
    {
      kind: 'Mortise',
      band: 'voices',
      summary: 'A kick drum.',
      params: [{ key: 'tune', label: 'Tune', default: 0.5, help: 'Pitch of the thump.' }],
      inputs: [{ key: 'gate', kind: 'gate', help: 'Triggers the hit.' }],
      outputs: [{ key: 'out', kind: 'audio', help: 'The drum.' }],
    },
  ],
  styles: [],
  keys: ['C', 'D'],
  scales: ['major'],
};

describe('kerf-syntax', () => {
  it('finds modules and their kinds', () => {
    const text = 'kerf 1\nmodule kick Mortise tune=0.4\npart bass "Bass" colour #d9622b\n  module b2 Chisel\n';
    expect([...scanModules(text)]).toEqual([
      ['kick', 'Mortise'],
      ['b2', 'Chisel'],
    ]);
  });

  it('tells a whole patch from a fragment', () => {
    expect(isCompletePatch('# note\n\nkerf 1\ntempo 120')).toBe(true);
    expect(isCompletePatch('cable a.out -> b.in')).toBe(false);
  });

  it('lexes a module line into roles', () => {
    const line = 'module kick Mortise tune=0.4';
    const roles = lexLine(line).map((s) => s.role);
    expect(roles[0]).toBe('directive');
    expect(roles).toContain('kind');
    expect(roles).toContain('param');
  });

  it('marks comments and unknown directives', () => {
    expect(lexLine('# hello')[0]?.role).toBe('comment');
    expect(lexLine('modul x Y')[0]?.role).toBe('bad-directive');
  });
});

describe('kerf-hints', () => {
  it('completes directives at the start of a line', () => {
    const s = suggest('mo', 'mo', completions);
    expect(s?.options.map((o) => o.label)).toContain('module');
  });

  it('offers module kinds after the id', () => {
    const s = suggest('module kick M', 'module kick M', completions);
    expect(s?.options.map((o) => o.label)).toContain('Mortise');
  });

  it('offers the kind’s params on a module line', () => {
    const text = 'module kick Mortise t';
    const s = suggest(text, text, completions);
    expect(s?.options.some((o) => o.label.startsWith('tune'))).toBe(true);
  });

  it('completes jacks after a module id and a dot', () => {
    const text = 'module kick Mortise\ncable kick.';
    const s = suggest('cable kick.', text, completions);
    expect(s?.options.map((o) => o.label)).toContain('kick.out');
  });

  it('explains a kind on hover', () => {
    const line = 'module kick Mortise';
    const h = hoverAt(line, line.indexOf('Mortise') + 2, line, completions);
    expect(h?.term).toBe('Mortise');
  });
});

describe('diagnosticSpan', () => {
  it('uses the reported range', () => {
    expect(diagnosticSpan(20, 8, 15)).toEqual([7, 14]);
  });
  it('marks one character when the range is empty', () => {
    expect(diagnosticSpan(20, 8, 8)).toEqual([7, 8]);
  });
  it('marks the whole line without a column', () => {
    expect(diagnosticSpan(20, 0, 0)).toEqual([0, 20]);
  });
  it('stays inside the line', () => {
    expect(diagnosticSpan(5, 40, 50)).toEqual([5, 5]);
  });
});
