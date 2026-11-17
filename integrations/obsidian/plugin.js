"use strict";
// Dedalo for Obsidian: draws ```dedalo code blocks as dungeon maps.
//
// The map comes from the same WebAssembly core as the web editor, inlined
// below by build.py, so the plugin works offline and draws the same bytes
// as the `dedalo` command. Plain CommonJS, no bundler: `obsidian` is the
// only module it requires.

const obsidian = require("obsidian");
const { Plugin, PluginSettingTab, Setting, Notice, Modal, Menu, MarkdownRenderChild, MarkdownView, normalizePath, setIcon } = obsidian;

const WASM_BASE64 = "__DEDALO_WASM__";

// ---------------------------------------------------------------------------
// Words

const STRINGS = {
  en: {
    "loading": "Loading Dedalo…",
    "audience.dm": "DM",
    "audience.player": "Players",
    "audience.clean": "Clean",
    "audience.label": "Who this map is for; click to switch",
    "level.all": "All levels",
    "level.label": "Level",
    "expand": "Open large",
    "more": "More",
    "copy.png": "Copy image",
    "copy.svg": "Copy SVG code",
    "save.svg": "Save as SVG",
    "save.png": "Save as PNG",
    "saved": "Saved {path}",
    "copied": "Copied",
    "copy.failed": "Could not copy: {reason}",
    "save.failed": "Could not save: {reason}",
    "line": "line {line}",
    "go": "Go to line {line}",
    "failed": "The map could not be drawn.",
    "alt": "Map: {name}",
    "alt.untitled": "Dungeon map",
    "options.unknown": "Unknown option {names} after dedalo; the options are {list}.",
    "viewer.hint": "Scroll to zoom, drag to move, double-click to fit.",
    "command.insert": "Insert a dungeon map",
    "command.format": "Format the dungeon map under the cursor",
    "command.audience": "Switch every map between DM and players",
    "format.none": "The cursor is not inside a dedalo code block.",
    "format.failed": "Fix the syntax errors before formatting.",
    "audience.now": "Maps now show the {audience} view",
    "packages.loaded": "Dedalo: loaded {id} {version}",
    "packages.failed": "Dedalo: {file} could not be loaded: {reason}",
    "packages.missing": "Dedalo: the plugin folder {folder} does not exist",
    "settings.audience": "Default audience",
    "settings.audience.desc": "Who maps are drawn for, unless a block says otherwise (```dedalo player). DM shows everything; Players hides secrets and DM notes.",
    "settings.language": "Label language",
    "settings.language.desc": "The language of legend labels and headings that a dungeon does not set itself.",
    "settings.language.auto": "Same as Obsidian",
    "settings.dark": "Maps in dark themes",
    "settings.dark.desc": "Paper keeps maps black on white; Inverted turns them light on dark.",
    "settings.dark.paper": "Paper",
    "settings.dark.invert": "Inverted",
    "settings.height": "Largest map height",
    "settings.height.desc": "In pixels. Open large shows the whole map.",
    "settings.packages": "Dedalo plugin folder",
    "settings.packages.desc": "A vault folder holding Dedalo plugin packages (.dedalopkg files or package folders), such as themes. Leave empty for none.",
    "settings.toolbar": "Always show the map toolbar",
    "settings.toolbar.desc": "Otherwise it appears when the pointer is over a map.",
    "starter.name": "My Dungeon",
    "starter.room": "Entrance",
  },
  it: {
    "loading": "Caricamento di Dedalo…",
    "audience.dm": "Master",
    "audience.player": "Giocatori",
    "audience.clean": "Pulita",
    "audience.label": "Per chi è questa mappa; clic per cambiare",
    "level.all": "Tutti i livelli",
    "level.label": "Livello",
    "expand": "Apri in grande",
    "more": "Altro",
    "copy.png": "Copia immagine",
    "copy.svg": "Copia il codice SVG",
    "save.svg": "Salva come SVG",
    "save.png": "Salva come PNG",
    "saved": "Salvato {path}",
    "copied": "Copiato",
    "copy.failed": "Copia non riuscita: {reason}",
    "save.failed": "Salvataggio non riuscito: {reason}",
    "line": "riga {line}",
    "go": "Vai alla riga {line}",
    "failed": "Non è stato possibile disegnare la mappa.",
    "alt": "Mappa: {name}",
    "alt.untitled": "Mappa del dungeon",
    "options.unknown": "Opzione sconosciuta {names} dopo dedalo; le opzioni sono {list}.",
    "viewer.hint": "Rotella per lo zoom, trascina per spostare, doppio clic per adattare.",
    "command.insert": "Inserisci una mappa di dungeon",
    "command.format": "Formatta la mappa sotto il cursore",
    "command.audience": "Alterna tutte le mappe tra master e giocatori",
    "format.none": "Il cursore non è dentro un blocco dedalo.",
    "format.failed": "Correggi gli errori di sintassi prima di formattare.",
    "audience.now": "Ora le mappe mostrano la vista {audience}",
    "packages.loaded": "Dedalo: caricato {id} {version}",
    "packages.failed": "Dedalo: impossibile caricare {file}: {reason}",
    "packages.missing": "Dedalo: la cartella dei plugin {folder} non esiste",
    "settings.audience": "Pubblico predefinito",
    "settings.audience.desc": "Per chi vengono disegnate le mappe, se il blocco non dice altro (```dedalo player). Master mostra tutto; Giocatori nasconde segreti e note del master.",
    "settings.language": "Lingua delle etichette",
    "settings.language.desc": "La lingua di legenda e titoli che il dungeon non imposta da sé.",
    "settings.language.auto": "Come Obsidian",
    "settings.dark": "Mappe nei temi scuri",
    "settings.dark.desc": "Carta lascia le mappe nere su bianco; Invertite le rende chiare su scuro.",
    "settings.dark.paper": "Carta",
    "settings.dark.invert": "Invertite",
    "settings.height": "Altezza massima della mappa",
    "settings.height.desc": "In pixel. Apri in grande mostra la mappa intera.",
    "settings.packages": "Cartella dei plugin di Dedalo",
    "settings.packages.desc": "Una cartella del vault con pacchetti di plugin di Dedalo (file .dedalopkg o cartelle di pacchetto), come i temi. Vuota per nessuno.",
    "settings.toolbar": "Mostra sempre la barra della mappa",
    "settings.toolbar.desc": "Altrimenti compare quando il puntatore è sopra una mappa.",
    "starter.name": "Il mio dungeon",
    "starter.room": "Ingresso",
  },
};

function obsidianLanguage() {
  try {
    const locale = (obsidian.moment && obsidian.moment.locale()) || window.localStorage.getItem("language") || "en";
    return String(locale).toLowerCase().startsWith("it") ? "it" : "en";
  } catch {
    return "en";
  }
}

let uiLanguage = "en";

function t(key, values = {}) {
  const text = (STRINGS[uiLanguage] && STRINGS[uiLanguage][key]) || STRINGS.en[key] || key;
  return text.replace(/\{(\w+)\}/g, (_, name) => String(values[name] ?? ""));
}

// ---------------------------------------------------------------------------
// Fence options, as the `dedalo` command reads them (crates/dedalo-core/src/markdown.rs)

const OPTIONS = ["export", "preset", "level", "rotation", "lang", "grid", "labels", "notes"];

/** Parses the words after `dedalo`: `key=value`, quoted values, a bare preset. */
function parseOptions(info) {
  const options = {};
  const pattern = /([^\s=]+)(?:=(?:"([^"]*)"?|(\S*)))?/g;
  let match;
  while ((match = pattern.exec(info))) {
    const [, key, quoted, plain] = match;
    if (quoted !== undefined || plain !== undefined) options[key] = quoted ?? plain;
    else if (["dm", "player", "clean"].includes(key)) options.preset = key;
    else options[key] = "true";
  }
  return options;
}

const FENCE = /^( {0,3})(`{3,}|~{3,})\s*dedalo(?:\s+(.*))?$/;

/** The info string of the block's opening fence, if the section is known. */
function fenceInfo(ctx, el) {
  const info = ctx.getSectionInfo(el);
  if (!info) return { options: {}, line: null };
  const first = info.text.split("\n")[info.lineStart] || "";
  const match = first.replace(/\r$/, "").match(FENCE);
  return { options: match ? parseOptions(match[3] || "") : {}, line: info.lineStart };
}

function flag(value) {
  if (value === undefined) return undefined;
  return ["true", "yes", "on"].includes(String(value).toLowerCase());
}

// ---------------------------------------------------------------------------
// The WebAssembly core (the same calls as web/dedalo.js)

const encoder = new TextEncoder();
const decoder = new TextDecoder();

function fromBase64(text) {
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

function toBase64(bytes) {
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary);
}

function packedCall(exports, request) {
  const input = request instanceof Uint8Array ? request : encoder.encode(typeof request === "string" ? request : JSON.stringify(request));
  const inputPtr = exports.dedalo_alloc(input.length);
  new Uint8Array(exports.memory.buffer, inputPtr, input.length).set(input);
  const packed = BigInt.asUintN(64, BigInt(exports.dedalo_call(inputPtr, input.length)));
  exports.dedalo_free(inputPtr, input.length);
  const ptr = Number(packed >> 32n);
  const len = Number(packed & 0xffffffffn);
  const bytes = new Uint8Array(exports.memory.buffer, ptr, len).slice();
  exports.dedalo_free(ptr, len);
  return bytes;
}

// SplitMix64, the same stream the binary gives plugins.
function nextRandom(state) {
  state.value = BigInt.asUintN(64, state.value + 0x9e3779b97f4a7c15n);
  let z = state.value;
  z = BigInt.asUintN(64, (z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n);
  z = BigInt.asUintN(64, (z ^ (z >> 27n)) * 0x94d049bb133111ebn);
  return z ^ (z >> 31n);
}

/** The core with a host for Dedalo plugins (themes and WebAssembly plugins). */
async function loadCore(bytes) {
  const plugins = new Map();
  let core = null;

  function runPlugin(plugin, request, seed) {
    let current = null;
    const rng = { value: seed };
    const read = (ptr, len) => new Uint8Array(current.exports.memory.buffer, ptr, len).slice();
    const give = (data) => {
      const ptr = current.exports.dedalo_alloc(data.length);
      new Uint8Array(current.exports.memory.buffer, ptr, data.length).set(data);
      return BigInt.asIntN(64, (BigInt(ptr >>> 0) << 32n) | BigInt(data.length));
    };
    const imports = {
      dedalo: {
        log: (level, ptr, len) => console.debug(`[dedalo:${plugin.id}] ${decoder.decode(read(ptr, len))}`),
        rand_u64: () => BigInt.asIntN(64, nextRandom(rng)),
        asset: (ptr, len) => {
          const file = plugin.files.get(decoder.decode(read(ptr, len)));
          return file ? give(file) : 0n;
        },
        measure_text: (ptr, len) => {
          const input = read(ptr, len);
          const e = core.exports;
          const inPtr = e.dedalo_alloc(input.length);
          new Uint8Array(e.memory.buffer, inPtr, input.length).set(input);
          const packed = BigInt.asUintN(64, BigInt(e.dedalo_measure(inPtr, input.length)));
          e.dedalo_free(inPtr, input.length);
          const at = Number(packed >> 32n);
          const size = Number(packed & 0xffffffffn);
          const out = new Uint8Array(e.memory.buffer, at, size).slice();
          e.dedalo_free(at, size);
          return give(out);
        },
      },
    };
    current = new WebAssembly.Instance(plugin.module, imports);
    return packedCall(current.exports, request);
  }

  const host = {
    dedalo_host: {
      plugin_call(idPtr, idLen, reqPtr, reqLen, seed) {
        const memory = core.exports.memory.buffer;
        const id = decoder.decode(new Uint8Array(memory, idPtr, idLen));
        const request = new Uint8Array(memory, reqPtr, reqLen).slice();
        const plugin = plugins.get(id);
        if (!plugin) return 0n;
        let answer;
        try {
          answer = runPlugin(plugin, request, BigInt.asUintN(64, BigInt(seed)));
        } catch (error) {
          answer = encoder.encode(JSON.stringify({ error: { message: `the plugin trapped: ${error.message}` } }));
        }
        const ptr = core.exports.dedalo_alloc(answer.length);
        new Uint8Array(core.exports.memory.buffer, ptr, answer.length).set(answer);
        return BigInt.asIntN(64, (BigInt(ptr >>> 0) << 32n) | BigInt(answer.length));
      },
    },
  };
  ({ instance: core } = await WebAssembly.instantiate(bytes, host));
  const call = (request) => JSON.parse(decoder.decode(packedCall(core.exports, request)));

  async function loadPlugin(files) {
    const encoded = {};
    for (const [path, data] of files) encoded[path] = toBase64(data);
    const result = call({ cmd: "load_plugin", files: encoded });
    if (!result.ok) return result;
    const manifest = decoder.decode(files.get("dedalo-plugin.toml"));
    const wasmPath = (manifest.match(/\[wasm\][^[]*?module\s*=\s*"([^"]+)"/) || [])[1];
    if (wasmPath) {
      // Same rule as the binary: only `dedalo.*` imports.
      const module = await WebAssembly.compile(files.get(wasmPath));
      const bad = WebAssembly.Module.imports(module).find((i) => i.module !== "dedalo");
      if (bad) return { ok: false, error: `the module imports ${bad.module}.${bad.name}` };
      plugins.set(result.plugin.id, { id: result.plugin.id, module, files });
    }
    return result;
  }

  return { call, loadPlugin };
}

// ---------------------------------------------------------------------------
// Images

function svgUrl(svg) {
  return URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" }));
}

/** Draws the SVG on a canvas: a PNG of at most `maxEdge` pixels a side. */
async function svgToPng(svg, maxEdge = 4096) {
  const url = svgUrl(svg);
  try {
    const img = new Image();
    img.src = url;
    await img.decode();
    const scale = Math.min(1, maxEdge / Math.max(img.naturalWidth, img.naturalHeight));
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(img.naturalWidth * scale));
    canvas.height = Math.max(1, Math.round(img.naturalHeight * scale));
    canvas.getContext("2d").drawImage(img, 0, 0, canvas.width, canvas.height);
    return await new Promise((resolve, reject) => canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("no image"))), "image/png"));
  } finally {
    URL.revokeObjectURL(url);
  }
}

function dungeonId(source) {
  const match = source.match(/dungeon\s+"([^"]+)"/);
  return match ? match[1] : "dungeon";
}

function dungeonName(source) {
  const match = source.match(/dungeon\s+"[^"]*"\s*\{[^]*?\bname\s*=\s*"([^"]+)"/);
  return match ? match[1] : null;
}

// ---------------------------------------------------------------------------
// One map in a note

const AUDIENCES = ["dm", "player", "clean"];

class DedaloMap extends MarkdownRenderChild {
  constructor(plugin, source, el, ctx) {
    super(el);
    this.plugin = plugin;
    this.source = source;
    this.ctx = ctx;
    this.url = null;
    this.svg = null;
  }

  get key() {
    return `${this.ctx.sourcePath}:${this.fence.line}`;
  }

  onload() {
    this.fence = fenceInfo(this.ctx, this.containerEl);
    this.plugin.maps.add(this);
    this.containerEl.addClass("dedalo-block");
    this.render();
  }

  onunload() {
    this.plugin.maps.delete(this);
    if (this.url) URL.revokeObjectURL(this.url);
  }

  /** Chosen on the map, else the fence, else the setting; with `export=`
   * and nothing chosen, the export's own audience (null). */
  audience() {
    const state = this.plugin.blockState.get(this.key) || {};
    const options = this.fence.options;
    return state.audience || options.preset || (options.export ? null : this.plugin.settings.audience);
  }

  level() {
    const state = this.plugin.blockState.get(this.key) || {};
    return state.level !== undefined ? state.level : this.fence.options.level || "";
  }

  setState(change) {
    this.plugin.blockState.set(this.key, { ...(this.plugin.blockState.get(this.key) || {}), ...change });
    this.render();
  }

  async render() {
    const el = this.containerEl;
    const settings = this.plugin.settings;
    el.style.setProperty("--dedalo-max-height", `${settings.maxHeight}px`);
    el.toggleClass("dedalo-invert", settings.dark === "invert");
    if (!this.plugin.core) {
      el.empty();
      el.createDiv({ cls: "dedalo-message", text: t("loading") });
      try {
        await this.plugin.ready;
      } catch (error) {
        el.empty();
        el.createDiv({ cls: "dedalo-message", text: `${t("failed")} ${error.message}` });
        return;
      }
    }
    const options = this.fence.options;
    const toggles = {};
    for (const [option, toggle] of [["grid", "grid"], ["labels", "room_labels"], ["notes", "notes"]]) {
      if (options[option] !== undefined) toggles[toggle] = flag(options[option]);
    }
    const rotation = Number(options.rotation);
    const request = {
      cmd: "render",
      source: this.source,
      format: "svg",
      force: true,
      preset: this.audience(),
      export: options.export || null,
      level: this.level() || null,
      rotation: [0, 90, 180, 270].includes(rotation) ? rotation : null,
      language: options.lang || this.plugin.labelLanguage(),
      toggles,
    };
    const result = this.plugin.render(request);
    this.show(result, request);
  }

  show(result, request) {
    const el = this.containerEl;
    el.empty();
    if (this.url) URL.revokeObjectURL(this.url);
    this.url = null;
    this.svg = typeof result.output === "string" ? result.output : null;
    const diagnostics = (result.report && result.report.diagnostics) || [];
    const unknown = Object.keys(this.fence.options).filter((k) => !OPTIONS.includes(k));

    if (this.svg) {
      const frame = el.createDiv({ cls: "dedalo-frame" });
      this.url = svgUrl(this.svg);
      const name = dungeonName(this.source);
      frame.createEl("img", { attr: { src: this.url, alt: name ? t("alt", { name }) : t("alt.untitled"), draggable: "false" } });
      el.toggleClass("dedalo-stale", diagnostics.some((d) => d.severity === "error"));
      if (result.preset && result.preset !== "dm") frame.createDiv({ cls: "dedalo-badge", text: t(`audience.${result.preset}`) });
      this.toolbar(frame, result, request);
    } else {
      el.createDiv({ cls: "dedalo-message", text: result.error ? `${t("failed")} ${result.error}` : t("failed") });
    }

    if (diagnostics.length || unknown.length) {
      const list = el.createEl("ul", { cls: "dedalo-problems" });
      if (unknown.length) {
        list.createEl("li", { cls: "is-warning", text: t("options.unknown", { names: unknown.join(", "), list: OPTIONS.join(", ") }) });
      }
      for (const d of diagnostics) this.problem(list, d);
    }
  }

  problem(list, d) {
    const item = list.createEl("li", { cls: d.severity === "error" ? "is-error" : "is-warning" });
    const line = d.span && d.span.start ? d.span.start[0] : null;
    if (line !== null && this.fence.line !== null) {
      // Lines count from the block's first line; the note's line is after the fence.
      const noteLine = this.fence.line + line;
      const link = item.createSpan({ cls: "dedalo-line", text: t("line", { line: noteLine + 1 }) });
      link.setAttr("role", "button");
      link.setAttr("tabindex", "0");
      link.setAttr("aria-label", t("go", { line: noteLine + 1 }));
      const go = (evt) => {
        evt.preventDefault();
        evt.stopPropagation();
        this.plugin.goTo(this.ctx.sourcePath, noteLine, Math.max(0, d.span.start[1] - 1));
      };
      link.addEventListener("click", go);
      link.addEventListener("keydown", (evt) => {
        if (evt.key === "Enter" || evt.key === " ") go(evt);
      });
    }
    item.createSpan({ text: d.message });
    if (d.suggestion) item.createDiv({ cls: "dedalo-help", text: d.suggestion });
  }

  toolbar(frame, result, request) {
    const bar = frame.createDiv({ cls: "dedalo-toolbar" });
    if (this.plugin.settings.alwaysShowToolbar) bar.addClass("is-pinned");
    // Clicks on the toolbar must not open the block for editing.
    for (const type of ["mousedown", "click", "pointerdown"]) bar.addEventListener(type, (evt) => evt.stopPropagation());

    const shown = result.preset || request.preset || "dm";
    const audience = bar.createEl("button", { text: t(`audience.${shown}`), attr: { "aria-label": t("audience.label") } });
    audience.addEventListener("click", () => {
      const next = AUDIENCES[(AUDIENCES.indexOf(shown) + 1) % AUDIENCES.length];
      this.setState({ audience: next });
    });

    const levels = result.levels || [];
    if (levels.length) {
      const select = bar.createEl("select", { attr: { "aria-label": t("level.label") } });
      select.createEl("option", { text: t("level.all"), value: "" });
      for (const level of levels) select.createEl("option", { text: level.name || level.id, value: level.id });
      select.value = request.level || "";
      select.addEventListener("change", () => this.setState({ level: select.value }));
    }

    const expand = bar.createEl("button", { cls: "dedalo-icon", attr: { "aria-label": t("expand") } });
    setIcon(expand, "maximize-2");
    expand.addEventListener("click", () => new DedaloViewer(this.plugin.app, this.svg, this.plugin.settings.dark === "invert").open());

    const more = bar.createEl("button", { cls: "dedalo-icon", attr: { "aria-label": t("more") } });
    setIcon(more, "more-horizontal");
    more.addEventListener("click", (evt) => {
      const menu = new Menu();
      menu.addItem((item) => item.setTitle(t("copy.png")).setIcon("image").onClick(() => this.copyPng()));
      menu.addItem((item) => item.setTitle(t("copy.svg")).setIcon("code").onClick(() => this.copySvg()));
      menu.addSeparator();
      menu.addItem((item) => item.setTitle(t("save.png")).setIcon("download").onClick(() => this.save("png", result)));
      menu.addItem((item) => item.setTitle(t("save.svg")).setIcon("download").onClick(() => this.save("svg", result)));
      menu.showAtMouseEvent(evt);
    });
  }

  async copyPng() {
    try {
      const png = await svgToPng(this.svg);
      await navigator.clipboard.write([new ClipboardItem({ "image/png": png })]);
      new Notice(t("copied"));
    } catch (error) {
      new Notice(t("copy.failed", { reason: error.message }));
    }
  }

  async copySvg() {
    try {
      await navigator.clipboard.writeText(this.svg);
      new Notice(t("copied"));
    } catch (error) {
      new Notice(t("copy.failed", { reason: error.message }));
    }
  }

  async save(format, result) {
    try {
      const audience = result.preset || this.audience();
      const level = this.level();
      const name = [dungeonId(this.source), result.export && result.export !== "default" ? result.export : audience, level].filter(Boolean).join("-");
      const data = format === "png" ? await (await svgToPng(this.svg, 8192)).arrayBuffer() : encoder.encode(this.svg).buffer;
      const path = await this.plugin.attachmentPath(`${name}.${format}`, this.ctx.sourcePath);
      await this.plugin.app.vault.createBinary(path, data);
      new Notice(t("saved", { path }));
    } catch (error) {
      new Notice(t("save.failed", { reason: error.message }));
    }
  }
}

// ---------------------------------------------------------------------------
// A large view that zooms and pans

class DedaloViewer extends Modal {
  constructor(app, svg, invert) {
    super(app);
    this.svg = svg;
    this.invert = invert;
  }

  onOpen() {
    this.modalEl.addClass("dedalo-modal");
    this.modalEl.toggleClass("dedalo-invert", this.invert);
    const viewer = this.contentEl.createDiv({ cls: "dedalo-viewer" });
    this.contentEl.createDiv({ cls: "dedalo-viewer-hint", text: t("viewer.hint") });
    this.url = svgUrl(this.svg);
    const img = viewer.createEl("img", { attr: { src: this.url, alt: t("alt.untitled"), draggable: "false" } });
    const view = { x: 0, y: 0, scale: 1 };
    const apply = () => {
      img.style.transform = `translate(${view.x}px, ${view.y}px) scale(${view.scale})`;
    };
    const fit = () => {
      const w = img.naturalWidth || 1;
      const h = img.naturalHeight || 1;
      view.scale = Math.min(viewer.clientWidth / w, viewer.clientHeight / h);
      view.x = (viewer.clientWidth - w * view.scale) / 2;
      view.y = (viewer.clientHeight - h * view.scale) / 2;
      apply();
    };
    img.decode().then(fit, fit);
    viewer.addEventListener("wheel", (evt) => {
      evt.preventDefault();
      const rect = viewer.getBoundingClientRect();
      const px = evt.clientX - rect.left;
      const py = evt.clientY - rect.top;
      const factor = Math.exp(-evt.deltaY * 0.0015);
      const next = Math.min(20, Math.max(0.05, view.scale * factor));
      view.x = px - ((px - view.x) * next) / view.scale;
      view.y = py - ((py - view.y) * next) / view.scale;
      view.scale = next;
      apply();
    }, { passive: false });
    let drag = null;
    viewer.addEventListener("pointerdown", (evt) => {
      drag = { x: evt.clientX - view.x, y: evt.clientY - view.y };
      viewer.setPointerCapture(evt.pointerId);
      viewer.addClass("is-dragging");
    });
    viewer.addEventListener("pointermove", (evt) => {
      if (!drag) return;
      view.x = evt.clientX - drag.x;
      view.y = evt.clientY - drag.y;
      apply();
    });
    const stop = () => {
      drag = null;
      viewer.removeClass("is-dragging");
    };
    viewer.addEventListener("pointerup", stop);
    viewer.addEventListener("pointercancel", stop);
    viewer.addEventListener("dblclick", fit);
  }

  onClose() {
    this.contentEl.empty();
    if (this.url) URL.revokeObjectURL(this.url);
  }
}

// ---------------------------------------------------------------------------
// Settings

const DEFAULTS = {
  audience: "dm",
  language: "auto",
  dark: "paper",
  maxHeight: 640,
  packagesFolder: "",
  alwaysShowToolbar: false,
};

class DedaloSettingTab extends PluginSettingTab {
  constructor(app, plugin) {
    super(app, plugin);
    this.plugin = plugin;
  }

  display() {
    const { containerEl } = this;
    const plugin = this.plugin;
    containerEl.empty();
    new Setting(containerEl)
      .setName(t("settings.audience"))
      .setDesc(t("settings.audience.desc"))
      .addDropdown((d) => {
        for (const a of AUDIENCES) d.addOption(a, t(`audience.${a}`));
        d.setValue(plugin.settings.audience).onChange((v) => plugin.update({ audience: v }));
      });
    new Setting(containerEl)
      .setName(t("settings.language"))
      .setDesc(t("settings.language.desc"))
      .addDropdown((d) =>
        d
          .addOption("auto", t("settings.language.auto"))
          .addOption("en", "English")
          .addOption("it", "Italiano")
          .setValue(plugin.settings.language)
          .onChange((v) => plugin.update({ language: v })),
      );
    new Setting(containerEl)
      .setName(t("settings.dark"))
      .setDesc(t("settings.dark.desc"))
      .addDropdown((d) =>
        d
          .addOption("paper", t("settings.dark.paper"))
          .addOption("invert", t("settings.dark.invert"))
          .setValue(plugin.settings.dark)
          .onChange((v) => plugin.update({ dark: v })),
      );
    new Setting(containerEl)
      .setName(t("settings.height"))
      .setDesc(t("settings.height.desc"))
      .addSlider((s) =>
        s
          .setLimits(200, 1600, 20)
          .setValue(plugin.settings.maxHeight)
          .setDynamicTooltip()
          .onChange((v) => plugin.update({ maxHeight: v })),
      );
    new Setting(containerEl)
      .setName(t("settings.toolbar"))
      .setDesc(t("settings.toolbar.desc"))
      .addToggle((g) => g.setValue(plugin.settings.alwaysShowToolbar).onChange((v) => plugin.update({ alwaysShowToolbar: v })));
    new Setting(containerEl)
      .setName(t("settings.packages"))
      .setDesc(t("settings.packages.desc"))
      .addText((x) =>
        x
          .setPlaceholder("Dedalo/plugins")
          .setValue(plugin.settings.packagesFolder)
          .onChange((v) => {
            plugin.settings.packagesFolder = v;
            plugin.saveSettings();
          }),
      );
  }

  hide() {
    // A changed folder takes effect once the settings close.
    this.plugin.reloadPackages();
  }
}

// ---------------------------------------------------------------------------
// The plugin

const STARTER = (name, room) => `dungeon "my_dungeon" {
    name = "${name}"

    room "entrance" {
        name  = "${room}"
        shape = rectangle(8, 6)
    }
}
`;

/** The `dedalo` block around a line of the editor: its content lines. */
function blockAround(editor, line) {
  let open = null;
  for (let i = 0; i < editor.lineCount(); i++) {
    const text = editor.getLine(i).replace(/\r$/, "");
    if (!open) {
      const fence = text.match(/^ {0,3}(`{3,}|~{3,})(.*)$/);
      if (fence && !(fence[1][0] === "`" && fence[2].includes("`"))) {
        open = { at: i, marker: fence[1], dedalo: FENCE.test(text) };
      } else if (i >= line) {
        return null;
      }
    } else {
      const close = text.match(/^ {0,3}(`{3,}|~{3,})\s*$/);
      if (close && close[1][0] === open.marker[0] && close[1].length >= open.marker.length) {
        if (open.dedalo && line > open.at && line < i) return { start: open.at + 1, end: i };
        if (i >= line) return null;
        open = null;
      }
    }
  }
  return open && open.dedalo && line > open.at ? { start: open.at + 1, end: editor.lineCount() } : null;
}

class DedaloPlugin extends Plugin {
  async onload() {
    uiLanguage = obsidianLanguage();
    this.settings = Object.assign({}, DEFAULTS, await this.loadData());
    this.maps = new Set();
    this.blockState = new Map();
    this.cache = new Map();
    this.core = null;
    this.loadedFolder = this.settings.packagesFolder;
    this.ready = this.start();

    this.registerMarkdownCodeBlockProcessor("dedalo", (source, el, ctx) => {
      ctx.addChild(new DedaloMap(this, source, el, ctx));
    });

    this.addSettingTab(new DedaloSettingTab(this.app, this));

    this.addCommand({
      id: "insert-map",
      name: t("command.insert"),
      editorCallback: (editor) => {
        const fence = "```dedalo\n" + STARTER(t("starter.name"), t("starter.room")) + "```\n";
        const cursor = editor.getCursor();
        const prefix = cursor.ch > 0 ? "\n\n" : "";
        editor.replaceSelection(prefix + fence);
      },
    });

    this.addCommand({
      id: "format-map",
      name: t("command.format"),
      editorCheckCallback: (checking, editor) => {
        const block = blockAround(editor, editor.getCursor().line);
        if (checking) return Boolean(block) && Boolean(this.core);
        if (!block) {
          new Notice(t("format.none"));
          return;
        }
        if (block.end <= block.start) return;
        const lines = [];
        for (let i = block.start; i < block.end; i++) lines.push(editor.getLine(i));
        const result = this.core.call({ cmd: "format", source: lines.join("\n") + "\n" });
        if (!result.ok) {
          new Notice(t("format.failed"));
          return;
        }
        const text = result.text.replace(/\n+$/, "");
        editor.replaceRange(text, { line: block.start, ch: 0 }, { line: block.end - 1, ch: editor.getLine(block.end - 1).length });
      },
    });

    this.addCommand({
      id: "switch-audience",
      name: t("command.audience"),
      callback: () => {
        const next = this.settings.audience === "dm" ? "player" : "dm";
        // A switch for the whole screen: what each map was set to is forgotten.
        this.blockState.clear();
        this.update({ audience: next });
        new Notice(t("audience.now", { audience: t(`audience.${next}`) }));
      },
    });
  }

  onunload() {
    this.maps.clear();
  }

  async start() {
    const core = await loadCore(fromBase64(WASM_BASE64));
    this.core = core;
    await this.loadPackages(core);
    return core;
  }

  /** Loads the plugin packages in the chosen vault folder. */
  async loadPackages(core) {
    const folder = (this.settings.packagesFolder || "").trim();
    if (!folder) return;
    const path = normalizePath(folder);
    const adapter = this.app.vault.adapter;
    if (!(await adapter.exists(path))) {
      new Notice(t("packages.missing", { folder: path }));
      return;
    }
    const listing = await adapter.list(path);
    const report = (file, result) => {
      if (result.ok) console.info(t("packages.loaded", { id: result.plugin.id, version: result.plugin.version }));
      else new Notice(t("packages.failed", { file, reason: result.error }));
    };
    for (const file of listing.files.filter((f) => f.toLowerCase().endsWith(".dedalopkg"))) {
      try {
        const bytes = new Uint8Array(await adapter.readBinary(file));
        const opened = core.call({ cmd: "unpack_package", package: toBase64(bytes) });
        if (!opened.ok) {
          report(file, opened);
          continue;
        }
        const files = new Map(Object.entries(opened.files).map(([p, b]) => [p, fromBase64(b)]));
        report(file, await core.loadPlugin(files));
      } catch (error) {
        report(file, { ok: false, error: error.message });
      }
    }
    for (const dir of listing.folders) {
      if (!(await adapter.exists(`${dir}/dedalo-plugin.toml`))) continue;
      try {
        const files = new Map();
        const walk = async (at) => {
          const inner = await adapter.list(at);
          for (const f of inner.files) files.set(f.slice(dir.length + 1), new Uint8Array(await adapter.readBinary(f)));
          for (const d of inner.folders) await walk(d);
        };
        await walk(dir);
        report(dir, await core.loadPlugin(files));
      } catch (error) {
        report(dir, { ok: false, error: error.message });
      }
    }
  }

  async reloadPackages() {
    if (this.settings.packagesFolder === this.loadedFolder) return;
    this.loadedFolder = this.settings.packagesFolder;
    // A fresh core: the registry only grows, so start over.
    this.core = null;
    this.cache.clear();
    this.ready = this.start();
    await this.ready;
    this.refresh();
  }

  labelLanguage() {
    return this.settings.language === "auto" ? obsidianLanguage() : this.settings.language;
  }

  /** Renders, remembering recent answers: notes re-render often. */
  render(request) {
    const key = JSON.stringify(request);
    if (this.cache.has(key)) {
      const hit = this.cache.get(key);
      this.cache.delete(key);
      this.cache.set(key, hit);
      return hit;
    }
    let result;
    try {
      result = this.core.call(request);
    } catch (error) {
      result = { ok: false, error: error.message };
    }
    this.cache.set(key, result);
    while (this.cache.size > 48) this.cache.delete(this.cache.keys().next().value);
    return result;
  }

  async update(change) {
    Object.assign(this.settings, change);
    await this.saveSettings();
    this.refresh();
  }

  refresh() {
    for (const map of this.maps) map.render();
  }

  async saveSettings() {
    await this.saveData(this.settings);
  }

  /** Where a new file belongs, by the vault's attachment setting. */
  async attachmentPath(name, sourcePath) {
    const manager = this.app.fileManager;
    if (manager && typeof manager.getAvailablePathForAttachment === "function") {
      return manager.getAvailablePathForAttachment(name, sourcePath);
    }
    const folder = sourcePath.includes("/") ? sourcePath.slice(0, sourcePath.lastIndexOf("/")) : "";
    return normalizePath(folder ? `${folder}/${name}` : name);
  }

  /** Puts the cursor on a line of a note open in an editor. */
  goTo(sourcePath, line, ch) {
    let target = null;
    this.app.workspace.iterateAllLeaves((leaf) => {
      if (!target && leaf.view instanceof MarkdownView && leaf.view.file && leaf.view.file.path === sourcePath) target = leaf;
    });
    if (!target) return;
    this.app.workspace.setActiveLeaf(target, { focus: true });
    const view = target.view;
    const go = () => {
      view.editor.setCursor({ line, ch });
      view.editor.scrollIntoView({ from: { line, ch: 0 }, to: { line, ch: 0 } }, true);
      view.editor.focus();
    };
    if (view.getMode && view.getMode() === "preview") {
      view.setState({ ...view.getState(), mode: "source" }, { history: false }).then(go);
    } else {
      go();
    }
  }
}

module.exports = DedaloPlugin;
// For the self-test page.
module.exports.parseOptions = parseOptions;
module.exports.loadCore = loadCore;
