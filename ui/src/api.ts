// Typed wrappers: one function per backend command, same names as the
// Tauri commands and the HTTP endpoints.
import type { Bridge, PathKind } from './bridge';
import type {
  CheckResult,
  Command,
  Completions,
  Description,
  CableSuggestion,
  LanguageDocs,
  Meter,
  Scope,
  Patch,
  PresetInfo,
  RenderSummary,
  SamplePeaks,
  State,
  StyleInfo,
} from './types';

export interface Api {
  getState(): Promise<State>;
  describe(): Promise<Description>;
  listPresets(): Promise<PresetInfo[]>;
  listStyles(): Promise<StyleInfo[]>;
  apply(cmd: Command): Promise<State>;
  /** The patch `cmd` would give, without changing the session or its undo history. */
  preview(cmd: Command): Promise<Patch>;
  openFile(path: string): Promise<State>;
  /** Saves to `path`, or to the open file when omitted. */
  saveFile(path?: string): Promise<State>;
  play(): Promise<State>;
  stop(): Promise<State>;
  meter(): Promise<Meter>;
  /** The master output's latest samples, for the footer scope; costs the audio thread nothing. */
  scope(): Promise<Scope>;
  renderWav(path: string, seconds: number): Promise<RenderSummary>;
  /** Parses `.kerf` text without applying it; diagnostics carry line and column. */
  checkText(text: string): Promise<CheckResult>;
  /** Everything the editor completes from: grammar keywords, kinds with params and jacks, styles, keys. */
  completions(): Promise<Completions>;
  /** The format guide and the generated module reference, as markdown sections. */
  languageDocs(): Promise<LanguageDocs>;
  /** Cables that would suit a module placed next to `near`; `new.<jack>` stands for a module not yet added. */
  suggestCables(q: { kind?: string; module?: string; near: string }): Promise<{ cables: CableSuggestion[] }>;
  /** The Veneer's waveform (its sample, or the demo sample). */
  samplePeaks(module: string): Promise<SamplePeaks>;
  /** The built-in demo sample's waveform, for a Veneer that is not in a patch (the catalogue). */
  demoSamplePeaks(): Promise<SamplePeaks>;
  /** Asks the user for a file path; null when they cancel. */
  pickPath(kind: PathKind, suggested: string): Promise<string | null>;
}

export function makeApi(b: Bridge): Api {
  return {
    getState: () => b.call('get_state'),
    describe: () => b.call('describe'),
    listPresets: () => b.call('list_presets'),
    listStyles: () => b.call('list_styles'),
    apply: (cmd) => b.call('apply', { cmd }),
    preview: (cmd) => b.call('preview', { cmd }),
    openFile: (path) => b.call('open_file', { path }),
    saveFile: (path) => b.call('save_file', path ? { path } : {}),
    play: () => b.call('transport_play'),
    stop: () => b.call('transport_stop'),
    meter: () => b.call('meter'),
    scope: () => b.call('scope'),
    renderWav: (path, seconds) => b.call('render_wav', { path, seconds }),
    samplePeaks: (module) => b.call('sample_peaks', { module }),
    demoSamplePeaks: () => b.call('demo_sample_peaks'),
    checkText: (text) => b.call('check_text', { text }),
    completions: () => b.call('completions'),
    languageDocs: () => b.call('language_docs'),
    suggestCables: (q) => b.call('suggest_cables', q),
    pickPath: (kind, suggested) => b.pickPath(kind, suggested),
  };
}
