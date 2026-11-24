import { describe, expect, it } from 'vitest';
import { pickTheme } from './theme';

describe('pickTheme', () => {
  it('uses a saved choice over the system preference', () => {
    expect(pickTheme('day', true)).toBe('day');
    expect(pickTheme('night', false)).toBe('night');
  });

  it('follows the system when nothing is saved', () => {
    expect(pickTheme(null, true)).toBe('night');
    expect(pickTheme(null, false)).toBe('day');
  });

  it('ignores a corrupt saved value', () => {
    expect(pickTheme('purple', false)).toBe('day');
    expect(pickTheme('', true)).toBe('night');
  });
});
