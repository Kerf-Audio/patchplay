// The footer: scenes with the morph fader, and the console. The console shows
// every command as it is sent and accepts the same JSON a script or agent would.
import { app, changed, notify, patch, send, tryOut } from './store';
import type { Command, SceneId } from './types';
import { $, $$, esc, messageOf } from './util';

const SCENES: SceneId[] = ['A', 'B', 'C', 'D'];

export function renderFooter(): void {
  const scenes = patch().scenes;
  $('#scenes').innerHTML = SCENES.map((k) => {
    const full = !!scenes[k];
    return `<button class="scene${full ? ' full' : ''}" data-scene="${k}" data-tip-title="Scene ${k}"
      data-tip="${full ? 'Click to recall. Right-click to clear.' : 'Empty. Click to store every knob as it is now.'}">${k}</button>`;
  }).join('');
  const log = $('#log');
  log.innerHTML = app.log
    .map((l) => `<span class="${l.kind === 'err' ? 'err' : ''}"><span class="src">${esc(l.src)}</span>${l.kind === 'cmd' ? '› ' : '  '}${esc(l.text)}</span>`)
    .join('\n');
  log.scrollTop = log.scrollHeight;
}

function parseLine(line: string): Command {
  const word = line.trim();
  if (word === 'undo' || word === 'redo') return { op: word };
  const cmd: unknown = JSON.parse(word);
  if (typeof cmd !== 'object' || cmd === null || typeof (cmd as Command).op !== 'string') {
    throw new Error('a command is a JSON object with an "op"');
  }
  return cmd as Command;
}

function morph(e: Event, commit: boolean): void {
  const scenes = patch().scenes;
  const slider = e.target as HTMLInputElement;
  if (!scenes.A || !scenes.B) {
    if (commit) notify('Store scenes A and B first', true);
    return;
  }
  const cmd: Command = { op: 'morph', a: 'A', b: 'B', t: Number(slider.value) };
  if (commit) void send(cmd);
  else {
    tryOut(cmd, (p) => {
      app.preview = p;
      changed();
    });
  }
}

export function initFooter(): void {
  $('#scenes').addEventListener('click', (e) => {
    const k = (e.target as HTMLElement).closest<HTMLElement>('[data-scene]')?.dataset.scene as SceneId | undefined;
    if (!k) return;
    void send(patch().scenes[k] ? { op: 'morph', a: k, b: k, t: 0 } : { op: 'store_scene', scene: k });
  });
  $('#scenes').addEventListener('contextmenu', (e) => {
    const k = (e.target as HTMLElement).closest<HTMLElement>('[data-scene]')?.dataset.scene as SceneId | undefined;
    if (k && patch().scenes[k]) {
      e.preventDefault();
      void send({ op: 'clear_scene', scene: k });
    }
  });
  const slider = $('#morph');
  slider.addEventListener('input', (e) => morph(e, false));
  slider.addEventListener('change', (e) => morph(e, true));
  $$('[data-con]').forEach((b) =>
    b.addEventListener('click', () => {
      $$('[data-con]').forEach((x) => {
        x.classList.toggle('on', x === b);
        x.setAttribute('aria-pressed', String(x === b));
      });
      $('#log').classList.toggle('hidden', b.dataset.con !== 'log');
      $('#editor-slot').classList.toggle('hidden', b.dataset.con !== 'text');
      $('footer').classList.toggle('tall', b.dataset.con === 'text');
    }),
  );
  $<HTMLInputElement>('#cmd').addEventListener('keydown', (e) => {
    const box = e.target as HTMLInputElement;
    if (e.key !== 'Enter' || !box.value.trim()) return;
    try {
      void send(parseLine(box.value), 'cli');
      box.value = '';
    } catch (err) {
      notify(messageOf(err), true);
    }
  });
}
