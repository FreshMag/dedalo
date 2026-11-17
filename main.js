// The Dedalo web editor (spec section 14).
//
// It holds no map logic: every answer comes from the core running in a Web
// Worker. Nothing leaves the browser except what the user chooses to share.

import { createEditor } from "./editor.js";
import { applyTranslations, currentLanguage, initLanguage, setLanguage, t } from "./i18n.js";

const STARTER = `dungeon "crypt" {
    name = "The Crypt"

    grid {
        unit = "5ft"
    }

    room "entrance" {
        name = "Entrance"
        shape = rectangle(8, 6)

        feature "torch" {
            type = lighting
            subtype = torch
            placement { wall = south, offset = 2 }
        }

        feature "plate" {
            type = trap
            subtype = pressure_plate
            position = [4, 3]
        }
    }

    room "vault" {
        name = "Vault"
        shape = square(6)
        layout { north_of = "entrance" }

        entity "skeletons" {
            type = creature
            position = [1, 1]
            properties { count = 3 }
        }
    }

    connection "vault_door" {
        from = "entrance"
        to = "vault"
        type = door
    }
}
`;

const $ = (id) => document.getElementById(id);
const status = $("status");

initLanguage();
$("language").value = currentLanguage();
applyTranslations();

// ----- the worker ------------------------------------------------------------

// Packages the user has loaded, kept to reload them into a new worker.
const loadedPackages = new Map();
const pending = new Map();
let nextId = 1;
let worker = null;

function startWorker() {
  worker = new Worker(new URL("worker.js", import.meta.url), { type: "module" });
  worker.onmessage = (event) => {
    const { id, response, error } = event.data;
    const entry = pending.get(id);
    pending.delete(id);
    if (entry) {
      clearTimeout(entry.timer);
      error ? entry.reject(new Error(error)) : entry.resolve(response);
    }
  };
  for (const files of loadedPackages.values()) post({ plugin: { files: Object.fromEntries(files) } });
}

// Plugins cannot be metered in the browser: a watchdog ends a call that
// takes too long by replacing the worker (spec 11.5).
const WATCHDOG_MS = 20000;
function post(message) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      worker.terminate();
      for (const entry of pending.values()) {
        clearTimeout(entry.timer);
        entry.reject(new Error("PLUGIN_TIMEOUT"));
      }
      pending.clear();
      startWorker();
      reject(new Error("PLUGIN_TIMEOUT: a plugin took too long and was stopped"));
    }, WATCHDOG_MS);
    pending.set(id, { resolve, reject, timer });
    worker.postMessage({ id, ...message });
  });
}
function ask(request, module = "core") {
  return post({ module, request });
}
startWorker();

// ----- the editor -------------------------------------------------------------

let generation = 0;
let lastSvg = "";
let lastReport = null;
const editor = createEditor($("editor"), {
  onChange: (text) => update(text),
  complete: (text, offset) => ask({ cmd: "complete", source: text, offset }),
  hover: async (text, offset) => {
    const r = await ask({ cmd: "hover", source: text, offset });
    return r.ok ? r : null;
  },
  label: () => t("editor.label"),
  describedBy: "editor-help",
});

function renderRequest(text, format) {
  // The page's language is used where the document chooses none.
  const request = { cmd: "render", source: text, format, force: true, language: currentLanguage(), toggles: { grid: $("grid").checked, room_labels: $("labels").checked } };
  if ($("preset").value) request.preset = $("preset").value;
  if ($("export").value) request.export = $("export").value;
  if ($("rotation").value) request.rotation = Number($("rotation").value);
  if ($("level").value) request.level = $("level").value;
  return request;
}

function showStatus(report) {
  const { errors, warnings } = report.summary;
  status.textContent = errors + warnings === 0 ? t("status.ok") : t("status.count", { errors, warnings });
  status.className = errors ? "bad" : warnings ? "warn" : "good";
}

async function update(text) {
  // Stale answers are dropped by generation number.
  const mine = ++generation;
  const compiled = await ask({ cmd: "compile", source: text });
  if (mine !== generation) return;
  // The level list comes first: a level that is gone must not be asked for.
  syncExports(compiled.exports || []);
  syncLevels(compiled.levels || []);
  const [rendered, outline, textmap] = await Promise.all([
    ask(renderRequest(text, "svg")),
    ask({ cmd: "outline", source: text }),
    ask(renderRequest(text, "txt")),
  ]);
  if (mine !== generation) return;
  const report = compiled.report;
  lastReport = report;
  editor.setDiagnostics(report.diagnostics);
  if (report.diagnostics.some((d) => d.code === "PLUGIN_NOT_FOUND")) offerMissingPlugins(report.diagnostics);
  showProblems(report.diagnostics, text);
  showOutline(outline.items);
  $("textmap").textContent = typeof textmap.output === "string" ? textmap.output : "";
  // The last good or partial render stays visible while errors exist.
  if (typeof rendered.output === "string" && rendered.output.includes("<path")) {
    lastSvg = rendered.output;
    $("canvas").innerHTML = lastSvg;
    labelPreview();
  }
  showStatus(report);
  try {
    localStorage.setItem("dedalo.draft", text);
  } catch {
    // Storage may be unavailable; drafts are a convenience.
  }
}

// The drawing is one image to assistive technology; the text map panel is
// its text alternative.
function labelPreview() {
  const svg = $("canvas").querySelector("svg");
  if (!svg) return;
  svg.setAttribute("role", "img");
  svg.setAttribute("aria-label", t("preview"));
  svg.setAttribute("focusable", "false");
}

function syncOptions(select, wanted, labelOf) {
  const current = select.value;
  const key = (list) => list.map((o) => `${o.value}=${o.label}`).join("|");
  const options = wanted.map((value) => ({ value, label: labelOf(value) }));
  if (key([...select.options].map((o) => ({ value: o.value, label: o.textContent }))) === key(options)) return;
  select.replaceChildren(...options.map((o) => new Option(o.label, o.value)));
  select.value = wanted.includes(current) ? current : "";
}

function syncExports(names) {
  syncOptions($("export"), ["", ...names], (n) => n || t("export.default"));
}

// The level picker appears only for a dungeon that has levels.
function syncLevels(levels) {
  $("level-field").hidden = levels.length === 0;
  const names = new Map(levels.map((l) => [l.id, l.name || l.id]));
  syncOptions($("level"), ["", ...names.keys()], (id) => (id ? names.get(id) : t("level.all")));
}

function showProblems(diagnostics, text) {
  const list = $("problems");
  list.replaceChildren();
  for (const d of diagnostics) {
    const item = document.createElement("li");
    item.className = d.severity;
    // The whole entry is one button that goes to the line; a fix, when
    // there is one, is a second button beside it.
    const jump = document.createElement("button");
    jump.type = "button";
    jump.className = "jump";
    const head = document.createElement("span");
    head.className = "head";
    head.innerHTML = `<span class="code"></span><span class="where"></span>`;
    head.querySelector(".code").textContent = d.code;
    head.querySelector(".where").textContent = d.span ? ` ${t("problem.line", { line: d.span.start[0] })}` : "";
    const message = document.createElement("span");
    message.className = "message";
    message.textContent = d.message;
    jump.append(head, message);
    if (d.suggestion) {
      const help = document.createElement("span");
      help.className = "help";
      help.textContent = d.suggestion;
      jump.append(help);
    }
    if (d.span) jump.addEventListener("click", () => editor.revealLine(d.span.start[0], d.span.start[1]));
    else jump.disabled = true;
    item.append(jump);
    if (d.fix) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "fix";
      button.textContent = t("problem.fix");
      button.addEventListener("click", async () => {
        const result = await ask({ cmd: "edit", source: text, ops: [d.fix] });
        if (result.ok) editor.value = result.source;
        else status.textContent = t("status.fix-failed");
      });
      item.append(button);
    }
    list.append(item);
  }
  if (diagnostics.length === 0) {
    const ok = document.createElement("li");
    ok.className = "empty";
    ok.textContent = t("problem.none");
    list.append(ok);
  }
}

function showOutline(items) {
  const list = $("outline");
  list.replaceChildren(
    ...items.map((item) => {
      const li = document.createElement("li");
      const jump = document.createElement("button");
      jump.type = "button";
      jump.className = "jump";
      jump.style.paddingLeft = `${12 + item.depth * 18}px`;
      jump.innerHTML = `<span class="kind"></span> <strong></strong> <span class="where"></span>`;
      jump.querySelector(".kind").textContent = item.kind;
      jump.querySelector("strong").textContent = item.id;
      jump.querySelector(".where").textContent = item.label;
      if (item.line) jump.addEventListener("click", () => editor.revealLine(item.line));
      else jump.disabled = true;
      li.append(jump);
      return li;
    }),
  );
}

// ----- tabs --------------------------------------------------------------------

// The tabs follow the usual keyboard pattern: one tab stop, arrows to move.
const tabs = [...document.querySelectorAll('[role="tab"]')];
function selectTab(tab, focus = false) {
  for (const other of tabs) {
    const on = other === tab;
    other.setAttribute("aria-selected", String(on));
    other.tabIndex = on ? 0 : -1;
    $(other.getAttribute("aria-controls")).hidden = !on;
  }
  if (focus) tab.focus();
}
for (const tab of tabs) {
  tab.addEventListener("click", () => selectTab(tab));
  tab.addEventListener("keydown", (event) => {
    const i = tabs.indexOf(tab);
    const next = { ArrowRight: tabs[(i + 1) % tabs.length], ArrowLeft: tabs[(i + tabs.length - 1) % tabs.length], Home: tabs[0], End: tabs[tabs.length - 1] }[event.key];
    if (!next) return;
    selectTab(next, true);
    event.preventDefault();
  });
}

// ----- preview: pan, zoom, click to source ------------------------------------------------

let zoom = 1;
let pan = { x: 0, y: 0 };
const canvas = $("canvas");
const viewport = $("viewport");
function applyView() {
  canvas.style.transform = `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`;
}
function fit() {
  const svg = canvas.querySelector("svg");
  if (!svg) return;
  const w = svg.width.baseVal.value || 1;
  const h = svg.height.baseVal.value || 1;
  zoom = Math.min(viewport.clientWidth / w, viewport.clientHeight / h, 1.5) * 0.96;
  pan = { x: (viewport.clientWidth - w * zoom) / 2, y: (viewport.clientHeight - h * zoom) / 2 };
  applyView();
}
function zoomBy(factor, cx = viewport.clientWidth / 2, cy = viewport.clientHeight / 2) {
  const next = Math.min(Math.max(zoom * factor, 0.05), 20);
  pan = { x: cx - ((cx - pan.x) * next) / zoom, y: cy - ((cy - pan.y) * next) / zoom };
  zoom = next;
  applyView();
}
viewport.addEventListener("wheel", (event) => {
  event.preventDefault();
  const rect = viewport.getBoundingClientRect();
  zoomBy(event.deltaY < 0 ? 1.15 : 1 / 1.15, event.clientX - rect.left, event.clientY - rect.top);
}, { passive: false });
let drag = null;
viewport.addEventListener("pointerdown", (event) => {
  drag = { x: event.clientX, y: event.clientY, pan: { ...pan }, moved: false };
  viewport.setPointerCapture(event.pointerId);
});
viewport.addEventListener("pointermove", (event) => {
  if (!drag) return;
  const dx = event.clientX - drag.x;
  const dy = event.clientY - drag.y;
  if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
  pan = { x: drag.pan.x + dx, y: drag.pan.y + dy };
  applyView();
});
viewport.addEventListener("pointerup", async (event) => {
  const wasClick = drag && !drag.moved;
  drag = null;
  if (!wasClick) return;
  // Click-to-source: DM maps annotate nodes with the element they draw.
  const hit = document.elementsFromPoint(event.clientX, event.clientY).find((el) => el.closest && el.closest("[data-id]"));
  const id = hit?.closest("[data-id]")?.getAttribute("data-id");
  if (!id) return;
  const found = await ask({ cmd: "locate", source: editor.value, id });
  if (found.ok) editor.revealLine(found.line, found.column);
});
// Everything the pointer does to the view, the keyboard does too.
viewport.addEventListener("keydown", (event) => {
  const step = event.shiftKey ? 200 : 50;
  const move = { ArrowLeft: [step, 0], ArrowRight: [-step, 0], ArrowUp: [0, step], ArrowDown: [0, -step] }[event.key];
  if (move) {
    pan = { x: pan.x + move[0], y: pan.y + move[1] };
    applyView();
  } else if (event.key === "+" || event.key === "=") zoomBy(1.25);
  else if (event.key === "-") zoomBy(0.8);
  else if (event.key === "0") fit();
  else return;
  event.preventDefault();
});
$("zoom-in").addEventListener("click", () => zoomBy(1.25));
$("zoom-out").addEventListener("click", () => zoomBy(0.8));
$("zoom-fit").addEventListener("click", fit);

for (const id of ["preset", "export", "level", "rotation", "grid", "labels"]) {
  $(id).addEventListener("change", async () => {
    await update(editor.value);
    fit();
  });
}

$("language").addEventListener("change", async () => {
  setLanguage($("language").value);
  applyTranslations();
  // Lists built by script carry words too; build them again.
  $("export").replaceChildren();
  $("level").replaceChildren();
  renderPluginList();
  await update(editor.value);
  fit();
});

// ----- files, examples, downloads ------------------------------------------------------

function download(name, data, type) {
  const link = document.createElement("a");
  link.href = URL.createObjectURL(new Blob([data], { type }));
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(link.href), 1000);
}
function fromBase64(text) {
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return bytes;
}
function toBase64(bytes) {
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary);
}
async function dungeonId() {
  const match = editor.value.match(/dungeon\s+"([^"]+)"/);
  return match ? match[1] : "dungeon";
}

$("open").addEventListener("click", () => $("file").click());
$("file").addEventListener("change", async () => {
  const file = $("file").files[0];
  if (!file) return;
  const text = await file.text();
  if (file.name.endsWith(".json")) {
    const converted = await ask({ cmd: "convert", source: text, to: "dsl" });
    editor.value = converted.ok ? converted.text : text;
  } else {
    editor.value = text;
  }
  fit();
});
$("save").addEventListener("click", async () => download(`${await dungeonId()}.dedalo`, editor.value, "text/plain"));
$("format").addEventListener("click", async () => {
  const result = await ask({ cmd: "format", source: editor.value });
  if (result.ok) editor.value = result.text;
  else status.textContent = t("status.fix-syntax");
});
// Choosing in a list changes nothing by itself; the button beside it acts.
$("example-load").addEventListener("click", async () => {
  const name = $("examples").value;
  if (!name) {
    status.textContent = t("status.choose-example");
    return;
  }
  if (name === "starter") {
    editor.value = STARTER;
  } else {
    // The site ships the examples; a development server serves the corpus.
    let response = await fetch(`examples/${name}.dedalo`).catch(() => null);
    if (!response || !response.ok) response = await fetch(`../tests/corpus/${name}/input.dedalo`);
    editor.value = await response.text();
  }
  $("level").value = "";
  setTimeout(fit, 50);
});
$("download").addEventListener("click", async () => {
  const kind = $("download-format").value;
  if (!kind) {
    status.textContent = t("status.choose-format");
    return;
  }
  const base = [await dungeonId(), $("level").value, $("preset").value || $("export").value || "map"].filter(Boolean).join(".");
  status.textContent = t("status.exporting");
  try {
    if (kind === "json") {
      const r = await ask({ cmd: "convert", source: editor.value, to: "json" });
      if (!r.ok) throw new Error(t("status.has-errors"));
      download(`${await dungeonId()}.json`, r.text, "application/json");
    } else if (kind === "svg" || kind === "txt") {
      const r = await ask({ ...renderRequest(editor.value, kind), force: false });
      if (typeof r.output !== "string") throw new Error(r.error || t("status.has-errors"));
      download(`${base}.${kind}`, r.output, kind === "svg" ? "image/svg+xml" : "text/plain");
    } else {
      // PNG and PDF come from the export module, loaded on first use.
      const r = await ask({ ...renderRequest(editor.value, "svg"), force: false });
      if (typeof r.output !== "string") throw new Error(r.error || t("status.has-errors"));
      const bytes = await ask({ cmd: kind, svg: r.output }, "export");
      if (!bytes.ok) throw new Error(bytes.error);
      download(`${base}.${kind}`, fromBase64(bytes.base64), bytes.media_type);
    }
    status.textContent = t("status.exported");
  } catch (error) {
    status.textContent = t("status.export-failed", { reason: error.message });
  }
});

// ----- share links -------------------------------------------------------------

async function deflate(text) {
  const stream = new Blob([text]).stream().pipeThrough(new CompressionStream("deflate-raw"));
  const bytes = new Uint8Array(await new Response(stream).arrayBuffer());
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}
async function inflate(encoded) {
  const bytes = fromBase64(encoded.replace(/-/g, "+").replace(/_/g, "/"));
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
  return new Response(stream).text();
}
$("share").addEventListener("click", async () => {
  const encoded = await deflate(editor.value);
  if (encoded.length > 8000) {
    status.textContent = t("status.too-long");
    download(`${await dungeonId()}.dedalo`, editor.value, "text/plain");
    return;
  }
  // The fragment never reaches the server. Plugins loaded from an address
  // travel with the link; whoever opens it is asked before any is loaded.
  const extras = [...remoteSources.values()].map((url) => `&plugin=${encodeURIComponent(url)}`).join("");
  const url = `${location.origin}${location.pathname}#src=${encoded}${extras}`;
  history.replaceState(null, "", url);
  try {
    await navigator.clipboard.writeText(url);
    status.textContent = t("status.copied");
  } catch {
    status.textContent = t("status.in-bar");
  }
});

// ----- start ----------------------------------------------------------------------

async function start() {
  let text = null;
  const shared = location.hash.match(/#src=([A-Za-z0-9_-]+)/);
  if (shared) {
    try {
      text = await inflate(shared[1]);
    } catch {
      status.textContent = t("status.damaged");
    }
  }
  if (text === null) {
    try {
      text = localStorage.getItem("dedalo.draft");
    } catch {
      text = null;
    }
  }
  editor.value = text || STARTER;
  setTimeout(fit, 100);
}
status.textContent = t("status.loading");
start();
window.addEventListener("resize", fit);

// Offline use after the first visit.
if ("serviceWorker" in navigator && location.protocol !== "file:") {
  navigator.serviceWorker.register(new URL("sw.js", import.meta.url)).catch(() => {});
}

// ----- plugins (spec 11.6) ----------------------------------------------------------

let pluginIndex = [];
let pluginBase = null;
// Plugins loaded from an address, by ID, with the address they came from.
const remoteSources = new Map();
const pluginStatus = $("plugin-status");

async function readIndex() {
  for (const url of ["plugins/index.json", "../plugins/index.json"]) {
    try {
      const response = await fetch(url);
      if (response.ok) return { list: await response.json(), base: new URL(url.replace("index.json", ""), location.href) };
    } catch {
      // Try the next place.
    }
  }
  return { list: [], base: null };
}

// The files of a package of the index. The index lists them, so the page
// loads exactly the package whose hash the index states.
async function fetchFiles(base, listed = null) {
  const decoder = new TextDecoder();
  const get = async (path) => {
    const response = await fetch(new URL(path, base));
    if (!response.ok) throw new Error(`${path}: ${response.status}`);
    return new Uint8Array(await response.arrayBuffer());
  };
  if (listed) return new Map(await Promise.all(listed.map(async (path) => [path, await get(path)])));
  const files = new Map([["dedalo-plugin.toml", await get("dedalo-plugin.toml")]]);
  for (const [, path] of decoder.decode(files.get("dedalo-plugin.toml")).matchAll(/^\s*(?:file|module)\s*=\s*"([^"]+)"/gm)) {
    files.set(path, await get(path));
  }
  return files;
}
const indexFiles = (entry) => fetchFiles(new URL(`${entry.id}/`, pluginBase), entry.files);

function say(text) {
  status.textContent = text;
  pluginStatus.textContent = text;
}

async function loadPackage(files) {
  const result = await post({ plugin: { files: Object.fromEntries(files) } });
  if (result.ok) {
    loadedPackages.set(result.plugin.id, files);
    say(t("status.plugin-loaded", result.plugin));
    await update(editor.value);
  } else {
    say(t("status.plugin-failed", { reason: result.error }));
  }
  renderPluginList();
  return result;
}

// Opens a zipped package without loading it: its files, and the ID,
// version and hash to show before asking.
async function openArchive(bytes) {
  const result = await ask({ cmd: "unpack_package", package: toBase64(bytes) });
  if (!result.ok) throw new Error(result.error);
  const files = new Map(Object.entries(result.files).map(([path, data]) => [path, fromBase64(data)]));
  return { info: result.plugin, files };
}

function stored(key) {
  try {
    return JSON.parse(localStorage.getItem(key) || "[]");
  } catch {
    return [];
  }
}
function saveEnabled() {
  try {
    localStorage.setItem("dedalo.plugins", JSON.stringify([...loadedPackages.keys()].filter((id) => pluginIndex.some((p) => p.id === id))));
  } catch {
    // Remembering plugins is a convenience.
  }
}
function saveRemote(url, sha256) {
  try {
    const list = stored("dedalo.pluginUrls").filter((entry) => entry.url !== url);
    localStorage.setItem("dedalo.pluginUrls", JSON.stringify([...list, { url, sha256 }]));
  } catch {
    // Remembering plugins is a convenience.
  }
}

// Loads a package from an address. `question` is the consent text to show,
// or nothing when the user already agreed to exactly this package.
async function loadFromUrl(url, question) {
  let address;
  try {
    address = new URL(url, location.href);
  } catch {
    say(t("status.plugin-failed", { reason: url }));
    return false;
  }
  try {
    const response = await fetch(address, { credentials: "omit", referrerPolicy: "no-referrer" });
    if (!response.ok) throw new Error(`${response.status} ${response.statusText}`.trim());
    const { info, files } = await openArchive(new Uint8Array(await response.arrayBuffer()));
    const known = stored("dedalo.pluginUrls").find((entry) => entry.url === address.href);
    const key = question || (known && known.sha256 === info.sha256 ? null : known ? "confirm.changed" : "confirm.url");
    if (key && !confirm(t(key, { ...info, origin: address.origin }))) return false;
    const result = await loadPackage(files);
    if (!result.ok) return false;
    remoteSources.set(result.plugin.id, address.href);
    saveRemote(address.href, info.sha256);
    renderPluginList();
    return true;
  } catch (error) {
    say(t("status.plugin-failed", { reason: error.message }));
    return false;
  }
}

function renderPluginList() {
  const list = $("plugin-list");
  list.replaceChildren(
    ...pluginIndex.map((p) => {
      const li = document.createElement("li");
      const label = document.createElement("label");
      const box = document.createElement("input");
      box.type = "checkbox";
      box.checked = loadedPackages.has(p.id);
      box.disabled = box.checked;
      box.addEventListener("change", async () => {
        await loadPackage(await indexFiles(p));
        saveEnabled();
      });
      label.append(box, ` ${p.id} ${p.version}`);
      const detail = document.createElement("small");
      detail.textContent = ` — ${(p.provides || []).join(", ")}`;
      li.append(label, detail);
      return li;
    }),
    ...[...loadedPackages.keys()]
      .filter((id) => !pluginIndex.some((p) => p.id === id))
      .map((id) => {
        const li = document.createElement("li");
        const from = remoteSources.get(id);
        li.textContent = from ? t("plugin.remembered", { id, origin: new URL(from).origin }) : t("plugin.session", { id });
        return li;
      }),
  );
}

$("plugins-open").addEventListener("click", () => {
  renderPluginList();
  pluginStatus.textContent = "";
  $("plugins").showModal();
});
$("plugin-files").addEventListener("change", async () => {
  const chosen = [...$("plugin-files").files];
  const manifest = chosen.find((f) => f.name === "dedalo-plugin.toml");
  if (!manifest) {
    say(t("status.choose-folder"));
    return;
  }
  const root = (manifest.webkitRelativePath || manifest.name).replace(/dedalo-plugin\.toml$/, "");
  const files = new Map();
  for (const f of chosen) {
    const path = (f.webkitRelativePath || f.name).slice(root.length);
    files.set(path, new Uint8Array(await f.arrayBuffer()));
  }
  // Loading from a file is the user's explicit choice; show what it is first.
  const text = new TextDecoder().decode(files.get("dedalo-plugin.toml"));
  const id = text.match(/^\s*id\s*=\s*"([^"]+)"/m)?.[1] ?? "?";
  if (confirm(t("confirm.files", { id }))) await loadPackage(files);
});
$("plugin-package").addEventListener("change", async () => {
  const file = $("plugin-package").files[0];
  if (!file) return;
  try {
    const { info, files } = await openArchive(new Uint8Array(await file.arrayBuffer()));
    if (confirm(t("confirm.files", info))) await loadPackage(files);
  } catch (error) {
    say(t("status.plugin-failed", { reason: error.message }));
  }
});
$("plugin-url-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  if (await loadFromUrl($("plugin-url").value, "confirm.url")) $("plugin-url").value = "";
});

// A document asking for a reviewed plugin offers it; nothing loads silently.
let offering = false;
async function offerMissingPlugins(diagnostics) {
  if (offering) return;
  for (const d of diagnostics.filter((x) => x.code === "PLUGIN_NOT_FOUND")) {
    const id = d.message.match(/plugin `([^`]+)`/)?.[1];
    const entry = pluginIndex.find((p) => p.id === id);
    if (!entry || loadedPackages.has(id)) continue;
    offering = true;
    const ok = confirm(t("confirm.index", entry));
    if (ok) {
      await loadPackage(await indexFiles(entry));
      saveEnabled();
    }
    offering = false;
    return;
  }
}

(async () => {
  const { list, base } = await readIndex();
  pluginIndex = list;
  pluginBase = base;
  for (const id of stored("dedalo.plugins")) {
    const entry = pluginIndex.find((p) => p.id === id);
    if (entry) await loadPackage(await indexFiles(entry));
  }
  // Packages the user loaded from an address before: the same bytes load
  // again; a package that changed is asked about.
  const asked = [...location.hash.matchAll(/&plugin=([^&]+)/g)].map((m) => decodeURIComponent(m[1]));
  for (const { url } of stored("dedalo.pluginUrls")) {
    if (!asked.includes(url)) await loadFromUrl(url, null);
  }
  // A shared link that needs a plugin outside the index names it; it is
  // shown with its origin and hash, and loads only on a yes (spec 11.6).
  for (const url of asked.slice(0, 5)) {
    const known = stored("dedalo.pluginUrls").some((entry) => entry.url === url);
    await loadFromUrl(url, known ? null : "confirm.shared");
  }
})();
