// The sums behind the footer scope, kept free of the DOM so they can be tested: a radix-2 FFT,
// the Hann-windowed spectrum in dB, its mapping onto log-spaced columns, the oscilloscope's
// rising-edge trigger and the spectrum's fast-attack, slow-decay smoothing.

/** The spectrum's size: the backend hands over this many samples. */
export const FFT_SIZE = 2048;
export const FLOOR_DB = -90;
export const MIN_HZ = 20;
export const MAX_HZ = 20000;

/** In-place radix-2 FFT. `re` and `im` must be the same power-of-two length. */
export function fft(re: Float32Array, im: Float32Array): void {
  const n = re.length;
  if (n < 2 || (n & (n - 1)) !== 0 || im.length !== n) throw new Error('fft: length must be a power of two');
  for (let i = 1, j = 0; i < n; i++) {
    let bit = n >> 1;
    for (; j & bit; bit >>= 1) j ^= bit;
    j ^= bit;
    if (i < j) {
      [re[i], re[j]] = [re[j] as number, re[i] as number];
      [im[i], im[j]] = [im[j] as number, im[i] as number];
    }
  }
  for (let len = 2; len <= n; len <<= 1) {
    const angle = (-2 * Math.PI) / len;
    const wr = Math.cos(angle);
    const wi = Math.sin(angle);
    for (let start = 0; start < n; start += len) {
      let cr = 1;
      let ci = 0;
      for (let k = 0; k < len / 2; k++) {
        const a = start + k;
        const b = a + len / 2;
        const tr = (re[b] as number) * cr - (im[b] as number) * ci;
        const ti = (re[b] as number) * ci + (im[b] as number) * cr;
        re[b] = (re[a] as number) - tr;
        im[b] = (im[a] as number) - ti;
        re[a] = (re[a] as number) + tr;
        im[a] = (im[a] as number) + ti;
        const next = cr * wr - ci * wi;
        ci = cr * wi + ci * wr;
        cr = next;
      }
    }
  }
}

/** A periodic Hann window, so a tone that sits on a bin leaks into its two neighbours only. */
export function hann(n: number): Float32Array {
  const w = new Float32Array(n);
  for (let i = 0; i < n; i++) w[i] = 0.5 - 0.5 * Math.cos((2 * Math.PI * i) / n);
  return w;
}

/**
 * The spectrum of the last `window.length` samples as dB per bin (`window.length / 2` of them,
 * bin k at k * rate / n Hz), scaled so a full-scale sine on a bin reads 0 dB.
 */
export function spectrumDb(samples: ArrayLike<number>, window: Float32Array): Float32Array {
  const n = window.length;
  const re = new Float32Array(n);
  const im = new Float32Array(n);
  const offset = samples.length - n;
  let gain = 0;
  for (let i = 0; i < n; i++) {
    re[i] = (samples[offset + i] ?? 0) * (window[i] as number);
    gain += window[i] as number;
  }
  fft(re, im);
  const out = new Float32Array(n / 2);
  const scale = 2 / gain;
  for (let k = 0; k < out.length; k++) {
    const mag = Math.hypot(re[k] as number, im[k] as number) * scale;
    const db = 20 * Math.log10(mag + 1e-12);
    // A NaN or infinite sample would otherwise stick in the smoothed levels for good.
    out[k] = Number.isFinite(db) && db > FLOOR_DB ? db : FLOOR_DB;
  }
  return out;
}

/** Where a column sits on the log axis: its edges and centre in Hz. */
function columnEdges(col: number, cols: number): [number, number, number] {
  const at = (t: number): number => MIN_HZ * (MAX_HZ / MIN_HZ) ** (t / cols);
  return [at(col), at(col + 0.5), at(col + 1)];
}

/**
 * Maps per-bin dB onto `cols` columns spaced evenly in log frequency from 20 Hz to 20 kHz. A
 * column that covers several bins shows the loudest; one narrower than a bin (the bass end)
 * interpolates between the two bins around its centre.
 */
export function columnLevels(db: Float32Array, rate: number, cols: number): Float32Array {
  const out = new Float32Array(cols);
  const binHz = rate / (2 * db.length);
  const top = db.length - 1;
  for (let c = 0; c < cols; c++) {
    const [lo, mid, hi] = columnEdges(c, cols);
    const first = Math.max(0, Math.ceil(lo / binHz));
    const last = Math.min(top, Math.floor(hi / binHz));
    if (last >= first) {
      let peak = FLOOR_DB;
      for (let k = first; k <= last; k++) peak = Math.max(peak, db[k] as number);
      out[c] = peak;
    } else {
      const pos = Math.min(top, mid / binHz);
      const k = Math.min(top - 1, Math.floor(pos));
      const f = pos - k;
      out[c] = (db[k] as number) * (1 - f) + (db[k + 1] as number) * f;
    }
  }
  return out;
}

/** Rises at once (`attack` of the way), falls at most `fallDb` per call, so peaks linger. */
export function smoothLevels(prev: Float32Array, next: Float32Array, attack = 0.7, fallDb = 2.5): void {
  for (let i = 0; i < prev.length; i++) {
    const p = prev[i] as number;
    const n = next[i] as number;
    prev[i] = n > p ? p + (n - p) * attack : Math.max(n, p - fallDb);
  }
}

/**
 * Where the oscilloscope trace should start so a periodic wave holds still: the first sample at
 * or above the middle of the wave that follows one clearly below it (a little hysteresis keeps
 * noise from retriggering). Only the first `searchEnd` samples are searched, so `samples.length
 * - searchEnd` samples are left to draw after it. Answers 0 for silence or a wave that never rises.
 */
export function findTrigger(samples: ArrayLike<number>, searchEnd: number): number {
  const end = Math.min(searchEnd, samples.length);
  let lo = Infinity;
  let hi = -Infinity;
  for (let i = 0; i < samples.length; i++) {
    const s = samples[i] as number;
    if (s < lo) lo = s;
    if (s > hi) hi = s;
  }
  const span = hi - lo;
  if (!(span > 1e-4)) return 0;
  const level = (hi + lo) / 2;
  const armedBelow = level - span * 0.05;
  let armed = false;
  for (let i = 0; i < end; i++) {
    const s = samples[i] as number;
    if (s < armedBelow) armed = true;
    else if (armed && s >= level) return i;
  }
  return 0;
}
