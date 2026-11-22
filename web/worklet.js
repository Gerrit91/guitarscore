// GuitarScore's AudioWorklet: a thin JavaScript shell around the Rust synthesizer
// (`crates/gscore-worklet`). It owns no sound itself; the wasm module does.
//
// Messages from the app (the main thread):
//   {type: "soundfont", bytes: ArrayBuffer}   load a SoundFont (SF2, or SF3: decoded here)
//   {type: "sequence", bytes: ArrayBuffer}    load a song (gscore-play's wire format)
//   {type: "replace", bytes: ArrayBuffer}     swap in a rebuilt song (live mixer changes)
//   {type: "sound", bytes: ArrayBuffer}       a string model, a chain, a bus, a drum part
//   {type: "midi", status, data1, data2}      one MIDI message now
//   {type: "command", name, value?}           play, pause, stop, seek, tempo
// Messages to the app:
//   {type: "ready"}                           the wasm module is up
//   {type: "loaded"}                          the SoundFont is loaded: playback can sound
//   {type: "status", position, state, time, rate, from}
//                                             every few blocks while playing, and in the
//                                             first block after a play, seek or stop:
//                                             the tick rendered up to, the context time
//                                             it will sound at, ticks a second, and where
//                                             this stretch of playing began
//   {type: "error", message}                  anything that went wrong

// The polyfills must be evaluated before wasm-bindgen's glue: module imports are evaluated
// in order, so this first import defines TextDecoder before the second one needs it.
import "./worklet-polyfills.js";
import { initSync, Synth } from "./worklet_synth.js";

// eslint-disable-next-line no-undef -- `sampleRate` is an AudioWorkletGlobalScope global.
const RATE = sampleRate;

// An AudioWorkletGlobalScope has no `fetch` and no `URL`, so the wasm module cannot be
// fetched here. The main thread compiles it and sends the `WebAssembly.Module` in a
// message (structured clone supports it); this module registers its processor
// synchronously and waits for that message. Messages that arrive first wait in `pending`.

class GuitarScoreSynth extends AudioWorkletProcessor {
  constructor() {
    super();
    this.synth = null;
    this.pending = [];
    this.left = new Float32Array(128);
    this.right = new Float32Array(128);
    this.blocks = 0;
    this.lastposition = -1;
    // A play, seek, stop or new song: the next block reports at once, and where it starts.
    this.restart = true;
    this.from = 0;
    this.port.onmessage = (event) => this.handle(event.data);
  }

  handle(message) {
    if (!message) return;
    if (message.type === "module") {
      try {
        initSync({ module: message.module });
        this.synth = new Synth(RATE);
        this.port.postMessage({ type: "ready" });
        for (const waiting of this.pending) this.handle(waiting);
        this.pending = [];
      } catch (error) {
        this.port.postMessage({ type: "error", message: `worklet wasm: ${error}` });
      }
      return;
    }
    if (!this.synth) {
      this.pending.push(message);
      return;
    }
    try {
      switch (message.type) {
        case "soundfont":
          this.synth.load_soundfont(new Uint8Array(message.bytes));
          this.port.postMessage({ type: "loaded" });
          break;
        case "sequence":
          this.restart = true;
          this.synth.load_sequence(new Uint8Array(message.bytes));
          break;
        case "replace":
          this.synth.replace_sequence(new Uint8Array(message.bytes));
          break;
        case "sound":
          this.synth.sound(new Uint8Array(message.bytes));
          break;
        case "midi":
          this.synth.send_midi(
            message.status & 0xff,
            message.data1 & 0x7f,
            message.data2 & 0x7f,
          );
          break;
        case "command":
          switch (message.name) {
            case "play":
              this.restart = true;
              this.synth.play();
              break;
            case "pause":
              this.synth.pause();
              break;
            case "stop":
              this.restart = true;
              this.synth.stop();
              break;
            case "seek":
              this.restart = true;
              this.synth.seek(Number(message.value) || 0);
              break;
            case "tempo":
              this.synth.set_tempo_scale(Number(message.value) || 1);
              break;
            default:
              break;
          }
          break;
        default:
          break;
      }
    } catch (error) {
      this.port.postMessage({ type: "error", message: String(error) });
    }
  }

  process(_inputs, outputs) {
    const output = outputs[0];
    if (!output || output.length === 0) return true;
    if (!this.synth) return true; // silence until the module is loaded

    const frames = output[0].length;
    if (frames === 0) return true;

    // The Rust side renders at most 128 frames (BLOCK); keep the arrays in step.
    let left = this.left;
    let right = this.right;
    if (frames !== left.length) {
      left = new Float32Array(frames);
      right = new Float32Array(frames);
      this.left = left;
      this.right = right;
    } else {
      left.fill(0);
      right.fill(0);
    }
    const restarted = this.restart;
    if (restarted) {
      this.restart = false;
      this.from = this.synth.position();
    }
    this.synth.render(left, right);
    output[0].set(left);
    if (output.length > 1) output[1].set(right);

    // Report the position about forty times a second (the page glides between reports),
    // and at once after a play, seek or stop.
    this.blocks += 1;
    if (restarted || this.blocks % 8 === 0) {
      const position = this.synth.position();
      const state = this.synth.state();
      if (restarted || position !== this.lastposition || state !== 0) {
        this.lastposition = position;
        this.port.postMessage({
          type: "status",
          position,
          state,
          // eslint-disable-next-line no-undef -- an AudioWorkletGlobalScope global.
          time: currentTime + frames / RATE,
          rate: this.synth.rate(),
          from: this.from,
        });
      }
    }
    return true;
  }
}

registerProcessor("gscore-synth", GuitarScoreSynth);
