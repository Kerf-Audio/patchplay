// Opening the code editor in its own window. On the desktop that is a Tauri
// webview window loading this same app with `?view=editor`; in a browser it
// is a same-origin `window.open`. Closing it brings the editor back in-page.
import { getCurrentWebviewWindow, WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { app, changed, notify } from './store';
import { isEditorWindow, onMessage, post } from './sync';
import { messageOf } from './util';

const LABEL = 'editor';
const URL_QUERY = 'view=editor';
const inTauri = (): boolean => '__TAURI_INTERNALS__' in window;

let browserWindow: Window | null = null;
let watcher = 0;

function setOpen(open: boolean): void {
  if (app.popout === open) return;
  app.popout = open;
  changed();
}

async function openTauri(): Promise<void> {
  const existing = await WebviewWindow.getByLabel(LABEL);
  if (existing) {
    await existing.setFocus();
    setOpen(true);
    return;
  }
  const win = new WebviewWindow(LABEL, {
    url: `index.html?${URL_QUERY}`,
    title: 'patchplay: code',
    width: 960,
    height: 720,
    minWidth: 560,
    minHeight: 360,
  });
  void win.once('tauri://created', () => setOpen(true));
  void win.once('tauri://error', (e) => notify(`Could not open the editor window: ${messageOf(e.payload)}`, true));
  void win.once('tauri://destroyed', () => setOpen(false));
}

function openBrowser(): void {
  if (browserWindow && !browserWindow.closed) {
    browserWindow.focus();
    return;
  }
  browserWindow = window.open(`${location.pathname}?${URL_QUERY}`, 'kerf-editor', 'popup,width=960,height=720');
  if (!browserWindow) {
    notify('The browser blocked the pop-up. Allow pop-ups for this page and try again.', true);
    changed();
    return;
  }
  setOpen(true);
  window.clearInterval(watcher);
  watcher = window.setInterval(() => {
    if (!browserWindow || browserWindow.closed) {
      window.clearInterval(watcher);
      browserWindow = null;
      setOpen(false);
    }
  }, 700);
}

export async function openPopout(): Promise<void> {
  try {
    if (inTauri()) await openTauri();
    else openBrowser();
  } catch (e) {
    notify(messageOf(e), true);
    changed();
  }
}

/** Closes the pop-out from the main window, so the editor returns in-page. */
export function closePopout(): void {
  post({ type: 'close-editor' });
  browserWindow?.close();
  setOpen(false);
}

export async function closeSelf(): Promise<void> {
  if (inTauri()) await getCurrentWebviewWindow().close();
  else window.close();
}

/** Wires the main window to hear about the pop-out opening and closing. */
export function initPopoutMain(): void {
  onMessage((m) => {
    if (m.type === 'popout') setOpen(m.open);
  });
  post({ type: 'popout-ping' });
}

/** Wires the pop-out window: it announces itself, answers pings and closes on request. */
export function initPopoutWindow(): void {
  if (!isEditorWindow) return;
  post({ type: 'popout', open: true });
  addEventListener('pagehide', () => post({ type: 'popout', open: false }));
  onMessage((m) => {
    if (m.type === 'popout-ping') post({ type: 'popout', open: true });
    if (m.type === 'close-editor') void closeSelf();
  });
}
