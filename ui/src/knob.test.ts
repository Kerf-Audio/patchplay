import { describe, expect, it } from 'vitest';
import { angle, DRAG_PIXELS, dragValue, FINE, keyValue, knobHtml, wheelValue } from './knob';

describe('dragValue', () => {
  it('sweeps the whole range over DRAG_PIXELS', () => {
    expect(dragValue(0, DRAG_PIXELS, false)).toBeCloseTo(1);
    expect(dragValue(0.5, -DRAG_PIXELS / 2, false)).toBeCloseTo(0);
  });

  it('clamps to 0..1', () => {
    expect(dragValue(0.9, 500, false)).toBe(1);
    expect(dragValue(0.1, -500, false)).toBe(0);
  });

  it('moves by FINE of the distance when shift is held', () => {
    expect(dragValue(0.2, 80, true)).toBeCloseTo(0.2 + 0.5 * FINE);
  });
});

describe('knob drawing', () => {
  it('points from -135 to +135 degrees', () => {
    expect(angle(0)).toBe(-135);
    expect(angle(1)).toBe(135);
    expect(angle(2)).toBe(135);
  });

  it('draws the value arc as a fraction of the ring and points the cap', () => {
    expect(knobHtml(0.5)).toContain('stroke-dasharray="50.00 100"');
    expect(knobHtml(0.5)).toContain('--a:0.0deg');
    expect(knobHtml(2)).toContain('stroke-dasharray="100.00 100"');
  });
});

describe('keyboard and wheel', () => {
  it('steps by the key step, finer with shift, and clamps', () => {
    expect(keyValue(0.5, 1, false)).toBeCloseTo(0.52);
    expect(keyValue(0.5, -1, true)).toBeCloseTo(0.5 - 0.02 * FINE);
    expect(keyValue(1, 1, false)).toBe(1);
    expect(keyValue(0, -1, false)).toBe(0);
  });

  it('raises the value when the wheel scrolls up', () => {
    expect(wheelValue(0.5, -100, false)).toBeGreaterThan(0.5);
    expect(wheelValue(0.5, 100, false)).toBeLessThan(0.5);
    expect(wheelValue(0.5, -100, true) - 0.5).toBeCloseTo((wheelValue(0.5, -100, false) - 0.5) * FINE);
  });
});
