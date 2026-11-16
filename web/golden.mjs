// Runs the cross-target golden check in Node:
//
//     node web/golden.mjs web/dedalo.wasm tests/corpus
//
// Exits with 1 when the WebAssembly build disagrees with the native build.

import { readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

import { loadDedalo } from "./dedalo.js";
import { runGolden } from "./golden-core.js";

const [wasmPath = "web/dedalo.wasm", corpus = "tests/corpus"] = process.argv.slice(2);
const call = await loadDedalo(await readFile(wasmPath));
const readText = async (relative) => (await readFile(path.join(corpus, relative), "utf8")).replaceAll("\r\n", "\n");
const result = await runGolden(call, readText);

for (const failure of result.failures) {
  console.error(`FAIL ${failure}`);
}
console.log(`${result.cases} cases, ${result.files} files, ${result.failures.length} failures`);
process.exit(result.failures.length === 0 ? 0 : 1);
