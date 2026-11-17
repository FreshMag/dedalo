// The Web Worker that hosts the Dedalo core (spec 14.2), so compiling never
// blocks typing. The export module (PNG, PDF) loads on first use. Plugins
// run here too, in the browser's own WebAssembly engine.
//
// Messages in:  { id, module: "core" | "export", request }
//               { id, plugin: { files: { path: Uint8Array } } }
// Messages out: { id, response } or { id, error }

import { loadCore, loadDedalo } from "./dedalo.js";

const core = loadCore(new URL("dedalo.wasm", import.meta.url));
let exporter = null;

self.onmessage = async (event) => {
  const { id, module, request, plugin } = event.data;
  try {
    if (plugin) {
      const host = await core;
      self.postMessage({ id, response: await host.loadPlugin(new Map(Object.entries(plugin.files))) });
      return;
    }
    if (module === "export") {
      exporter ||= loadDedalo(new URL("dedalo-export.wasm", import.meta.url));
      self.postMessage({ id, response: (await exporter)(request) });
      return;
    }
    self.postMessage({ id, response: (await core).call(request) });
  } catch (error) {
    self.postMessage({ id, error: String(error && error.message ? error.message : error) });
  }
};
