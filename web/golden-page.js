import { loadDedalo } from "./dedalo.js";
import { runGolden } from "./golden-core.js";

const result = document.getElementById("result");

async function readText(relative) {
  const response = await fetch(new URL(`../tests/corpus/${relative}`, import.meta.url));
  if (!response.ok) {
    throw new Error(`cannot read ${relative}: HTTP ${response.status}`);
  }
  return (await response.text()).replaceAll("\r\n", "\n");
}

try {
  const call = await loadDedalo(new URL("dedalo.wasm", import.meta.url));
  const outcome = await runGolden(call, readText);
  const verdict = outcome.failures.length === 0 ? "GOLDEN-PASS" : "GOLDEN-FAIL";
  result.textContent = [`${verdict} ${outcome.cases} cases, ${outcome.files} files, ${outcome.failures.length} failures`, ...outcome.failures].join("\n");
} catch (error) {
  result.textContent = `GOLDEN-ERROR ${error.message}`;
}

// Report the verdict to `web/serve.py --result`, when it is listening.
fetch("/__golden_result", { method: "POST", body: result.textContent }).catch(() => {});
