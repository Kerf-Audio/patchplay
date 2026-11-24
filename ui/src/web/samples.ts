// Samples in the browser. A dropped or chosen audio file is decoded with decodeAudioData, mixed
// down to mono in JS and kept under the pseudo path `web:<file name>`. That path is what the
// patch (and its `.kerf` text) holds, so a saved patch names the file the way it would on a
// desktop. Samples persist in IndexedDB, so a reload finds them again.
const DB = 'patchplay';
const STORE = 'samples';
const DECODE_RATE = 48_000;

export const PREFIX = 'web:';

export interface Pcm {
  mono: Float32Array;
  rate: number;
}

const wrap = <T>(req: IDBRequest<T>): Promise<T> =>
  new Promise((done, fail) => {
    req.onsuccess = () => done(req.result);
    req.onerror = () => fail(req.error ?? new Error('IndexedDB failed'));
  });

async function openDb(): Promise<IDBDatabase | null> {
  if (typeof indexedDB === 'undefined') return null;
  const req = indexedDB.open(DB, 1);
  req.onupgradeneeded = () => req.result.createObjectStore(STORE);
  return wrap(req).catch(() => null);
}

export function downmix(channels: Float32Array[]): Float32Array {
  const first = channels[0];
  if (!first) return new Float32Array(0);
  if (channels.length === 1) return first.slice();
  const mono = new Float32Array(first.length);
  for (const c of channels) for (let i = 0; i < mono.length; i++) mono[i] = (mono[i] ?? 0) + (c[i] ?? 0) / channels.length;
  return mono;
}

export class SampleStore {
  private readonly samples = new Map<string, Pcm>();

  private constructor(private readonly db: IDBDatabase | null) {}

  /** Opens the database and loads every sample kept from earlier visits. */
  static async open(): Promise<SampleStore> {
    const store = new SampleStore(await openDb());
    if (!store.db) return store;
    const tx = store.db.transaction(STORE, 'readonly').objectStore(STORE);
    const [keys, values] = await Promise.all([wrap(tx.getAllKeys()), wrap(tx.getAll())]);
    keys.forEach((k, i) => store.samples.set(String(k), values[i] as Pcm));
    return store;
  }

  all(): [string, Pcm][] {
    return [...this.samples];
  }

  /** Decodes a browser File and keeps it; answers its pseudo path. */
  async add(file: File): Promise<{ path: string; pcm: Pcm }> {
    const ctx = new OfflineAudioContext(1, 1, DECODE_RATE);
    const decoded = await ctx.decodeAudioData(await file.arrayBuffer()).catch(() => {
      throw new Error(`${file.name} is not audio this browser can read (try WAV, MP3, Ogg or FLAC).`);
    });
    const channels = Array.from({ length: decoded.numberOfChannels }, (_, c) => decoded.getChannelData(c));
    const pcm = { mono: downmix(channels), rate: decoded.sampleRate };
    const path = PREFIX + file.name;
    this.samples.set(path, pcm);
    if (this.db) void wrap(this.db.transaction(STORE, 'readwrite').objectStore(STORE).put(pcm, path)).catch(() => undefined);
    return { path, pcm };
  }
}
