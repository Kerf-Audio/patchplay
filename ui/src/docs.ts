// The Docs view: a table of contents, search and the rendered sections from the
// backend's `language_docs`. It runs full-page in the Docs tab and as a side
// panel in the pop-out. Code examples can be pasted into the editor.
import { insertIntoEditor } from './editor-bridge';
import type { InsertMode } from './kerf-syntax';
import { filterSections, renderMarkdown } from './markdown';
import { app, notify, send } from './store';
import type { DocSection } from './types';
import { $, esc, messageOf, sharedPromise } from './util';

export interface DocsView {
  /** Shows the docs for a module kind or directive, scrolled to its heading. */
  openTerm(term: string): void;
}

/** The docs sections, fetched once; a failed fetch is tried again. The command palette reads the snippets out of them. */
export const docSections = sharedPromise(() => app.api.languageDocs().then((d) => d.sections));

const withTitle = (s: DocSection): string => (/^\s*#/.test(s.markdown) ? s.markdown : `## ${s.title}\n\n${s.markdown}`);

function findHeading(body: HTMLElement, term: string): HTMLElement | null {
  const t = term.toLowerCase();
  const heads = [...body.querySelectorAll<HTMLElement>('h1, h2, h3, h4, h5, h6')];
  return heads.find((h) => h.dataset.heading === t) ?? heads.find((h) => (h.dataset.heading ?? '').startsWith(`${t} `)) ?? null;
}

async function copyCode(code: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(code);
    notify('Copied');
  } catch {
    notify('Could not copy: the browser did not allow it', true);
  }
}

export function mountDocs(host: HTMLElement): DocsView {
  host.innerHTML = `<div class="docs">
    <nav class="docs-toc"><input type="search" placeholder="Search the docs" aria-label="Search the docs"><ul></ul></nav>
    <article class="docs-body"><p class="hint">Loading the docs…</p></article></div>`;
  const search = $<HTMLInputElement>('input', host);
  const toc = $('ul', host);
  const body = $('.docs-body', host);
  let all: DocSection[] = [];

  const draw = (): void => {
    const shown = filterSections(all, search.value);
    toc.innerHTML = shown.map((s) => `<li><a href="#doc-${esc(s.id)}" data-toc="${esc(s.id)}">${esc(s.title)}</a></li>`).join('');
    body.innerHTML = shown.length
      ? shown.map((s) => `<section id="doc-${esc(s.id)}">${renderMarkdown(withTitle(s), `${s.id}--`)}</section>`).join('')
      : `<p class="hint">Nothing in the docs matches “${esc(search.value)}”.</p>`;
  };

  const scrollTo = (el: Element | null): void => {
    el?.scrollIntoView({ block: 'start', behavior: 'smooth' });
    if (el instanceof HTMLElement) {
      el.classList.add('flash');
      setTimeout(() => el.classList.remove('flash'), 1400);
    }
  };

  /** Settles once the docs are drawn: `false` when they could not be loaded. */
  const ready = docSections().then(
    (s) => {
      all = s;
      draw();
      return true;
    },
    (e: unknown) => {
      body.innerHTML = `<p class="hint err">Could not load the docs: ${esc(messageOf(e))}</p>`;
      return false;
    },
  );

  search.addEventListener('input', draw);
  toc.addEventListener('click', (e) => {
    const id = (e.target as HTMLElement).closest<HTMLElement>('[data-toc]')?.dataset.toc;
    if (!id) return;
    e.preventDefault();
    scrollTo(document.getElementById(`doc-${id}`));
  });
  body.addEventListener('click', (e) => {
    const t = e.target as HTMLElement;
    const code = t.closest('.code')?.querySelector('code')?.textContent ?? '';
    const insert = t.closest<HTMLElement>('[data-code-insert]');
    const link = t.closest<HTMLElement>('[data-doc-link]');
    if (insert) insertIntoEditor(code, (insert.dataset.codeInsert ?? 'cursor') as InsertMode);
    else if (t.closest('[data-code-apply]')) void send({ op: 'replace_text', text: code }).then((ok) => ok && notify('Applied the example as the patch'));
    else if (t.closest('[data-code-copy]')) void copyCode(code);
    else if (link) {
      e.preventDefault();
      const id = link.dataset.docLink ?? '';
      scrollTo(document.getElementById(`doc-${id}`) ?? body.querySelector(`[id$="--${CSS.escape(id)}"]`));
    }
  });

  return {
    openTerm(term) {
      void ready.then((loaded) => {
        if (!loaded) return;
        search.value = '';
        draw();
        const heading = findHeading(body, term);
        if (heading) scrollTo(heading);
        else {
          search.value = term;
          draw();
        }
      });
    },
  };
}
