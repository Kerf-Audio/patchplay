import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { set_scope, take_audio_update } from '@kerf-web/kerf_web.js';
import { WebAudio } from './audio';
import type { FromWorklet, ToWorklet } from './worklet';

vi.mock('@kerf-web/kerf_web.js', () => ({ set_levels: vi.fn(), set_scope: vi.fn(), take_audio_update: vi.fn() }));
vi.mock('./worklet.ts?worker&url', () => ({ default: 'worklet.js' }));

class FakeNode {
  static made: FakeNode[] = [];
  port = { postMessage: vi.fn<(m: ToWorklet) => void>(), onmessage: null as ((e: MessageEvent<FromWorklet>) => void) | null };
  connect = vi.fn();

  constructor() {
    FakeNode.made.push(this);
  }
}

class FakeContext {
  static made: FakeContext[] = [];
  static failures = 0;
  static gate: Promise<void> | null = null;
  destination = {};
  close = vi.fn(async () => undefined);
  resume = vi.fn(async () => undefined);
  suspend = vi.fn(async () => undefined);
  audioWorklet = {
    addModule: async (): Promise<void> => {
      await FakeContext.gate;
      if (FakeContext.failures-- > 0) throw new Error('the worklet would not load');
    },
  };

  constructor() {
    FakeContext.made.push(this);
  }
}

/** Has the bridge queue these updates for the worklet, one per `take_audio_update`. */
const queue = (...updates: string[]): void => {
  vi.mocked(take_audio_update).mockImplementation(() => updates.shift() ?? '');
};

beforeEach(() => {
  FakeContext.made = [];
  FakeNode.made = [];
  FakeContext.failures = 0;
  FakeContext.gate = null;
  vi.stubGlobal('AudioContext', FakeContext);
  vi.stubGlobal('AudioWorkletNode', FakeNode);
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('window', globalThis);
});
afterEach(() => vi.unstubAllGlobals());

describe('WebAudio.flush', () => {
  it('starts one context and reuses it for later updates', async () => {
    const audio = new WebAudio(new ArrayBuffer(0), () => []);
    queue('patch\none', 'patch\ntwo');
    await audio.flush();
    queue('patch\nthree');
    await audio.flush();
    expect(FakeContext.made).toHaveLength(1);
  });

  it('tries again after a start fails, and gives up the context it abandoned', async () => {
    const audio = new WebAudio(new ArrayBuffer(0), () => []);
    FakeContext.failures = 1;
    queue('patch\none');
    await expect(audio.flush()).rejects.toThrow('the worklet would not load');
    expect(FakeContext.made).toHaveLength(1);
    expect(FakeContext.made[0]?.close).toHaveBeenCalledTimes(1);

    queue('patch\none');
    await audio.flush();
    expect(FakeContext.made).toHaveLength(2);
    expect(FakeContext.made[1]?.close).not.toHaveBeenCalled();
    expect(FakeContext.made[1]?.resume).toHaveBeenCalled();
  });
});

describe('WebAudio.flush ordering', () => {
  it('delivers a stop after the play that was still waiting for the context', async () => {
    const audio = new WebAudio(new ArrayBuffer(0), () => []);
    let open: () => void = () => undefined;
    FakeContext.gate = new Promise((resolve) => (open = resolve));
    queue('patch\none');
    const first = audio.flush();
    await new Promise((resolve) => setTimeout(resolve, 0));
    queue('stop\n');
    const second = audio.flush();
    open();
    await Promise.all([first, second]);
    const sent = FakeNode.made[0]?.port.postMessage.mock.calls.map(([m]) => (m.type === 'audio' ? m.kind : m.type));
    expect(sent).toEqual(['patch', 'stop']);
  });
});

describe('WebAudio scope', () => {
  it('pings the worklet at most every quarter second and hands its samples to the bridge', async () => {
    const audio = new WebAudio(new ArrayBuffer(0), () => []);
    queue('patch\none');
    await audio.flush();
    const node = FakeNode.made[0];
    if (!node) throw new Error('the page should have made a node');
    const now = vi.spyOn(performance, 'now');
    for (const t of [1000, 1100, 1300]) {
      now.mockReturnValue(t);
      audio.wantScope();
    }
    expect(node.port.postMessage.mock.calls.filter(([m]) => m.type === 'scope')).toHaveLength(2);

    const samples = new Float32Array([0.1, -0.1]);
    node.port.onmessage?.({ data: { type: 'scope', samples, rate: 44100 } } as MessageEvent<FromWorklet>);
    expect(set_scope).toHaveBeenCalledWith(samples, 44100);
    now.mockRestore();
  });
});
