// Pure helpers for the starting points page: filter chips and grouping by category tag.
import type { PresetInfo } from './types';

/** The main categories, in the order they are shown. Other tags follow alphabetically. */
export const CATEGORIES = ['four-four', 'abstract', 'crazy', 'reggae', 'ominous', 'synthwave'];
const OTHER = 'other';

/** Every tag present in the data: categories first (only those present), then the rest alphabetically. */
export function collectTags(presets: PresetInfo[]): string[] {
  const seen = new Set(presets.flatMap((p) => p.tags));
  const main = CATEGORIES.filter((c) => seen.has(c));
  const rest = [...seen].filter((t) => !CATEGORIES.includes(t)).sort();
  return [...main, ...rest];
}

/** The heading a preset is filed under: its first category tag, else "other". */
export function categoryOf(p: PresetInfo): string {
  return p.tags.find((t) => CATEGORIES.includes(t)) ?? OTHER;
}

export function filterByTag(presets: PresetInfo[], tag: string): PresetInfo[] {
  return tag === 'all' ? presets : presets.filter((p) => p.tags.includes(tag));
}

/** Groups in category order, "other" last; empty groups are dropped. */
export function groupByCategory(presets: PresetInfo[]): { category: string; items: PresetInfo[] }[] {
  const order = [...CATEGORIES, OTHER];
  return order
    .map((category) => ({ category, items: presets.filter((p) => categoryOf(p) === category) }))
    .filter((g) => g.items.length > 0);
}

/** Tags shown as small text on a row: everything except the heading it sits under. */
export function extraTags(p: PresetInfo, heading: string | null): string[] {
  return p.tags.filter((t) => t !== heading);
}
