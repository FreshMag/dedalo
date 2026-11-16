import { loadCore, loadDedalo } from "./dedalo.js";
import { runGolden } from "./golden-core.js";

const result = document.getElementById("result");
const root = new URL("../", import.meta.url);

async function readBytes(path) {
  const response = await fetch(new URL(path, root));
  if (!response.ok) throw new Error(`cannot read ${path}: HTTP ${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
}
async function readText(path) {
  return new TextDecoder().decode(await readBytes(path)).replaceAll("\r\n", "\n");
}

try {
  const core = await loadCore(new URL("dedalo.wasm", import.meta.url));
  const callExport = await loadDedalo(new URL("dedalo-export.wasm", import.meta.url));
  const outcome = await runGolden(core, callExport, readText, readBytes);
  const verdict = outcome.failures.length === 0 ? "GOLDEN-PASS" : "GOLDEN-FAIL";
  result.textContent = [`${verdict} ${outcome.cases} cases, ${outcome.files} files, ${outcome.failures.length} failures`, ...outcome.failures].join("\n");
} catch (error) {
  result.textContent = `GOLDEN-ERROR ${error.message}`;
}

// Report the verdict to `web/serve.py --result`, when it is listening.
fetch("/__golden_result", { method: "POST", body: result.textContent }).catch(() => {});
