// Types for the wasm-bindgen output of `crates/kerf-web`, which `scripts/build-web.sh` writes to
// `src/web/pkg/` (not committed). Vite maps `@kerf-web` to that folder in every build, so this
// file is the only place the generated glue's shape is written down.

/** True in the static web build (`VITE_TARGET=web`): the engine runs in WebAssembly in the page. */
declare const __WEB__: boolean;

declare module '@kerf-web/kerf_web.js' {
  export interface Instance {
    memory: WebAssembly.Memory;
  }
  export default function init(input: { module_or_path: BufferSource | WebAssembly.Module }): Promise<Instance>;
  export function initSync(input: { module: BufferSource | WebAssembly.Module }): Instance;
  /** Runs a bridge command with its arguments as JSON; answers JSON. Throws the rejection message. */
  export function call(name: string, args: string): string;
  export function render_wav(seconds: number): Uint8Array<ArrayBuffer>;
  export function render_summary(): string;
  export function provide_sample(name: string, mono: Float32Array, rate: number): void;
  export function forget_sample(name: string): void;
  export function set_levels(peakL: number, peakR: number, load: number): void;
  /** The worklet's latest output samples (mono) and their sample rate, for the bridge's `scope`. */
  export function set_scope(samples: Float32Array, rate: number): void;
  /** The next message for the worklet as `kind\n.kerf text`; empty when there is none. */
  export function take_audio_update(): string;
  export class EngineHandle {
    constructor(sampleRate: number);
    apply(kind: string, kerfText: string): void;
    output_ptr(): number;
    /** Renders up to 1024 frames into the output buffer; answers how many. */
    process(frames: number): number;
    /** The worklet's timing of one `process` call. */
    record_load(busyMs: number, frames: number): void;
    /** peak_l, peak_r, load */
    levels(): Float64Array;
    /** The latest output samples, mono, oldest first. */
    scope(): Float32Array<ArrayBuffer>;
    free(): void;
  }
}

declare module '@kerf-web/kerf_web_bg.wasm?url' {
  const url: string;
  export default url;
}
