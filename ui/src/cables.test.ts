import { describe, expect, it } from 'vitest';
import { cablePath, orient, splitRef } from './cables';

describe('cablePath', () => {
  it('hangs below both ends', () => {
    expect(cablePath([0, 0], [0, 100])).toBe('M0 0 C0 55 0 155 0 100');
  });

  it('caps the sag for long cables', () => {
    expect(cablePath([0, 0], [10000, 0])).toBe('M0 0 C0 165 10000 165 10000 0');
  });
});

describe('splitRef', () => {
  it('splits module and jack', () => {
    expect(splitRef('mix.in1')).toEqual(['mix', 'in1']);
    expect(splitRef('mix')).toEqual(['mix', '']);
  });
});

describe('orient', () => {
  const out = { ref: 'a.out', dir: 'out' };
  const inn = { ref: 'b.in', dir: 'in' };

  it('puts the output first whichever way it was dragged', () => {
    expect(orient(out, inn)).toEqual({ from: 'a.out', to: 'b.in' });
    expect(orient(inn, out)).toEqual({ from: 'a.out', to: 'b.in' });
  });

  it('refuses two jacks of the same direction', () => {
    expect(orient(out, { ref: 'c.out', dir: 'out' })).toBeNull();
  });
});
