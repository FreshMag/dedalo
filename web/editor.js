// A small code editor: a textarea over a highlighted copy of its text, with
// line numbers, diagnostic underlines, a completion list and hover help.
//
// It knows only how to colour tokens. Completion, hover and diagnostics come
// from the Dedalo core through the callbacks passed to `createEditor`.

const BLOCKS = new Set([
  "dedalo", "dungeon", "grid", "settings", "render", "legend", "properties", "room", "corridor", "connection",
  "feature", "entity", "note", "export", "layout", "placement", "visibility", "entry", "plugin",
]);

function escapeHtml(text) {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

// Splits text into [className, text] pieces. Unknown text has class "".
export function highlight(text) {
  const out = [];
  const push = (cls, t) => out.push([cls, t]);
  const re = /(#[^\n]*|\/\/[^\n]*|\/\*[\s\S]*?(?:\*\/|$))|(<<-?([A-Za-z_]+)\n[\s\S]*?(?:\n[ \t]*\3\b|$))|("(?:[^"\\\n]|\\.)*"?)|(-?\d+(?:\.\d+)?)|([A-Za-z_][\w-]*(?:\.[A-Za-z_][\w-]*)*)|([{}[\](),=])/g;
  let last = 0;
  let m;
  let lineStart = true;
  while ((m = re.exec(text))) {
    if (m.index > last) {
      const gap = text.slice(last, m.index);
      push("", gap);
      if (gap.includes("\n")) lineStart = true;
    }
    const [token, comment, heredoc, , string, number, ident, punct] = m;
    if (comment) push("c-comment", token);
    else if (heredoc || string) push("c-string", token);
    else if (number) push("c-number", token);
    else if (ident) {
      const rest = text.slice(re.lastIndex);
      if (/^\s*=/.test(rest)) push("c-attr", token);
      else if (lineStart && BLOCKS.has(token)) push("c-keyword", token);
      else if (token === "true" || token === "false" || token === "null") push("c-literal", token);
      else if (/^\s*\(/.test(rest)) push("c-call", token);
      else push("c-symbol", token);
    } else if (punct) push("c-punct", token);
    lineStart = false;
    last = re.lastIndex;
    if (token.includes("\n")) lineStart = true;
  }
  if (last < text.length) push("", text.slice(last));
  return out;
}

function lineStarts(text) {
  const starts = [0];
  for (let i = 0; i < text.length; i += 1) if (text[i] === "\n") starts.push(i + 1);
  return starts;
}

/**
 * @param {HTMLElement} host
 * @param {{ onChange: (text: string) => void,
 *           complete: (text: string, offset: number) => Promise<{from: number, items: any[]}>,
 *           hover: (text: string, offset: number) => Promise<{from: number, to: number, markdown: string} | null> }} hooks
 */
export function createEditor(host, hooks) {
  host.classList.add("editor-host");
  host.innerHTML = `
    <div class="gutter" aria-hidden="true"></div>
    <div class="code">
      <pre class="layer hl" aria-hidden="true"></pre>
      <pre class="layer marks" aria-hidden="true"></pre>
      <textarea spellcheck="false" autocapitalize="off" autocomplete="off" wrap="off" aria-label="Dungeon description"></textarea>
    </div>
    <ul class="completions" role="listbox" hidden></ul>
    <div class="hover" role="tooltip" hidden></div>`;
  const gutter = host.querySelector(".gutter");
  const hl = host.querySelector(".hl");
  const marks = host.querySelector(".marks");
  const area = host.querySelector("textarea");
  const list = host.querySelector(".completions");
  const tip = host.querySelector(".hover");
  let diagnostics = [];
  let completion = null;
  let tabLeaves = false;

  const metrics = () => {
    const style = getComputedStyle(area);
    const probe = document.createElement("span");
    probe.textContent = "0".repeat(100);
    probe.style.font = style.font;
    probe.style.position = "absolute";
    probe.style.visibility = "hidden";
    document.body.append(probe);
    const charWidth = probe.getBoundingClientRect().width / 100;
    probe.remove();
    return { charWidth, lineHeight: parseFloat(style.lineHeight), padLeft: parseFloat(style.paddingLeft), padTop: parseFloat(style.paddingTop) };
  };

  function paint() {
    const text = area.value;
    hl.innerHTML = highlight(text).map(([cls, t]) => (cls ? `<span class="${cls}">${escapeHtml(t)}</span>` : escapeHtml(t))).join("") + "\n";
    const starts = lineStarts(text);
    const offsetOf = ([line, col]) => Math.min((starts[line - 1] ?? text.length) + col - 1, text.length);
    // Underlines: the text again, transparent, with marked ranges.
    const ranges = diagnostics
      .filter((d) => d.span)
      .map((d) => {
        let from = offsetOf(d.span.start);
        let to = offsetOf(d.span.end);
        if (to <= from) to = Math.min(from + 1, text.length);
        if (to <= from && from > 0) from -= 1;
        return { from, to, cls: d.severity === "error" ? "m-error" : "m-warning" };
      })
      .sort((a, b) => a.from - b.from);
    let html = "";
    let cursor = 0;
    for (const r of ranges) {
      if (r.from < cursor) continue;
      html += escapeHtml(text.slice(cursor, r.from)) + `<mark class="${r.cls}">${escapeHtml(text.slice(r.from, r.to))}</mark>`;
      cursor = r.to;
    }
    marks.innerHTML = html + escapeHtml(text.slice(cursor)) + "\n";
    const errorLines = new Set(diagnostics.filter((d) => d.span).map((d) => d.span.start[0]));
    gutter.innerHTML = starts.map((_, i) => `<div class="${errorLines.has(i + 1) ? "g-mark" : ""}">${i + 1}</div>`).join("");
    sync();
  }

  function sync() {
    for (const layer of [hl, marks]) {
      layer.style.transform = `translate(${-area.scrollLeft}px, ${-area.scrollTop}px)`;
    }
    gutter.scrollTop = area.scrollTop;
  }

  function caretPoint(offset) {
    const { charWidth, lineHeight, padLeft, padTop } = metrics();
    const before = area.value.slice(0, offset);
    const line = before.split("\n").length - 1;
    const col = before.length - (before.lastIndexOf("\n") + 1);
    return {
      x: padLeft + col * charWidth - area.scrollLeft + area.offsetLeft,
      y: padTop + (line + 1) * lineHeight - area.scrollTop + area.offsetTop,
    };
  }

  function closeCompletion() {
    completion = null;
    list.hidden = true;
  }

  function renderCompletion() {
    if (!completion || completion.items.length === 0) {
      closeCompletion();
      return;
    }
    list.innerHTML = completion.items
      .map((item, i) => `<li role="option" data-i="${i}" aria-selected="${i === completion.selected}"><span class="k-${item.kind}">${escapeHtml(item.label)}</span><small>${escapeHtml(item.detail || "")}</small></li>`)
      .join("");
    const point = caretPoint(completion.from);
    list.style.left = `${point.x + gutter.offsetWidth}px`;
    list.style.top = `${point.y}px`;
    list.hidden = false;
    list.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
  }

  async function openCompletion(explicit) {
    const offset = area.selectionStart;
    const result = await hooks.complete(area.value, offset);
    if (area.selectionStart !== offset) return;
    if (!result || result.items.length === 0 || (!explicit && result.items.length === 1 && result.items[0].label === area.value.slice(result.from, offset).replace(/"/g, ""))) {
      closeCompletion();
      return;
    }
    completion = { from: result.from, items: result.items.slice(0, 60), selected: 0 };
    renderCompletion();
  }

  function accept(index) {
    const item = completion.items[index];
    const text = area.value;
    const lineStart = text.lastIndexOf("\n", completion.from - 1) + 1;
    const indent = text.slice(lineStart).match(/^[ \t]*/)[0];
    const insert = item.insert.replace(/\n/g, `\n${indent}`);
    area.setRangeText(insert, completion.from, area.selectionStart, "end");
    closeCompletion();
    changed();
  }

  let changeTimer = 0;
  function changed() {
    paint();
    clearTimeout(changeTimer);
    changeTimer = setTimeout(() => hooks.onChange(area.value), 150);
  }

  area.addEventListener("input", (event) => {
    changed();
    const data = event.data ?? "";
    const before = area.value.slice(0, area.selectionStart);
    const word = event.inputType === "insertText" && /[A-Za-z_"]/.test(data);
    const afterEquals = data === " " && /=\s$/.test(before);
    if (word || afterEquals) {
      openCompletion(false);
    } else if (completion) {
      openCompletion(false);
    }
  });
  area.addEventListener("scroll", () => {
    sync();
    closeCompletion();
  });
  area.addEventListener("blur", () => setTimeout(closeCompletion, 150));
  area.addEventListener("keydown", (event) => {
    if (completion) {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        const n = completion.items.length;
        completion.selected = (completion.selected + (event.key === "ArrowDown" ? 1 : n - 1)) % n;
        renderCompletion();
        event.preventDefault();
        return;
      }
      if (event.key === "Enter" || event.key === "Tab") {
        accept(completion.selected);
        event.preventDefault();
        return;
      }
      if (event.key === "Escape") {
        closeCompletion();
        event.preventDefault();
        return;
      }
    }
    if (event.key === " " && event.ctrlKey) {
      openCompletion(true);
      event.preventDefault();
      return;
    }
    if (event.key === "Escape") {
      // The next Tab moves focus out of the editor, for keyboard users.
      tabLeaves = true;
      return;
    }
    if (event.key === "Tab" && !tabLeaves && !event.ctrlKey && !event.altKey) {
      area.setRangeText("    ", area.selectionStart, area.selectionEnd, "end");
      changed();
      event.preventDefault();
      return;
    }
    tabLeaves = false;
    if (event.key === "Enter") {
      const before = area.value.slice(0, area.selectionStart);
      const line = before.slice(before.lastIndexOf("\n") + 1);
      let indent = line.match(/^[ \t]*/)[0];
      if (/\{\s*$/.test(line)) indent += "    ";
      area.setRangeText(`\n${indent}`, area.selectionStart, area.selectionEnd, "end");
      changed();
      event.preventDefault();
    }
  });
  list.addEventListener("mousedown", (event) => {
    const li = event.target.closest("li");
    if (li) {
      accept(Number(li.dataset.i));
      event.preventDefault();
    }
  });

  let hoverTimer = 0;
  area.addEventListener("mousemove", (event) => {
    clearTimeout(hoverTimer);
    tip.hidden = true;
    hoverTimer = setTimeout(async () => {
      const { charWidth, lineHeight, padLeft, padTop } = metrics();
      const rect = area.getBoundingClientRect();
      const line = Math.floor((event.clientY - rect.top - padTop + area.scrollTop) / lineHeight);
      const col = Math.floor((event.clientX - rect.left - padLeft + area.scrollLeft) / charWidth);
      const starts = lineStarts(area.value);
      if (line < 0 || line >= starts.length || col < 0) return;
      const end = line + 1 < starts.length ? starts[line + 1] - 1 : area.value.length;
      const offset = starts[line] + col;
      if (offset >= end) return;
      const result = await hooks.hover(area.value, offset);
      if (!result) return;
      tip.innerHTML = escapeHtml(result.markdown).replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>").replace(/`([^`]+)`/g, "<code>$1</code>").replace(/\n/g, "<br>");
      tip.style.left = `${event.clientX - host.getBoundingClientRect().left + 12}px`;
      tip.style.top = `${event.clientY - host.getBoundingClientRect().top + 16}px`;
      tip.hidden = false;
    }, 350);
  });
  area.addEventListener("mouseleave", () => {
    clearTimeout(hoverTimer);
    tip.hidden = true;
  });

  return {
    get value() {
      return area.value;
    },
    set value(text) {
      area.value = text;
      paint();
      hooks.onChange(text);
    },
    setDiagnostics(list) {
      diagnostics = list;
      paint();
    },
    // Selects a line, scrolls it into view and focuses the editor.
    revealLine(line, column = 1) {
      const starts = lineStarts(area.value);
      const start = (starts[line - 1] ?? 0) + column - 1;
      const end = line < starts.length ? starts[line] - 1 : area.value.length;
      area.focus();
      area.setSelectionRange(start, end);
      const { lineHeight } = metrics();
      area.scrollTop = Math.max(0, (line - 4) * lineHeight);
      sync();
    },
    focus() {
      area.focus();
    },
  };
}
