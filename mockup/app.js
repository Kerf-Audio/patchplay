// Kerf Modular mockup: controller. Every GUI action is sent as a text command
// through exec(), the same path the console (CLI) and agents use.
import { MODULES, BANDS, KINDS } from './modules.js';
import { STYLES, PRESETS, PARTS, generate, evolve, breed, toText, explain } from './gen.js';
import { renderModule, knobSvg } from './panel.js';
import { ICON } from './icons.js';

const $ = (s, el = document) => el.querySelector(s);
const $$ = (s, el = document) => [...el.querySelectorAll(s)];
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
const u = (s) => s.replace(/ /g, '_');
const clamp = (v) => Math.min(1, Math.max(0, v));
const newSeed = () => 1000 + Math.floor(Math.random() * 9000);
const BAND_COLOUR = { time: '#d08a1e', motion: '#3d63c9', voices: '#2f8f6b', shape: '#8a4fb0', space: '#4f7f9c', out: '#6b645a' };

const lineage = [];
let cur = null;
let mate = null;
const undo = [];
let explainOn = false;
let playing = false;
let step = 0;
let timer = null;
const catMods = {};

const node = (id) => lineage.find((n) => n.id === id);
const patch = () => node(cur).patch;
const findMod = (uid) => patch().mods.find((m) => m.uid === uid);
const getMod = (uid) => findMod(uid) || catMods[uid];
const splitRef = (ref) => { const i = ref.indexOf('.'); return i < 0 ? [ref, ''] : [ref.slice(0, i), ref.slice(i + 1).replace(/_/g, ' ')]; };
const partOf = (p, id) => p.parts.find((x) => x.id === id);

// ---------- command interpreter ----------
function log(html, cls = '') {
  const el = $('#log');
  el.insertAdjacentHTML('beforeend', `<span class="${cls}">${html}</span>\n`);
  el.scrollTop = el.scrollHeight;
}

function addNode(p, label) {
  const id = lineage.length + 1;
  if (cur) undo.push({ cur, patch: structuredClone(patch()) });
  lineage.push({ id, parent: cur, patch: p, label });
  cur = id;
  return `#${id} ${label}`;
}

function snapshot() { undo.push({ cur, patch: structuredClone(patch()) }); }

function jackExists(ref) {
  const [uid, j] = splitRef(ref);
  const m = findMod(uid);
  if (!m) return null;
  const d = MODULES[m.type];
  if (d.ins.some((x) => x.n === j)) return 'in';
  if (d.outs.some((x) => x.n === j)) return 'out';
  return null;
}

const COMMANDS = {
  help: () => Object.keys(COMMANDS).join(' · '),
  random([style = $('#style').value, seed]) {
    if (!STYLES[style]) throw new Error(`unknown style "${style}"`);
    const s = seed ? +seed : newSeed();
    return addNode(generate(style, s), `random ${style} · ${s}`);
  },
  preset(args) {
    const name = args.join(' ').toLowerCase();
    const pr = PRESETS.find((p) => p.name.toLowerCase() === name);
    if (!pr) throw new Error(`unknown preset "${name}"`);
    return addNode(generate(pr.style, pr.seed, pr.only), pr.name);
  },
  evolve(args, opts) {
    const strength = args[0] ? +args[0] : +$('#strength').value;
    const seed = args[1] ? +args[1] : newSeed();
    const src = structuredClone(patch());
    if (opts.part) {
      if (!partOf(src, opts.part)) throw new Error(`no part "${opts.part}"`);
      for (const pt of src.parts) pt.locked = pt.locked || pt.id !== opts.part;
    }
    const out = evolve(src, strength, seed);
    out.parts = structuredClone(patch().parts);
    return addNode(out, `evolve ${strength}${opts.part ? ' ' + opts.part : ''} · ${seed}`);
  },
  breed([a, b, seed]) {
    const na = node(+String(a).replace('#', ''));
    const nb = node(+String(b).replace('#', ''));
    if (!na || !nb) throw new Error('breed needs two lineage ids');
    const s = seed ? +seed : newSeed();
    return addNode(breed(na.patch, nb.patch, s), `breed #${na.id}×#${nb.id} · ${s}`);
  },
  checkout([id]) {
    const n = node(+String(id).replace('#', ''));
    if (!n) throw new Error(`no node ${id}`);
    undo.push({ cur, patch: structuredClone(patch()) });
    cur = n.id;
    return `at #${n.id}`;
  },
  set([ref, value]) {
    const [uid, k] = splitRef(ref);
    const m = findMod(uid);
    if (!m || !(k in m.values)) throw new Error(`no knob ${ref}`);
    snapshot();
    m.values[k] = clamp(+value);
    return `${uid}.${u(k)} = ${m.values[k].toFixed(2)}`;
  },
  lock: (args) => lockCmd(args[0], true),
  unlock: (args) => lockCmd(args[0], false),
  connect(args) {
    let [a, , b] = args;
    const da = jackExists(a); const db = jackExists(b);
    if (!da || !db) throw new Error('unknown jack');
    if (da === db) throw new Error('connect an output to an input');
    if (da === 'in') [a, b] = [b, a];
    const from = splitRef(a).join('.'); const to = splitRef(b).join('.');
    snapshot();
    patch().cables.push({ from, to });
    return `${u(from)} → ${u(to)}`;
  },
  disconnect(args) {
    const from = splitRef(args[0]).join('.'); const to = splitRef(args[2]).join('.');
    const cs = patch().cables;
    const i = cs.findIndex((c) => (c.from === from && c.to === to) || (c.from === to && c.to === from));
    if (i < 0) throw new Error('no such cable');
    snapshot();
    cs.splice(i, 1);
    return 'removed';
  },
  mute: ([id]) => { const pt = partOf(patch(), id); if (!pt) throw new Error(`no part ${id}`); pt.muted = !pt.muted; return `${id} ${pt.muted ? 'muted' : 'unmuted'}`; },
  tempo([bpm]) {
    const t = patch().mods.find((m) => m.type === 'Tally');
    snapshot();
    t.values.Tempo = clamp((+bpm - 40) / 200);
    patch().tempo = Math.round(+bpm);
    return `${patch().tempo} bpm`;
  },
  scene([op, slot]) {
    const p = patch();
    slot = (slot || '').toUpperCase();
    if (!'ABCD'.includes(slot) || !slot) throw new Error('scene store|recall|clear A–D');
    if (op === 'store') { p.scenes[slot] = Object.fromEntries(p.mods.map((m) => [m.uid, { ...m.values }])); return `stored ${slot}`; }
    if (op === 'clear') { delete p.scenes[slot]; return `cleared ${slot}`; }
    if (op === 'recall') { snapshot(); applyMorph(slot, slot, 0); return `recalled ${slot}`; }
    throw new Error('scene store|recall|clear A–D');
  },
  morph([a, b, t]) {
    if (!patch().scenes[a] || !patch().scenes[b]) throw new Error('store both scenes first');
    snapshot();
    applyMorph(a, b, +t);
    return `${a}→${b} ${(+t).toFixed(2)}`;
  },
  undo() {
    const s = undo.pop();
    if (!s) throw new Error('nothing to undo');
    cur = s.cur;
    node(cur).patch = s.patch;
    return `at #${cur}`;
  },
};

function lockCmd(target, on) {
  if (!target) throw new Error('lock <module> | <module>.<knob> | part:<id>');
  const p = patch();
  if (target.startsWith('part:')) {
    const pt = partOf(p, target.slice(5));
    if (!pt) throw new Error(`no part ${target}`);
    pt.locked = on;
  } else {
    const [uid, k] = splitRef(target);
    const m = findMod(uid);
    if (!m) throw new Error(`no module ${uid}`);
    if (k) {
      if (!(k in m.values)) throw new Error(`no knob ${target}`);
      m.locked = on ? [...new Set([...m.locked, k])] : m.locked.filter((x) => x !== k);
    } else m.modLocked = on;
  }
  return `${target} ${on ? 'locked' : 'unlocked'}`;
}

function applyMorph(a, b, t) {
  const p = patch();
  const A = p.scenes[a]; const B = p.scenes[b];
  for (const m of p.mods) {
    if (m.modLocked || partOf(p, m.part)?.locked || !A[m.uid] || !B[m.uid]) continue;
    for (const k in m.values) if (!m.locked.includes(k)) m.values[k] = A[m.uid][k] + (B[m.uid][k] - A[m.uid][k]) * t;
  }
}

function tokenize(line) {
  return (line.match(/"[^"]*"|\S+/g) || []).map((t) => t.replace(/^"|"$/g, ''));
}

export function exec(line, src = 'gui') {
  const toks = tokenize(line.trim());
  if (!toks.length) return;
  const [name, ...rest] = toks;
  const opts = {}; const args = [];
  for (const t of rest) { const m = t.match(/^(\w+)=(.+)$/); if (m) opts[m[1]] = m[2]; else args.push(t); }
  log(`<span class="src">${src}</span>› ${esc(line)}`);
  const fn = COMMANDS[name];
  try {
    if (!fn) throw new Error(`unknown command "${name}" — try help`);
    const res = fn(args, opts);
    log(`<span class="src"></span>  ${esc(res)}`, 'ok');
  } catch (e) {
    log(`<span class="src"></span>  ✗ ${esc(e.message)}`, 'err');
  }
  render();
}

// ---------- rendering ----------
function render() {
  const p = patch();
  $('#bpm').textContent = p.tempo;
  $('#key').textContent = p.key;
  $('#style').value = p.style;
  renderBench();
  renderLineage();
  renderScenes();
  $('#text').textContent = toText(p);
  const c = p.check;
  const chk = $('#check');
  chk.innerHTML = `${ICON.check}<span>checked · audible · peak ${c.peak} dBFS · ${c.lufs} LUFS · ${esc(c.key)}${c.retries ? ` · ${c.retries} seed${c.retries > 1 ? 's' : ''} rejected` : ''}</span>`;
  chk.dataset.tipTitle = 'Safe random';
  chk.dataset.tip = 'Before you see a patch it is rendered offline for 8 bars and checked: audible, no clipping, no DC, no runaway feedback, in key. Failing seeds are skipped. (Values are fake in this mockup.)';
  $('#btn-breed').disabled = !mate || mate === cur;
  if (playing) restartClock();
}

function renderBench() {
  const p = patch();
  const order = [...p.parts.filter((x) => x.id !== 'master'), ...p.parts.filter((x) => x.id === 'master')];
  $('#bench').innerHTML = order.map((pt) => {
    const n = p.mods.filter((m) => m.part === pt.id).length;
    return `
    <div class="part-row${pt.muted ? ' muted' : ''}" style="--part:${pt.colour}">
      <div class="part-label">
        <div class="part-name"><b>${esc(pt.name)}</b><span class="count">${n} module${n === 1 ? '' : 's'}</span></div>
        <div class="tools">
          <button data-part-lock="${pt.id}" class="${pt.locked ? 'on' : ''}" data-tip-title="Lock part" data-tip="Evolve leaves every module in this part alone.">${pt.locked ? ICON.lock : ICON.unlock}</button>
          ${pt.id !== 'master' ? `<button data-part-mute="${pt.id}" class="${pt.muted ? 'on' : ''}" data-tip-title="Mute part" data-tip="Silences this part while the rest plays.">${ICON.mute}</button>
          <button data-part-evolve="${pt.id}" data-tip-title="Evolve this part" data-tip="Mutates only this part, at the current strength. Everything else stays.">${ICON.evolve}</button>` : ''}
        </div>
      </div>
      ${p.mods.filter((m) => m.part === pt.id).map((m) => renderModule(m, pt.colour)).join('')}
    </div>`;
  }).join('') + '<svg id="cables"></svg>';
  requestAnimationFrame(drawCables);
}

function holeCentre(ref) {
  const el = $(`#bench .jack[data-ref="${CSS.escape(ref)}"] .hole`);
  if (!el) return null;
  const b = $('#bench').getBoundingClientRect();
  const r = el.getBoundingClientRect();
  return [r.left - b.left + r.width / 2, r.top - b.top + r.height / 2];
}

const cablePath = ([x1, y1], [x2, y2]) => {
  const sag = Math.min(140, Math.hypot(x2 - x1, y2 - y1) * 0.3) + 25;
  return `M${x1} ${y1} C${x1} ${y1 + sag} ${x2} ${y2 + sag} ${x2} ${y2}`;
};

function drawCables() {
  const svg = $('#cables');
  if (!svg) return;
  const bench = $('#bench');
  svg.setAttribute('width', bench.scrollWidth);
  svg.setAttribute('height', bench.scrollHeight);
  const p = patch();
  let html = '';
  p.cables.forEach((c, i) => {
    const a = holeCentre(c.from); const b = holeCentre(c.to);
    if (!a || !b) return;
    const kind = $(`#bench .jack[data-ref="${CSS.escape(c.from)}"]`).dataset.kind;
    const muted = partOf(p, findMod(splitRef(c.from)[0]).part)?.muted;
    const d = cablePath(a, b);
    html += `<path class="shadow" d="${d}"/><path d="${d}" data-i="${i}" class="${muted ? 'faint' : ''}" style="stroke:var(--${kind})"
      data-tip-title="Cable" data-tip="${esc(u(c.from))} → ${esc(u(c.to))}. Click to remove."/><path class="sheen" d="${d}"/>`;
  });
  svg.innerHTML = html;
}

function renderLineage() {
  const kids = {};
  for (const n of lineage) (kids[n.parent ?? 0] ||= []).push(n);
  const rows = [];
  const walk = (parent, depth, guides) => {
    const list = kids[parent] || [];
    list.forEach((n, i) => {
      const last = i === list.length - 1;
      const colour = PARTS[n.patch.parts[1]?.id || 'master'].colour;
      const tree = depth ? guides.map((g) => `<i class="g${g ? ' v' : ''}"></i>`).join('') + `<i class="g ${last ? 'l' : 't'}"></i>` : '';
      rows.push(`<div class="node${n.id === cur ? ' cur' : ''}${n.id === mate ? ' mate' : ''}" data-node="${n.id}">
        <span class="tree">${tree}</span><span class="dot" style="background:${colour}"></span>
        <span class="nlabel"><b>#${n.id}</b> ${esc(n.label.split(' · ')[0])}</span><small>${esc(n.label.split(' · ')[1] || '')}</small>
        <span class="mate-btn${n.id === mate ? ' on' : ''}" data-mate="${n.id}" data-tip-title="Pick as mate" data-tip="Breed crosses the current patch with this one.">${n.id === mate ? ICON.heartOn : ICON.heart}</span></div>`);
      walk(n.id, depth + 1, depth ? [...guides, !last] : guides);
    });
  };
  walk(0, 0, []);
  $('#lineage').innerHTML = rows.join('');
}

function renderScenes() {
  const s = patch().scenes;
  $('#scenes').innerHTML = 'ABCD'.split('').map((k) => `<button class="scene${s[k] ? ' full' : ''}" data-scene="${k}"
    data-tip-title="Scene ${k}" data-tip="${s[k] ? 'Click to recall. Right-click to clear.' : 'Empty. Click to store every knob as it is now.'}">${k}</button>`).join('');
}

function renderCatalogue() {
  const legend = Object.entries(KINDS).map(([k, v]) => `<span class="k-${k}" data-tip-title="${v.label}" data-tip="${esc(v.tip)}"><i></i>${v.label}</span>`).join('');
  $('#catalogue').innerHTML = `<p class="intro">Twenty-five modules in six bands, all named after joinery and woodwork. Hover any knob or jack for help. Every jack has a kind and colour; the colours are a guide, and any output can go to any input.</p>
    <div class="legend">${legend}</div>` +
    BANDS.map((b) => {
      const types = Object.keys(MODULES).filter((t) => MODULES[t].band === b.id);
      const panels = types.map((t) => {
        const uid = `cat-${t}`;
        catMods[uid] ||= { uid, type: t, values: Object.fromEntries(MODULES[t].knobs.map((k) => [k.n, k.v])), locked: [], modLocked: false, pattern: MODULES[t].steps ? Array.from({ length: 16 }, (_, i) => i % 3 === 0 || i % 5 === 0) : null };
        return renderModule(catMods[uid], BAND_COLOUR[b.id]);
      }).join('');
      return `<h2 style="color:${BAND_COLOUR[b.id]}">${b.name}<small>${esc(b.tip)}</small></h2><div class="cat-row">${panels}</div>`;
    }).join('');
}

function renderStyles() {
  const chips = (parts) => parts.map((p) => `<span class="chip" style="--c:${PARTS[p].colour}">${PARTS[p].name}</span>`).join('');
  $('#styles').innerHTML = `<h2>Styles<small>rulebooks for Random: parts, modules, tempo, key, ranges</small></h2><div class="cards">` +
    Object.entries(STYLES).map(([id, s]) => `<div class="card"><h4>${esc(s.name)}</h4><div class="chips">${chips(s.parts)}</div>
      <p>${esc(s.blurb)}</p><p>${s.tempo[0]}–${s.tempo[1]} bpm · ${s.keys.join(', ')}</p>
      <button class="btn primary" data-go="random ${id}">${ICON.random}Random in this style</button></div>`).join('') +
    `</div><h2>Presets<small>hand-picked seeds; each opens as a new lineage root you can evolve</small></h2><div class="cards">` +
    PRESETS.map((p) => `<div class="card"><h4>${esc(p.name)}</h4><div class="chips">${chips(p.only || STYLES[p.style].parts)}</div>
      <p>${esc(p.blurb)}</p><p>${esc(STYLES[p.style].name)} · seed ${p.seed}</p>
      <button class="btn" data-go="preset ${esc(p.name)}">Load</button></div>`).join('') + '</div>';
}

// ---------- explain ----------
function showExplain(uid) {
  const p = patch();
  const ex = explain(p, uid);
  const lit = new Set([...ex.upstream, ...ex.downstream]);
  $$('#bench .mod').forEach((el) => {
    el.classList.toggle('dim', !lit.has(el.dataset.uid));
    el.classList.toggle('focus', el.dataset.uid === uid);
  });
  $$('#cables path[data-i]').forEach((el) => {
    const c = p.cables[+el.dataset.i];
    const on = lit.has(splitRef(c.from)[0]) && lit.has(splitRef(c.to)[0]);
    el.classList.toggle('hot', on); el.classList.toggle('faint', !on);
  });
  $('#explain-text').innerHTML = ex.text.map((t, i) => `<p class="${i === 0 ? 'lead' : ''}">${esc(t)}</p>`).join('');
}
function clearExplain() {
  $$('#bench .mod').forEach((el) => el.classList.remove('dim', 'focus'));
  drawCables();
}

// ---------- tooltip ----------
const tip = $('#tip');
function tipFor(el) {
  let body = `<b>${esc(el.dataset.tipTitle || '')}${el.dataset.tipKind ? `<span class="kindtag k-${el.dataset.tipKind}">${KINDS[el.dataset.tipKind].label}</span>` : ''}</b>${esc(el.dataset.tip)}`;
  if (el.hasAttribute('data-tip-knob')) {
    const m = getMod(el.closest('.mod').dataset.uid);
    const k = el.dataset.k;
    body += `<div class="meta">value ${m.values[k].toFixed(2)}${m.locked.includes(k) ? ' · locked' : ''} · drag to change · Alt-click to ${m.locked.includes(k) ? 'unlock' : 'lock'}</div>`;
  }
  if (el.classList.contains('jack') && findMod(el.dataset.ref.split('.')[0])) {
    const ref = el.dataset.ref;
    const n = patch().cables.filter((c) => c.from === ref || c.to === ref).length;
    body += `<div class="meta">${n ? `${n} cable${n > 1 ? 's' : ''} · double-click to unplug` : 'drag to patch'}</div>`;
  }
  return body;
}
function placeTip(e) {
  const x = Math.min(e.clientX + 14, innerWidth - 280);
  const y = e.clientY + 18 + tip.offsetHeight > innerHeight ? e.clientY - tip.offsetHeight - 10 : e.clientY + 18;
  tip.style.left = `${x}px`; tip.style.top = `${y}px`;
}
document.addEventListener('mouseover', (e) => {
  const el = e.target.closest('[data-tip]');
  if (!el) { tip.classList.add('hidden'); return; }
  tip.innerHTML = tipFor(el); tip.classList.remove('hidden'); placeTip(e);
});
document.addEventListener('mousemove', (e) => { if (!tip.classList.contains('hidden')) placeTip(e); });

// ---------- interaction ----------
let drag = null;
document.addEventListener('pointerdown', (e) => {
  const knob = e.target.closest('.knob');
  const jack = e.target.closest('.jack');
  if (knob) {
    const uid = knob.closest('.mod').dataset.uid;
    const k = knob.dataset.k;
    const m = getMod(uid);
    if (e.altKey && findMod(uid)) { exec(`${m.locked.includes(k) ? 'unlock' : 'lock'} ${uid}.${u(k)}`); return; }
    drag = { type: 'knob', el: knob, m, k, uid, y: e.clientY, v0: m.values[k] };
    knob.setPointerCapture(e.pointerId);
    e.preventDefault();
  } else if (jack && findMod(jack.dataset.ref.split('.')[0])) {
    drag = { type: 'jack', ref: jack.dataset.ref, dir: jack.dataset.dir, kind: jack.dataset.kind, start: holeCentre(jack.dataset.ref) };
    $('#cables').insertAdjacentHTML('beforeend', `<path class="temp" id="temp" style="stroke:var(--${drag.kind})"/>`);
    e.preventDefault();
  }
});
document.addEventListener('pointermove', (e) => {
  if (!drag) return;
  if (drag.type === 'knob') {
    const v = clamp(drag.v0 + ((drag.y - e.clientY) / 160) * (e.shiftKey ? 0.2 : 1));
    drag.m.values[drag.k] = v;
    drag.el.querySelector('svg').outerHTML = knobSvg(v);
    if (drag.m.type === 'Tally' && drag.k === 'Tempo' && findMod(drag.uid)) $('#bpm').textContent = Math.round(40 + v * 200);
    tip.innerHTML = tipFor(drag.el);
  } else {
    const b = $('#bench').getBoundingClientRect();
    const end = [e.clientX - b.left, e.clientY - b.top];
    $('#temp')?.setAttribute('d', cablePath(drag.start, end));
    $$('.jack.target').forEach((j) => j.classList.remove('target'));
    const over = document.elementFromPoint(e.clientX, e.clientY)?.closest('.jack');
    if (over && over.dataset.dir !== drag.dir) over.classList.add('target');
  }
});
document.addEventListener('pointerup', (e) => {
  if (!drag) return;
  const d = drag; drag = null;
  if (d.type === 'knob') {
    const v = d.m.values[d.k];
    if (findMod(d.uid) && Math.abs(v - d.v0) > 0.001) {
      d.m.values[d.k] = d.v0;
      if (d.m.type === 'Tally' && d.k === 'Tempo') exec(`tempo ${Math.round(40 + v * 200)}`);
      else exec(`set ${d.uid}.${u(d.k)} ${v.toFixed(2)}`);
    }
    return;
  }
  $('#temp')?.remove();
  $$('.jack.target').forEach((j) => j.classList.remove('target'));
  const over = document.elementFromPoint(e.clientX, e.clientY)?.closest('.jack');
  if (over && over.dataset.dir !== d.dir) exec(`connect ${u(d.ref)} -> ${u(over.dataset.ref)}`);
});
document.addEventListener('dblclick', (e) => {
  const jack = e.target.closest('#bench .jack');
  if (!jack) return;
  const ref = jack.dataset.ref;
  for (const c of patch().cables.filter((x) => x.from === ref || x.to === ref)) exec(`disconnect ${u(c.from)} -> ${u(c.to)}`);
});
document.addEventListener('click', (e) => {
  const t = e.target;
  const path = t.closest('#cables path[data-i]');
  if (path) { const c = patch().cables[+path.dataset.i]; exec(`disconnect ${u(c.from)} -> ${u(c.to)}`); return; }
  const lk = t.closest('#bench [data-lock]');
  if (lk) { const m = findMod(lk.closest('.mod').dataset.uid); exec(`${m.modLocked ? 'unlock' : 'lock'} ${m.uid}`); return; }
  if (t.dataset.partLock) { const pt = partOf(patch(), t.dataset.partLock); exec(`${pt.locked ? 'unlock' : 'lock'} part:${pt.id}`); return; }
  if (t.dataset.partMute) { exec(`mute ${t.dataset.partMute}`); return; }
  if (t.dataset.partEvolve) { exec(`evolve ${$('#strength').value} part=${t.dataset.partEvolve}`); return; }
  if (t.dataset.mate) { mate = mate === +t.dataset.mate ? null : +t.dataset.mate; render(); return; }
  const nd = t.closest('[data-node]');
  if (nd) { exec(`checkout ${nd.dataset.node}`); return; }
  if (t.dataset.scene) { const k = t.dataset.scene; exec(patch().scenes[k] ? `scene recall ${k}` : `scene store ${k}`); return; }
  if (t.dataset.go) { exec(t.dataset.go); showView('bench'); return; }
  if (t.dataset.view) { showView(t.dataset.view); return; }
  if (t.dataset.con) {
    $$('[data-con]').forEach((b) => b.classList.toggle('on', b === t));
    $('#log').classList.toggle('hidden', t.dataset.con !== 'log');
    $('#text').classList.toggle('hidden', t.dataset.con !== 'text');
  }
});
document.addEventListener('contextmenu', (e) => {
  const s = e.target.closest('[data-scene]');
  if (s && patch().scenes[s.dataset.scene]) { e.preventDefault(); exec(`scene clear ${s.dataset.scene}`); }
});
$('#bench').addEventListener('mouseover', (e) => {
  if (!explainOn) return;
  const m = e.target.closest('.mod');
  if (m) showExplain(m.dataset.uid);
});
$('#bench').addEventListener('mouseleave', () => explainOn && clearExplain());

function showView(v) {
  $$('nav button').forEach((b) => b.classList.toggle('on', b.dataset.view === v));
  $('#bench-scroll').classList.toggle('hidden', v !== 'bench');
  $('#catalogue').classList.toggle('hidden', v !== 'catalogue');
  $('#styles').classList.toggle('hidden', v !== 'styles');
  $('#toolbar').classList.toggle('hidden', v !== 'bench');
  if (v === 'bench') requestAnimationFrame(drawCables);
}

$('#btn-random').onclick = () => exec(`random ${$('#style').value}`);
$('#btn-evolve').onclick = () => exec(`evolve ${$('#strength').value}`);
$('#btn-breed').onclick = () => exec(`breed ${cur} ${mate}`);
$('#btn-undo').onclick = () => exec('undo');
$('#btn-explain').onclick = () => toggleExplain();
function toggleExplain() {
  explainOn = !explainOn;
  $('#btn-explain').classList.toggle('on', explainOn);
  if (!explainOn) { clearExplain(); $('#explain-text').innerHTML = '<p class="hint">Turn on Explain and point at a module.</p>'; }
}
$('#morph').addEventListener('input', (e) => {
  const p = patch();
  if (!p.scenes.A || !p.scenes.B) return;
  applyMorph('A', 'B', +e.target.value);
  renderBench();
});
$('#morph').addEventListener('change', (e) => {
  if (!patch().scenes.A || !patch().scenes.B) { log('  ✗ store scenes A and B first', 'err'); return; }
  exec(`morph A B ${(+e.target.value).toFixed(2)}`);
});
$('#cmd').addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && e.target.value.trim()) { exec(e.target.value, 'cli'); e.target.value = ''; }
});
document.addEventListener('keydown', (e) => {
  if (e.target.matches('input, select, textarea')) return;
  const k = e.key.toLowerCase();
  if (k === 'r') $('#btn-random').click();
  else if (k === 'e') $('#btn-evolve').click();
  else if (k === 'z') exec('undo');
  else if (k === 'x') toggleExplain();
  else if (k === ' ') { e.preventDefault(); togglePlay(); }
});
addEventListener('resize', () => requestAnimationFrame(drawCables));

// ---------- clock (cosmetic only) ----------
function tickLights() {
  step = (step + 1) % 16;
  $$('[data-steps]').forEach((el) => [...el.children].forEach((s, i) => s.classList.toggle('now', i === step)));
  const lvl = playing ? 45 + Math.random() * 40 + (step % 4 === 0 ? 15 : 0) : 0;
  $$('[data-meter] i').forEach((i, n) => { i.style.height = `${Math.min(100, lvl - n * 4)}%`; });
}
function restartClock() {
  clearInterval(timer);
  timer = setInterval(tickLights, 60000 / patch().tempo / 4);
}
function togglePlay() {
  playing = !playing;
  $('#play').classList.toggle('on', playing);
  $('#play').innerHTML = playing ? ICON.stop : ICON.play;
  if (playing) restartClock(); else { clearInterval(timer); tickLights(); }
}
$('#play').onclick = togglePlay;

// ---------- boot ----------
$$('[data-ico]').forEach((b) => b.insertAdjacentHTML('afterbegin', ICON[b.dataset.ico]));
$('#play').innerHTML = ICON.play;
$('#style').innerHTML = Object.entries(STYLES).map(([id, s]) => `<option value="${id}">${s.name}</option>`).join('');
renderCatalogue();
renderStyles();
exec('preset Harbour Fog', 'cli');
exec('lock part:drums', 'agent');
exec('evolve 0.3 2207', 'agent');
