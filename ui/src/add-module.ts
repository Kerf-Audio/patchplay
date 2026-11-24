// Adding a module to the patch: the id and part choices, and the command(s)
// that carry them out. The palette, the catalogue's "Add to patch" button and
// dragging onto the rack all come through `addModule`.
import { readStored, writeStored } from './storage';
import { app, notify, patch, send } from './store';

export const ID_PATTERN = /^[a-z][a-z0-9_]*$/;
const NEW_PART_COLOURS = ['#d9622b', '#3d63c9', '#2f8f6b', '#8a4fb0', '#d08a1e', '#c4513d'];

/** The first free `<kind><n>` id, such as `chisel1`. */
export function suggestId(kind: string, taken: Iterable<string>): string {
  const used = new Set(taken);
  const base = kind.toLowerCase().replace(/[^a-z0-9]/g, '');
  let n = 1;
  while (used.has(`${base}${n}`)) n++;
  return `${base}${n}`;
}

export interface AddOptions {
  /** An existing part's id, `''` for the loose rack, or `null` to let the backend choose. */
  part?: string | null;
  /** Make a new part of this id first and put the module in it. */
  newPart?: string;
  id?: string;
  /** Module the new one is dropped on or near: its cables are preferred. */
  near?: string;
  autoPatch: boolean;
}

export const autoPatchPreference = (): boolean => readStored('kerf.autopatch') !== 'off';
export const setAutoPatchPreference = (on: boolean): void => writeStored('kerf.autopatch', on ? 'on' : 'off');

const cableKey = (c: { from: string; to: string }): string => `${c.from}>${c.to}`;

/** Toasts how many cables a command made, by comparing the patch before and after. */
function announce(before: Set<string>, what: string): void {
  const made = patch().cables.filter((c) => !before.has(cableKey(c)));
  notify(made.length ? `${what}: ${made.length} cable${made.length === 1 ? '' : 's'} connected` : `${what}: no cables made, patch it by hand`);
  requestAnimationFrame(() => {
    const fresh = new Set(made.map(cableKey));
    document.querySelectorAll<SVGElement>('#cables path[data-i]').forEach((p) => {
      const c = patch().cables[Number(p.dataset.i)];
      if (c && fresh.has(cableKey(c))) p.classList.add('fresh');
    });
  });
}

export async function addModule(kind: string, o: AddOptions): Promise<boolean> {
  const id = o.id || suggestId(kind, patch().modules.map((m) => m.id));
  if (!ID_PATTERN.test(id)) {
    notify('A module id starts with a lower-case letter and uses only a–z, 0–9 and _', true);
    return false;
  }
  const before = new Set(patch().cables.map(cableKey));
  let part = o.part;
  if (o.newPart) {
    const colour = NEW_PART_COLOURS[patch().parts.length % NEW_PART_COLOURS.length] ?? '#d9622b';
    if (!(await send({ op: 'add_part', id: o.newPart, name: o.newPart, colour }))) return false;
    part = o.newPart;
  }
  const cmd: Record<string, unknown> = { op: 'add_module', kind, id, auto_patch: o.autoPatch };
  if (part) cmd.part = part;
  if (o.near) cmd.near = o.near;
  if (!(await send(cmd as { op: string }))) return false;
  if (o.autoPatch) announce(before, `Added ${id}`);
  else notify(`Added ${id}`);
  return true;
}

/** Wires an existing module in; says what it connected. */
export async function autoPatchModule(module: string): Promise<boolean> {
  const before = new Set(patch().cables.map(cableKey));
  const ok = await send({ op: 'auto_patch', module });
  if (ok) announce(before, `Auto-patched ${module}`);
  return ok;
}

export async function removeModule(module: string): Promise<boolean> {
  const ok = await send({ op: 'remove_module', id: module });
  if (ok) notify(`Removed ${module}. Undo brings it back.`);
  return ok;
}

export const partIds = (): string[] => app.state?.patch.parts.map((p) => p.id) ?? [];
