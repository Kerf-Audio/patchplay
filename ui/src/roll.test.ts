import { describe, expect, it } from 'vitest';
import {
  cellOf,
  clickValue,
  isBlack,
  isStepParam,
  lengthOf,
  noteName,
  octavesOf,
  readout,
  semitonesOf,
  soundName,
  stepKey,
  strokeParams,
  transposeLabel,
  valueOf,
} from './roll';

describe('step values', () => {
  it('round-trips every drawable cell', () => {
    for (let row = 0; row < 12; row++) {
      for (const accent of [false, true]) {
        expect(cellOf(valueOf({ row, accent }))).toEqual({ row, accent });
      }
    }
    expect(cellOf(valueOf(null))).toBeNull();
  });

  it('uses short decimals so the patch text stays readable', () => {
    expect(valueOf({ row: 0, accent: false })).toBe(0.04);
    expect(valueOf({ row: 11, accent: true })).toBe(0.96);
    expect(String(valueOf({ row: 6, accent: true }))).toBe('0.76');
  });

  it('reads the engine encoding: level 1 to 12 plain, 13 to 24 accented, the top step capped', () => {
    expect(cellOf(0.04)).toEqual({ row: 0, accent: false });
    expect(cellOf(0.52)).toEqual({ row: 0, accent: true });
    expect(cellOf(1)).toEqual({ row: 11, accent: true });
    expect(cellOf(0.01)).toBeNull();
  });
});

describe('drawing', () => {
  it('draws a new row, clears the same row, and flips accent with shift', () => {
    const e = valueOf({ row: 4, accent: false });
    expect(cellOf(clickValue(0, 4, false))).toEqual({ row: 4, accent: false });
    expect(cellOf(clickValue(0, 4, true))).toEqual({ row: 4, accent: true });
    expect(cellOf(clickValue(e, 7, false))).toEqual({ row: 7, accent: false });
    expect(cellOf(clickValue(e, 4, false))).toBeNull();
    expect(cellOf(clickValue(e, 4, true))).toEqual({ row: 4, accent: true });
    expect(cellOf(clickValue(clickValue(e, 4, true), 4, true))).toEqual({ row: 4, accent: false });
  });
});

describe('names and keys', () => {
  it('knows the black keys', () => {
    expect([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11].filter(isBlack)).toEqual([1, 3, 6, 8, 10]);
    expect(noteName(9)).toBe('A');
  });

  it('names the sounding note, with C4 at 0 V', () => {
    expect(soundName(0, 0, 0)).toBe('C4');
    expect(soundName(0, 7, 1)).toBe('G5');
    expect(soundName(11, 1, 0)).toBe('C5');
    expect(soundName(0, -12, -1)).toBe('C2');
    expect(soundName(0, -1, 0)).toBe('B3');
  });

  it('only treats step1 to step16 as step params', () => {
    expect(stepKey(0)).toBe('step1');
    expect(stepKey(15)).toBe('step16');
    expect(isStepParam('step16')).toBe(true);
    for (const k of ['step0', 'step17', 'step', 'steps', 'rate', 'step01']) expect(isStepParam(k)).toBe(false);
  });
});

describe('knob mappings', () => {
  it('matches the engine ranges', () => {
    expect([lengthOf(0), lengthOf(1)]).toEqual([1, 16]);
    expect([semitonesOf(0), semitonesOf(0.5), semitonesOf(1)]).toEqual([-12, 0, 12]);
    expect([octavesOf(0), octavesOf(0.5), octavesOf(1)]).toEqual([-2, 0, 2]);
  });

  it('describes the transposition', () => {
    expect(transposeLabel(0, 0)).toBe('Not transposed');
    expect(transposeLabel(7, 1)).toBe('Transposed +7 st, +1 oct');
    expect(transposeLabel(-3, 0)).toBe('Transposed -3 st');
  });

  it('gives Inlay knobs a readout and leaves other modules alone', () => {
    expect(readout('Inlay', 'length', 0)).toBe('1 step');
    expect(readout('Inlay', 'length', 1)).toBe('16 steps');
    expect(readout('Inlay', 'transpose', 0.5 + 7 / 24)).toBe('+7 semitones');
    expect(readout('Inlay', 'octave', 0.75)).toBe('+1 octave');
    expect(readout('Inlay', 'rate', 0.6)).toBe('1/16 notes');
    expect(readout('Inlay', 'gate', 0.5)).toBe('50% of the step');
    expect(readout('Inlay', 'glide', 0.5)).toBeNull();
    expect(readout('Dovetail', 'length', 0.5)).toBeNull();
  });
});

describe('strokes', () => {
  it('sends only the steps that changed, as one list', () => {
    const before = Array<number>(16).fill(0);
    before[2] = 0.2;
    const edits = new Map([
      [0, 0.04],
      [2, 0.2],
      [5, 0],
    ]);
    expect(strokeParams(before, edits)).toEqual([{ param: 'step1', value: 0.04 }]);
  });
});
