import { describe, expect, it } from 'vitest';
import { grainRegion, grainSvg, sliceIndex, waveSvg } from './sample';
import { takesSample } from './util';

describe('sliceIndex', () => {
  it('maps the Slice knob onto the 16 slices the engine plays', () => {
    expect(sliceIndex(0, 16)).toBe(0);
    expect(sliceIndex(0.2, 16)).toBe(3);
    expect(sliceIndex(0.999, 16)).toBe(15);
    expect(sliceIndex(1, 16)).toBe(15);
    expect(sliceIndex(-1, 16)).toBe(0);
  });
});

describe('waveSvg', () => {
  const peaks: [number, number][] = Array.from({ length: 256 }, (_, i) => (i === 100 ? [-1, 1] : [-0.2, 0.2]));
  const svg = waveSvg(peaks, 16, 3);

  it('draws a column per peak pair, full height for a full-scale peak', () => {
    const d = /class="wv-peaks" d="([^"]*)"/.exec(svg)?.[1] ?? '';
    expect(d.split('M').length - 1).toBe(256);
    expect(d).toContain('M100.5 2.0V62.0');
  });

  it('marks the 15 boundaries between 16 slices', () => {
    const marks = /class="wv-marks" d="([^"]*)"/.exec(svg)?.[1] ?? '';
    expect(marks.split('M').length - 1).toBe(15);
  });

  it('shades the selected slice', () => {
    expect(svg).toMatch(/class="wv-sel" x="48.00"[^>]*width="16.00"/);
  });

  it('shades nothing when no slice is selected, but keeps the marks', () => {
    const plain = waveSvg(peaks, 16, null);
    expect(plain).not.toContain('wv-sel');
    expect(plain).toContain('wv-marks');
  });
});

describe('grainRegion', () => {
  it('centres the spray on the read point and keeps it inside the sample', () => {
    expect(grainRegion(0.5, 0.2)).toEqual({ from: 0.4, to: 0.6, at: 0.5 });
    expect(grainRegion(0.05, 0.5)).toEqual({ from: 0, to: 0.3, at: 0.05 });
    expect(grainRegion(1, 1)).toEqual({ from: 0.5, to: 1, at: 1 });
  });
});

describe('grainSvg', () => {
  const peaks: [number, number][] = Array.from({ length: 200 }, () => [-0.2, 0.2]);

  it('draws the read point and shades the spray instead of marking slices', () => {
    const svg = grainSvg(peaks, 0.5, 0.2);
    expect(svg).toMatch(/class="wv-spray" x="80.00"[^>]*width="40.00"/);
    expect(svg).toContain('class="wv-pos" d="M100.00 0V64"');
    expect(svg).not.toContain('wv-marks');
  });
});

describe('takesSample', () => {
  it('is true for the modules that play an audio file', () => {
    expect(takesSample('Veneer')).toBe(true);
    expect(takesSample('Splinter')).toBe(true);
    expect(takesSample('Lathe')).toBe(false);
  });
});
