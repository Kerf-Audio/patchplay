import { describe, expect, it } from 'vitest';
import { shortcutFor, type KeyPress, type KeyTarget } from './shortcuts';

const press = (key: string, extra: Partial<KeyPress> = {}): KeyPress => ({
  key,
  metaKey: false,
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  repeat: false,
  ...extra,
});
const on = (target: KeyTarget, key: string, extra: Partial<KeyPress> = {}, onBench = true) =>
  shortcutFor(press(key, extra), target, onBench);

describe('shortcutFor', () => {
  it('maps the plain keys', () => {
    expect(on('page', ' ')).toBe('play');
    expect(on('page', 'r')).toBe('random');
    expect(on('page', 'E')).toBe('evolve');
    expect(on('page', 'x')).toBe('explain');
    expect(on('page', 'q')).toBeNull();
  });

  it('maps the command chords, Ctrl or Cmd', () => {
    expect(on('page', 'z', { metaKey: true })).toBe('undo');
    expect(on('page', 'z', { ctrlKey: true, shiftKey: true })).toBe('redo');
    expect(on('page', 'y', { ctrlKey: true })).toBe('redo');
    expect(on('page', 's', { metaKey: true })).toBe('save');
    expect(on('page', 'S', { metaKey: true, shiftKey: true })).toBe('save-as');
    expect(on('page', 'o', { metaKey: true })).toBe('open');
    expect(on('page', 'r', { metaKey: true })).toBeNull();
  });

  it('does not re-fire random, evolve, play or explain while a key is held', () => {
    for (const key of [' ', 'r', 'e', 'x']) expect(on('page', key, { repeat: true })).toBeNull();
  });

  it('does not repeat save or open while their chord is held', () => {
    expect(on('page', 's', { metaKey: true, repeat: true })).toBeNull();
    expect(on('page', 'o', { ctrlKey: true, repeat: true })).toBeNull();
  });

  it('lets a held undo or redo keep stepping through the history', () => {
    expect(on('page', 'z', { metaKey: true, repeat: true })).toBe('undo');
    expect(on('page', 'z', { metaKey: true, shiftKey: true, repeat: true })).toBe('redo');
  });

  it('leaves Space to a focused button, but not the letter keys', () => {
    expect(on('button', ' ')).toBeNull();
    expect(on('button', 'r')).toBe('random');
    expect(on('button', 'z', { metaKey: true })).toBe('undo');
  });

  it('does nothing while typing in a field', () => {
    for (const key of [' ', 'r', 'e', 'x']) expect(on('field', key)).toBeNull();
    expect(on('field', 'z', { metaKey: true })).toBeNull();
    expect(on('field', 's', { metaKey: true })).toBeNull();
  });

  it('ignores Alt chords, which lock things on the bench', () => {
    expect(on('page', 'r', { altKey: true })).toBeNull();
  });

  it('keeps random, evolve and explain to the workbench, whose toolbar the other views hide', () => {
    for (const key of ['r', 'e', 'x']) expect(on('page', key, {}, false)).toBeNull();
    expect(on('page', ' ', {}, false)).toBe('play');
    expect(on('page', 'z', { metaKey: true }, false)).toBe('undo');
    expect(on('page', 's', { metaKey: true }, false)).toBe('save');
  });
});
