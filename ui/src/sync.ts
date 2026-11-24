// Keeps the main window and the code editor's pop-out window in step. The
// backend holds the one patch; windows only need to know when to re-read it.
// Messages go over a BroadcastChannel (browser) and Tauri events (desktop),
// and a slow poll covers anything either of them drops.
import { emit, listen } from '@tauri-apps/api/event';
import type { InsertMode } from './kerf-syntax';
import { adoptState, app, onCommitted } from './store';

export type Message =
  | { type: 'state' }
  | { type: 'popout'; open: boolean }
  | { type: 'popout-ping' }
  | { type: 'close-editor' }
  | { type: 'insert'; text: string; mode: InsertMode };

interface Wire {
  from: string;
  msg: Message;
}

const EVENT = 'kerf-sync';
const POLL_MS = 2000;
const me = Math.random().toString(36).slice(2);
const channel = typeof BroadcastChannel === 'undefined' ? null : new BroadcastChannel(EVENT);
const handlers = new Set<(m: Message) => void>();
const inTauri = (): boolean => '__TAURI_INTERNALS__' in window;

export const isEditorWindow = new URLSearchParams(location.search).get('view') === 'editor';

export function post(msg: Message): void {
  const wire: Wire = { from: me, msg };
  channel?.postMessage(wire);
  if (inTauri()) void emit(EVENT, wire);
}

/** Registers a handler for messages from the other window. */
export const onMessage = (fn: (m: Message) => void): void => void handlers.add(fn);

async function refresh(): Promise<void> {
  try {
    adoptState(await app.api.getState());
  } catch {
    // The backend is busy or gone; the next poll tries again.
  }
}

function receive(wire: Wire): void {
  if (wire.from === me) return;
  if (wire.msg.type === 'state') void refresh();
  handlers.forEach((fn) => fn(wire.msg));
}

export function initSync(): void {
  if (channel) channel.onmessage = (e: MessageEvent<Wire>) => receive(e.data);
  if (inTauri()) void listen<Wire>(EVENT, (e) => receive(e.payload));
  onCommitted.add(() => post({ type: 'state' }));
  setInterval(() => {
    if (isEditorWindow || app.popout) void refresh();
  }, POLL_MS);
}
