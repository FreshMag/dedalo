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
  // Before index.html arrives the frame holds an empty about:blank document
  // that is already complete; wait for the editor's own document.
  await until(() => frame.contentDocument?.readyState === "complete" && frame.contentDocument.getElementById("editor"), "the editor page", 30000);
  const win = frame.contentWindow;
  win.addEventListener("error", (e) => errors.push(String(e.message)));
  win.addEventListener("unhandledrejection", (e) => errors.push(String(e.reason)));
  const doc = frame.contentDocument;
  const $ = (sel) => doc.querySelector(sel);

  await until(() => $("#canvas svg"), "the first preview", 45000);
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
  await until(() => $("#problems button.fix"), "a fix button");
  // Every problem is a button that goes to its line.
  $("#problems button.jump")?.click();
  expect(doc.activeElement === area && area.selectionStart !== area.selectionEnd, "a problem's button selects its line in the editor");
  $("#problems button.fix")?.click();
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

  // Hover help from the keyboard: Control+I explains the word at the cursor.
  const word = area.value.indexOf("shape = rectangle") + 2;
  area.setSelectionRange(word, word);
  area.dispatchEvent(new win.KeyboardEvent("keydown", { key: "i", ctrlKey: true, bubbles: true }));
  await until(() => !$(".hover").hidden && $(".hover").textContent.length > 10, "hover help from the keyboard");
  area.dispatchEvent(new win.KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  expect($(".hover").hidden, "Escape closes the hover help");

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

  // ----- levels ---------------------------------------------------------------
  expect($("#level-field").hidden, "the level picker is hidden for a dungeon on one level");
  const tower = await (await fetch("../tests/corpus/levels_tower/input.dedalo")).text();
  setText(tower);
  await until(() => !$("#level-field").hidden && $("#level").options.length === 4, "the level picker");
  await until(() => $("#textmap").textContent.split("\n")[1]?.includes("   #"), "all levels side by side");
  const whole = $("#canvas svg").getAttribute("width");
  $("#level").value = "upper";
  $("#level").dispatchEvent(new Event("change"));
  await until(() => $("#canvas svg").getAttribute("width") !== whole, "the map of one level");
  expect(!$("#textmap").textContent.includes("*"), "one level leaves out the light of another");

  // ----- Italian ----------------------------------------------------------------
  const language = $("#language");
  language.value = "it";
  language.dispatchEvent(new Event("change"));
  await until(() => $("#status").textContent === "Nessun problema", "the Italian status line");
  expect(doc.documentElement.lang === "it", "the page says it is in Italian");
  expect($("#open").textContent === "Apri" && $("#tab-problems").textContent === "Problemi", "the controls are in Italian");
  expect($("#level").options[0].textContent === "Tutti i livelli", "lists built by script are in Italian");
  expect($("#textmap").textContent.includes("porta"), "default map labels follow the page language");
  setText(tower.replace('name = "The Leaning Tower"', 'name = "The Leaning Tower"\n    render { language = en }'));
  await until(() => $("#textmap").textContent.includes("door"), "the document's own language wins");
  language.value = "en";
  language.dispatchEvent(new Event("change"));
  await until(() => $("#status").textContent === "No problems", "English again");
  const { keysByLanguage } = await import("./i18n.js");
  const keys = keysByLanguage();
  expect(keys.en.join() === keys.it.join(), "both languages have the same keys");
  for (const el of doc.querySelectorAll("[data-i18n], [data-i18n-title], [data-i18n-aria-label]")) {
    for (const key of [el.dataset.i18n, el.dataset.i18nTitle, el.dataset.i18nAriaLabel]) {
      if (key && !keys.en.includes(key)) failures.push(`the page uses the unknown text key ${key}`);
    }
  }

  // ----- accessibility ------------------------------------------------------------
  // Every control has a name.
  const named = (el) => {
    const labelled = (el.getAttribute("aria-labelledby") || "").split(" ").map((id) => doc.getElementById(id)?.textContent || "").join("");
    return (el.getAttribute("aria-label") || labelled || el.labels?.[0]?.textContent || el.textContent || el.title || "").trim();
  };
  for (const el of doc.querySelectorAll("button, select, textarea, input:not([type=hidden]):not([hidden]), a[href], [role=tab], [role=tabpanel], [tabindex='0']")) {
    if (!named(el)) failures.push(`a control has no accessible name: ${el.outerHTML.slice(0, 80)}`);
  }
  expect($("#canvas svg").getAttribute("role") === "img" && $("#canvas svg").getAttribute("aria-label"), "the map is one labelled image");
  // Tabs: one tab stop, arrows move, the panel follows.
  const tabs = [...doc.querySelectorAll('[role="tab"]')];
  expect(tabs.filter((tab) => tab.tabIndex === 0).length === 1, "the tab list is one tab stop");
  tabs[0].focus();
  tabs[0].dispatchEvent(new win.KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
  expect(doc.activeElement === tabs[1] && tabs[1].getAttribute("aria-selected") === "true" && !$("#outline").hidden && $("#problems").hidden, "the right arrow moves to the next tab");
  expect($("#outline button.jump"), "outline entries are buttons");
  tabs[1].dispatchEvent(new win.KeyboardEvent("keydown", { key: "Home", bubbles: true }));
  expect(tabs[0].getAttribute("aria-selected") === "true", "Home goes to the first tab");
  // The preview answers the keyboard.
  const before = $("#canvas").style.transform;
  $("#viewport").dispatchEvent(new win.KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true }));
  $("#viewport").dispatchEvent(new win.KeyboardEvent("keydown", { key: "+", bubbles: true }));
  expect($("#canvas").style.transform !== before, "arrow keys and plus move and zoom the preview");
  // Escape then Tab leaves the editor instead of indenting.
  area.focus();
  const length = area.value.length;
  area.dispatchEvent(new win.KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  const tab = new win.KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true });
  area.dispatchEvent(tab);
  expect(!tab.defaultPrevented && area.value.length === length, "Escape then Tab leaves the editor");
  expect(area.getAttribute("aria-describedby") === "editor-help" && $("#editor-help").textContent.includes("Escape"), "the way out of the editor is described");
  // The completion list is tied to the text area.
  expect(area.getAttribute("aria-controls") === "editor-completions" && $("#editor-completions").getAttribute("role") === "listbox", "the completion list is announced with the editor");

  // ----- a zipped package -----------------------------------------------------------
  // `dedalo plugin pack plugins/blueprint -o web/test-package.dedalopkg`
  const packed = await fetch("test-package.dedalopkg").catch(() => null);
  if (packed && packed.ok) {
    let question = "";
    win.confirm = (text) => {
      question = text;
      return true;
    };
    $("#plugin-url").value = new URL("test-package.dedalopkg", location.href).href;
    $("#plugin-url-form").dispatchEvent(new win.Event("submit", { cancelable: true }));
    await until(() => $("#plugin-status").textContent.includes("blueprint"), "the package from an address", 30000);
    expect(question.includes("blueprint") && question.includes("sha256:") && question.includes(location.origin), `the consent prompt names the plugin, its origin and its hash: ${question}`);
    expect([...doc.querySelectorAll("#plugin-list li")].some((li) => li.textContent.includes("blueprint")), "the loaded package is listed");
    setText(header.replace("hand_drawn", "blueprint").replace("^1.2", "^1") + original.replace('name = "The Crypt"', themed.replace("hand_drawn.sepia", "blueprint.default")));
    await until(() => $("#canvas svg")?.outerHTML.includes("#1d4e89") && $("#status").textContent === "No problems", "the blueprint preview", 30000);
  } else {
    failures.push("web/test-package.dedalopkg is missing: run `dedalo plugin pack plugins/blueprint -o web/test-package.dedalopkg`");
  }

  expect(errors.length === 0, `page errors: ${errors.join("; ")}`);
}

try {
  await run();
} catch (error) {
  failures.push(`self-test crashed: ${error.message}`);
}
result.textContent = failures.length === 0 ? "SELFTEST-PASS" : ["SELFTEST-FAIL", ...failures].join("\n");
fetch("/__golden_result", { method: "POST", body: result.textContent }).catch(() => {});
