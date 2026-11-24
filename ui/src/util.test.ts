import { describe, expect, it, vi } from 'vitest';
import { baseName, bootFailureHtml, cableStroke, esc, sharedPromise } from './util';

describe('sharedPromise', () => {
  it('builds once and shares the result', async () => {
    const make = vi.fn(async () => ({}));
    const get = sharedPromise(make);
    const [a, b] = await Promise.all([get(), get()]);
    expect(await get()).toBe(a);
    expect(b).toBe(a);
    expect(make).toHaveBeenCalledTimes(1);
  });

  it('forgets a failure, so the next call tries again', async () => {
    const make = vi
      .fn<() => Promise<string>>()
      .mockRejectedValueOnce(new Error('not ready'))
      .mockResolvedValue('docs');
    const get = sharedPromise(make);
    await expect(get()).rejects.toThrow('not ready');
    await expect(get()).resolves.toBe('docs');
    await expect(get()).resolves.toBe('docs');
    expect(make).toHaveBeenCalledTimes(2);
  });

  it('gives callers that are waiting together the same failure', async () => {
    const get = sharedPromise(() => Promise.reject(new Error('down')));
    const results = await Promise.allSettled([get(), get()]);
    expect(results.map((r) => r.status)).toEqual(['rejected', 'rejected']);
  });
});

describe('bootFailureHtml', () => {
  it('escapes markup in the backend message', () => {
    const html = bootFailureHtml(new Error('<img src=x onerror=alert(1)>'));
    expect(html).not.toContain('<img');
    expect(html).toContain('&lt;img src=x onerror=alert(1)&gt;');
  });

  it('shows a thrown string too', () => {
    expect(bootFailureHtml('plain "text"')).toContain('plain &quot;text&quot;');
  });
});

describe('esc', () => {
  it('escapes the characters that break out of text and double-quoted attributes', () => {
    expect(esc(`<a href="x">&</a>`)).toBe('&lt;a href=&quot;x&quot;&gt;&amp;&lt;/a&gt;');
  });
});

describe('baseName', () => {
  it('keeps the last part of a path from either platform', () => {
    expect(baseName('/Users/me/Music/kick.wav')).toBe('kick.wav');
    expect(baseName('C:\\Users\\me\\kick.wav')).toBe('kick.wav');
    expect(baseName('patch.kerf')).toBe('patch.kerf');
  });

  it('hides the web: mark of a file the browser holds', () => {
    expect(baseName('web:kerf.wav')).toBe('kerf.wav');
  });
});

describe('cableStroke', () => {
  it('keeps a plain kind and strips anything that could break out of the style attribute', () => {
    expect(cableStroke('audio')).toBe('stroke:var(--audio)');
    expect(cableStroke('x);fill:red" onmouseover="alert(1)')).toBe('stroke:var(--xfillredonmouseoveralert1)');
  });
});
