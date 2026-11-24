import { describe, expect, it } from 'vitest';
import { columnLevels, FFT_SIZE, FLOOR_DB, fft, findTrigger, hann, MAX_HZ, MIN_HZ, smoothLevels, spectrumDb } from './scope-math';

const RATE = 48000;

function sine(freq: number, n: number, amp = 1, phase = 0): Float32Array {
  return Float32Array.from({ length: n }, (_, i) => amp * Math.sin(2 * Math.PI * freq * (i / RATE) + phase));
}

const loudest = (a: Float32Array): number => a.reduce((best, v, i) => (v > (a[best] as number) ? i : best), 0);

describe('fft', () => {
  it('turns an impulse into a flat spectrum and a constant into one bin', () => {
    const re = new Float32Array(8);
    const im = new Float32Array(8);
    re[0] = 1;
    fft(re, im);
    expect([...re]).toEqual(Array(8).fill(1));
    expect([...im]).toEqual(Array(8).fill(0));

    const dc = new Float32Array(8).fill(0.5);
    const zero = new Float32Array(8);
    fft(dc, zero);
    expect(dc[0]).toBeCloseTo(4);
    expect([...dc.slice(1)].every((v) => Math.abs(v) < 1e-6)).toBe(true);
  });

  it('rejects a length that is not a power of two', () => {
    expect(() => fft(new Float32Array(6), new Float32Array(6))).toThrow();
  });
});

describe('spectrumDb with bad samples', () => {
  it('reads the floor, not NaN, when a sample is NaN or infinite', () => {
    const window = hann(FFT_SIZE);
    for (const bad of [Number.NaN, Number.POSITIVE_INFINITY]) {
      const samples = new Float32Array(FFT_SIZE);
      samples[10] = bad;
      expect(Array.from(spectrumDb(samples, window)).every((db) => db === FLOOR_DB)).toBe(true);
    }
  });
});

describe('spectrumDb', () => {
  const window = hann(FFT_SIZE);

  it('puts a sine on its bin, at its level', () => {
    const bin = 100;
    const freq = (bin * RATE) / FFT_SIZE;
    const full = spectrumDb(sine(freq, FFT_SIZE), window);
    expect(loudest(full)).toBe(bin);
    expect(full[bin]).toBeCloseTo(0, 1);

    const half = spectrumDb(sine(freq, FFT_SIZE, 0.5), window);
    expect(loudest(half)).toBe(bin);
    expect(half[bin]).toBeCloseTo(-6.02, 1);
  });

  it('finds a tone between bins at the nearest one and keeps far bins low', () => {
    const freq = 1000;
    const db = spectrumDb(sine(freq, FFT_SIZE, 0.8), window);
    expect(loudest(db)).toBe(Math.round((freq * FFT_SIZE) / RATE));
    expect(db[500] as number).toBeLessThan(-70);
  });

  it('reads silence as the floor and uses the latest samples when given more', () => {
    expect(spectrumDb(new Float32Array(FFT_SIZE), window).every((v) => v === FLOOR_DB)).toBe(true);
    const bin = 64;
    const freq = (bin * RATE) / FFT_SIZE;
    const longer = new Float32Array(3 * FFT_SIZE);
    longer.set(sine(freq, FFT_SIZE), 2 * FFT_SIZE);
    expect(loudest(spectrumDb(longer, window))).toBe(bin);
  });
});

describe('columnLevels', () => {
  it('puts a tone in the column its frequency belongs to on the log axis', () => {
    const cols = 160;
    for (const freq of [60, 440, 3000, 12000]) {
      const bin = Math.round((freq * FFT_SIZE) / RATE);
      const exact = (bin * RATE) / FFT_SIZE;
      const db = spectrumDb(sine(exact, FFT_SIZE), hann(FFT_SIZE));
      const levels = columnLevels(db, RATE, cols);
      const expected = Math.floor((cols * Math.log(exact / MIN_HZ)) / Math.log(MAX_HZ / MIN_HZ));
      // Neighbouring columns share a bin at the bass end, so allow one column either way.
      expect(Math.abs(loudest(levels) - expected)).toBeLessThanOrEqual(2);
      expect(levels[loudest(levels)] as number).toBeGreaterThan(-8);
    }
  });

  it('shows the loudest bin where a column covers several', () => {
    const db = new Float32Array(1024).fill(-80);
    db[900] = -10; // above 20 kHz at this rate (bin 853), so off the axis
    db[850] = -20;
    const levels = columnLevels(db, RATE, 100);
    expect(Math.max(...levels)).toBe(-20);
  });
});

describe('smoothLevels', () => {
  it('rises quickly and falls slowly', () => {
    const shown = Float32Array.of(-60, -20);
    smoothLevels(shown, Float32Array.of(-30, -90), 1, 2.5);
    expect([...shown]).toEqual([-30, -22.5]);
    smoothLevels(shown, Float32Array.of(-30, -90), 0.5, 2.5);
    expect(shown[1]).toBe(-25);
  });
});

describe('findTrigger', () => {
  it('starts every frame of a periodic wave at the same phase', () => {
    const freq = 220;
    const starts = [0, 37, 311, 1000, 1500].map((shift) => {
      const frame = Float32Array.from({ length: 2048 }, (_, i) => Math.sin((2 * Math.PI * freq * (i + shift)) / RATE));
      const at = findTrigger(frame, 1024);
      return Math.sin((2 * Math.PI * freq * (at + shift)) / RATE);
    });
    for (const v of starts) {
      expect(v).toBeGreaterThanOrEqual(0);
      expect(v).toBeLessThan(0.1);
    }
  });

  it('triggers on a rising edge only', () => {
    const wave = Float32Array.from([0.5, 0.2, -0.5, -0.5, -0.1, 0.5, 0.5, -0.5]);
    expect(findTrigger(wave, 8)).toBe(5);
  });

  it('ignores ripple smaller than the hysteresis and finds the waves offset', () => {
    // A wave riding on an offset: its middle is 0.6, not zero.
    const frame = Float32Array.from({ length: 2048 }, (_, i) => 0.6 + 0.3 * Math.sin((2 * Math.PI * 200 * i) / RATE));
    const at = findTrigger(frame, 1024);
    expect(at).toBeGreaterThan(0);
    expect(frame[at - 1] as number).toBeLessThan(0.6);
    expect(frame[at] as number).toBeGreaterThanOrEqual(0.6);
  });

  it('answers 0 for silence, a falling-only ramp and a search window with no edge', () => {
    expect(findTrigger(new Float32Array(2048), 1024)).toBe(0);
    expect(findTrigger(Float32Array.from({ length: 100 }, (_, i) => 1 - i / 100), 50)).toBe(0);
    const late = new Float32Array(2048).fill(-0.5);
    late.fill(0.5, 1500);
    expect(findTrigger(late, 1024)).toBe(0);
  });
});
