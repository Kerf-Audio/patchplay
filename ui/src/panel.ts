// One module panel as HTML, drawn from its descriptor and (on the bench) its values.
import { knobHtml } from './knob';
import { ICON } from './icons';
import { demoSampleBlock, sampleBlock } from './sample';
import { isStepParam, rollBlock } from './roll';
import type { JackDesc, Module, ModuleDesc } from './types';
import { esc, takesSample } from './util';

function jack(uid: string, j: JackDesc, dir: 'in' | 'out'): string {
  return `<div class="jack k-${esc(j.kind)}" data-ref="${esc(uid)}.${esc(j.key)}" data-dir="${dir}" data-kind="${esc(j.kind)}"
    data-tip-title="${esc(j.label)} ${dir}" data-tip="${esc(j.tooltip)}" data-tip-kind="${esc(j.kind)}">
    <span class="hole"></span><label>${esc(j.label)}</label></div>`;
}

/** `mod` is null for a catalogue panel, which shows the defaults and is not wired to anything. */
export function renderPanel(d: ModuleDesc, mod: Module | null, colour: string): string {
  const uid = mod?.id ?? `cat-${d.kind}`;
  const locked = mod?.locked ?? false;
  const sampler = takesSample(d.kind) ? (mod ? sampleBlock(mod) : demoSampleBlock(d)) : '';
  const roll = d.kind === 'Inlay' ? rollBlock(d, mod) : '';
  const extra =
    d.kind === 'Finish'
      ? '<div class="meter" data-meter><span><i style="height:0"></i></span><span><i style="height:0"></i></span></div>'
      : sampler || roll;
  // The roll draws the step params, so they get no knobs.
  const knobs = d.params
    .filter((p) => !(roll && isStepParam(p.key)))
    .map((p) => {
      const have = mod?.params.find((q) => q.key === p.key);
      const v = have?.value ?? p.default;
      const live = mod ? ` tabindex="0" role="slider" aria-label="${esc(p.label)}" aria-valuemin="0" aria-valuemax="1" aria-valuenow="${v.toFixed(2)}"` : '';
      return `<div class="knob${have?.locked ? ' locked' : ''}" data-k="${esc(p.key)}"${live} data-tip-title="${esc(p.label)}" data-tip="${esc(p.tooltip)}" data-tip-knob>
      ${knobHtml(v)}<label>${esc(p.label)}</label></div>`;
    })
    .join('');
  const lockButton = mod
    ? `<button class="lockbtn${locked ? ' on' : ''}" data-lock aria-label="Lock module" aria-pressed="${locked}" data-tip-title="Lock module" data-tip="Evolve and Breed leave a locked module alone.">${locked ? ICON.lock : ICON.unlock}</button>
      <button class="lockbtn" data-autopatch aria-label="Auto-patch module" data-tip-title="Auto-patch" data-tip="Wire this module into the patch with sensible cables. Existing cables are never touched.">${ICON.wand}</button>
      <button class="lockbtn" data-remove aria-label="Remove module" data-tip-title="Remove module" data-tip="Take this module and its cables out. Undo brings it back.">${ICON.x}</button>`
    : `<button class="lockbtn" data-add="${esc(d.kind)}" aria-label="Add ${esc(d.kind)} to patch" data-tip-title="Add to patch" data-tip="Add a ${esc(d.kind)} to the patch. Drag it onto the rack to place it.">${ICON.plus}</button>`;
  return `<div class="mod${locked ? ' locked' : ''}${sampler ? ' sampler' : ''}${roll ? ' roll-mod' : ''}" data-uid="${esc(uid)}" data-kind="${esc(d.kind)}" style="--w:${d.hp};--part:${esc(colour)}">
    <div class="mod-head"><span class="screw"></span><b data-tip-title="${esc(d.kind)}" data-tip="${esc(d.summary)} Drag knobs up and down; drag from a jack to patch a cable.">${esc(d.kind)}</b>${lockButton}<span class="screw"></span></div>
    <div class="mod-sub">${esc(d.summary)}${mod ? ` · ${esc(uid)}` : ''}</div>
    ${extra}
    <div class="knobs">${knobs}</div>
    <div class="jacks">
      ${d.inputs.length ? `<div class="jackrow ins">${d.inputs.map((j) => jack(uid, j, 'in')).join('')}</div>` : ''}
      ${d.outputs.length ? `<div class="jackrow outs">${d.outputs.map((j) => jack(uid, j, 'out')).join('')}</div>` : ''}
    </div>
  </div>`;
}
