// The Dedalo web editor (spec section 14).
//
// It holds no map logic: every answer comes from the core running in a Web
// Worker. Nothing leaves the browser except what the user chooses to share.

import { createEditor } from "./editor.js";

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
const editor = createEditor($("editor"), {
  onChange: (text) => update(text),
  complete: (text, offset) => ask({ cmd: "complete", source: text, offset }),
  hover: async (text, offset) => {
    const r = await ask({ cmd: "hover", source: text, offset });
    return r.ok ? r : null;
  },
});

function renderRequest(text, format) {
  const request = { cmd: "render", source: text, format, force: true, toggles: { grid: $("grid").checked, room_labels: $("labels").checked } };
  if ($("preset").value) request.preset = $("preset").value;
  if ($("export").value) request.export = $("export").value;
  if ($("rotation").value) request.rotation = Number($("rotation").value);
  return request;
}

async function update(text) {
  // Stale answers are dropped by generation number.
  const mine = ++generation;
  const [compiled, rendered, outline, textmap] = await Promise.all([
    ask({ cmd: "compile", source: text }),
    ask(renderRequest(text, "svg")),
    ask({ cmd: "outline", source: text }),
    ask(renderRequest(text, "txt")),
  ]);
  if (mine !== generation) return;
  const report = compiled.report;
  editor.setDiagnostics(report.diagnostics);
  if (report.diagnostics.some((d) => d.code === "PLUGIN_NOT_FOUND")) offerMissingPlugins(report.diagnostics);
  showProblems(report.diagnostics, text);
  showOutline(outline.items);
  $("textmap").textContent = typeof textmap.output === "string" ? textmap.output : "";
  syncExports(compiled.exports || []);
  // The last good or partial render stays visible while errors exist.
  if (typeof rendered.output === "string" && rendered.output.includes("<path")) {
    lastSvg = rendered.output;
    $("canvas").innerHTML = lastSvg;
  }
  const { errors, warnings } = report.summary;
  status.textContent = errors + warnings === 0 ? "No problems" : `${errors} error(s), ${warnings} warning(s)`;
  status.className = errors ? "bad" : warnings ? "warn" : "good";
  try {
    localStorage.setItem("dedalo.draft", text);
  } catch {
    // Storage may be unavailable; drafts are a convenience.
  }
}

function syncExports(names) {
  const select = $("export");
  const current = select.value;
  const wanted = ["", ...names];
  if ([...select.options].map((o) => o.value).join("|") === wanted.join("|")) return;
  select.replaceChildren(...wanted.map((n) => new Option(n || "default", n)));
  select.value = wanted.includes(current) ? current : "";
}

function showProblems(diagnostics, text) {
  const list = $("problems");
  list.replaceChildren();
  for (const d of diagnostics) {
    const item = document.createElement("li");
    item.className = d.severity;
    item.tabIndex = 0;
    const head = document.createElement("div");
    head.innerHTML = `<span class="code"></span><span class="where"></span>`;
    head.querySelector(".code").textContent = d.code;
    head.querySelector(".where").textContent = d.span ? ` line ${d.span.start[0]}` : "";
    const message = document.createElement("div");
    message.textContent = d.message;
    item.append(head, message);
    if (d.suggestion) {
      const help = document.createElement("div");
      help.className = "help";
      help.textContent = d.suggestion;
      item.append(help);
    }
    if (d.fix) {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = "Apply fix";
      button.addEventListener("click", async (event) => {
        event.stopPropagation();
        const result = await ask({ cmd: "edit", source: text, ops: [d.fix] });
        if (result.ok) editor.value = result.source;
        else status.textContent = "The fix could not be applied";
      });
      item.append(button);
    }
    const reveal = () => d.span && editor.revealLine(d.span.start[0], d.span.start[1]);
    item.addEventListener("click", reveal);
    item.addEventListener("keydown", (e) => e.key === "Enter" && reveal());
    list.append(item);
  }
  if (diagnostics.length === 0) {
    const ok = document.createElement("li");
    ok.className = "empty";
    ok.textContent = "No problems.";
    list.append(ok);
  }
}

function showOutline(items) {
  const list = $("outline");
  list.replaceChildren(
    ...items.map((item) => {
      const li = document.createElement("li");
      li.tabIndex = 0;
      li.style.paddingLeft = `${12 + item.depth * 18}px`;
      li.innerHTML = `<span class="kind"></span> <strong></strong> <span class="where"></span>`;
      li.querySelector(".kind").textContent = item.kind;
      li.querySelector("strong").textContent = item.id;
      li.querySelector(".where").textContent = item.label;
      const reveal = () => item.line && editor.revealLine(item.line);
      li.addEventListener("click", reveal);
      li.addEventListener("keydown", (e) => e.key === "Enter" && reveal());
      return li;
    }),
  );
}

// ----- tabs --------------------------------------------------------------------

for (const tab of document.querySelectorAll('[role="tab"]')) {
  tab.addEventListener("click", () => {
    for (const other of document.querySelectorAll('[role="tab"]')) {
      const on = other === tab;
      other.setAttribute("aria-selected", String(on));
      $(other.getAttribute("aria-controls")).hidden = !on;
    }
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
$("zoom-in").addEventListener("click", () => zoomBy(1.25));
$("zoom-out").addEventListener("click", () => zoomBy(0.8));
$("zoom-fit").addEventListener("click", fit);

for (const id of ["preset", "export", "rotation", "grid", "labels"]) {
  $(id).addEventListener("change", async () => {
    await update(editor.value);
    fit();
  });
}

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
  else status.textContent = "Fix the syntax errors before formatting";
});
$("examples").addEventListener("change", async () => {
  const name = $("examples").value;
  $("examples").value = "";
  if (!name) return;
  if (name === "starter") {
    editor.value = STARTER;
  } else {
    // The site ships the examples; a development server serves the corpus.
    let response = await fetch(`examples/${name}.dedalo`).catch(() => null);
    if (!response || !response.ok) response = await fetch(`../tests/corpus/${name}/input.dedalo`);
    editor.value = await response.text();
  }
  setTimeout(fit, 50);
});
$("download").addEventListener("change", async () => {
  const kind = $("download").value;
  $("download").value = "";
  if (!kind) return;
  const base = `${await dungeonId()}.${$("preset").value || $("export").value || "map"}`;
  status.textContent = "Exporting…";
  try {
    if (kind === "json") {
      const r = await ask({ cmd: "convert", source: editor.value, to: "json" });
      if (!r.ok) throw new Error("the document has errors");
      download(`${await dungeonId()}.json`, r.text, "application/json");
    } else if (kind === "svg" || kind === "txt") {
      const r = await ask({ ...renderRequest(editor.value, kind), force: false });
      if (typeof r.output !== "string") throw new Error(r.error || "the dungeon has errors");
      download(`${base}.${kind}`, r.output, kind === "svg" ? "image/svg+xml" : "text/plain");
    } else {
      // PNG and PDF come from the export module, loaded on first use.
      const r = await ask({ ...renderRequest(editor.value, "svg"), force: false });
      if (typeof r.output !== "string") throw new Error(r.error || "the dungeon has errors");
      const bytes = await ask({ cmd: kind, svg: r.output }, "export");
      if (!bytes.ok) throw new Error(bytes.error);
      download(`${base}.${kind}`, fromBase64(bytes.base64), bytes.media_type);
    }
    status.textContent = "Exported";
  } catch (error) {
    status.textContent = `Export failed: ${error.message}`;
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
    status.textContent = "Too long for a link; saving a file instead";
    download(`${await dungeonId()}.dedalo`, editor.value, "text/plain");
    return;
  }
  // The fragment never reaches the server.
  const url = `${location.origin}${location.pathname}#src=${encoded}`;
  history.replaceState(null, "", url);
  try {
    await navigator.clipboard.writeText(url);
    status.textContent = "Link copied";
  } catch {
    status.textContent = "Link in the address bar";
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
      status.textContent = "The shared link is damaged";
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
status.textContent = "Loading…";
start();
window.addEventListener("resize", fit);

// Offline use after the first visit.
if ("serviceWorker" in navigator && location.protocol !== "file:") {
  navigator.serviceWorker.register(new URL("sw.js", import.meta.url)).catch(() => {});
}

// ----- plugins (spec 11.6) ----------------------------------------------------------

let pluginIndex = [];
let pluginBase = null;

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

async function fetchFiles(base) {
  const decoder = new TextDecoder();
  const get = async (path) => new Uint8Array(await (await fetch(new URL(path, base))).arrayBuffer());
  const files = new Map([["dedalo-plugin.toml", await get("dedalo-plugin.toml")]]);
  for (const [, path] of decoder.decode(files.get("dedalo-plugin.toml")).matchAll(/^\s*(?:file|module)\s*=\s*"([^"]+)"/gm)) {
    files.set(path, await get(path));
  }
  return files;
}

async function loadPackage(files) {
  const result = await post({ plugin: { files: Object.fromEntries(files) } });
  if (result.ok) {
    loadedPackages.set(result.plugin.id, files);
    status.textContent = `Plugin ${result.plugin.id} ${result.plugin.version} loaded`;
    await update(editor.value);
  } else {
    status.textContent = `The plugin could not be loaded: ${result.error}`;
  }
  renderPluginList();
  return result;
}

function enabledIds() {
  try {
    return JSON.parse(localStorage.getItem("dedalo.plugins") || "[]");
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
        await loadPackage(await fetchFiles(new URL(`${p.id}/`, pluginBase)));
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
        li.textContent = `${id} (from files, this session only)`;
        return li;
      }),
  );
}

$("plugins-open").addEventListener("click", () => {
  renderPluginList();
  $("plugins").showModal();
});
$("plugin-files").addEventListener("change", async () => {
  const chosen = [...$("plugin-files").files];
  const manifest = chosen.find((f) => f.name === "dedalo-plugin.toml");
  if (!manifest) {
    status.textContent = "Choose the folder that holds dedalo-plugin.toml";
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
  if (confirm(`Load the plugin "${id}" from your files? It will run sandboxed in this page.`)) await loadPackage(files);
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
    const ok = confirm(`This dungeon uses the plugin "${id}" ${entry.version} from the index of this site (${entry.sha256}). Load it?`);
    if (ok) {
      await loadPackage(await fetchFiles(new URL(`${id}/`, pluginBase)));
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
  for (const id of enabledIds()) {
    if (pluginIndex.some((p) => p.id === id)) await loadPackage(await fetchFiles(new URL(`${id}/`, base)));
  }
})();
