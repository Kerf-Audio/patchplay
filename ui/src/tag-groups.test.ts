import { describe, expect, it } from 'vitest';
import { categoryOf, collectTags, extraTags, filterByTag, groupByCategory } from './tag-groups';
import type { PresetInfo } from './types';

const p = (name: string, tags: string[]): PresetInfo => ({ name, path: `${name}.kerf`, tempo: 120, key: null, style: null, modules: 4, tags });
const list = [p('a', ['house', 'four-four']), p('b', ['crazy', 'abstract']), p('c', ['ambient']), p('d', ['four-four']), p('e', [])];

describe('starting point tags', () => {
  it('lists categories first, then other tags alphabetically, from the data only', () => {
    expect(collectTags(list)).toEqual(['four-four', 'abstract', 'crazy', 'ambient', 'house']);
    expect(collectTags([p('x', ['techno'])])).toEqual(['techno']);
  });

  it('files each preset under its first category tag, else other', () => {
    expect(categoryOf(list[0]!)).toBe('four-four');
    expect(categoryOf(list[1]!)).toBe('crazy');
    expect(categoryOf(list[2]!)).toBe('other');
    expect(categoryOf(list[4]!)).toBe('other');
  });

  it('filters by tag and keeps everything for all', () => {
    expect(filterByTag(list, 'all')).toHaveLength(5);
    expect(filterByTag(list, 'four-four').map((x) => x.name)).toEqual(['a', 'd']);
    expect(filterByTag(list, 'house').map((x) => x.name)).toEqual(['a']);
  });

  it('groups in category order with other last and no empty groups', () => {
    const groups = groupByCategory(list);
    expect(groups.map((g) => g.category)).toEqual(['four-four', 'crazy', 'other']);
    expect(groups[0]!.items.map((x) => x.name)).toEqual(['a', 'd']);
    expect(groups[2]!.items.map((x) => x.name)).toEqual(['c', 'e']);
  });

  it('files reggae and ominous patches under their own headings', () => {
    const more = [p('r', ['reggae', 'dub']), p('o', ['ominous', 'drone']), p('t', ['dub', 'techno', 'reggae'])];
    expect(categoryOf(more[0]!)).toBe('reggae');
    expect(categoryOf(more[1]!)).toBe('ominous');
    expect(categoryOf(more[2]!)).toBe('reggae');
    expect(groupByCategory([...list, ...more]).map((g) => g.category)).toEqual(['four-four', 'crazy', 'reggae', 'ominous', 'other']);
  });

  it('files synthwave patches under their own heading, after the other categories', () => {
    const wave = [p('s', ['synthwave', 'outrun']), p('n', ['synthwave', 'night drive'])];
    expect(categoryOf(wave[0]!)).toBe('synthwave');
    expect(collectTags(wave)).toEqual(['synthwave', 'night drive', 'outrun']);
    expect(groupByCategory([...list, ...wave]).map((g) => g.category)).toEqual(['four-four', 'crazy', 'synthwave', 'other']);
  });

  it('shows the tags other than the heading on a row', () => {
    expect(extraTags(list[0]!, 'four-four')).toEqual(['house']);
    expect(extraTags(list[0]!, null)).toEqual(['house', 'four-four']);
  });
});
