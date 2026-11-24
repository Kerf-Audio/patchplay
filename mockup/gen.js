// Kerf Modular mockup: generator. Mirrors kerf-gen: seeded, deterministic,
// style rulebooks as data. Audio checks are faked here; the real one renders offline.
import { MODULES } from './modules.js';

export function rng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const clamp = (v) => Math.min(1, Math.max(0, v));
const pick = (r, xs) => xs[Math.floor(r() * xs.length)];

export const PARTS = {
  master: { name: 'Master', colour: '#6b645a' },
  drums: { name: 'Drums', colour: '#d9622b' },
  bass: { name: 'Bass', colour: '#3d63c9' },
  pad: { name: 'Pad', colour: '#2f8f6b' },
  lead: { name: 'Lead', colour: '#8a4fb0' },
  texture: { name: 'Texture', colour: '#a37a3c' },
  perc: { name: 'Perc', colour: '#c2417a' },
  keys: { name: 'Keys', colour: '#4f7f9c' },
};

export const STYLES = {
  'dub-chamber': { name: 'Dub Chamber', tempo: [112, 122], keys: ['A minor', 'D minor', 'F minor'], parts: ['drums', 'pad', 'texture'], wild: 0.25, delay: true, blurb: 'Soft kick, stabs drowned in delay, a hiss floor.' },
  'acid-line': { name: 'Acid Line', tempo: [126, 136], keys: ['A minor', 'E minor', 'G minor'], parts: ['drums', 'bass'], wild: 0.35, acid: true, snare: true, blurb: 'A squelching filtered line over a straight four.' },
  'slow-grain': { name: 'Slow Grain', tempo: [60, 80], keys: ['C major', 'E♭ major', 'B minor'], parts: ['pad', 'texture', 'lead'], wild: 0.3, delay: true, blurb: 'No drums. Long drones, a granular cloud, a wandering lead.' },
  'broken-joinery': { name: 'Broken Joinery', tempo: [124, 132], keys: ['D dorian', 'G dorian'], parts: ['drums', 'perc', 'bass'], wild: 0.4, snare: true, blurb: 'Off-grid hits, modal percussion, a rubbery bass.' },
  foundry: { name: 'Foundry', tempo: [136, 146], keys: ['C♯ minor', 'F minor'], parts: ['drums', 'texture', 'bass'], wild: 0.5, grit: true, blurb: 'Distorted drums, noise and metal, a driven bass.' },
  'tape-keys': { name: 'Tape Keys', tempo: [78, 90], keys: ['F major', 'A♭ major', 'C minor'], parts: ['drums', 'keys', 'pad'], wild: 0.25, delay: true, blurb: 'Dusty drums, sliced keys, a warm pad.' },
};

export const PRESETS = [
  { name: 'First Cut', style: 'acid-line', seed: 1, blurb: 'Start here: one voice, one filter, one envelope.', only: ['bass'] },
  { name: 'Harbour Fog', style: 'dub-chamber', seed: 4821, blurb: 'Stabs that hang in the mist.' },
  { name: '303 Offcuts', style: 'acid-line', seed: 303, blurb: 'Resonant and rude.' },
  { name: 'Long Exposure', style: 'slow-grain', seed: 77, blurb: 'Twelve minutes on one note.' },
  { name: 'Broken Hinge', style: 'broken-joinery', seed: 908, blurb: 'Swung and slightly wrong.' },
  { name: 'Steel Shop', style: 'foundry', seed: 1440, blurb: 'Hammers on sheet metal.' },
  { name: 'Cassette Cabinet', style: 'tape-keys', seed: 85, blurb: 'Wobbly keys from an old tape.' },
  { name: 'Morning Sawmill', style: 'broken-joinery', seed: 5150, blurb: 'Bright wooden percussion.' },
];

function defaults(type) {
  const v = {};
  for (const k of MODULES[type].knobs) v[k.n] = k.v;
  return v;
}

function pattern(r, n, density) {
  return Array.from({ length: n }, () => r() < density);
}

export function generate(styleId, seed, only) {
  const style = STYLES[styleId];
  const r = rng(seed);
  const [lo, hi] = style.tempo;
  const p = {
    style: styleId, seed, tempo: Math.round(lo + r() * (hi - lo)), key: pick(r, style.keys),
    parts: [], mods: [], cables: [], scenes: {}, origin: `random ${styleId} seed ${seed}`,
  };
  const count = {};
  const add = (type, part, over = {}) => {
    count[type] = (count[type] || 0) + 1;
    const uid = type.toLowerCase() + count[type];
    const values = defaults(type);
    for (const k in values) values[k] = clamp(values[k] + (r() - 0.5) * style.wild);
    Object.assign(values, over);
    const m = { uid, type, part, values, locked: [], modLocked: false };
    if (MODULES[type].steps) m.pattern = pattern(r, 16, type === 'Ruler' ? 0.35 : 0.6);
    p.mods.push(m);
    return uid;
  };
  const c = (from, to) => p.cables.push({ from, to });
  const usePart = (id) => p.parts.push({ id, ...PARTS[id], locked: false, muted: false });

  usePart('master');
  const tally = add('Tally', 'master', { Tempo: (p.tempo - 40) / 200, Swing: style.snare ? 0.25 : 0.05 });
  const router = add('Router', 'master');
  let ch = 1;
  const mix = (jack) => { if (ch <= 6) c(jack, `${router}.In ${ch++}`); };
  const ctx = { ruler: null, fret: null };

  const builders = {
    drums() {
      const ru = add('Ruler', 'drums', { 'Hits A': 0.25, 'Steps A': 1 });
      ctx.ruler = ru;
      c(`${tally}.Clock`, `${ru}.Clock`);
      const kick = add('Mortise', 'drums', { Engine: 0, Pitch: 0.25 + r() * 0.15 });
      c(`${ru}.A`, `${kick}.Trig`);
      if (style.grit) {
        const ra = add('Rasp', 'drums', { Drive: 0.7, Fold: 0.4 });
        c(`${kick}.Out`, `${ra}.In`); mix(`${ra}.Out`);
      } else mix(`${kick}.Out`);
      const hat = add('Mortise', 'drums', { Engine: 0.5, Decay: 0.12, Tone: 0.8 });
      c(`${ru}.C`, `${hat}.Trig`); mix(`${hat}.Out`);
      if (style.snare) {
        const sn = add('Mortise', 'drums', { Engine: 0.25 });
        c(`${ru}.B`, `${sn}.Trig`); mix(`${sn}.Out`);
      }
    },
    bass() {
      const sq = add('Dovetail', 'bass');
      const fr = add('Fret', 'bass');
      const osc = add(r() < 0.6 ? 'Chisel' : 'Lathe', 'bass', { Tune: 0.25 });
      const flt = add('Plane', 'bass', style.acid ? { Res: 0.75, Cutoff: 0.3 } : {});
      const env = add('Bevel', 'bass', { Attack: 0.02, Sustain: 0.1 });
      const vca = add('Clamp', 'bass');
      c(`${tally}.Clock`, `${sq}.Clock`);
      c(`${sq}.Pitch`, `${fr}.Pitch`);
      c(`${fr}.Pitch`, `${osc}.V/Oct`);
      c(osc.startsWith('chisel') ? `${osc}.Saw` : `${osc}.Out`, `${flt}.In`);
      c(`${sq}.Gate`, `${env}.Gate`);
      c(`${env}.Env`, `${flt}.Cutoff`);
      let last = `${flt}.Out`;
      if (style.acid || style.grit) {
        const ra = add('Rasp', 'bass', { Drive: 0.5 });
        c(last, `${ra}.In`); last = `${ra}.Out`;
      }
      c(last, `${vca}.In`);
      c(`${env}.Env`, `${vca}.CV`);
      mix(`${vca}.Out`);
    },
    pad() {
      const bu = add('Burl', 'pad', { Smooth: 0.2 });
      const fr = add('Fret', 'pad');
      const ply = add('Ply', 'pad');
      const lfo = add('Spokeshave', 'pad', { Rate: 0.12 });
      const flt = add('Plane', 'pad', { Cutoff: 0.5, Res: 0.2 });
      c(`${tally}.Bar`, `${bu}.Trig`);
      c(`${bu}.Stepped`, `${fr}.Pitch`);
      c(`${fr}.Pitch`, `${ply}.V/Oct`);
      c(`${tally}.Bar`, `${ply}.Gate`);
      c(`${ply}.L`, `${flt}.In`);
      c(`${lfo}.1`, `${flt}.Cutoff`);
      mix(`${flt}.Out`);
    },
    lead() {
      const sq = add('Dovetail', 'lead', { Chance: 0.5, Glide: 0.5 });
      const osc = add('Lathe', 'lead');
      const lfo = add('Spokeshave', 'lead', { Rate: 0.2 });
      const env = add('Bevel', 'lead', { Attack: 0.3, Release: 0.6 });
      const vca = add('Clamp', 'lead', { Response: 0.7 });
      c(`${tally}.÷4`, `${sq}.Clock`);
      c(`${sq}.Pitch`, `${osc}.V/Oct`);
      c(`${lfo}.2`, `${osc}.Turn`);
      c(`${sq}.Gate`, `${env}.Gate`);
      c(`${osc}.Out`, `${vca}.In`);
      c(`${env}.Env`, `${vca}.CV`);
      mix(`${vca}.Out`);
    },
    texture() {
      const n = add('Sawdust', 'texture');
      if (style.grit) {
        const bu = add('Burl', 'texture');
        const flt = add('Plane', 'texture', { Mode: 0.33, Res: 0.6 });
        c(`${n}.Noise`, `${flt}.In`); c(`${bu}.Drift`, `${flt}.Cutoff`); mix(`${flt}.Out`);
      } else {
        const g = add('Shavings', 'texture');
        c(`${n}.Dust`, `${g}.In`); mix(`${g}.L`);
      }
    },
    perc() {
      const ru = ctx.ruler;
      const bu = add('Burl', 'perc');
      const ma = add('Mallet', 'perc');
      if (ru) c(`${ru}.D`, `${bu}.Trig`); else c(`${tally}.Clock`, `${bu}.Trig`);
      c(`${bu}.Coin`, `${ma}.Trig`);
      c(`${bu}.Stepped`, `${ma}.V/Oct`);
      mix(`${ma}.Out`);
    },
    keys() {
      const ve = add('Veneer', 'keys');
      const bu = add('Burl', 'keys');
      c(`${ctx.ruler ? ctx.ruler + '.D' : tally + '.÷4'}`, `${ve}.Trig`);
      c(`${bu}.Stepped`, `${ve}.Slice`);
      mix(`${ve}.L`);
    },
  };

  for (const part of only || style.parts) { usePart(part); builders[part](); }

  if (style.delay) {
    const d = add('Bandsaw', 'master', { Feedback: 0.55, Mix: 1 });
    c(`${router}.Send`, `${d}.In`); c(`${tally}.÷4`, `${d}.Clock`); mix(`${d}.L`);
  }
  const verb = add('Kiln', 'master');
  const fin = add('Finish', 'master');
  c(`${router}.Mix L`, `${verb}.In L`); c(`${router}.Mix R`, `${verb}.In R`);
  c(`${verb}.L`, `${fin}.L`); c(`${verb}.R`, `${fin}.R`);
  p.check = check(p);
  return p;
}

// Fake of kerf-gen::check. The real check renders 8 bars offline and measures.
export function check(p) {
  const r = rng(p.seed * 7 + p.mods.length);
  const retries = r() < 0.7 ? 0 : 1 + Math.floor(r() * 2);
  return {
    retries,
    peak: -(1.1 + r() * 2.4).toFixed(1),
    lufs: -(9 + r() * 6).toFixed(1),
    key: p.key,
    dc: false,
  };
}

const clone = (p) => structuredClone(p);
const partLocked = (p, id) => p.parts.find((x) => x.id === id)?.locked;

export function evolve(p, strength, seed) {
  const r = rng(seed);
  const n = clone(p);
  for (const m of n.mods) {
    if (m.modLocked || partLocked(n, m.part)) continue;
    for (const k in m.values) {
      if (m.locked.includes(k) || r() > 0.35 + strength * 0.5) continue;
      m.values[k] = clamp(m.values[k] + (r() - 0.5) * strength);
    }
    if (m.pattern) m.pattern = m.pattern.map((s) => (r() < strength * 0.25 ? !s : s));
  }
  n.seed = seed;
  n.origin = `evolve ${strength.toFixed(2)} seed ${seed}`;
  n.check = check(n);
  return n;
}

export function breed(a, b, seed) {
  const r = rng(seed);
  const n = clone(a);
  for (const m of n.mods) {
    const mate = b.mods.find((x) => x.uid === m.uid && x.type === m.type);
    if (!mate) continue;
    for (const k in m.values) if (!m.locked.includes(k) && r() < 0.5) m.values[k] = mate.values[k];
    if (m.pattern && mate.pattern && r() < 0.5) m.pattern = [...mate.pattern];
  }
  n.tempo = Math.round((a.tempo + b.tempo) / 2);
  n.seed = seed;
  n.origin = `breed seed ${seed}`;
  n.check = check(n);
  return n;
}

export function toText(p) {
  const f = (v) => v.toFixed(2);
  const lines = ['kerf 1', `tempo ${p.tempo}`, `key ${p.key}`, `style ${p.style} seed ${p.seed}`, ''];
  for (const part of p.parts) {
    lines.push(`part ${part.id} "${part.name}" colour ${part.colour}${part.locked ? ' lock' : ''}`);
    for (const m of p.mods.filter((x) => x.part === part.id)) {
      const vals = Object.entries(m.values).map(([k, v]) => `${k.toLowerCase().replace(/[^a-z0-9]+/g, '_')}=${f(v)}`).join(' ');
      lines.push(`  module ${m.uid.padEnd(11)} ${m.type.padEnd(10)} ${vals}${m.modLocked ? ' lock' : ''}`);
    }
    lines.push('');
  }
  for (const c of p.cables) lines.push(`cable ${c.from.padEnd(18)} -> ${c.to}`);
  for (const [slot, s] of Object.entries(p.scenes)) if (s) lines.push(`scene ${slot} (${Object.keys(s).length} modules)`);
  return lines.join('\n');
}

const jackOf = (ref) => { const i = ref.indexOf('.'); return [ref.slice(0, i), ref.slice(i + 1)]; };

// Plain-language trace of what feeds a module and where it goes.
export function explain(p, uid) {
  const byId = Object.fromEntries(p.mods.map((m) => [m.uid, m]));
  const name = (id) => `${byId[id].type} (${id})`;
  const up = new Set([uid]);
  const steps = [];
  const queue = [uid];
  while (queue.length) {
    const id = queue.shift();
    for (const c of p.cables) {
      const [fm, fj] = jackOf(c.from);
      const [tm, tj] = jackOf(c.to);
      if (tm !== id || fm === 'tally1' && id !== uid) continue;
      steps.push(`${name(tm)} takes ${fj} from ${name(fm)} into ${tj}.`);
      if (!up.has(fm)) { up.add(fm); queue.push(fm); }
    }
  }
  const down = new Set();
  const dq = [uid];
  while (dq.length) {
    const id = dq.shift();
    for (const c of p.cables) {
      const [fm] = jackOf(c.from);
      const [tm] = jackOf(c.to);
      if (fm === id && !down.has(tm)) { down.add(tm); dq.push(tm); }
    }
  }
  const path = [...down].map((id) => byId[id].type);
  return {
    upstream: up, downstream: down,
    text: [`${name(uid)}: ${MODULES[byId[uid].type].desc.toLowerCase()}.`, ...steps.slice(0, 8).reverse(),
      path.length ? `Then it travels through ${path.join(' → ')}.` : 'Nothing is patched from its outputs, so you will not hear it.'],
  };
}
