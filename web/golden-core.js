// Cross-target golden check (spec 15.1): the WebAssembly build must produce,
// for every corpus file, exactly the bytes the native build recorded.
//
// `core` is the object `loadCore` returns, `callExport` the export module
// (PNG and PDF). `readText` and `readBytes` read files by path relative to
// the repository root, with line endings as stored. The same function runs
// in Node (CI) and in a browser.

import { fetchPackage } from "./dedalo.js";

const REFERENCE_PLUGINS = ["blueprint", "hand_drawn", "uvtt"];

async function sha256Hex(bytes) {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function fromBase64(text) {
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

export async function runGolden(core, callExport, readText, readBytes) {
  const failures = [];
  // The plugin case needs the reference plugins, run by the browser's engine.
  // The plugin index lists every file of each package, so that the page
  // loads the package whose hash the index states.
  const listed = JSON.parse(await readText("plugins/index.json"));
  for (const id of REFERENCE_PLUGINS) {
    const entry = listed.find((p) => p.id === id);
    const files = await fetchPackage(null, (path) => readBytes(`plugins/${id}/${path}`), entry?.files);
    const loaded = await core.loadPlugin(files);
    if (!loaded.ok) failures.push(`plugin ${id}: ${loaded.error}`);
    else if (entry && loaded.plugin.sha256 !== entry.sha256) failures.push(`plugin ${id}: the page loaded ${loaded.plugin.sha256}, the index states ${entry.sha256}`);
  }
  const index = JSON.parse(await readText("tests/corpus/index.json"));
  let files = 0;
  for (const [name, expectedFiles] of Object.entries(index)) {
    const source = await readText(`tests/corpus/${name}/input.dedalo`);
    const actual = core.call({ cmd: "golden", source }).files;
    // The core module records text outputs; hashes come from below.
    const textFiles = expectedFiles.filter((f) => !f.endsWith(".sha256"));
    const actualNames = Object.keys(actual).sort();
    if (JSON.stringify(actualNames) !== JSON.stringify([...textFiles].sort())) {
      const probe = core.call({ cmd: "render", source, format: "svg" });
      const why = probe.error || probe.report?.diagnostics?.map((d) => d.message).join("; ") || "";
      failures.push(`${name}: produced ${actualNames.join(", ")}; expected ${textFiles.join(", ")}. ${why}`);
      continue;
    }
    for (const file of expectedFiles) {
      const expected = await readText(`tests/corpus/${name}/${file}`);
      files += 1;
      const raster = file.match(/^(.*)\.(png|pdf)\.sha256$/);
      const plugin = !raster && file.match(/^(.*?)\.(\w+\.\w+)\.sha256$/);
      let bytes = null;
      if (raster) {
        const response = callExport({ cmd: raster[2], svg: actual[`${raster[1]}.svg`] });
        if (!response.ok) {
          failures.push(`${name}/${file}: ${response.error}`);
          continue;
        }
        bytes = fromBase64(response.base64);
      } else if (plugin) {
        // A plugin format: the export module makes the PNG it may need.
        const png = callExport({ cmd: "png", svg: actual[`${plugin[1]}.svg`] });
        const response = core.call({ cmd: "render", source, export: plugin[1], format: plugin[2], host_png: png.base64 });
        if (typeof response.output !== "string") {
          failures.push(`${name}/${file}: ${response.error || JSON.stringify(response.report?.diagnostics)}`);
          continue;
        }
        bytes = new TextEncoder().encode(response.output);
      }
      if (bytes) {
        if ((await sha256Hex(bytes)) !== expected.trim()) failures.push(`${name}/${file}: differs from the native output`);
      } else if (expected !== actual[file]) {
        failures.push(`${name}/${file}: differs from the native output`);
      }
    }
  }
  return { cases: Object.keys(index).length, files, failures };
}
