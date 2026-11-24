// How the rest of the UI reaches "the" code editor, wherever it lives: in this
// window, or in the pop-out (then requests travel over the sync channel).
import type { KerfEditor } from './kerf-editor';
import type { InsertMode } from './kerf-syntax';
import { app } from './store';
import { isEditorWindow, onMessage, post } from './sync';

let local: KerfEditor | null = null;
let reveal: (() => void) | null = null;
let docsOpener: ((term: string) => void) | null = null;

/** The window's editor registers itself; `show` brings it into view (the footer tab, say). */
export function registerEditor(editor: KerfEditor | null, show: () => void = () => undefined): void {
  local = editor;
  reveal = show;
}

/** Puts code into the editor: at the cursor, over everything, or at the end. Nothing is applied until the user does. */
export function insertIntoEditor(text: string, mode: InsertMode): void {
  if (app.popout && !isEditorWindow) {
    post({ type: 'insert', text, mode });
    return;
  }
  reveal?.();
  local?.insert(text, mode);
}

export function initEditorBridge(): void {
  if (!isEditorWindow) return;
  onMessage((m) => {
    if (m.type === 'insert') local?.insert(m.text, m.mode);
  });
}

/** Each window says where its docs live; hover and completion details call `openDocs`. */
export const setDocsOpener = (fn: (term: string) => void): void => {
  docsOpener = fn;
};

export const openDocs = (term: string): void => docsOpener?.(term);
