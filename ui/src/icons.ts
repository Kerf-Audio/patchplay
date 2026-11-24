// Inline SVG icons on one 16px grid with fine 1.25px strokes, drawn in currentColor. Simple geometric glyphs only.
const svg = (body: string): string =>
  `<svg class="ico" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;

const HEART = '<path d="M8 13.3S2.7 10 2.7 6.2A2.9 2.9 0 0 1 8 4.8a2.9 2.9 0 0 1 5.3 1.4C13.3 10 8 13.3 8 13.3z"';

export const ICON = {
  lock: svg('<rect x="3.5" y="7" width="9" height="6.5" rx="1.6"/><path d="M5.5 7V5.3a2.5 2.5 0 0 1 5 0V7"/>'),
  unlock: svg('<rect x="3.5" y="7" width="9" height="6.5" rx="1.6"/><path d="M5.5 7V5.3a2.5 2.5 0 0 1 4.7-1.2"/>'),
  evolve: svg('<path d="M8 13.5V7.8"/><path d="M8 8.8C8 6.4 6.3 4.8 3.4 4.7 3.4 7.3 5 8.8 8 8.8z"/><path d="M8 7.4C8 5 9.6 3.2 12.6 3.1 12.6 5.8 11 7.4 8 7.4z"/>'),
  random: svg('<rect x="2.5" y="2.5" width="11" height="11" rx="2.6"/><circle cx="5.8" cy="5.8" r=".7" fill="currentColor"/><circle cx="8" cy="8" r=".7" fill="currentColor"/><circle cx="10.2" cy="10.2" r=".7" fill="currentColor"/>'),
  breed: svg('<circle cx="5.8" cy="8" r="3.5"/><circle cx="10.2" cy="8" r="3.5"/>'),
  undo: svg('<path d="M5.5 3.2 3 5.8l2.5 2.6"/><path d="M3.4 5.8H9a3.6 3.6 0 0 1 0 7.2H6"/>'),
  redo: svg('<path d="M10.5 3.2 13 5.8l-2.5 2.6"/><path d="M12.6 5.8H7a3.6 3.6 0 0 0 0 7.2h3"/>'),
  explain: svg('<circle cx="8" cy="8" r="5.6"/><path d="M8 7.3v3.4"/><circle cx="8" cy="5.2" r=".7" fill="currentColor"/>'),
  heart: svg(`${HEART}/>`),
  heartOn: svg(`${HEART} fill="currentColor"/>`),
  mute: svg('<path d="M3 6.4h2.2L8.6 3.8v8.4L5.2 9.6H3z"/><path d="m11 6.2 3 3.6M14 6.2l-3 3.6"/>'),
  play: svg('<path d="M5 3.4v9.2l7.6-4.6z" fill="currentColor"/>'),
  stop: svg('<rect x="4" y="4" width="8" height="8" rx="1" fill="currentColor"/>'),
  check: svg('<path d="m3.4 8.4 3 3 6.2-7"/>'),
  open: svg('<path d="M2.5 5.5V12a1 1 0 0 0 1 1h9a1 1 0 0 0 1-1V6.5a1 1 0 0 0-1-1H8L6.8 4H3.5a1 1 0 0 0-1 1z"/>'),
  save: svg('<path d="M3 3.5h8l2 2V12.5H3z"/><path d="M5.5 3.5v3h4v-3M5.5 12.5v-3h5v3"/>'),
  wav: svg('<path d="M2 8h1.5l1.2-3.5 2 7 2-9 1.8 6.5L11 8h3"/>'),
  code: svg('<path d="M5.6 4.4 2.2 8l3.4 3.6M10.4 4.4 13.8 8l-3.4 3.6"/>'),
  plus: svg('<path d="M8 3.2v9.6M3.2 8h9.6"/>'),
  x: svg('<path d="m4 4 8 8M12 4l-8 8"/>'),
  wand: svg('<path d="m3 13 7-7"/><path d="m9.2 4.2.8-1.6.8 1.6 1.6.8-1.6.8-.8 1.6-.8-1.6-1.6-.8z" fill="currentColor"/><path d="M12.5 10.2v2.6M11.2 11.5h2.6"/>'),
  sun: svg('<circle cx="8" cy="8" r="2.7"/><path d="M8 2v1.6M8 12.4V14M2 8h1.6M12.4 8H14M3.8 3.8l1.1 1.1M11.1 11.1l1.1 1.1M12.2 3.8l-1.1 1.1M4.9 11.1l-1.1 1.1"/>'),
  moon: svg('<path d="M12.6 9.7A5.2 5.2 0 0 1 6.3 3.4a5.2 5.2 0 1 0 6.3 6.3z"/>'),
} as const;
