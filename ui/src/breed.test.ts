import { describe, expect, it } from 'vitest';
import { breedMate } from './breed';
import type { Lineage, LineageNode } from './types';

const HEADER = 'kerf 1\ntempo 120\n';
const loose = `${HEADER}module osc1 Chisel tune=0\n`;
const inPart = `${HEADER}part bass "Bass" colour #3d63c9\n  module osc1 Chisel tune=0\n`;

const node = (id: number, patch: string): LineageNode => ({ id, parents: [], origin: { kind: 'root' }, patch });
const lineage = (current: number, ...nodes: LineageNode[]): Lineage => ({ current, nodes });

describe('breedMate', () => {
  it('is null when the current patch is the only one', () => {
    expect(breedMate(lineage(1, node(1, loose)), null)).toBeNull();
  });

  it('prefers the hearted patch, unless it is the current one', () => {
    const l = lineage(3, node(1, loose), node(2, loose), node(3, loose));
    expect(breedMate(l, 1)).toBe(1);
    expect(breedMate(l, 3)).toBe(2);
  });

  it('otherwise takes the newest other patch', () => {
    expect(breedMate(lineage(4, node(1, loose), node(3, loose), node(4, loose), node(2, loose)), null)).toBe(3);
  });

  it('skips patches with no modules', () => {
    expect(breedMate(lineage(2, node(1, HEADER), node(2, loose)), null)).toBeNull();
  });

  it('counts a patch whose modules all sit inside parts', () => {
    expect(breedMate(lineage(2, node(1, inPart), node(2, loose)), null)).toBe(1);
  });

  it('does not mistake a comment for a module', () => {
    expect(breedMate(lineage(2, node(1, `${HEADER}# module osc1 Chisel\n`), node(2, loose)), null)).toBeNull();
  });
});
