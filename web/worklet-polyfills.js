// An AudioWorkletGlobalScope has no TextDecoder/TextEncoder, which wasm-bindgen's glue
// uses. This module is imported before the glue, so its globals exist by the time the glue
// is evaluated. Minimal UTF-8 versions: enough for the strings wasm-bindgen passes.

if (typeof globalThis.TextDecoder === "undefined") {
  globalThis.TextDecoder = class TextDecoder {
    decode(bytes) {
      if (!bytes) return "";
      const input = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
      let out = "";
      let i = 0;
      while (i < input.length) {
        const first = input[i++];
        let code = first;
        if (first >= 0xf0) {
          code =
            ((first & 0x07) << 18) |
            ((input[i++] & 0x3f) << 12) |
            ((input[i++] & 0x3f) << 6) |
            (input[i++] & 0x3f);
        } else if (first >= 0xe0) {
          code = ((first & 0x0f) << 12) | ((input[i++] & 0x3f) << 6) | (input[i++] & 0x3f);
        } else if (first >= 0xc0) {
          code = ((first & 0x1f) << 6) | (input[i++] & 0x3f);
        }
        if (code === 0xfeff && out.length === 0) continue; // a BOM
        out += String.fromCodePoint(code);
      }
      return out;
    }
  };
}

if (typeof globalThis.TextEncoder === "undefined") {
  globalThis.TextEncoder = class TextEncoder {
    encode(text) {
      const bytes = [];
      for (const character of text) {
        const code = character.codePointAt(0);
        if (code < 0x80) {
          bytes.push(code);
        } else if (code < 0x800) {
          bytes.push(0xc0 | (code >> 6), 0x80 | (code & 0x3f));
        } else if (code < 0x10000) {
          bytes.push(0xe0 | (code >> 12), 0x80 | ((code >> 6) & 0x3f), 0x80 | (code & 0x3f));
        } else {
          bytes.push(
            0xf0 | (code >> 18),
            0x80 | ((code >> 12) & 0x3f),
            0x80 | ((code >> 6) & 0x3f),
            0x80 | (code & 0x3f),
          );
        }
      }
      return new Uint8Array(bytes);
    }
  };
}
