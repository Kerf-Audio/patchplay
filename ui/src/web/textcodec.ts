// An AudioWorkletGlobalScope has no TextEncoder or TextDecoder, and the wasm-bindgen glue makes
// both when it loads. This stand-in does UTF-8 and nothing else; the worklet imports it first.
type Scope = { TextEncoder?: unknown; TextDecoder?: unknown };
const scope = globalThis as Scope;

class Utf8Encoder {
  readonly encoding = 'utf-8';

  encode(text = ''): Uint8Array {
    const bytes: number[] = [];
    for (const ch of text) {
      const c = ch.codePointAt(0) ?? 0xfffd;
      if (c < 0x80) bytes.push(c);
      else if (c < 0x800) bytes.push(0xc0 | (c >> 6), 0x80 | (c & 63));
      else if (c < 0x10000) bytes.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
      else bytes.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 63), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
    }
    return Uint8Array.from(bytes);
  }

  encodeInto(text: string, into: Uint8Array): { read: number; written: number } {
    const bytes = this.encode(text);
    const written = Math.min(bytes.length, into.length);
    into.set(bytes.subarray(0, written));
    return { read: text.length, written };
  }
}

class Utf8Decoder {
  readonly encoding = 'utf-8';

  decode(input?: ArrayBufferView): string {
    if (!input) return '';
    const b = new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
    let out = '';
    for (let i = 0; i < b.length; ) {
      const lead = b[i++] ?? 0;
      const extra = lead >= 0xf0 ? 3 : lead >= 0xe0 ? 2 : lead >= 0xc0 ? 1 : 0;
      let c = extra === 0 ? lead : lead & (0x3f >> extra);
      for (let k = 0; k < extra; k++) c = (c << 6) | ((b[i++] ?? 0) & 63);
      out += String.fromCodePoint(c);
    }
    return out;
  }
}

scope.TextEncoder ??= Utf8Encoder;
scope.TextDecoder ??= Utf8Decoder;
