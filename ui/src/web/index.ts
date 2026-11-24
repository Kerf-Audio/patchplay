// Picks the web backend for this window and hands it out at once, before the wasm has loaded:
// calls wait for it. The main window runs the engine; pop-outs call into the main window.
import type { Bridge, PathKind } from '../bridge';
import { RemoteBridge } from './remote';
import type { WasmBridge } from './wasm-bridge';

const isPopOut = (): boolean => new URLSearchParams(location.search).get('view') === 'editor';

class Deferred implements Bridge {
  constructor(private readonly ready: Promise<Bridge>) {}

  async call<T>(name: string, args?: Record<string, unknown>): Promise<T> {
    return (await this.ready).call<T>(name, args);
  }

  async pickPath(kind: PathKind, suggested: string): Promise<string | null> {
    return (await this.ready).pickPath(kind, suggested);
  }
}

let main: Promise<WasmBridge> | undefined;

/**
 * The main window's bridge, for what only it can do (a sample dropped on a panel). `__WEB__` is
 * a build-time constant: other builds drop the import, so they need no wasm.
 */
export const mainBridge = (): Promise<WasmBridge> =>
  (main ??= __WEB__
    ? import('./wasm-bridge').then((m) => m.createWasmBridge())
    : Promise.reject(new Error('This is not the web build.')));

let shared: Bridge | undefined;

export const webBridge = (): Bridge => (shared ??= new Deferred(isPopOut() ? Promise.resolve(new RemoteBridge()) : mainBridge()));
