// Kerf Modular mockup: renders one module panel as HTML.
import { MODULES } from './modules.js';
import { ICON } from './icons.js';

const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);

const pt = (a, r) => {
  const t = (a * Math.PI) / 180;
  return `${(19 + r * Math.sin(t)).toFixed(2)} ${(19 - r * Math.cos(t)).toFixed(2)}`;
};
const arc = (a0, a1, r) => `M ${pt(a0, r)} A ${r} ${r} 0 ${a1 - a0 > 180 ? 1 : 0} 1 ${pt(a1, r)}`;
const TICKS = Array.from({ length: 11 }, (_, i) => pt(-135 + i * 27, 17.7).split(' '));

// Crafted knob: scale dots, value arc, skirt, gradient cap (gradients live in index.html), pointer.
export function knobSvg(v) {
  const a = -135 + v * 270;
  const [x1, y1] = pt(a, 2.5).split(' ');
  const [x2, y2] = pt(a, 8.4).split(' ');
  return `<svg viewBox="0 0 38 38">${TICKS.map(([x, y]) => `<circle class="tick" cx="${x}" cy="${y}" r=".7"/>`).join('')}` +
    `<path class="track" d="${arc(-135, 135, 14.8)}"/>` +
    (v > 0.004 ? `<path class="val" d="${arc(-135, a, 14.8)}"/>` : '') +
    `<circle class="skirt" cx="19" cy="19" r="11.6"/><circle class="cap" cx="19" cy="19" r="9.6"/>` +
    `<circle class="sheen" cx="19" cy="19" r="9.6"/><line class="ptr" x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}"/></svg>`;
}

function wave(seed) {
  let d = 'M0 17';
  for (let x = 0; x <= 100; x += 1) {
    const y = 17 + Math.sin(x * 0.9 + seed) * Math.sin(x * 0.13) * 13 * Math.abs(Math.sin(x * 0.05 + 1));
    d += ` L${x} ${y.toFixed(1)}`;
  }
  return `<svg class="wave" viewBox="0 0 100 34" preserveAspectRatio="none" width="100%"><path d="${d}"/></svg>`;
}

function jack(uid, j, dir) {
  return `<div class="jack k-${j.kind}" data-ref="${esc(uid)}.${esc(j.n)}" data-dir="${dir}" data-kind="${j.kind}"
    data-tip-title="${esc(j.n)} ${dir === 'in' ? 'in' : 'out'}" data-tip="${esc(j.tip)}" data-tip-kind="${j.kind}">
    <span class="hole"></span><label>${esc(j.n)}</label></div>`;
}

export function renderModule(mod, colour) {
  const d = MODULES[mod.type];
  const extra = d.steps
    ? `<div class="steps" data-steps>${(mod.pattern || []).map((on) => `<span class="${on ? 'on' : ''}"></span>`).join('')}</div>`
    : d.wave ? wave(mod.uid.length)
    : d.meter ? '<div class="meter" data-meter><span><i style="height:0"></i></span><span><i style="height:0"></i></span></div>'
    : '';
  const knobs = d.knobs.map((k) => {
    const v = mod.values[k.n];
    const locked = mod.locked.includes(k.n);
    return `<div class="knob${locked ? ' locked' : ''}" data-k="${esc(k.n)}" data-tip-title="${esc(k.n)}" data-tip="${esc(k.tip)}" data-tip-knob>
      ${knobSvg(v)}<label>${esc(k.n)}</label></div>`;
  }).join('');
  return `<div class="mod${mod.modLocked ? ' locked' : ''}" data-uid="${esc(mod.uid)}" style="--w:${d.hp};--part:${colour}">
    <div class="mod-head"><span class="screw"></span><b data-tip-title="${esc(mod.type)}" data-tip="${esc(d.desc)}. Drag knobs up and down; drag from a jack to patch a cable.">${esc(mod.type)}</b>
      <button class="lockbtn${mod.modLocked ? ' on' : ''}" data-lock data-tip-title="Lock module" data-tip="Evolve and Breed leave a locked module alone.">${mod.modLocked ? ICON.lock : ICON.unlock}</button><span class="screw"></span></div>
    <div class="mod-sub">${esc(d.desc)}${mod.uid.startsWith('cat-') ? '' : ` · ${esc(mod.uid)}`}</div>
    ${extra}
    <div class="knobs">${knobs}</div>
    <div class="jacks">
      ${d.ins.length ? `<div class="jackrow ins">${d.ins.map((j) => jack(mod.uid, j, 'in')).join('')}</div>` : ''}
      ${d.outs.length ? `<div class="jackrow outs">${d.outs.map((j) => jack(mod.uid, j, 'out')).join('')}</div>` : ''}
    </div>
  </div>`;
}
