// localStorage that never throws. A browser with storage blocked (Safari's "Block all cookies",
// a sandboxed frame) throws a SecurityError on any access, even `typeof localStorage`, and a
// full or private-mode store throws on write. Nothing kept here is worth failing for: a choice
// just does not survive a reload.

export function readStored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function writeStored(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Storage is blocked or full; the value will not survive a reload.
  }
}

export function forgetStored(key: string): void {
  try {
    localStorage.removeItem(key);
  } catch {
    // Nothing to forget in a store that cannot be read.
  }
}
