// The UI's single source of truth: the last `State` the backend sent, plus
// the little that is only about looking (view, explain, mate, console log).
// Every change to the patch is a `Command` sent through `send`.
import type { Api } from './api';
import { readStored } from './storage';
import type { Command, Completions, Description, Meter, ModuleDesc, Patch, PresetInfo, State, StyleInfo } from './types';
import { messageOf } from './util';

export type View = 'bench' | 'catalogue' | 'starting-points' | 'docs';

export interface LogLine {
  src: string;
  text: string;
  kind: 'cmd' | 'err';
}

export interface Notice {
  text: string;
  error: boolean;
}

export interface App {
  api: Api;
  state: State | null;
  /** A previewed patch (a morph in progress) drawn instead of the real one. */
  preview: Patch | null;
  desc: Description | null;
  descs: Map<string, ModuleDesc>;
  styles: StyleInfo[];
  presets: PresetInfo[];
  /** The lineage node chosen as Breed's other parent. */
  mate: number | null;
  explain: boolean;
  view: View;
  log: LogLine[];
  notice: Notice | null;
  meter: Meter;
  /** What the code editor completes from; loaded once at boot. */
  completions: Completions | null;
  /** Beginner mode: dragging a module over another suggests cables, and dropping wires it. */
  beginner: boolean;
  /** The code editor is open in its own window. */
  popout: boolean;
}

export const app = {
  state: null,
  preview: null,
  desc: null,
  descs: new Map(),
  styles: [],
  presets: [],
  mate: null,
  explain: false,
  view: 'bench',
  log: [],
  notice: null,
  meter: { peak_l: 0, peak_r: 0, load: 0 },
  completions: null,
  beginner: readStored('kerf.beginner') !== 'off',
  popout: false,
} as Omit<App, 'api'> as App;

const listeners = new Set<() => void>();
/** Called after this window changed the backend state, so other windows can follow. */
export const onCommitted = new Set<() => void>();
export const subscribe = (fn: () => void): void => void listeners.add(fn);
export const changed = (): void => listeners.forEach((fn) => fn());

/** The patch to draw: the preview while a morph is dragged, else the real one. */
export function patch(): Patch {
  const p = app.preview ?? app.state?.patch;
  if (!p) throw new Error('no state yet');
  return p;
}

let noticeTimer: ReturnType<typeof setTimeout> | undefined;
export function notify(text: string, error = false): void {
  app.notice = { text, error };
  changed();
  clearTimeout(noticeTimer);
  noticeTimer = setTimeout(() => {
    app.notice = null;
    changed();
  }, error ? 6000 : 3000);
}

function record(src: string, text: string, kind: LogLine['kind']): void {
  app.log.push({ src, text, kind });
  if (app.log.length > 300) app.log.shift();
}

/** Runs a backend call that yields a new `State`; shows and logs a rejection instead of throwing. Only the newest call's answer is applied, so answers that arrive out of order cannot roll the state back. */
let issued = 0;
export async function run(what: string, call: Promise<State>, src = 'gui'): Promise<boolean> {
  const ticket = ++issued;
  try {
    const state = await call;
    // A newer command has been sent since: its answer is the fresher state, so this one is stale.
    if (ticket !== issued) return true;
    app.state = state;
    app.preview = null;
    if (app.mate !== null && !app.state.lineage.nodes.some((n) => n.id === app.mate)) app.mate = null;
    changed();
    onCommitted.forEach((fn) => fn());
    return true;
  } catch (e) {
    record(src, `✗ ${what}: ${messageOf(e)}`, 'err');
    notify(messageOf(e), true);
    return false;
  }
}

const fingerprint = (s: State): string =>
  `${s.text}|${s.lineage.current}|${s.lineage.nodes.length}|${s.can_undo}|${s.can_redo}|${s.playing}|${s.path}`;

/** Takes a state another window produced; redraws only when something differs. */
export function adoptState(s: State): void {
  if (app.state && fingerprint(app.state) === fingerprint(s)) return;
  app.state = s;
  app.preview = null;
  changed();
}

/** Sends one command, as the GUI, the CLI and agents all do. */
export function send(cmd: Command, src = 'gui'): Promise<boolean> {
  record(src, JSON.stringify(cmd), 'cmd');
  return run(cmd.op, app.api.apply(cmd), src);
}

let pending: Command | null = null;
let inFlight = false;

/** Tries `cmd` on a scratch copy, one request at a time with the latest winning. */
export function tryOut(cmd: Command, onPatch?: (p: Patch) => void): void {
  pending = cmd;
  if (inFlight) return;
  inFlight = true;
  void (async () => {
    while (pending) {
      const next = pending;
      pending = null;
      try {
        const p = await app.api.preview(next);
        onPatch?.(p);
      } catch (e) {
        notify(messageOf(e), true);
        pending = null;
      }
    }
    inFlight = false;
  })();
}

export function moduleValue(patchValue: Patch, id: string, key: string): number | undefined {
  return patchValue.modules.find((m) => m.id === id)?.params.find((p) => p.key === key)?.value;
}
