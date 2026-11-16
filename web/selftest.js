// Drives the editor in a frame and checks what a user would see.

const frame = document.getElementById("app");
const result = document.getElementById("result");
const failures = [];
const errors = [];

const wait = (ms) => new Promise((r) => setTimeout(r, ms));
async function until(check, what, ms = 15000) {
  const start = performance.now();
  while (performance.now() - start < ms) {
    try {
      if (check()) return true;
    } catch {
      // Not ready yet.
    }
    await wait(100);
  }
  failures.push(`timed out waiting for ${what}`);
  return false;
}
function expect(ok, what) {
  if (!ok) failures.push(what);
}

async function run() {
  await new Promise((r) => (frame.contentDocument?.readyState === "complete" ? r() : frame.addEventListener("load", r, { once: true })));
  const win = frame.contentWindow;
  win.addEventListener("error", (e) => errors.push(String(e.message)));
  win.addEventListener("unhandledrejection", (e) => errors.push(String(e.reason)));
  const doc = frame.contentDocument;
  const $ = (sel) => doc.querySelector(sel);

  await until(() => $("#canvas svg"), "the first preview");
  expect($("#status").textContent === "No problems", `status says "${$("#status").textContent}"`);
  expect($("#outline").children.length >= 4, "the outline lists the starter's elements");
  expect($("#textmap").textContent.includes("#"), "the text map is drawn");

  // Type a mistake: the problems panel, the underline and the gutter react.
  const area = $("#editor textarea");
  const setText = (text) => {
    area.value = text;
    area.dispatchEvent(new win.InputEvent("input", { bubbles: true, inputType: "insertReplacementText" }));
  };
  const original = area.value;
  setText(original.replace('to = "vault"', 'to = "vaultt"'));
  await until(() => [...doc.querySelectorAll("#problems .code")].some((c) => c.textContent === "UNKNOWN_REFERENCE"), "the unknown reference");
  expect(doc.querySelector(".marks mark.m-error"), "the error is underlined");
  expect(doc.querySelector(".gutter .g-mark"), "the gutter marks the line");

  // A diagnostic with a fix: apply it from the problems panel.
  setText(original.replace("placement { wall = south, offset = 2 }", "placement { wall = south, offset = 2 }\n            position = [4, 3]").replace('position = [4, 3]\n        }\n    }', 'position = [9, 3]\n        }\n    }'));
  await until(() => $("#problems button"), "a fix button");
  $("#problems button")?.click();
  await until(() => $("#status").textContent === "No problems", "the fix to apply");

  // Completion after `type = ` inside a feature.
  setText(original);
  await until(() => $("#status").textContent === "No problems", "the original text");
  const at = area.value.indexOf("subtype = torch") + "subtype = ".length;
  area.focus();
  area.setSelectionRange(at, at + "torch".length);
  area.setRangeText("", at, at + "torch".length, "end");
  area.dispatchEvent(new win.KeyboardEvent("keydown", { key: " ", ctrlKey: true, bubbles: true }));
  await until(() => !$(".completions").hidden, "the completion list");
  const labels = [...doc.querySelectorAll(".completions li span")].map((s) => s.textContent);
  expect(labels.join(",") === "torch,brazier,candle,glowing_crystal", `completions were ${labels.join(",")}`);
  area.dispatchEvent(new win.KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
  area.dispatchEvent(new win.KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  expect(area.value.includes("subtype = brazier"), "Enter inserts the selected completion");

  // Player audience hides the trap and the creature.
  const preset = $("#preset");
  preset.value = "player";
  preset.dispatchEvent(new Event("change"));
  await until(() => !$("#textmap").textContent.includes("T trap"), "the player text map");
  expect(!$("#canvas svg").outerHTML.includes("data-id"), "the player preview carries no element IDs");

  // PNG export goes through the export module.
  const pngRequest = await new Promise((resolve) => {
    const worker = new Worker(new URL("worker.js", import.meta.url), { type: "module" });
    worker.onmessage = (e) => {
      resolve(e.data);
      worker.terminate();
    };
    worker.postMessage({ id: 1, module: "export", request: { cmd: "png", svg: $("#canvas svg").outerHTML } });
  });
  expect(pngRequest.response?.ok && pngRequest.response.base64.startsWith("iVBORw0KGgo"), `PNG export: ${JSON.stringify(pngRequest).slice(0, 120)}`);

  // A document that needs a reviewed plugin offers it; accepting loads it.
  preset.value = "dm";
  preset.dispatchEvent(new Event("change"));
  let asked = "";
  win.confirm = (text) => {
    asked = text;
    return true;
  };
  const header = `dedalo {
    plugin "hand_drawn" {
        version = "^1.2"
    }
}

`;
  const themed = `name = "The Crypt"

    render {
        theme = hand_drawn.sepia
    }`;
  setText(header + original.replace('name = "The Crypt"', themed));
  await until(() => asked.includes("hand_drawn"), "the consent prompt for the plugin");
  await until(() => $("#canvas svg")?.outerHTML.includes("#efe3c8") && $("#status").textContent === "No problems", "the sepia preview", 30000);

  expect(errors.length === 0, `page errors: ${errors.join("; ")}`);
}

try {
  await run();
} catch (error) {
  failures.push(`self-test crashed: ${error.message}`);
}
result.textContent = failures.length === 0 ? "SELFTEST-PASS" : ["SELFTEST-FAIL", ...failures].join("\n");
fetch("/__golden_result", { method: "POST", body: result.textContent }).catch(() => {});
