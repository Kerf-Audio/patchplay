import { afterEach, describe, expect, it, vi } from 'vitest';
import { app, notify, run, subscribe } from './store';
import type { State } from './types';

afterEach(() => {
  vi.useRealTimers();
  app.notice = null;
});

describe('notify', () => {
  it('redraws at once, so a notice raised after a command is seen, and again when it clears', () => {
    vi.useFakeTimers();
    const redraw = vi.fn();
    subscribe(redraw);
    notify('Saved patch.kerf');
    expect(app.notice).toEqual({ text: 'Saved patch.kerf', error: false });
    expect(redraw).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(3000);
    expect(app.notice).toBeNull();
    expect(redraw).toHaveBeenCalledTimes(2);
  });

  it('keeps an error up for longer', () => {
    vi.useFakeTimers();
    notify('Disk full', true);
    vi.advanceTimersByTime(3000);
    expect(app.notice).toEqual({ text: 'Disk full', error: true });
    vi.advanceTimersByTime(3000);
    expect(app.notice).toBeNull();
  });
});

describe('run', () => {
  it('shows a rejection as one error notice and one redraw, without throwing', async () => {
    vi.useFakeTimers();
    const redraw = vi.fn();
    subscribe(redraw);
    await expect(run('save', Promise.reject(new Error('no permission')))).resolves.toBe(false);
    expect(app.notice).toEqual({ text: 'no permission', error: true });
    expect(redraw).toHaveBeenCalledTimes(1);
    expect(app.log.at(-1)?.text).toBe('✗ save: no permission');
    expect(app.state).toBeNull();
  });
});

describe('run ordering', () => {
  it('ignores an older answer that arrives after a newer one', async () => {
    const state = (text: string): State => ({ text, lineage: { nodes: [] } }) as unknown as State;
    let older: (s: State) => void = () => undefined;
    const first = run('set_param', new Promise<State>((resolve) => (older = resolve)));
    const second = run('set_param', Promise.resolve(state('newer')));
    await second;
    older(state('older'));
    await expect(first).resolves.toBe(true);
    expect(app.state?.text).toBe('newer');
  });
});
