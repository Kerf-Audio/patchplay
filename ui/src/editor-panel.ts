// The code editor with its toolbar: Apply, Revert, the unsaved marker and the
// list of problems. The same panel sits in the footer of the main window and
// fills the pop-out window. The backend's text is the truth; the editor holds
// a draft until it is applied.
import { buildCommands, filterCommands, snippetsFromDocs, type Command } from './command-palette';
import { docSections } from './docs';
import { openDocs, registerEditor } from './editor-bridge';
import { createKerfEditor, type KerfEditor } from './kerf-editor';
import { closePopout, closeSelf, openPopout } from './popout';
import { isEditorWindow } from './sync';
import { app, send, subscribe } from './store';
import type { Diagnostic } from './types';
import { $, esc } from './util';

export interface EditorPanelOptions {
  /** Brings the editor into view in its window (the footer tab, say). */
  show?: () => void;
  /** Pop-out only: toggles the docs side panel. */
  toggleDocs?: () => void;
}

export function mountEditorPanel(host: HTMLElement, opts: EditorPanelOptions = {}): void {
  const popped = isEditorWindow;
  host.innerHTML = `<div class="ed">
    <div class="ed-bar">
      <span class="ed-state" role="status"></span>
      <span class="ed-spacer"></span>
      ${popped ? '<button class="btn" data-ed="docs" data-tip="Show or hide the language docs beside the editor.">Docs</button>' : ''}
      <button class="btn ghost" data-ed="cmds" data-tip="Insert a statement, module or snippet. Opens with Ctrl-K or ⌘K.">Commands <kbd>⌘K</kbd></button>
      <button class="btn ghost" data-ed="revert" data-tip="Throw away the draft and reload the patch's text.">Revert</button>
      <button class="btn primary" data-ed="apply" data-tip="Replace the patch with this text, as one undoable step.">Apply <kbd>⌘↵</kbd></button>
      <button class="btn" data-ed="${popped ? 'back' : 'pop'}" data-tip="${
        popped ? 'Close this window and edit in the main window again.' : 'Open the editor in its own window.'
      }">${popped ? 'Bring back' : 'Pop out'}</button>
    </div>
    <div class="ed-host"></div>
    <ul class="ed-errors"></ul>
    <div class="ed-palette hidden" role="dialog" aria-label="Commands">
      <input type="search" placeholder="Insert a module, snippet or statement" aria-label="Filter commands">
      <ul role="listbox"></ul>
    </div>
    <div class="ed-away hidden"><p>The editor is open in its own window.</p><button class="btn" data-ed="back">Bring it back</button></div>
  </div>`;
  const stateEl = $('.ed-state', host);
  const list = $('.ed-errors', host);
  const apply = $<HTMLButtonElement>('[data-ed=apply]', host);
  const revert = $<HTMLButtonElement>('[data-ed=revert]', host);
  let baseline = app.state?.text ?? '';
  let problems: Diagnostic[] = [];
  let busy = false;

  const dirty = (): boolean => editor.getText() !== baseline;
  const stale = (): boolean => (app.state?.text ?? '') !== baseline;

  const paint = (): void => {
    const errors = problems.filter((d) => d.severity === 'error').length;
    const d = dirty();
    stateEl.className = `ed-state ${d ? (stale() ? 'stale' : 'dirty') : 'clean'}`;
    stateEl.textContent = d
      ? stale()
        ? 'Draft, and the patch changed elsewhere. Apply overwrites it; Revert loads it.'
        : errors
          ? `Draft with ${errors} error${errors === 1 ? '' : 's'}`
          : 'Unapplied changes'
      : stale()
        ? 'The patch changed: Revert to load it'
        : 'In sync with the patch';
    apply.disabled = busy || !d || errors > 0;
    revert.disabled = !d && !stale();
    list.innerHTML = problems
      .map(
        (p, i) =>
          `<li class="${p.severity}" data-i="${i}"><b>${p.line}:${p.col}</b> ${esc(p.message)}</li>`,
      )
      .join('');
  };

  const load = (): void => {
    baseline = app.state?.text ?? '';
    if (editor.getText() !== baseline) editor.setText(baseline);
    paint();
  };

  const doApply = async (): Promise<void> => {
    if (!dirty() || busy) return;
    busy = true;
    paint();
    const ok = await send({ op: 'replace_text', text: editor.getText() });
    busy = false;
    if (ok) load();
    else paint();
  };

  const editor: KerfEditor = createKerfEditor($('.ed-host', host), baseline, {
    completions: () => app.completions,
    checkText: async (text) => (await app.api.checkText(text)).diagnostics,
    onChange: paint,
    onApply: () => void doApply(),
    onDiagnostics: (d) => {
      problems = d;
      paint();
    },
    openDocs,
    openCommands: () => void openPalette(),
  });
  registerEditor(editor, opts.show);

  const palette = $('.ed-palette', host);
  const paletteInput = $<HTMLInputElement>('input', palette);
  const paletteList = $('ul', palette);
  let shown: Command[] = [];
  let all: Command[] = [];
  let active = 0;

  const paintPalette = (): void => {
    shown = filterCommands(all, paletteInput.value);
    active = Math.min(active, Math.max(shown.length - 1, 0));
    paletteList.innerHTML = shown.length
      ? shown
          .map(
            (c, i) =>
              `<li role="option" data-i="${i}" class="${i === active ? 'on' : ''}"><span class="pal-group">${esc(c.group)}</span><b>${esc(c.label)}</b><span class="pal-detail">${esc(c.detail)}</span></li>`,
          )
          .join('')
      : '<li class="empty">No command matches.</li>';
    paletteList.querySelector('.on')?.scrollIntoView({ block: 'nearest' });
  };
  const closePalette = (): void => {
    palette.classList.add('hidden');
    editor.focus();
  };
  const runCommand = (c: Command | undefined): void => {
    if (!c) return;
    palette.classList.add('hidden');
    editor.insert(c.text, c.mode, c.complete);
  };
  async function openPalette(): Promise<void> {
    const docs = await docSections().catch(() => []);
    all = buildCommands(app.completions, snippetsFromDocs(docs), editor.getText());
    paletteInput.value = '';
    active = 0;
    palette.classList.remove('hidden');
    paintPalette();
    paletteInput.focus();
  }
  paletteInput.addEventListener('input', () => {
    active = 0;
    paintPalette();
  });
  paletteInput.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') closePalette();
    else if (e.key === 'Enter') runCommand(shown[active]);
    else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      active = Math.min(Math.max(active + (e.key === 'ArrowDown' ? 1 : -1), 0), Math.max(shown.length - 1, 0));
      paintPalette();
    } else return;
    e.preventDefault();
  });
  paletteList.addEventListener('click', (e) =>
    runCommand(shown[Number((e.target as HTMLElement).closest<HTMLElement>('li')?.dataset.i)]),
  );
  if (!app.completions) {
    void app.api.completions().then((c) => {
      app.completions = c;
    });
  }

  host.addEventListener('click', (e) => {
    const what = (e.target as HTMLElement).closest<HTMLElement>('[data-ed]')?.dataset.ed;
    const item = (e.target as HTMLElement).closest<HTMLElement>('.ed-errors li');
    const p = problems[Number(item?.dataset.i)];
    if (what === 'apply') void doApply();
    else if (what === 'cmds') void openPalette();
    else if (what === 'revert') load();
    else if (what === 'pop') void openPopout();
    else if (what === 'back') {
      if (popped) void closeSelf();
      else closePopout();
    } else if (what === 'docs') opts.toggleDocs?.();
    else if (p) editor.goTo(p.line, p.col);
  });

  subscribe(() => {
    $('.ed-away', host).classList.toggle('hidden', popped || !app.popout);
    $('.ed-bar', host).classList.toggle('hidden', !popped && app.popout);
    $('.ed-host', host).classList.toggle('hidden', !popped && app.popout);
    $('.ed-errors', host).classList.toggle('hidden', !popped && app.popout);
    if (!dirty()) load();
    else paint();
  });
  if (popped) {
    addEventListener('beforeunload', (e) => {
      if (dirty()) e.preventDefault();
    });
  }
  paint();
}
