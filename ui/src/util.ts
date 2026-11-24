export const $ = <T extends HTMLElement = HTMLElement>(sel: string, el: ParentNode = document): T => {
  const found = el.querySelector<T>(sel);
  if (!found) throw new Error(`missing element ${sel}`);
  return found;
};

export const $$ = <T extends HTMLElement = HTMLElement>(sel: string, el: ParentNode = document): T[] => [
  ...el.querySelectorAll<T>(sel),
];

const ESCAPES: Record<string, string> = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' };
export const esc = (s: string | number): string => String(s).replace(/[&<>"]/g, (c) => ESCAPES[c] ?? c);

/** The inline style that colours a cable by its jack kind. The kind comes from the backend, so only identifier characters are kept: it cannot close the attribute or add a declaration. */
export const cableStroke = (kind: string): string => `stroke:var(--${kind.replace(/[^\w-]/g, '')})`;

/** A fresh seed for generator commands. */
export const newSeed = (): number => 1000 + Math.floor(Math.random() * 9000);

/** The file name at the end of a path, without the `web:` mark a file held by the browser carries. */
export const baseName = (path: string): string => (path.split(/[\\/]/).pop() ?? path).replace(/^web:/, '');

/** The text of whatever was thrown: an `Error`, or the string a host rejects with. */
export const messageOf = (e: unknown): string => (e instanceof Error ? e.message : String(e));

/** Whether a module kind plays an audio file, so its panel carries the sample block. */
export const takesSample = (kind: string): boolean => kind === 'Veneer' || kind === 'Splinter';

/**
 * Builds a promise on first use and shares it afterwards, except that a rejection is forgotten so
 * the next call tries again. A plain `??=` cache would keep the first failure for good.
 */
export function sharedPromise<T>(make: () => Promise<T>): () => Promise<T> {
  let pending: Promise<T> | null = null;
  return () =>
    (pending ??= make().catch((e: unknown) => {
      pending = null;
      throw e;
    }));
}

/** The notice for an app that cannot start. The message comes from the backend, so it is escaped. */
export function bootFailureHtml(e: unknown): string {
  return `<p role="alert" style="padding:24px;color:#c4513d">Could not reach patchplay's backend: ${esc(messageOf(e))}</p>`;
}
