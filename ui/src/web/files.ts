// Files without a native dialog: a hidden file input to open, a download to save.

/** Asks for a file with a file input. `null` when the user cancels. */
export function chooseFile(accept: string): Promise<File | null> {
  return new Promise((done) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = accept;
    input.hidden = true;
    // Attached while the dialog is open: a detached input can be collected before it reports.
    const finish = (file: File | null): void => {
      input.remove();
      done(file);
    };
    input.onchange = () => finish(input.files?.[0] ?? null);
    input.oncancel = () => finish(null);
    document.body.append(input);
    input.click();
  });
}

/** A file the user picked in a save dialog, written in place. */
export interface SaveTarget {
  write(data: BlobPart): Promise<void>;
}

interface FilePickerWindow {
  showSaveFilePicker?: (options: {
    suggestedName: string;
    types: { description: string; accept: Record<string, string[]> }[];
  }) => Promise<{ createWritable(): Promise<{ write(data: BlobPart): Promise<void>; close(): Promise<void> }> }>;
}

/**
 * Opens the browser's own save dialog where it has one (Chrome, Edge). `undefined` when it has
 * none, so the caller falls back to a download; `null` when the user cancels.
 */
export async function pickSaveTarget(name: string, description: string, mime: string, ext: string): Promise<SaveTarget | null | undefined> {
  const picker = (window as FilePickerWindow).showSaveFilePicker;
  if (!picker) return undefined;
  try {
    const handle = await picker.call(window, { suggestedName: name, types: [{ description, accept: { [mime]: [`.${ext}`] } }] });
    return {
      async write(data) {
        const out = await handle.createWritable();
        await out.write(data);
        await out.close();
      },
    };
  } catch (e) {
    if (e instanceof DOMException && e.name === 'AbortError') return null;
    throw e;
  }
}

/** Offers bytes or text to the user as a download. */
export function download(name: string, data: BlobPart, type: string): void {
  const url = URL.createObjectURL(new Blob([data], { type }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.hidden = true;
  document.body.append(a);
  a.click();
  setTimeout(() => {
    a.remove();
    URL.revokeObjectURL(url);
  }, 60_000);
}

export const withExtension = (name: string, ext: string): string =>
  name.toLowerCase().endsWith(`.${ext}`) ? name : `${name}.${ext}`;
