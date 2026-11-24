import './tokens.css';
import './body.css';
import './display.css';
import './editor.css';
import { makeApi } from './api';
import { createBridge } from './bridge';
import { renderBench } from './bench';
import { mountDocs } from './docs';
import { bootEditorWindow } from './editor-window';
import { initEditorBridge, setDocsOpener } from './editor-bridge';
import { mountEditorPanel } from './editor-panel';
import { initFooter, renderFooter } from './footer';
import { initInteract } from './interact';
import { initRoll } from './roll';
import { initPages, renderCatalogue, renderStartingPoints } from './pages';
import { initPalette } from './palette';
import { initPopoutMain } from './popout';
import { initPartFader } from './part-fader';
import { initSampleDrop } from './sample';
import { initScope, syncScope } from './scope';
import { initSidebar, renderLineage } from './sidebar';
import { app, changed, subscribe, type View } from './store';
import { initSync, isEditorWindow } from './sync';
import { initTheme, mountThemeToggle } from './theme';
import { initToolbar, renderToolbar } from './toolbar';
import { initTooltip } from './tooltip';
import { $, $$, bootFailureHtml } from './util';

function render(): void {
  renderToolbar();
  renderFooter();
  syncScope();
  renderLineage();
  if (app.view === 'bench') renderBench();
}

function showView(view: View): void {
  app.view = view;
  $$('nav button').forEach((b) => {
    const current = b.dataset.view === view;
    b.classList.toggle('on', current);
    b.setAttribute('aria-current', current ? 'page' : 'false');
  });
  $('#bench-scroll').classList.toggle('hidden', view !== 'bench');
  $('#catalogue').classList.toggle('hidden', view !== 'catalogue');
  $('#starting-points').classList.toggle('hidden', view !== 'starting-points');
  $('#docs').classList.toggle('hidden', view !== 'docs');
  $('#toolbar').classList.toggle('hidden', view !== 'bench');
  if (view === 'starting-points') {
    void app.api.listPresets().then((presets) => {
      app.presets = presets;
      renderStartingPoints();
    });
  }
  changed();
}

async function boot(): Promise<void> {
  app.api = makeApi(createBridge());
  const [desc, styles, presets, state] = await Promise.all([
    app.api.describe(),
    app.api.listStyles(),
    app.api.listPresets(),
    app.api.getState(),
  ]);
  Object.assign(app, { desc, styles, presets, state });
  app.descs = new Map(desc.modules.map((m) => [m.kind, m]));
  initTooltip();
  initToolbar();
  mountThemeToggle($('#btn-theme'));
  initInteract();
  initRoll();
  void initSampleDrop();
  initSidebar();
  initFooter();
  initScope();
  const docs = mountDocs($('#docs'));
  setDocsOpener((term) => {
    showView('docs');
    docs.openTerm(term);
  });
  initSync();
  initPopoutMain();
  initEditorBridge();
  mountEditorPanel($('#editor-slot'), { show: () => $('[data-con=text]').click() });
  initPages(() => showView('bench'));
  initPalette();
  initPartFader();
  $$('nav button').forEach((b) => b.addEventListener('click', () => showView(b.dataset.view as View)));
  addEventListener('resize', () => requestAnimationFrame(changed));
  renderCatalogue();
  renderStartingPoints();
  subscribe(render);
  render();
}

initTheme();
(isEditorWindow ? bootEditorWindow() : boot()).catch((e: unknown) => {
  document.body.insertAdjacentHTML('afterbegin', bootFailureHtml(e));
});
