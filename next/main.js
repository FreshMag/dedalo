// The bare page of milestone M0: a text area and a live preview.
//
// It holds no map logic. Every keystroke is answered by the same core the
// `dedalo` binary runs, compiled to WebAssembly. The worker, CodeMirror and
// the rest of the editor arrive with milestone M4.

import { loadDedalo } from "./dedalo.js";

const STARTER = `dungeon "crypt" {
    name = "The Crypt"

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

const source = document.getElementById("source");
const preview = document.getElementById("preview");
const problems = document.getElementById("problems");
const status = document.getElementById("status");
const preset = document.getElementById("preset");
const rotation = document.getElementById("rotation");

let call = null;
let lastSvg = "";
let timer = 0;

function showProblems(report) {
  problems.replaceChildren();
  for (const d of report.diagnostics) {
    const item = document.createElement("li");
    item.className = d.severity;
    const code = document.createElement("span");
    code.className = "code";
    code.textContent = d.code;
    const where = document.createElement("span");
    where.className = "where";
    where.textContent = d.span ? `  line ${d.span.start[0]}` : "";
    const message = document.createElement("div");
    message.textContent = d.message;
    item.append(code, where, message);
    if (d.suggestion) {
      const help = document.createElement("div");
      help.className = "help";
      help.textContent = d.suggestion;
      item.append(help);
    }
    if (d.span) {
      item.addEventListener("click", () => jumpTo(d.span.start[0]));
    }
    problems.append(item);
  }
}

function jumpTo(line) {
  const lines = source.value.split("\n");
  const start = lines.slice(0, line - 1).reduce((sum, text) => sum + text.length + 1, 0);
  source.focus();
  source.setSelectionRange(start, start + (lines[line - 1] ?? "").length);
}

function update() {
  if (!call) {
    return;
  }
  const response = call({
    cmd: "render",
    source: source.value,
    preset: preset.value,
    rotation: Number(rotation.value),
    // Render whatever resolved, so the preview stays live while typing.
    force: true,
  });
  const report = response.report ?? { diagnostics: [], summary: { errors: 0, warnings: 0 } };
  showProblems(report);
  if (typeof response.output === "string") {
    lastSvg = response.output;
    // The SVG comes from Dedalo's own serialiser, whose scene cannot
    // express scripts, event handlers or external references.
    preview.innerHTML = lastSvg;
  }
  const { errors, warnings } = report.summary;
  status.textContent = errors + warnings === 0 ? "ok" : `${errors} error(s), ${warnings} warning(s)`;
  try {
    localStorage.setItem("dedalo.draft", source.value);
  } catch {
    // Storage may be unavailable; the page works without it.
  }
}

function schedule() {
  clearTimeout(timer);
  timer = setTimeout(update, 150);
}

source.addEventListener("input", schedule);
preset.addEventListener("change", update);
rotation.addEventListener("change", update);

document.getElementById("format").addEventListener("click", () => {
  const response = call?.({ cmd: "format", source: source.value });
  if (response?.ok) {
    source.value = response.text;
    update();
  } else if (response?.report) {
    showProblems(response.report);
  }
});

document.getElementById("download").addEventListener("click", () => {
  if (!lastSvg) {
    return;
  }
  const link = document.createElement("a");
  link.href = URL.createObjectURL(new Blob([lastSvg], { type: "image/svg+xml" }));
  link.download = `dungeon.${preset.value}.svg`;
  link.click();
  URL.revokeObjectURL(link.href);
});

let draft = null;
try {
  draft = localStorage.getItem("dedalo.draft");
} catch {
  // Storage may be unavailable.
}
source.value = draft || STARTER;
status.textContent = "loading…";

loadDedalo(new URL("dedalo.wasm", import.meta.url)).then(
  (loaded) => {
    call = loaded;
    update();
  },
  (error) => {
    status.textContent = `could not load the core: ${error.message}`;
  },
);
