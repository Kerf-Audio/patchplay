// The pop-out window: the code editor, an error list under it and the docs in
// a side panel, in a compact layout. It loads this same app with
// `?view=editor` and talks to the same backend; the main window follows along.
import { makeApi } from './api';
import { createBridge } from './bridge';
import { mountDocs } from './docs';
import { initEditorBridge, setDocsOpener } from './editor-bridge';
import { mountEditorPanel } from './editor-panel';
import { initPopoutWindow } from './popout';
import { app, changed, subscribe } from './store';
import { initSync } from './sync';
import { $ } from './util';

export async function bootEditorWindow(): Promise<void> {
  app.api = makeApi(createBridge());
  app.state = await app.api.getState();
  document.title = 'patchplay: code';
  document.body.className = 'editor-window';
  document.body.innerHTML = `<div class="ew">
    <div class="ew-main" id="ew-editor"></div>
    <aside class="ew-docs hidden" id="ew-docs"></aside>
  </div><div id="toast" class="hidden"></div>`;
  initSync();
  initPopoutWindow();
  initEditorBridge();
  const docsHost = $('#ew-docs');
  const docs = mountDocs(docsHost);
  const toggleDocs = (force?: boolean): void => {
    docsHost.classList.toggle('hidden', force === undefined ? !docsHost.classList.contains('hidden') : !force);
  };
  setDocsOpener((term) => {
    toggleDocs(true);
    docs.openTerm(term);
  });
  mountEditorPanel($('#ew-editor'), { toggleDocs: () => toggleDocs() });
  subscribe(renderToast);
  changed();
}

function renderToast(): void {
  const el = $('#toast');
  el.textContent = app.notice?.text ?? '';
  el.classList.toggle('hidden', !app.notice);
  el.classList.toggle('err', !!app.notice?.error);
}
