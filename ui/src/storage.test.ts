import { afterEach, describe, expect, it, vi } from 'vitest';
import { forgetStored, readStored, writeStored } from './storage';

const original = Object.getOwnPropertyDescriptor(globalThis, 'localStorage');

/** A browser that refuses storage: merely touching `localStorage` throws, as Safari does. */
function blockStorage(): void {
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    get() {
      throw new DOMException('The operation is insecure.', 'SecurityError');
    },
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
  if (original) Object.defineProperty(globalThis, 'localStorage', original);
  else Reflect.deleteProperty(globalThis, 'localStorage');
});

describe('stored values', () => {
  it('round-trips through a working store', () => {
    const data = new Map<string, string>();
    vi.stubGlobal('localStorage', {
      getItem: (k: string) => data.get(k) ?? null,
      setItem: (k: string, v: string) => void data.set(k, v),
      removeItem: (k: string) => void data.delete(k),
    });
    writeStored('a', 'on');
    expect(readStored('a')).toBe('on');
    expect(readStored('missing')).toBeNull();
    forgetStored('a');
    expect(readStored('a')).toBeNull();
  });

  it('reads nothing and writes nothing when storage is blocked', () => {
    blockStorage();
    expect(readStored('a')).toBeNull();
    expect(() => writeStored('a', 'on')).not.toThrow();
    expect(() => forgetStored('a')).not.toThrow();
  });

  it('survives a full store', () => {
    vi.stubGlobal('localStorage', {
      setItem: () => {
        throw new DOMException('Quota', 'QuotaExceededError');
      },
    });
    expect(() => writeStored('a', 'on')).not.toThrow();
  });
});
