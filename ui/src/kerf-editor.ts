// The CodeMirror 6 editor for `.kerf` text: highlighting from kerf-syntax,
// completion and hover from kerf-hints, lint from the backend's `check_text`.
import { autocompletion, completionKeymap, snippetCompletion, startCompletion, type Completion } from '@codemirror/autocomplete';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { HighlightStyle, StreamLanguage, syntaxHighlighting } from '@codemirror/language';
import { linter, lintGutter, type Diagnostic as CmDiagnostic } from '@codemirror/lint';
import { Compartment, EditorState, Prec, type Extension } from '@codemirror/state';
import { drawSelection, EditorView, highlightActiveLine, hoverTooltip, keymap, lineNumbers } from '@codemirror/view';
import { tags as t } from '@lezer/highlight';
import { diagnosticSpan } from './diagnostics';
import { hoverAt, suggest, type Option } from './kerf-hints';
import { appendChanges, type InsertMode, lexLine, TAG, type Span } from './kerf-syntax';
import { currentTheme, onTheme, type Theme } from './theme';
import type { Completions, Diagnostic } from './types';

export interface EditorHooks {
  completions(): Completions | null;
  checkText(text: string): Promise<Diagnostic[]>;
  onChange(text: string): void;
  onApply(): void;
  onDiagnostics(list: Diagnostic[]): void;
  openDocs(term: string): void;
  /** ⌘K / Ctrl-K: the command palette. */
  openCommands(): void;
}

export interface KerfEditor {
  getText(): string;
  setText(text: string): void;
  /** Inserts at the cursor, replaces everything, or appends to the end of the draft. Never applies. */
  insert(text: string, mode: InsertMode, complete?: boolean): void;
  goTo(line: number, col: number): void;
  focus(): void;
  destroy(): void;
}

interface LineState {
  spans: Span[];
}

const kerfLanguage = StreamLanguage.define<LineState>({
  startState: () => ({ spans: [] }),
  copyState: (s) => ({ spans: s.spans }),
  token(stream, state) {
    if (stream.sol()) state.spans = lexLine(stream.string);
    const pos = stream.pos;
    const here = state.spans.find((s) => s.from <= pos && pos < s.to);
    if (here) {
      stream.pos = here.to;
      return TAG[here.role];
    }
    stream.pos = state.spans.find((s) => s.from > pos)?.from ?? stream.string.length;
    return null;
  },
});

// Colours are CSS variables from tokens.css, so day and night share one highlight style.
const highlight = HighlightStyle.define([
  { tag: t.keyword, color: 'var(--syn-kw)', fontWeight: '500' },
  { tag: t.typeName, color: 'var(--syn-type)', fontWeight: '500' },
  { tag: t.propertyName, color: 'var(--syn-prop)' },
  { tag: t.variableName, color: 'var(--syn-var)' },
  { tag: t.number, color: 'var(--syn-num)' },
  { tag: t.string, color: 'var(--syn-str)' },
  { tag: t.atom, color: 'var(--syn-atom)' },
  { tag: t.operator, color: 'var(--syn-op)' },
  { tag: t.comment, color: 'var(--syn-cmt)', fontStyle: 'italic' },
  { tag: t.invalid, color: 'var(--syn-atom)', textDecoration: 'underline wavy' },
]);

/** Day is a light CodeMirror base theme, night a dark one; the colours themselves come from tokens. */
const editorTheme = (theme: Theme): Extension =>
  EditorView.theme(
    {
      '&': { height: '100%', fontSize: '12px', background: 'var(--code)', color: 'var(--screen-ink)' },
      '&.cm-focused': { outline: 'none' },
      '.cm-scroller': { fontFamily: 'ui-monospace, "SF Mono", Menlo, monospace', lineHeight: '1.55' },
      '.cm-content': { caretColor: 'var(--screen-ink)' },
      '&.cm-focused .cm-cursor': { borderLeftColor: 'var(--screen-ink)' },
      '.cm-gutters': { background: 'var(--code)', color: 'var(--code-gutter)', border: 'none', borderRight: '1px solid var(--code-line)' },
      '.cm-activeLine': { background: 'var(--code-active)' },
      '.cm-activeLineGutter': { background: 'var(--code-active)', color: 'var(--screen-ink)' },
      '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': { background: 'var(--code-select)' },
      '.cm-tooltip': { border: '1px solid var(--screen-line)', background: 'var(--code)', color: 'var(--screen-ink)', borderRadius: '8px' },
      '.cm-tooltip-autocomplete ul li[aria-selected]': { background: 'var(--screen-ink)', color: 'var(--code)' },
      '.cm-completionDetail': { color: 'var(--screen-muted)', fontStyle: 'normal', marginLeft: '8px' },
      '.cm-tooltip-autocomplete ul li[aria-selected] .cm-completionDetail': { color: 'var(--code-gutter)' },
    },
    { dark: theme === 'night' },
  );

const themeSlot = new Compartment();

function infoNode(o: Option, openDocs: (term: string) => void): HTMLElement | null {
  if (!o.info) return null;
  const el = document.createElement('div');
  el.className = 'ed-info';
  const p = document.createElement('p');
  p.textContent = o.info;
  el.append(p);
  const term = o.term;
  if (term) {
    const a = document.createElement('a');
    a.textContent = `Open docs: ${term}`;
    a.href = '#';
    a.addEventListener('mousedown', (e) => {
      e.preventDefault();
      openDocs(term);
    });
    el.append(a);
  }
  return el;
}

function toCompletion(o: Option, openDocs: (term: string) => void): Completion {
  const base = { label: o.label, detail: o.detail, type: o.type, boost: o.boost, info: () => infoNode(o, openDocs) };
  return o.snippet ? snippetCompletion(o.snippet, base) : base;
}

function hoverDom(title: string, body: string, term: string | undefined, openDocs: (term: string) => void): HTMLElement {
  const el = document.createElement('div');
  el.className = 'ed-hover';
  const b = document.createElement('b');
  b.textContent = title;
  const p = document.createElement('p');
  p.textContent = body;
  el.append(b, p);
  if (term) {
    const a = document.createElement('a');
    a.textContent = 'Open docs';
    a.href = '#';
    a.addEventListener('click', (e) => {
      e.preventDefault();
      openDocs(term);
    });
    el.append(a);
  }
  return el;
}

export function createKerfEditor(host: HTMLElement, text: string, hooks: EditorHooks): KerfEditor {
  const completion = autocompletion({
    activateOnTyping: true,
    override: [
      (ctx) => {
        const c = hooks.completions();
        if (!c) return null;
        const line = ctx.state.doc.lineAt(ctx.pos);
        const found = suggest(line.text.slice(0, ctx.pos - line.from), ctx.state.doc.toString(), c);
        if (!found || (!ctx.explicit && found.options.length === 0)) return null;
        return { from: line.from + found.from, options: found.options.map((o) => toCompletion(o, hooks.openDocs)) };
      },
    ],
  });

  const lint = linter(
    async (view): Promise<CmDiagnostic[]> => {
      const source = view.state.doc.toString();
      const list = await hooks.checkText(source);
      hooks.onDiagnostics(list);
      if (view.state.doc.toString() !== source) return [];
      return list.map((d) => {
        const line = view.state.doc.line(Math.min(Math.max(d.line, 1), view.state.doc.lines));
        const [from, to] = diagnosticSpan(line.length, d.col, d.end_col);
        return { from: line.from + from, to: line.from + to, severity: d.severity, message: d.message };
      });
    },
    { delay: 450 },
  );

  const hover = hoverTooltip((view, pos) => {
    const c = hooks.completions();
    if (!c) return null;
    const line = view.state.doc.lineAt(pos);
    const h = hoverAt(line.text, pos - line.from, view.state.doc.toString(), c);
    if (!h) return null;
    return {
      pos: line.from + h.from,
      end: line.from + h.to,
      above: true,
      create: () => ({ dom: hoverDom(h.title, h.body, h.term, hooks.openDocs) }),
    };
  });

  const view = new EditorView({
    parent: host,
    state: EditorState.create({
      doc: text,
      extensions: [
        Prec.highest(keymap.of([{ key: 'Mod-Enter', run: () => (hooks.onApply(), true) }])),
        lineNumbers(),
        history(),
        drawSelection(),
        highlightActiveLine(),
        kerfLanguage,
        syntaxHighlighting(highlight),
        themeSlot.of(editorTheme(currentTheme())),
        completion,
        lint,
        lintGutter(),
        hover,
        keymap.of([{ key: 'Mod-k', run: () => (hooks.openCommands(), true) }, ...completionKeymap, ...historyKeymap, ...defaultKeymap]),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) hooks.onChange(u.state.doc.toString());
          const head = u.state.selection.main.head;
          const typed = u.transactions.some((tr) => tr.isUserEvent('input.type'));
          if (typed && u.docChanged && u.state.doc.sliceString(head - 1, head) === ' ') {
            setTimeout(() => startCompletion(u.view), 0);
          }
        }),
      ],
    }),
  });
  const stopTheme = onTheme((theme) => view.dispatch({ effects: themeSlot.reconfigure(editorTheme(theme)) }));

  return {
    getText: () => view.state.doc.toString(),
    setText(next) {
      if (next === view.state.doc.toString()) return;
      const head = Math.min(view.state.selection.main.head, next.length);
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: next }, selection: { anchor: head } });
    },
    insert(insertText, mode, complete = false) {
      if (mode === 'append') {
        view.dispatch({ changes: appendChanges(view.state.doc.toString(), insertText) });
        view.dispatch({ selection: { anchor: view.state.doc.length }, scrollIntoView: true });
        view.focus();
        return;
      }
      const replaceAll = mode === 'replace';
      const sel = view.state.selection.main;
      const range = replaceAll ? { from: 0, to: view.state.doc.length } : { from: sel.from, to: sel.to };
      const body = !replaceAll && !insertText.endsWith('\n') && !complete ? `${insertText}\n` : insertText;
      view.dispatch({ changes: { ...range, insert: body }, selection: { anchor: range.from + body.length } });
      view.focus();
      if (complete) startCompletion(view);
    },
    goTo(line, col) {
      const l = view.state.doc.line(Math.min(Math.max(line, 1), view.state.doc.lines));
      view.dispatch({ selection: { anchor: l.from + Math.min(Math.max(col - 1, 0), l.length) }, scrollIntoView: true });
      view.focus();
    },
    focus: () => view.focus(),
    destroy() {
      stopTheme();
      view.destroy();
    },
  };
}
