// Kerf Modular mockup: inline SVG icons. One 16px grid, 1.5px strokes, drawn in currentColor.
const svg = (body) => `<svg class="ico" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;

export const ICON = {
  lock: svg('<rect x="3.5" y="7" width="9" height="6.5" rx="1.6"/><path d="M5.5 7V5.3a2.5 2.5 0 0 1 5 0V7"/>'),
  unlock: svg('<rect x="3.5" y="7" width="9" height="6.5" rx="1.6"/><path d="M5.5 7V5.3a2.5 2.5 0 0 1 4.7-1.2"/>'),
  evolve: svg('<path d="M8 13.5V8.5"/><path d="M8 9C8 6.4 6.2 4.8 2.8 4.8c0 2.8 1.7 4.2 5.2 4.2z"/><path d="M8 7.2c0-2.3 1.5-3.9 4.7-3.9 0 2.6-1.5 3.9-4.7 3.9z"/>'),
  random: svg('<rect x="2.5" y="2.5" width="11" height="11" rx="2.6"/><circle cx="5.8" cy="5.8" r=".7" fill="currentColor"/><circle cx="8" cy="8" r=".7" fill="currentColor"/><circle cx="10.2" cy="10.2" r=".7" fill="currentColor"/>'),
  breed: svg('<circle cx="5.8" cy="8" r="3.5"/><circle cx="10.2" cy="8" r="3.5"/>'),
  undo: svg('<path d="M5.5 3.2 3 5.8l2.5 2.6"/><path d="M3.4 5.8H9a3.6 3.6 0 0 1 0 7.2H6"/>'),
  explain: svg('<circle cx="7" cy="7" r="4.2"/><path d="m10.2 10.2 3.3 3.3"/>'),
  heart: svg('<path d="M8 13.3S2.7 10 2.7 6.2A2.9 2.9 0 0 1 8 4.8a2.9 2.9 0 0 1 5.3 1.4C13.3 10 8 13.3 8 13.3z"/>'),
  heartOn: svg('<path d="M8 13.3S2.7 10 2.7 6.2A2.9 2.9 0 0 1 8 4.8a2.9 2.9 0 0 1 5.3 1.4C13.3 10 8 13.3 8 13.3z" fill="currentColor"/>'),
  mute: svg('<path d="M3 6.4h2.2L8.6 3.8v8.4L5.2 9.6H3z"/><path d="m11 6.2 3 3.6M14 6.2l-3 3.6"/>'),
  play: svg('<path d="M5 3.4v9.2l7.6-4.6z" fill="currentColor"/>'),
  stop: svg('<rect x="4" y="4" width="8" height="8" rx="1.6" fill="currentColor"/>'),
  check: svg('<path d="m3.4 8.4 3 3 6.2-7"/>'),
};
