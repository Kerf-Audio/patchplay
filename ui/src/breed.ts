// Which patch Breed crosses the current one with. Pure, so the choice is testable.
import type { Lineage } from './types';

/** A `module` line at any indent: modules inside a part are indented, loose ones are not. */
const HAS_MODULE = /^[ \t]*module /m;

/**
 * The patch to breed with: the hearted one, or else the newest other patch in the lineage that
 * has modules. `null` when there is nothing to cross.
 */
export function breedMate(lineage: Lineage, hearted: number | null): number | null {
  const { current } = lineage;
  if (hearted !== null && hearted !== current) return hearted;
  const others = lineage.nodes.filter((n) => n.id !== current && HAS_MODULE.test(n.patch));
  return others.length ? Math.max(...others.map((n) => n.id)) : null;
}
