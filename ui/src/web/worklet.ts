// The audio thread. An AudioWorkletProcessor that runs the engine (`EngineHandle`, from the same
// wasm module the page uses) one 128-frame block at a time. The page sends it `.kerf` text and
// decoded samples through the port; it reports levels about 30 times a second. No
// SharedArrayBuffer: everything crosses by message.
import './textcodec';
import { EngineHandle, initSync, provide_sample, forget_sample } from '@kerf-web/kerf_web.js';

export type ToWorklet =
  | { type: 'audio'; kind: string; text: string }
  | { type: 'sample'; name: string; mono: Float32Array; rate: number }
  | { type: 'forget'; name: string }
  /** The page is drawing the scope: send the latest output samples with each report for a second. */
  | { type: 'scope' };

export type FromWorklet =
  | { type: 'levels'; v: number[] }
  | { type: 'scope'; samples: Float32Array; rate: number }
  | { type: 'error'; message: string };

interface Scope {
  AudioWorkletProcessor: new () => { port: MessagePort };
  registerProcessor(name: string, processor: unknown): void;
  sampleRate: number;
}
const scope = globalThis as unknown as Scope;
// Not every browser gives a worklet `performance`; `Date.now` ticks in whole milliseconds, but a
// tick lands at a random point in each block, so summed over a report it still averages out.
const now: () => number = typeof performance === 'undefined' ? Date.now : () => performance.now();

const REPORT_PER_SECOND = 30;
/** How long one `scope` message keeps the samples coming, in seconds of audio. */
const SCOPE_HOLD_SECONDS = 1;

class Patchplay extends scope.AudioWorkletProcessor {
  private readonly engine: EngineHandle;
  private readonly memory: WebAssembly.Memory;
  private readonly pointer: number;
  private view: Float32Array;
  private since = 0;
  private scopeFrames = 0;
  private readonly every = Math.round(scope.sampleRate / REPORT_PER_SECOND);

  constructor(options: { processorOptions: { wasm: ArrayBuffer } }) {
    super();
    this.memory = initSync({ module: options.processorOptions.wasm }).memory;
    this.engine = new EngineHandle(scope.sampleRate);
    this.pointer = this.engine.output_ptr();
    this.view = new Float32Array(this.memory.buffer, this.pointer, 2048);
    this.port.onmessage = (e: MessageEvent<ToWorklet>) => this.receive(e.data);
  }

  private receive(m: ToWorklet): void {
    try {
      if (m.type === 'audio') this.engine.apply(m.kind, m.text);
      else if (m.type === 'sample') provide_sample(m.name, m.mono, m.rate);
      else if (m.type === 'scope') this.scopeFrames = Math.round(scope.sampleRate * SCOPE_HOLD_SECONDS);
      else forget_sample(m.name);
    } catch (e) {
      this.port.postMessage({ type: 'error', message: String(e) } satisfies FromWorklet);
    }
  }

  process(_inputs: Float32Array[][], outputs: Float32Array[][]): boolean {
    const [left, right] = outputs[0] ?? [];
    if (!left) return true;
    const frames = left.length;
    const started = now();
    // The engine renders at most 1024 frames a call; larger blocks take several.
    let done = 0;
    while (done < frames) {
      const n = this.engine.process(frames - done);
      if (n === 0) break;
      // Growing the module's memory detaches the old view; the buffer is rebuilt only then.
      if (this.view.buffer !== this.memory.buffer) this.view = new Float32Array(this.memory.buffer, this.pointer, 2048);
      const v = this.view;
      for (let i = 0; i < n; i++) {
        left[done + i] = v[2 * i] ?? 0;
        if (right) right[done + i] = v[2 * i + 1] ?? 0;
      }
      done += n;
    }
    this.engine.record_load(now() - started, frames);
    this.since += frames;
    this.scopeFrames = Math.max(0, this.scopeFrames - frames);
    if (this.since >= this.every) {
      this.since = 0;
      this.port.postMessage({ type: 'levels', v: Array.from(this.engine.levels()) } satisfies FromWorklet);
      if (this.scopeFrames > 0) {
        const samples = this.engine.scope();
        this.port.postMessage({ type: 'scope', samples, rate: scope.sampleRate } satisfies FromWorklet, [samples.buffer]);
      }
    }
    return true;
  }
}

scope.registerProcessor('patchplay', Patchplay);
