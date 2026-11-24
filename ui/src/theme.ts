// Day and night themes. The choice is stored in localStorage; with no choice saved, the
// system's `prefers-color-scheme` decides (first launch only). Every window follows a
// change: BroadcastChannel and the storage event in browsers, a Tauri event on the desktop.
import { emit, listen } from '@tauri-apps/api/event';
import { ICON } from './icons';
import { readStored, writeStored } from './storage';

export type Theme = 'day' | 'night';

const KEY = 'patchplay.theme';
const EVENT = 'patchplay-theme';
const channel = typeof BroadcastChannel === 'undefined' ? null : new BroadcastChannel(EVENT);
const listeners = new Set<(theme: Theme) => void>();

/** A saved choice wins; otherwise follow the system. */
export function pickTheme(saved: string | null, prefersDark: boolean): Theme {
  if (saved === 'day' || saved === 'night') return saved;
  return prefersDark ? 'night' : 'day';
}

export const currentTheme = (): Theme => (document.documentElement.dataset.theme === 'night' ? 'night' : 'day');

/** Registers a listener for theme changes; returns a function that removes it. */
export function onTheme(fn: (theme: Theme) => void): () => void {
  listeners.add(fn);
  return () => void listeners.delete(fn);
}

function apply(theme: Theme, fade: boolean): void {
  const root = document.documentElement;
  if (fade && !matchMedia('(prefers-reduced-motion: reduce)').matches) {
    root.classList.add('theming');
    setTimeout(() => root.classList.remove('theming'), 260);
  }
  root.dataset.theme = theme;
  listeners.forEach((fn) => fn(theme));
}

/** Chooses and remembers a theme, and tells the other windows. */
export function setTheme(theme: Theme): void {
  writeStored(KEY, theme);
  apply(theme, true);
  channel?.postMessage(theme);
  if ('__TAURI_INTERNALS__' in window) void emit(EVENT, theme);
}

const asTheme = (v: unknown): Theme | null => (v === 'day' || v === 'night' ? v : null);

/** Applies the starting theme and follows changes made in other windows. Call once, first. */
export function initTheme(): void {
  const dark = matchMedia('(prefers-color-scheme: dark)').matches;
  apply(pickTheme(readStored(KEY), dark), false);
  const follow = (v: unknown): void => {
    const next = asTheme(v);
    if (next && next !== currentTheme()) apply(next, true);
  };
  if (channel) channel.onmessage = (e: MessageEvent<unknown>) => follow(e.data);
  addEventListener('storage', (e) => e.key === KEY && follow(e.newValue));
  if ('__TAURI_INTERNALS__' in window) void listen<unknown>(EVENT, (e) => follow(e.payload));
}

/** Fills the header button with a sun or moon and a lowercase label, and toggles on click. */
export function mountThemeToggle(button: HTMLElement): void {
  const paint = (theme: Theme): void => {
    button.innerHTML = `${theme === 'night' ? ICON.moon : ICON.sun}${theme}`;
    button.dataset.tipTitle = `${theme} mode`;
    button.dataset.tip = `Switch to ${theme === 'night' ? 'day' : 'night'} mode. Your choice is remembered.`;
    button.setAttribute('aria-pressed', String(theme === 'night'));
  };
  paint(currentTheme());
  onTheme(paint);
  button.addEventListener('click', () => setTheme(currentTheme() === 'night' ? 'day' : 'night'));
}
