// Pop-out windows in the browser. The engine lives in the main window's page, so a second
// window (`?view=editor`) has no backend of its own: it sends each call over a
// BroadcastChannel and the main window answers. Same-origin only, which `window.open` gives.
import type { Bridge } from '../bridge';
import { messageOf } from '../util';

const CHANNEL = 'patchplay-rpc';
const TIMEOUT_MS = 5000;

type Request = { kind: 'req'; id: number; name: string; args: Record<string, unknown> };
type Response = { kind: 'res'; id: number; ok: boolean; value?: unknown; error?: string };

/** The main window: answers calls from pop-outs with its own bridge. */
export function serveRemote(local: Bridge): void {
  if (typeof BroadcastChannel === 'undefined') return;
  const channel = new BroadcastChannel(CHANNEL);
  channel.onmessage = async (e: MessageEvent<Request | Response>) => {
    const m = e.data;
    if (m.kind !== 'req') return;
    try {
      const value = await local.call(m.name, m.args);
      channel.postMessage({ kind: 'res', id: m.id, ok: true, value } satisfies Response);
    } catch (err) {
      channel.postMessage({ kind: 'res', id: m.id, ok: false, error: messageOf(err) } satisfies Response);
    }
  };
}

/** A pop-out window's bridge. */
export class RemoteBridge implements Bridge {
  private readonly channel = new BroadcastChannel(CHANNEL);
  private readonly waiting = new Map<
    number,
    { resolve: (v: unknown) => void; reject: (e: Error) => void; timer: ReturnType<typeof setTimeout> }
  >();
  private next = 1;

  constructor() {
    this.channel.onmessage = (e: MessageEvent<Request | Response>) => {
      const m = e.data;
      if (m.kind !== 'res') return;
      const w = this.waiting.get(m.id);
      if (!w) return;
      clearTimeout(w.timer);
      this.waiting.delete(m.id);
      if (m.ok) w.resolve(m.value);
      else w.reject(new Error(m.error));
    };
  }

  private ask(req: Omit<Request, 'kind' | 'id'>): Promise<unknown> {
    const id = this.next++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        if (this.waiting.delete(id)) reject(new Error('The main patchplay window did not answer. Is it still open?'));
      }, TIMEOUT_MS);
      this.waiting.set(id, { resolve, reject, timer });
      this.channel.postMessage({ kind: 'req', id, ...req } satisfies Request);
    });
  }

  call<T>(name: string, args: Record<string, unknown> = {}): Promise<T> {
    return this.ask({ name, args }) as Promise<T>;
  }

  async pickPath(): Promise<string | null> {
    return null;
  }
}
