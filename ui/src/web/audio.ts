// The page's side of the audio: an AudioContext (started by Play, which is a user gesture), the
// worklet node, and the traffic between the node and the wasm bridge. `?mute` routes the sound
// through a zero gain and exposes an analyser as `window.__patchplayProbe`, for automated tests.
import { set_levels, set_scope, take_audio_update } from '@kerf-web/kerf_web.js';
import { sharedPromise } from '../util';
import workletUrl from './worklet.ts?worker&url';
import type { FromWorklet, ToWorklet } from './worklet';
import type { Pcm } from './samples';

const STOP_DELAY_MS = 150;
/** The worklet keeps sending samples for a second after a ping; ping again well inside that. */
const SCOPE_PING_MS = 250;

export class WebAudio {
  private ctx: AudioContext | null = null;
  private node: AudioWorkletNode | null = null;
  private readonly starting = sharedPromise(() => this.start());
  private stopTimer = 0;
  private mute = false;
  private lastScopePing = -Infinity;
  /** Flushes run one after another: a stop must not overtake a play that is still waiting for the context. */
  private queued: Promise<unknown> = Promise.resolve();

  constructor(
    private readonly wasm: ArrayBuffer,
    private readonly samples: () => [string, Pcm][],
  ) {}

  private async start(): Promise<void> {
    this.mute = new URLSearchParams(location.search).has('mute');
    const ctx = new AudioContext({ latencyHint: 'interactive' });
    try {
      await ctx.audioWorklet.addModule(workletUrl);
      const node = new AudioWorkletNode(ctx, 'patchplay', {
        outputChannelCount: [2],
        processorOptions: { wasm: this.wasm },
      });
      node.port.onmessage = (e: MessageEvent<FromWorklet>) => this.heard(e.data);
      for (const [name, pcm] of this.samples()) this.send({ type: 'sample', name, mono: pcm.mono.slice(), rate: pcm.rate }, node);
      if (this.mute) {
        const probe = ctx.createAnalyser();
        const silence = ctx.createGain();
        silence.gain.value = 0;
        node.connect(probe);
        probe.connect(silence).connect(ctx.destination);
        (window as unknown as { __patchplayProbe: AnalyserNode }).__patchplayProbe = probe;
      } else {
        node.connect(ctx.destination);
      }
      this.ctx = ctx;
      this.node = node;
    } catch (e) {
      // A browser allows only a few contexts, so give this one up before the next try makes another.
      void ctx.close();
      throw e;
    }
  }

  private heard(m: FromWorklet): void {
    if (m.type === 'levels') {
      const [pl = 0, pr = 0, load = 0] = m.v;
      set_levels(pl, pr, load);
    } else if (m.type === 'scope') {
      set_scope(m.samples, m.rate);
    } else {
      console.error('audio:', m.message);
    }
  }

  private send(m: ToWorklet, node = this.node): void {
    node?.port.postMessage(m);
  }

  sample(name: string, pcm: Pcm): void {
    this.send({ type: 'sample', name, mono: pcm.mono.slice(), rate: pcm.rate });
  }

  /** The page is drawing the scope: have the worklet send samples (it stops by itself a second after the last call). */
  wantScope(): void {
    const now = performance.now();
    if (now - this.lastScopePing < SCOPE_PING_MS) return;
    this.lastScopePing = now;
    this.send({ type: 'scope' });
  }

  /** Passes on whatever the bridge has queued for the worklet. Call it after every command. */
  flush(): Promise<void> {
    const run = this.queued.then(() => this.drain());
    this.queued = run.catch(() => undefined);
    return run;
  }

  private async drain(): Promise<void> {
    for (;;) {
      const next = take_audio_update();
      if (!next) return;
      const cut = next.indexOf('\n');
      const kind = next.slice(0, cut);
      const text = next.slice(cut + 1);
      if (kind === 'stop') {
        this.send({ type: 'audio', kind, text });
        set_levels(0, 0, 0);
        set_scope(new Float32Array(0), 0);
        this.stopTimer = window.setTimeout(() => void this.ctx?.suspend(), STOP_DELAY_MS);
        continue;
      }
      clearTimeout(this.stopTimer);
      await this.starting();
      await this.ctx?.resume();
      this.send({ type: 'audio', kind, text });
    }
  }
}
