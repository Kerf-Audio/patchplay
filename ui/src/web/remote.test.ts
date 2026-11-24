import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { Bridge } from '../bridge';
import { RemoteBridge, serveRemote } from './remote';

/** The main window's own bridge: `hang` never answers, `fail` rejects, anything else echoes. */
const main: Bridge = {
  async call<T>(name: string, args?: Record<string, unknown>): Promise<T> {
    if (name === 'hang') return new Promise<T>(() => undefined);
    if (name === 'fail') throw new Error('no such patch');
    return { name, args } as T;
  },
  pickPath: async () => null,
};

beforeAll(() => serveRemote(main));
afterEach(() => vi.useRealTimers());

describe('RemoteBridge', () => {
  it('has the main window run a call and answer it', async () => {
    const remote = new RemoteBridge();
    await expect(remote.call('set_tempo', { bpm: 98 })).resolves.toEqual({ name: 'set_tempo', args: { bpm: 98 } });
  });

  it('carries a failure back as an error', async () => {
    await expect(new RemoteBridge().call('fail')).rejects.toThrow('no such patch');
  });

  it('gives up when the main window never answers', async () => {
    vi.useFakeTimers();
    const asked = expect(new RemoteBridge().call('hang')).rejects.toThrow('did not answer');
    await vi.advanceTimersByTimeAsync(5000);
    await asked;
  });

  it('leaves no timer pending once a call is answered', async () => {
    vi.useFakeTimers();
    const remote = new RemoteBridge();
    await remote.call('get_state');
    expect(vi.getTimerCount()).toBe(0);
  });
});
