// Runs the cross-target golden check in Node:
//
//     node web/golden.mjs
//
// from the repository root, after building both modules into web/ and the
// reference plugins into plugins/. Exits with 1 when the WebAssembly build
// disagrees with the native build.

import { readFile } from "node:fs/promises";
import process from "node:process";

import { loadCore, loadDedalo } from "./dedalo.js";
import { runGolden } from "./golden-core.js";

const readBytes = async (path) => new Uint8Array(await readFile(path));
const readText = async (path) => (await readFile(path, "utf8")).replaceAll("\r\n", "\n");
const core = await loadCore(await readFile("web/dedalo.wasm"));
const callExport = await loadDedalo(await readFile("web/dedalo-export.wasm"));
const result = await runGolden(core, callExport, readText, readBytes);

for (const failure of result.failures) {
  console.error(`FAIL ${failure}`);
}
console.log(`${result.cases} cases, ${result.files} files, ${result.failures.length} failures`);
process.exit(result.failures.length === 0 ? 0 : 1);
