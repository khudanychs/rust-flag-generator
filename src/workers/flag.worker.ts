/// <reference lib="webworker" />

import init, {
  generate_procedural_texture,
  apply_flag_filter,
  describe_layout,
} from "../pkg/flag_processor.js";

// Re-declare `self` so `postMessage` resolves to the worker overload
// (the app tsconfig only ships the DOM lib by default, which would
// otherwise type `self` as `Window`).
declare const self: DedicatedWorkerGlobalScope;

let wasmReady: Promise<void> | null = null;

function ensureInit(): Promise<void> {
  if (!wasmReady) {
    wasmReady = init().then(() => undefined);
  }
  return wasmReady;
}

self.onmessage = async (e: MessageEvent) => {
  try {
    await ensureInit();

    const { action } = e.data;

    if (action === "GENERATE") {
      const { width, height, seed, enableFabric = true } = e.data;

      // Rasterise pixels and pull the matching geometric description in
      // a single round-trip so the UI thread can ship an SVG export
      // without having to call back into the worker.
      const pixels = generate_procedural_texture(
        width,
        height,
        seed,
        enableFabric
      );
      const buffer = new Uint8Array(pixels);

      let layout: unknown = null;
      try {
        layout = JSON.parse(describe_layout(seed));
      } catch {
        // Non-fatal: the canvas image is still valid even if the
        // descriptor parse fails. SVG export will simply be disabled.
        layout = null;
      }

      self.postMessage(
        { action, pixels: buffer, width, height, layout },
        // Zero-copy: hand the underlying buffer to the main thread.
        [buffer.buffer]
      );
    } else if (action === "FILTER") {
      const { pixels, filterType, width, height } = e.data;
      const input = new Uint8Array(pixels);
      const result = apply_flag_filter(input, filterType);
      const buffer = new Uint8Array(result);
      self.postMessage({ action, pixels: buffer, width, height }, [
        buffer.buffer,
      ]);
    }
  } catch (err) {
    self.postMessage({ action: "ERROR", error: String(err) });
  }
};
