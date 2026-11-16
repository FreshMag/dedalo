// The reference pages (spec 14.1): the same Markdown sources as
// `dedalo guide`, published by `dedalo skill export` under `skill/`.

const nav = document.getElementById("nav");
const topic = document.getElementById("topic");

function escapeHtml(text) {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function inline(text) {
  return escapeHtml(text)
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>");
}

// Enough Markdown for the guide: headings, paragraphs, lists, tables, code.
function markdown(text) {
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  const out = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (line.startsWith("```")) {
      const code = [];
      i += 1;
      while (i < lines.length && !lines[i].startsWith("```")) code.push(lines[i++]);
      out.push(`<pre><code>${escapeHtml(code.join("\n"))}</code></pre>`);
      i += 1;
    } else if (/^#{1,4} /.test(line)) {
      const level = line.match(/^#+/)[0].length;
      out.push(`<h${level}>${inline(line.slice(level + 1))}</h${level}>`);
      i += 1;
    } else if (line.startsWith("|")) {
      const rows = [];
      while (i < lines.length && lines[i].startsWith("|")) rows.push(lines[i++]);
      const cells = (row) => row.slice(1, -1).split("|").map((c) => inline(c.trim()));
      const body = rows.slice(2).map((r) => `<tr>${cells(r).map((c) => `<td>${c}</td>`).join("")}</tr>`).join("");
      out.push(`<table><thead><tr>${cells(rows[0]).map((c) => `<th>${c}</th>`).join("")}</tr></thead><tbody>${body}</tbody></table>`);
    } else if (/^[-*] /.test(line) || /^\d+\. /.test(line)) {
      const ordered = /^\d+\. /.test(line);
      const items = [];
      while (i < lines.length && (/^[-*] /.test(lines[i]) || /^\d+\. /.test(lines[i]) || /^ {2,}\S/.test(lines[i]))) {
        if (/^ {2,}\S/.test(lines[i]) && items.length) items[items.length - 1] += ` ${lines[i].trim()}`;
        else items.push(lines[i].replace(/^([-*]|\d+\.) /, ""));
        i += 1;
      }
      const tag = ordered ? "ol" : "ul";
      out.push(`<${tag}>${items.map((x) => `<li>${inline(x)}</li>`).join("")}</${tag}>`);
    } else if (line.trim() === "" || line.startsWith("---")) {
      i += 1;
    } else {
      const para = [];
      while (i < lines.length && lines[i].trim() !== "" && !/^(#|\||```|[-*] |\d+\. )/.test(lines[i])) para.push(lines[i++]);
      out.push(`<p>${inline(para.join(" "))}</p>`);
    }
  }
  return out.join("\n");
}

const DOCS = ["tutorial.md", "cookbook.md", "plugins.md", "compatibility.md"];

async function fetchTopic(path) {
  // The user documentation lives beside the site, or in the repository's
  // docs/ folder when served for development.
  if (path.startsWith("docs/")) {
    const site = await fetch(path).catch(() => null);
    return site && site.ok ? site : fetch(`../${path}`);
  }
  return fetch(`skill/references/${path}`);
}

async function show(path) {
  const response = await fetchTopic(path);
  if (!response.ok) {
    topic.innerHTML = "<p>This topic is not available. Run <code>dedalo skill export web/skill</code> when developing locally.</p>";
    return;
  }
  const text = await response.text();
  topic.innerHTML = path.endsWith(".dedalo") ? `<h1>${escapeHtml(path)}</h1><pre><code>${escapeHtml(text)}</code></pre>` : markdown(text);
  topic.querySelectorAll("a").forEach((a) => a.setAttribute("rel", "noopener"));
}

async function start() {
  const response = await fetch("skill/references/index.json").catch(() => null);
  const files = response && response.ok ? await response.json() : ["quickstart.md", "syntax.md", "layout.md", "catalog.md", "errors.md"];
  const groups = new Map([["documentation", DOCS.map((d) => `docs/${d}`)]]);
  for (const file of files) {
    const group = file.includes("/") ? file.split("/")[0] : "reference";
    if (!groups.has(group)) groups.set(group, []);
    groups.get(group).push(file);
  }
  nav.innerHTML = [...groups]
    .map(([group, list]) => `<strong>${escapeHtml(group)}</strong><ul>${list.map((f) => `<li><a href="#${encodeURIComponent(f)}">${escapeHtml(f.replace(/^.*\//, "").replace(/\.md$/, ""))}</a></li>`).join("")}</ul>`)
    .join("");
  const route = () => show(decodeURIComponent(location.hash.slice(1)) || "docs/tutorial.md");
  window.addEventListener("hashchange", route);
  route();
}
start();
