// A small, safe Markdown renderer for the docs: headings, paragraphs, lists,
// tables, quotes, rules, inline code, bold, italics, links and fenced code.
// Everything is escaped before any markup is added, so section text can never
// inject HTML. `kerf` code blocks are highlighted with the editor's own lexer.
import { isCompletePatch, lexLine } from './kerf-syntax';
import type { DocSection } from './types';
import { esc } from './util';

export const slug = (s: string): string =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');

const SAFE_LINK = /^(https?:\/\/|#)/;

function emphasis(escaped: string): string {
  return escaped
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/(^|[^*\w])\*([^*\s][^*]*)\*(?!\w)/g, '$1<em>$2</em>')
    .replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_whole: string, label: string, url: string) => {
      const href = url.replace(/&amp;/g, '&');
      if (!SAFE_LINK.test(href)) return label;
      return href.startsWith('#')
        ? `<a href="${url}" data-doc-link="${esc(href.slice(1))}">${label}</a>`
        : `<a href="${url}" target="_blank" rel="noopener noreferrer">${label}</a>`;
    });
}

/** Inline markup for one line of text. */
export function inline(text: string): string {
  return text
    .split('`')
    .map((part, i) => (i % 2 === 1 ? `<code>${esc(part)}</code>` : emphasis(esc(part))))
    .join('');
}

function highlightKerf(code: string): string {
  return code
    .split('\n')
    .map((line) => {
      let at = 0;
      let html = '';
      for (const s of lexLine(line)) {
        html += `${esc(line.slice(at, s.from))}<span class="t-${s.role}">${esc(line.slice(s.from, s.to))}</span>`;
        at = s.to;
      }
      return html + esc(line.slice(at));
    })
    .join('\n');
}

function codeBlock(info: string, code: string): string {
  const [lang = '', ...flags] = info.split(/\s+/);
  const isKerf = lang === 'kerf';
  const complete = isKerf && isCompletePatch(code);
  const snippet = isKerf && flags.includes('snippet');
  const insert = snippet
    ? `<button data-code-insert="append" data-tip="Adds this to the end of the editor's draft. Nothing is applied until you press Apply.">Add to patch</button>`
    : `<button data-code-insert="${complete ? 'replace' : 'cursor'}">${complete ? 'Insert as editor text' : 'Insert at cursor'}</button>${
        complete ? '<button data-code-apply>Apply as patch</button>' : ''
      }`;
  const tools = isKerf ? insert : '';
  return `<div class="code"><pre><code>${isKerf ? highlightKerf(code) : esc(code)}</code></pre>
    <div class="code-tools">${tools}<button data-code-copy>Copy</button></div></div>`;
}

const cells = (row: string): string[] =>
  row
    .trim()
    .replace(/^\||\|$/g, '')
    .split('|')
    .map((c) => c.trim());

const isTableRule = (l: string | undefined): boolean => !!l && /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/.test(l);

function table(rows: string[]): string {
  const [head, , ...body] = rows;
  const th = cells(head ?? '').map((c) => `<th>${inline(c)}</th>`).join('');
  const tr = body.map((r) => `<tr>${cells(r).map((c) => `<td>${inline(c)}</td>`).join('')}</tr>`).join('');
  return `<div class="tbl"><table><thead><tr>${th}</tr></thead><tbody>${tr}</tbody></table></div>`;
}

/** Markdown to HTML. `idPrefix` keeps heading ids unique across sections. */
export function renderMarkdown(md: string, idPrefix = ''): string {
  const lines = md.replace(/\r\n?/g, '\n').split('\n');
  const out: string[] = [];
  let i = 0;
  const startsBlock = (l: string): boolean => /^(#{1,6}\s|```|>|\s*[-*]\s|\s*\d+\.\s|---+\s*$)/.test(l) || l.includes('|');
  while (i < lines.length) {
    const line = lines[i] ?? '';
    const fence = /^```(.*)$/.exec(line);
    const heading = /^(#{1,6})\s+(.*?)\s*$/.exec(line);
    if (!line.trim()) {
      i++;
    } else if (fence) {
      const body: string[] = [];
      i++;
      while (i < lines.length && !(lines[i] ?? '').startsWith('```')) body.push(lines[i++] ?? '');
      i++;
      out.push(codeBlock(fence[1]?.trim() ?? '', body.join('\n')));
    } else if (heading) {
      const level = heading[1]?.length ?? 2;
      const text = heading[2] ?? '';
      out.push(`<h${level} id="${esc(idPrefix + slug(text))}" data-heading="${esc(text.toLowerCase())}">${inline(text)}</h${level}>`);
      i++;
    } else if (/^---+\s*$/.test(line)) {
      out.push('<hr>');
      i++;
    } else if (line.includes('|') && isTableRule(lines[i + 1])) {
      const rows = [line, lines[i + 1] ?? ''];
      i += 2;
      while (i < lines.length && (lines[i] ?? '').includes('|')) rows.push(lines[i++] ?? '');
      out.push(table(rows));
    } else if (/^\s*([-*]|\d+\.)\s/.test(line)) {
      const ordered = /^\s*\d+\./.test(line);
      const items: string[] = [];
      while (i < lines.length && /^\s*([-*]|\d+\.)\s/.test(lines[i] ?? '')) {
        let item = (lines[i++] ?? '').replace(/^\s*([-*]|\d+\.)\s+/, '');
        while (i < lines.length && /^\s{2,}\S/.test(lines[i] ?? '') && !/^\s*([-*]|\d+\.)\s/.test(lines[i] ?? '')) {
          item += ` ${(lines[i++] ?? '').trim()}`;
        }
        items.push(`<li>${inline(item)}</li>`);
      }
      out.push(`<${ordered ? 'ol' : 'ul'}>${items.join('')}</${ordered ? 'ol' : 'ul'}>`);
    } else if (line.startsWith('>')) {
      const quote: string[] = [];
      while (i < lines.length && (lines[i] ?? '').startsWith('>')) quote.push((lines[i++] ?? '').replace(/^>\s?/, ''));
      out.push(`<blockquote>${inline(quote.join(' '))}</blockquote>`);
    } else {
      const para: string[] = [line.trim()];
      i++;
      while (i < lines.length && (lines[i] ?? '').trim() && !startsBlock(lines[i] ?? '')) para.push((lines[i++] ?? '').trim());
      out.push(`<p>${inline(para.join(' '))}</p>`);
    }
  }
  return out.join('\n');
}

/** Sections that contain every word of `query` (case-insensitive) in title or body. */
export function filterSections(sections: DocSection[], query: string): DocSection[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return sections;
  return sections.filter((s) => {
    const hay = `${s.title}\n${s.markdown}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}
