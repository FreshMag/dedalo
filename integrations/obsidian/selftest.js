// Runs the built plugin (dist/main.js) against a small stand-in for
// Obsidian's API and checks what a user would see: maps drawn from code
// blocks, fence options, problems with the note's line numbers, the level
// picker, the audience switch, saving, the commands and plugin packages.
"use strict";

const failures = [];
const expect = (ok, message) => {
  if (!ok) failures.push(message);
};

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function until(test, what, ms = 15000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (test()) return true;
    await sleep(25);
  }
  failures.push(`timed out waiting for ${what}`);
  return false;
}

// --- The DOM helpers Obsidian adds to every element -------------------------

function info(el, o) {
  if (!o) return el;
  if (typeof o === "string") o = { cls: o };
  if (o.cls) for (const c of [].concat(o.cls)) el.classList.add(...String(c).split(/\s+/).filter(Boolean));
  if (o.text !== undefined) el.textContent = o.text;
  if (o.attr) for (const [k, v] of Object.entries(o.attr)) el.setAttribute(k, v);
  if (o.value !== undefined) el.value = o.value;
  return el;
}
Object.assign(HTMLElement.prototype, {
  createEl(tag, o) {
    return this.appendChild(info(document.createElement(tag), o));
  },
  createDiv(o) {
    return this.createEl("div", o);
  },
  createSpan(o) {
    return this.createEl("span", o);
  },
  empty() {
    this.replaceChildren();
  },
  addClass(...c) {
    this.classList.add(...c);
  },
  removeClass(...c) {
    this.classList.remove(...c);
  },
  toggleClass(c, on) {
    this.classList.toggle(c, on);
  },
  setAttr(k, v) {
    this.setAttribute(k, v);
  },
});

// --- A stand-in for the `obsidian` module -----------------------------------

const notices = [];
const vault = new Map();
const saved = new Map();
let stored = {};

class Menu {
  constructor() {
    this.items = [];
    Menu.last = this;
  }
  addItem(build) {
    const item = { title: "", setTitle(t) { this.title = t; return this; }, setIcon() { return this; }, onClick(f) { this.click = f; return this; } };
    build(item);
    this.items.push(item);
    return this;
  }
  addSeparator() {
    return this;
  }
  showAtMouseEvent() {}
}

const children = (path) => [...vault.keys()].filter((k) => k.startsWith(path + "/"));
const obsidian = {
  Plugin: class {
    constructor(app, manifest) {
      this.app = app;
      this.manifest = manifest;
      this.processors = {};
      this.commands = {};
    }
    registerMarkdownCodeBlockProcessor(language, handler) {
      this.processors[language] = handler;
    }
    addSettingTab() {}
    addCommand(command) {
      this.commands[command.id] = command;
    }
    async loadData() {
      return stored;
    }
    async saveData(data) {
      stored = JSON.parse(JSON.stringify(data));
    }
  },
  PluginSettingTab: class {},
  Setting: class {},
  Modal: class {
    constructor(app) {
      this.app = app;
    }
  },
  Menu,
  Notice: class {
    constructor(message) {
      notices.push(message);
    }
  },
  MarkdownRenderChild: class {
    constructor(el) {
      this.containerEl = el;
    }
  },
  MarkdownView: class {},
  normalizePath: (p) => p.replace(/\\/g, "/").replace(/\/+/g, "/").replace(/^\/|\/$/g, ""),
  setIcon: (el, name) => {
    el.dataset.icon = name;
  },
  moment: { locale: () => "en" },
};

const app = {
  vault: {
    adapter: {
      exists: async (path) => vault.has(path) || children(path).length > 0,
      list: async (path) => {
        const files = new Set();
        const folders = new Set();
        for (const k of children(path)) {
          const rest = k.slice(path.length + 1);
          if (rest.includes("/")) folders.add(`${path}/${rest.split("/")[0]}`);
          else files.add(k);
        }
        return { files: [...files], folders: [...folders] };
      },
      readBinary: async (path) => vault.get(path).slice().buffer,
    },
    createBinary: async (path, data) => {
      saved.set(path, new Uint8Array(data));
    },
  },
  fileManager: { getAvailablePathForAttachment: async (name) => `Maps/${name}` },
  workspace: { iterateAllLeaves() {}, setActiveLeaf() {} },
};


const text = async (url) => (await (await fetch(url)).text()).replace(/\r\n/g, "\n");

// --- One code block in a note ----------------------------------------------

let notes = 0;
async function draw(plugin, source, fence = "") {
  notes += 1;
  const lines = ["# Note", "", "```dedalo" + (fence ? " " + fence : ""), ...source.replace(/\n$/, "").split("\n"), "```"];
  const el = document.getElementById("note").appendChild(document.createElement("div"));
  const ctx = {
    sourcePath: `Note ${notes}.md`,
    getSectionInfo: () => ({ text: lines.join("\n"), lineStart: 2, lineEnd: lines.length - 1 }),
    addChild(child) {
      this.child = child;
      child.onload();
    },
  };
  plugin.processors.dedalo(source, el, ctx);
  await until(() => el.querySelector("img, .dedalo-problems") || (el.textContent && !el.textContent.includes("Loading")), "a drawn block");
  return { el, map: ctx.child };
}

async function imageLoads(img) {
  try {
    await img.decode();
    return img.naturalWidth > 0;
  } catch {
    return false;
  }
}

function fakeEditor(lines, cursor) {
  return {
    lines,
    lineCount: () => lines.length,
    getLine: (i) => lines[i],
    getCursor: () => ({ line: cursor, ch: 0 }),
    replaceRange(text, from, to) {
      const before = lines[from.line].slice(0, from.ch);
      const after = lines[to.line].slice(to.ch);
      lines.splice(from.line, to.line - from.line + 1, ...(before + text + after).split("\n"));
    },
    replaceSelection(text) {
      this.inserted = text;
    },
  };
}

// --- The run ----------------------------------------------------------------

(async () => {
  try {
    // As Obsidian loads a plugin: the code wrapped in a function that
    // receives `require`, `module` and `exports`.
    const code = await (await fetch("dist/main.js")).text();
    const module = { exports: {} };
    const require = (name) => {
      if (name === "obsidian") return obsidian;
      throw new Error(`no module ${name}`);
    };
    new Function("require", "module", "exports", code)(require, module, module.exports);
    const DedaloPlugin = module.exports;

    const o = DedaloPlugin.parseOptions('player level="ground floor" rotation=90 grid=false mystery');
    expect(o.preset === "player" && o.level === "ground floor" && o.rotation === "90" && o.grid === "false" && o.mystery === "true", `fence options: ${JSON.stringify(o)}`);

    const pkg = await fetch("/web/test-package.dedalopkg").then((r) => (r.ok ? r.arrayBuffer() : null), () => null);
    if (pkg) vault.set("Dedalo/plugins/blueprint.dedalopkg", new Uint8Array(pkg));
    else failures.push("web/test-package.dedalopkg is missing: run `dedalo plugin pack plugins/blueprint -o web/test-package.dedalopkg`");
    stored = { packagesFolder: "Dedalo/plugins" };

    const plugin = new DedaloPlugin(app, { id: "dedalo", dir: ".obsidian/plugins/dedalo" });
    await plugin.onload();
    await plugin.ready;
    expect(plugin.processors.dedalo, "a processor for dedalo code blocks");

    // The same bytes as the binary: the golden SVG files.
    const crypt = await text("/tests/corpus/forgotten_crypt/input.dedalo");
    const dm = await draw(plugin, crypt);
    const img = dm.el.querySelector(".dedalo-frame img");
    expect(img && (await imageLoads(img)), "the DM map is an image that loads");
    expect(dm.map.svg === (await text("/tests/corpus/forgotten_crypt/dm.svg")), "the DM map is the golden dm.svg");
    expect(!dm.el.querySelector(".dedalo-badge"), "no audience badge on a DM map");
    expect(img && img.alt.includes("La Cripta Dimenticata"), `the image is named after the dungeon: ${img && img.alt}`);

    const player = await draw(plugin, crypt, "player");
    expect(player.map.svg === (await text("/tests/corpus/forgotten_crypt/player.svg")), "```dedalo player draws the golden player.svg");
    expect(player.el.querySelector(".dedalo-badge")?.textContent === "Players", "a player map says so");

    // The audience switch on the map.
    dm.el.querySelector(".dedalo-toolbar button").click();
    await until(() => dm.el.querySelector(".dedalo-badge")?.textContent === "Players", "the switch to the player view");
    expect(dm.map.svg === player.map.svg, "the switched map is the player map");

    // Problems, with the note's line numbers.
    const broken = await draw(plugin, await text("/tests/corpus/err_unknown_attribute/input.dedalo"));
    const problems = [...broken.el.querySelectorAll(".dedalo-problems li")];
    expect(problems.length === 2, `two problems listed, got ${problems.length}`);
    expect(problems[0]?.querySelector(".dedalo-line")?.textContent === "line 7", `the first problem is on line 7 of the note: ${problems[0]?.textContent}`);
    expect(problems[1]?.querySelector(".dedalo-help"), "a suggestion is shown under its problem");

    const odd = await draw(plugin, crypt, "colour=red");
    expect(odd.el.querySelector(".dedalo-problems li.is-warning")?.textContent.includes("colour"), "an unknown fence option is pointed out");

    // Levels: a picker, and one level at a time.
    const tower = await draw(plugin, await text("/tests/corpus/levels_tower/input.dedalo"));
    const select = tower.el.querySelector(".dedalo-toolbar select");
    expect(select && select.options.length > 2, "a level picker with every level");
    if (select) {
      const whole = tower.map.svg;
      select.value = select.options[2].value;
      select.dispatchEvent(new Event("change"));
      await until(() => tower.map.svg !== whole, "the chosen level");
      expect(tower.el.querySelector(".dedalo-toolbar select")?.value === select.options[2].value, "the picker keeps the chosen level");
    }

    // Saving a PNG next to the note.
    const buttons = player.el.querySelectorAll(".dedalo-toolbar button");
    buttons[buttons.length - 1].click();
    const save = Menu.last?.items.find((i) => i.title === "Save as PNG");
    expect(save, "the menu offers Save as PNG");
    if (save) {
      await save.click();
      const png = saved.get("Maps/forgotten_crypt-player.png");
      expect(png && png[0] === 0x89 && png[1] === 0x50 && png[2] === 0x4e && png[3] === 0x47, `a PNG saved beside the note: ${[...saved.keys()]}`);
    }

    // A theme from a plugin package in the vault.
    const header = 'dedalo {\n    plugin "blueprint" {\n        version = "^1"\n    }\n}\n\n';
    const themed = await draw(plugin, header + crypt.replace('name = "La Cripta Dimenticata"', 'name = "La Cripta Dimenticata"\n\n    render {\n        theme = blueprint.default\n    }'));
    expect(themed.map.svg && themed.map.svg.includes("#1d4e89"), `the blueprint theme from the vault's plugin folder: ${themed.el.textContent.slice(0, 300)}`);

    // Commands.
    const editor = fakeEditor(["Some text.", "```dedalo player", 'dungeon "d" {', 'room "a" { shape = rectangle(4,4) }', "}", "```", "More text."], 3);
    const format = plugin.commands["format-map"];
    expect(format.editorCheckCallback(true, editor), "Format is offered inside a block");
    format.editorCheckCallback(false, editor);
    expect(editor.lines.join("\n") === 'Some text.\n```dedalo player\ndungeon "d" {\n    room "a" { shape = rectangle(4, 4) }\n}\n```\nMore text.', `the block is formatted in place:\n${editor.lines.join("\n")}`);
    const outside = fakeEditor(["```js", "let a = 1;", "```", "```dedalo", 'dungeon "d" {}', "```"], 1);
    expect(!format.editorCheckCallback(true, outside), "Format is not offered in another language's block");
    expect(!format.editorCheckCallback(true, fakeEditor(["text", "```dedalo", "x", "```"], 0)), "Format is not offered outside a block");

    const insert = fakeEditor([""], 0);
    plugin.commands["insert-map"].editorCallback(insert);
    expect(insert.inserted && insert.inserted.startsWith("```dedalo\ndungeon"), "Insert writes a dedalo block");
    const starter = await draw(plugin, insert.inserted.replace(/^```dedalo\n/, "").replace(/```\n$/, ""));
    expect(starter.el.querySelector("img") && !starter.el.querySelector(".dedalo-problems"), "the inserted starter draws without problems");

    plugin.commands["switch-audience"].callback();
    await until(() => tower.el.querySelector(".dedalo-badge")?.textContent === "Players", "every map switched to the player view");
    expect(stored.audience === "player", "the switch is remembered");
  } catch (error) {
    failures.push(`threw: ${error && error.stack ? error.stack : error}`);
  }
  const result = document.getElementById("result");
  result.textContent = failures.length === 0 ? "OBSIDIAN-PASS" : ["OBSIDIAN-FAIL", ...failures].join("\n");
  fetch("/__golden_result", { method: "POST", body: result.textContent }).catch(() => {});
})();
