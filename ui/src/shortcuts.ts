// Keyboard shortcuts as a pure function from a key press to what it asks for, so what a key does
// (and where it must do nothing) is testable without a page.

export type Shortcut = 'undo' | 'redo' | 'save' | 'save-as' | 'open' | 'play' | 'random' | 'evolve' | 'explain';

export interface KeyPress {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  /** The key is being held down and has started to auto-repeat. */
  repeat: boolean;
}

/** Where the key was pressed: in a `field` that takes typing, on a `button` or link (which Space activates), or anywhere else on the `page`. */
export type KeyTarget = 'field' | 'button' | 'page';

/**
 * What a key press asks for, or `null` to leave it alone. Holding a key down repeats only undo and
 * redo, which is how scrubbing back through the history works; random, evolve and play would
 * otherwise fire again and again, each one an undo step. Random, evolve and explain work the
 * toolbar, which only the workbench shows, so they do nothing in the other views.
 */
export function shortcutFor(e: KeyPress, target: KeyTarget, onBench: boolean): Shortcut | null {
  if (target === 'field') return null;
  const k = e.key.toLowerCase();
  if (e.metaKey || e.ctrlKey) {
    if (k === 'z') return e.shiftKey ? 'redo' : 'undo';
    if (k === 'y') return 'redo';
    if (e.repeat) return null;
    if (k === 's') return e.shiftKey ? 'save-as' : 'save';
    return k === 'o' ? 'open' : null;
  }
  if (e.altKey || e.repeat) return null;
  // Space on a focused button presses that button; it is not a second play key.
  if (k === ' ') return target === 'button' ? null : 'play';
  if (!onBench) return null;
  if (k === 'r') return 'random';
  if (k === 'e') return 'evolve';
  return k === 'x' ? 'explain' : null;
}
