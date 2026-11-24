// The third backend: the engine and the session run in this page, as WebAssembly. Commands are
// the desktop's own (`kerf-bridge`); what a desktop does with files and dialogs is done with
// pseudo paths here. `preset:<name>` is a factory starting point, `web:<name>` is a file the
// user chose or saved. Nothing leaves the browser.
import init, {
  call,
  forget_sample,
  provide_sample,
  render_summary,
  render_wav,
} from '@kerf-web/kerf_web.js';
import wasmUrl from '@kerf-web/kerf_web_bg.wasm?url';
import type { Bridge, PathKind } from '../bridge';
import { forgetStored, readStored, writeStored } from '../storage';
import type { RenderSummary, State } from '../types';
import { baseName } from '../util';
import { WebAudio } from './audio';
import { chooseFile, download, pickSaveTarget, type SaveTarget, withExtension } from './files';
import { PREFIX, SampleStore } from './samples';
import { serveRemote } from './remote';

const SESSION = 'patchplay:session';
/** Commands whose answer is a `State` worth keeping for the next visit. */
const READS = new Set(['get_state', 'meter', 'describe', 'check_text']);

const asJson = (args: unknown): string => JSON.stringify(args ?? {});
const stripped = (path: string): string => (path.startsWith(PREFIX) ? path.slice(PREFIX.length) : path);

export class WasmBridge implements Bridge {
  /** Text of the file the user just chose, waiting for `open_file` to ask for it. */
  private readonly chosen = new Map<string, string>();
  /** Files picked in a save dialog, by pseudo path, so Save writes to the same file again. */
  private readonly targets = new Map<string, SaveTarget>();
  private saving = 0;

  constructor(
    private readonly audio: WebAudio,
    private readonly store: SampleStore,
  ) {}

  async call<T>(name: string, args: Record<string, unknown> = {}): Promise<T> {
    const result = await this.run(name, args);
    await this.audio.flush();
    if (!READS.has(name) && result && typeof result === 'object' && 'text' in result) this.remember(result as State);
    return result as T;
  }

  private async run(name: string, args: Record<string, unknown>): Promise<unknown> {
    switch (name) {
      case 'open_file':
        return this.open(String(args.path));
      case 'save_file':
        return this.save(typeof args.path === 'string' ? args.path : undefined);
      case 'render_wav':
        return this.render(String(args.path), Number(args.seconds));
      default:
        if (name === 'scope') this.audio.wantScope();
        return JSON.parse(call(name, asJson(args)));
    }
  }

  private open(path: string): State {
    if (path.startsWith('preset:')) return JSON.parse(call('open_preset', asJson({ name: path.slice(7) })));
    const text = this.chosen.get(path);
    if (text === undefined) throw new Error(`${baseName(path)} is not open here: choose it with Open.`);
    this.chosen.delete(path);
    return JSON.parse(call('open_text', asJson({ name: stripped(path), text })));
  }

  private async deliver(path: string, data: BlobPart, type: string): Promise<void> {
    const target = this.targets.get(path);
    if (target) await target.write(data);
    else download(baseName(path), data, type);
  }

  private async save(path?: string): Promise<State> {
    const state: State = JSON.parse(call('get_state', '{}'));
    const target = path ?? (state.path ? PREFIX + state.path : null);
    if (!target) throw new Error('Save needs a file name.');
    await this.deliver(target, state.text, 'text/plain');
    return path ? JSON.parse(call('set_path', asJson({ path: stripped(path) }))) : state;
  }

  private async render(path: string, seconds: number): Promise<RenderSummary> {
    const bytes = render_wav(seconds);
    const summary: RenderSummary = JSON.parse(render_summary());
    await this.deliver(path, bytes, 'audio/wav');
    return { ...summary, path };
  }

  private async saveDialog(suggested: string, description: string, mime: string, ext: string): Promise<string | null> {
    const name = withExtension(baseName(suggested), ext);
    const target = await pickSaveTarget(name, description, mime, ext);
    if (target === null) return null;
    const path = PREFIX + name;
    if (target) this.targets.set(path, target);
    else this.targets.delete(path);
    return path;
  }

  async pickPath(kind: PathKind, suggested: string): Promise<string | null> {
    if (kind === 'save-kerf') return this.saveDialog(suggested || 'patch.kerf', 'Kerf patch', 'text/plain', 'kerf');
    if (kind === 'save-wav') return this.saveDialog(suggested || 'patchplay.wav', 'WAV audio', 'audio/wav', 'wav');
    if (kind === 'open-kerf') {
      const file = await chooseFile('.kerf,.txt,text/plain');
      if (!file) return null;
      this.targets.delete(PREFIX + file.name);
      this.chosen.set(PREFIX + file.name, await file.text());
      return PREFIX + file.name;
    }
    const file = await chooseFile('audio/*,.wav,.aif,.aiff,.flac,.mp3,.ogg,.oga');
    return file ? this.adopt(file) : null;
  }

  /** Decodes an audio file for the engine (here and in the worklet); answers its pseudo path. */
  async adopt(file: File): Promise<string> {
    const { path, pcm } = await this.store.add(file);
    provide_sample(path, pcm.mono, pcm.rate);
    this.audio.sample(path, pcm);
    return path;
  }

  /** Writes the session to localStorage soon after the last change. */
  private remember(state: State): void {
    clearTimeout(this.saving);
    this.saving = window.setTimeout(() => {
      writeStored(SESSION, JSON.stringify({ text: state.text, path: state.path }));
    }, 400);
  }

  /** Reopens the session left by the last visit, if any. */
  restore(): void {
    try {
      const saved: unknown = JSON.parse(readStored(SESSION) ?? 'null');
      if (saved && typeof saved === 'object' && 'text' in saved && typeof saved.text === 'string') {
        const path = 'path' in saved && typeof saved.path === 'string' ? saved.path : 'session.kerf';
        call('open_text', asJson({ name: path, text: saved.text }));
        if (!('path' in saved) || typeof saved.path !== 'string') call('set_path', asJson({}));
      }
    } catch {
      forgetStored(SESSION);
    }
  }

  forget(name: string): void {
    forget_sample(name);
  }
}

export async function createWasmBridge(): Promise<WasmBridge> {
  const bytes = await (await fetch(wasmUrl)).arrayBuffer();
  await init({ module_or_path: bytes });
  const store = await SampleStore.open();
  for (const [name, pcm] of store.all()) provide_sample(name, pcm.mono, pcm.rate);
  const bridge = new WasmBridge(new WebAudio(bytes, () => store.all()), store);
  bridge.restore();
  serveRemote(bridge);
  return bridge;
}
