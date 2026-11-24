import { describe, expect, it } from 'vitest';
import { trace } from './explain';

const cable = (from: string, to: string) => ({ from, to });

describe('trace', () => {
  const chain = [cable('a.out', 'b.in'), cable('b.out', 'c.in')];

  it('follows cables in both directions through any depth', () => {
    const t = trace(chain, 'b');
    expect([...t.upstream]).toEqual(['a']);
    expect([...t.downstream]).toEqual(['c']);
    expect([...trace(chain, 'c').upstream].sort()).toEqual(['a', 'b']);
  });

  it('terminates on a feedback loop and never lists the module itself', () => {
    const t = trace([cable('a.out', 'b.in'), cable('b.out', 'a.in')], 'a');
    expect([...t.upstream]).toEqual(['b']);
    expect([...t.downstream]).toEqual(['b']);
  });

  it('is empty for an unpatched module', () => {
    const t = trace(chain, 'lonely');
    expect(t.upstream.size + t.downstream.size).toBe(0);
  });
});
