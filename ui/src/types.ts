// The shapes the backend sends. They mirror kerf-core's `Patch` and `ModuleDescriptor`
// and kerf-app's `State`, so nothing is translated on the way in.

export type BandId = 'time' | 'motion' | 'voices' | 'shape' | 'space' | 'out';
export type JackKind = 'audio' | 'cv' | 'pitch' | 'gate' | 'clock';
export type SceneId = 'A' | 'B' | 'C' | 'D';

export interface ParamDesc {
  key: string;
  label: string;
  default: number;
  tooltip: string;
}

export interface JackDesc {
  key: string;
  label: string;
  kind: JackKind;
  tooltip: string;
}

export interface ModuleDesc {
  kind: string;
  band: BandId;
  hp: number;
  summary: string;
  params: ParamDesc[];
  inputs: JackDesc[];
  outputs: JackDesc[];
}

export interface Description {
  bands: { id: BandId; name: string; summary: string }[];
  jack_kinds: { id: JackKind; label: string; tooltip: string }[];
  modules: ModuleDesc[];
}

export interface ParamValue {
  key: string;
  value: number;
  locked: boolean;
}

export interface Module {
  id: string;
  kind: string;
  part: string | null;
  params: ParamValue[];
  locked: boolean;
  /** The audio file a Veneer plays; absent for the demo sample. */
  sample?: string;
}

export interface Part {
  id: string;
  name: string;
  colour: string;
  locked: boolean;
  muted: boolean;
  /** Part level, 0..1 with 1 as unity; older backends omit it. */
  level?: number;
}

export interface Cable {
  from: string;
  to: string;
}

export interface SceneValue {
  module: string;
  param: string;
  value: number;
}

export interface Patch {
  tempo: number;
  key: string | null;
  style: { name: string; seed: number } | null;
  parts: Part[];
  modules: Module[];
  cables: Cable[];
  scenes: Partial<Record<SceneId, SceneValue[]>>;
}

export type Origin =
  | { kind: 'root' | 'edited' | 'imported' }
  | { kind: 'random'; style: string; seed: number }
  | { kind: 'evolve'; strength: number; seed: number }
  | { kind: 'breed'; seed: number };

export interface LineageNode {
  id: number;
  parents: number[];
  origin: Origin;
  /** `.kerf` text. */
  patch: string;
}

export interface Lineage {
  nodes: LineageNode[];
  current: number;
}

export interface State {
  patch: Patch;
  text: string;
  can_undo: boolean;
  can_redo: boolean;
  path: string | null;
  playing: boolean;
  lineage: Lineage;
}

export interface StyleInfo {
  name: string;
  title: string;
  summary: string;
  tempo: [number, number];
  keys: string[];
  parts: { id: string; name: string; optional: boolean }[];
}

export interface PresetInfo {
  name: string;
  path: string;
  tempo: number;
  key: string | null;
  style: string | null;
  modules: number;
  tags: string[];
}

export interface RenderSummary {
  path: string;
  seconds: number;
  peak_db: number;
  rms_db: number;
  silent_kinds: string[];
  warnings: string[];
}

export interface Meter {
  peak_l: number;
  peak_r: number;
  /** Audio render time over audio length since the last read; 1 only just keeps up. */
  load: number;
}

/** The latest output samples (mono, oldest first) and the rate they were played at; empty when stopped. */
export interface Scope {
  rate: number;
  samples: number[];
}

export type LockTarget =
  | { kind: 'module'; module: string }
  | { kind: 'param'; module: string; param: string }
  | { kind: 'part'; part: string };

/** A command as it travels over the wire: an `op` tag plus fields. */
export type Command = { op: string; [field: string]: unknown };

/** One cable `suggest_cables` proposes; `new.<jack>` is the module being dragged. */
export interface CableSuggestion {
  from: string;
  to: string;
  reason: string;
  score: number;
}

/** What `sample_peaks` returns: the waveform the engine plays, about 256 min/max pairs. */
export interface SamplePeaks {
  peaks: [number, number][];
  seconds: number;
  slices: number;
  /** The loaded file's name; null while the demo sample is in use. */
  file: string | null;
  /** Why the demo sample is playing when a file was asked for. */
  warning: string | null;
}

/** One problem `check_text` found. `line` and `col` are 1-based; `end_col` is exclusive. */
export interface Diagnostic {
  line: number;
  col: number;
  end_col: number;
  severity: 'error' | 'warning';
  message: string;
}

export interface CheckResult {
  ok: boolean;
  diagnostics: Diagnostic[];
}

export interface KeywordDoc {
  word: string;
  /** A CodeMirror snippet template, e.g. `tempo ${1:120}`. */
  snippet?: string;
  doc: string;
}

export interface KindParamDoc {
  key: string;
  label: string;
  default: number;
  help: string;
}

export interface KindJackDoc {
  key: string;
  kind: JackKind;
  help: string;
}

export interface KindDoc {
  kind: string;
  band: BandId;
  summary: string;
  params: KindParamDoc[];
  inputs: KindJackDoc[];
  outputs: KindJackDoc[];
}

/** Everything the code editor completes from; the backend derives it from the descriptors and styles. */
export interface Completions {
  keywords: KeywordDoc[];
  kinds: KindDoc[];
  styles: { name: string; title: string; summary: string }[];
  keys: string[];
  scales: string[];
}

export interface DocSection {
  id: string;
  title: string;
  markdown: string;
}

export interface LanguageDocs {
  sections: DocSection[];
}
