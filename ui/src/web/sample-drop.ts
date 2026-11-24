// Dropping an audio file on a Veneer or Splinter panel, in the browser: the file is decoded and becomes the
// panel's sample, as the desktop app does with a dropped path.
import { setSample } from '../sample';
import { notify } from '../store';
import { messageOf } from '../util';
import { mainBridge } from './index';

const AUDIO = /\.(wav|aiff?|flac|mp3|ogg|oga)$/i;

const panelAt = (x: number, y: number): HTMLElement | null => {
  const p = document.elementFromPoint(x, y)?.closest<HTMLElement>('#bench .mod');
  return p?.querySelector('[data-sample]') ? p : null;
};

const highlight = (panel: HTMLElement | null): void => {
  document.querySelectorAll('.mod.drop-hover').forEach((el) => el !== panel && el.classList.remove('drop-hover'));
  panel?.classList.add('drop-hover');
};

export function initWebSampleDrop(): void {
  addEventListener('dragover', (e) => {
    if (!e.dataTransfer?.types.includes('Files')) return;
    e.preventDefault();
    highlight(panelAt(e.clientX, e.clientY));
  });
  addEventListener('dragleave', () => highlight(null));
  addEventListener('drop', (e) => {
    const file = [...(e.dataTransfer?.files ?? [])].find((f) => AUDIO.test(f.name));
    if (!e.dataTransfer?.files.length) return;
    e.preventDefault();
    highlight(null);
    const at = panelAt(e.clientX, e.clientY);
    if (!at) notify('Drop the file on a Veneer or Splinter panel.', true);
    else if (!file) notify('patchplay reads WAV, AIFF, FLAC, MP3 and Ogg files.', true);
    else {
      void mainBridge()
        .then((b) => b.adopt(file))
        .then((path) => setSample(at.dataset.uid ?? '', path))
        .catch((err: unknown) => notify(messageOf(err), true));
    }
  });
}
