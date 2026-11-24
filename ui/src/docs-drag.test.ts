import { describe, expect, it } from 'vitest';
import { filterModules } from './palette';
import { suggestId } from './add-module';
import { ghostLayout, hitModule, insertIndex, type Box } from './ghost';
import { filterSections, renderMarkdown, slug } from './markdown';
import { levelFromY, levelLabel } from './part-fader';

const box = (id: string, left: number, top = 0, w = 100, h = 200): Box => ({ id, left, top, right: left + w, bottom: top + h });

describe('markdown', () => {
  it('escapes HTML everywhere', () => {
    const html = renderMarkdown('# <img src=x onerror=alert(1)>\n\ntext <script>x</script> and `<b>`');
    expect(html).not.toContain('<img');
    expect(html).not.toContain('<script>');
    expect(html).toContain('&lt;script&gt;');
  });

  it('renders headings with ids, lists, tables and links', () => {
    const html = renderMarkdown('## Cables & jacks\n\n- one\n- two\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n[docs](https://example.com)', 's--');
    expect(html).toContain('<h2 id="s--cables-jacks"');
    expect(html).toContain('<ul><li>one</li><li>two</li></ul>');
    expect(html).toContain('<th>a</th>');
    expect(html).toContain('<td>2</td>');
    expect(html).toContain('rel="noopener noreferrer"');
  });

  it('drops unsafe links', () => {
    expect(renderMarkdown('[x](javascript:alert(1))')).not.toContain('href');
  });

  it('offers Apply only on complete kerf examples', () => {
    const whole = renderMarkdown('```kerf\nkerf 1\ntempo 120\n```');
    const part = renderMarkdown('```kerf\ncable a.out -> b.in\n```');
    expect(whole).toContain('data-code-apply');
    expect(part).not.toContain('data-code-apply');
    expect(part).toContain('data-code-insert="cursor"');
  });

  it('filters sections by every word', () => {
    const sections = [
      { id: 'a', title: 'Cables', markdown: 'jacks and colours' },
      { id: 'b', title: 'Scenes', markdown: 'morph between scenes' },
    ];
    expect(filterSections(sections, 'jacks cables').map((s) => s.id)).toEqual(['a']);
    expect(filterSections(sections, '')).toHaveLength(2);
    expect(slug('Hello, World!')).toBe('hello-world');
  });
});

describe('palette helpers', () => {
  it('suggests the first free id', () => {
    expect(suggestId('Chisel', ['chisel1', 'chisel2', 'x'])).toBe('chisel3');
    expect(suggestId('Mortise', [])).toBe('mortise1');
  });

  it('filters modules by words', () => {
    const mods = [
      { kind: 'Mortise', band: 'voices', summary: 'A kick drum.' },
      { kind: 'Chisel', band: 'voices', summary: 'A bright lead.' },
    ] as Parameters<typeof filterModules>[0];
    expect(filterModules(mods, 'kick').map((m) => m.kind)).toEqual(['Mortise']);
    expect(filterModules(mods, 'voices')).toHaveLength(2);
  });
});

describe('drag hit-testing', () => {
  const boxes = [box('a', 0), box('b', 140)];
  it('hits the module under the pointer', () => {
    expect(hitModule(50, 100, boxes)).toBe('a');
  });
  it('snaps to the nearest within the margin', () => {
    expect(hitModule(120, 100, boxes, 28)).toBe('a');
    expect(hitModule(125, 100, boxes, 28)).toBe('b');
  });
  it('misses empty rack space', () => {
    expect(hitModule(400, 100, boxes)).toBeNull();
  });
  it('counts siblings left of the pointer for the drop index', () => {
    expect(insertIndex(10, boxes)).toBe(0);
    expect(insertIndex(120, boxes)).toBe(1);
    expect(insertIndex(500, boxes)).toBe(2);
  });
});

describe('ghost cables', () => {
  const jacks: Record<string, [number, number]> = { 'kick.out': [10, 20], 'mix.in1': [200, 40] };
  const at = (ref: string): [number, number] | null => jacks[ref] ?? null;
  it('runs the new end to the chip and the real end to the jack', () => {
    const g = ghostLayout(
      [
        { from: 'new.out', to: 'mix.in1', reason: 'into a free mixer channel', score: 0.5 },
        { from: 'kick.out', to: 'new.in', reason: 'kick feeds it', score: 0.9 },
      ],
      [100, 100],
      at,
    );
    expect(g.map((x) => x.ref)).toEqual(['kick.out', 'mix.in1']);
    expect(g[0]?.label).toBe('kick feeds it');
    expect(g[0]?.mid).toEqual([55, 60]);
  });
  it('skips jacks that are not on screen and cables without a new end', () => {
    const g = ghostLayout(
      [
        { from: 'new.out', to: 'gone.in', reason: 'x', score: 1 },
        { from: 'kick.out', to: 'mix.in1', reason: 'y', score: 1 },
      ],
      [0, 0],
      at,
    );
    expect(g).toEqual([]);
  });
});

describe('part fader maths', () => {
  it('maps the pointer to a level, top being unity', () => {
    expect(levelFromY(100, 300, 100)).toBe(1);
    expect(levelFromY(100, 300, 300)).toBe(0);
    expect(levelFromY(100, 300, 200)).toBe(0.5);
    expect(levelFromY(100, 300, 10)).toBe(1);
    expect(levelFromY(100, 300, 900)).toBe(0);
  });
  it('labels in decibels', () => {
    expect(levelLabel(1)).toBe('0 dB');
    expect(levelLabel(0.5)).toBe('−6.0 dB');
    expect(levelLabel(0)).toBe('−∞ dB');
  });
});
