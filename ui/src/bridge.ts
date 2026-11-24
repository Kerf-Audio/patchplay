// The one door to the backend. In the desktop app calls go through Tauri's
// `invoke`; in a browser they are `POST /api/<name>` to `kerf-app --serve`; in the static web
// build the engine runs in WebAssembly in the page (src/web/). All three take the same
// arguments, so nothing else in the UI knows which it is.
import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import { messageOf } from './util';
import { webBridge } from './web';

export type PathKind = 'open-kerf' | 'save-kerf' | 'save-wav' | 'open-sample';

export interface Bridge {
  call<T>(name: string, args?: Record<string, unknown>): Promise<T>;
  /** Asks the user for a file. `null` when they cancel. */
  pickPath(kind: PathKind, suggested: string): Promise<string | null>;
}

export class HttpBridge implements Bridge {
  async call<T>(name: string, args: Record<string, unknown> = {}): Promise<T> {
    const res = await fetch(`/api/${name}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(args),
    });
    const body: unknown = await res.json().catch(() => null);
    if (!res.ok) {
      const error = (body as { error?: string } | null)?.error;
      throw new Error(error ?? `${name}: HTTP ${res.status}`);
    }
    return body as T;
  }

  async pickPath(kind: PathKind, suggested: string): Promise<string | null> {
    const what =
      kind === 'open-kerf'
        ? 'Open which .kerf file?'
        : kind === 'open-sample'
          ? 'Path of an audio file (WAV, AIFF, FLAC, MP3 or Ogg)?'
          : 'Save to which path?';
    return window.prompt(what, suggested)?.trim() || null;
  }
}

const KERF = [{ name: 'Kerf patch', extensions: ['kerf'] }];
const WAV = [{ name: 'WAV audio', extensions: ['wav'] }];
const AUDIO = [{ name: 'Audio', extensions: ['wav', 'aif', 'aiff', 'flac', 'mp3', 'ogg', 'oga'] }];

export class TauriBridge implements Bridge {
  async call<T>(name: string, args: Record<string, unknown> = {}): Promise<T> {
    try {
      return await invoke<T>(name, args);
    } catch (e) {
      throw new Error(messageOf(e));
    }
  }

  async pickPath(kind: PathKind, suggested: string): Promise<string | null> {
    if (kind === 'open-kerf') {
      const picked = await open({ filters: KERF });
      return typeof picked === 'string' ? picked : null;
    }
    if (kind === 'open-sample') {
      const picked = await open({ filters: AUDIO });
      return typeof picked === 'string' ? picked : null;
    }
    return save({
      defaultPath: suggested,
      filters: kind === 'save-wav' ? WAV : KERF,
    });
  }
}

/**
 * Desktop: Tauri. Static web build (`VITE_TARGET=web`): the engine in WebAssembly, in this page.
 * Otherwise a browser talking to `kerf-app --serve` (what `npm run dev` proxies to).
 */
export const createBridge = (): Bridge => {
  if ('__TAURI_INTERNALS__' in window) return new TauriBridge();
  return __WEB__ ? webBridge() : new HttpBridge();
};
