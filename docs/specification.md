# Dedalo — Implementation Specification & Milestones

Nov 16, 2026 · @Francesco

## 1. Summary and key decisions

Dedalo is built as one Rust core that compiles a dungeon description into a map, wrapped by three thin hosts: a command-line binary, a static web editor and an MCP server. This document turns product draft v1.0 into an implementable design, settles the points the draft left open, and adds the four requirements stated after it: web distribution on GitHub Pages, a standalone binary, agent tooling, and a plugin architecture that can replace the entire graphic output.

The draft remains the source of product intent. Where this document tightens or changes it, section 20 lists the difference.

### Decisions that shape everything else

1. **One core, three hosts.** All language, geometry and rendering logic lives in a Rust workspace compiled both to native code and to WebAssembly. The binary, the web editor and the MCP server contain no map logic of their own, so they cannot disagree.
2. **A compiler pipeline with inspectable stages.** Source text becomes a document model, then a resolved model with absolute geometry, then an audience-filtered view, then a scene, then bytes. Every stage is serialisable JSON with a versioned schema.
3. **The DSL is a strict subset of HCL native syntax.** Every `.dedalo` file is valid HCL, which gives editor highlighting and model familiarity for free. Dedalo uses its own error-tolerant, lossless parser so it can report all errors at once and edit files without disturbing comments.
4. **JSON is a first-class equal of the DSL.** The document model has a published JSON Schema. Conversion in both directions preserves all semantics and comments.
5. **Layout is deterministic and rule-based.** Rooms are polygons on a cell grid; walls are derived from space boundaries; openings are cut by connections. No constraint solver before 1.0.
6. **Redaction happens before rendering.** A player map is built from a filtered model, so secret doors, traps and DM notes never reach the scene, the output file or any plugin.
7. **The scene is the only rendering contract.** Painters emit a restricted vector scene. SVG is serialised from it; PNG and PDF are derived from that SVG. Replacing painters replaces the look.
8. **Two plugin tiers.** Declarative packs cover themes, symbols, catalogs and templates with no code. Sandboxed WebAssembly plugins cover painters, scene passes, full renderers, exporters, validators and shapes. One plugin file runs in both the binary and the browser.
9. **Built-ins use the plugin seams.** The default catalog and the default style are registered through the same extension points third parties get, which keeps those points honest.
10. **Agents get four surfaces.** JSON output on every command, ID-addressed edit operations, an MCP server inside the binary, and a packaged skill with the language reference.

## 2. Scope and releases

Dedalo ships as four deliverables built from one shared core. The draft's MVP (its section 13) becomes release 0.1; the plugin platform follows in 0.2; 1.0 freezes the language and the plugin API.

### 2.1 Deliverables

| # | Deliverable | What the user gets |
| --- | --- | --- |
| D1 | `dedalo` binary | One self-contained executable for Linux, macOS and Windows that validates, formats, converts, edits and renders `.dedalo` files to SVG, PNG, PDF and text. |
| D2 | Web editor | A static site on GitHub Pages: DSL editor with diagnostics and completion, live map preview, audience toggle, export, share-by-link. No backend. |
| D3 | Agent interface | Structured JSON on every CLI command, an edit-operation engine addressed by element ID, an MCP server built into the binary (`dedalo mcp`), a published JSON Schema, and a packaged agent skill. |
| D4 | Plugin platform | Declarative packs (themes, catalogs, templates) and sandboxed WebAssembly plugins (painters, scene passes, renderers, exporters, validators, shapes) that run unchanged in the binary and in the browser. |

A documentation site (language reference, catalog reference, plugin guide) is published with D2 from the same repository.

### 2.2 Release definitions

| Release | Milestones | Definition of done |
| --- | --- | --- |
| 0.1 MVP | M0 to M5 | All 20 acceptance criteria of the draft pass through the binary and the web editor. An agent can author, validate, edit and render a dungeon through CLI or MCP. Built-in catalog and one built-in style only. |
| 0.2 Extensible | M6 | Third-party packs and WASM plugins load in both hosts. Three reference plugins ship. Plugin API is public but marked unstable. |
| 1.0 | M7 | DSL version 1 and plugin API version 1 are frozen under semantic versioning. Signed binaries, install channels, complete documentation, performance and fuzzing gates green. |

### 2.3 Non-goals through 1.0

- A drag-and-drop visual editor (draft 11.9). The web editor is text-first; the preview is read-only apart from click-to-source.
- Multi-level dungeons, dynamic element states, shadow-casting light, hex and isometric grids (draft 11.2 to 11.4, 11.7). The model reserves fields for them; see section 18.
- A constraint solver for layout. Placement is deterministic and rule-based (section 7).
- Built-in procedural generation. Generators are a plugin extension point after 1.0.
- Any server-side component: accounts, cloud storage, real-time collaboration.
- A creature or rules database. Entities carry an external `reference` string only.
- Virtual-tabletop runtime features such as live fog of war. Export to VTT formats is a plugin.

## 3. Architecture

Dedalo is a compiler: six pure stages turn source text into output bytes, and three hosts call those stages. Nothing outside the core knows how a dungeon is laid out or drawn.

&#91;embedded content: architecture · 3 hosts, 6 stages, 1 registry\]

Every host calls the same six stages in order. Built-ins, packs and plugins all enter through one registry, and everything to the right of View works only on the audience-filtered model.

### 3.1 Hosts

| Host | Form | What it adds to the core |
| --- | --- | --- |
| CLI | Native binary `dedalo` | File I/O, argument parsing, plugin discovery on disk, human-readable diagnostics |
| MCP server | `dedalo mcp`, a subcommand of the same binary | Protocol framing, workspace confinement, image packaging for model input |
| Web editor | Static site; the core runs as WebAssembly in a Web Worker | Editor UI, browser storage, share links, plugin fetch |

### 3.2 Pipeline

| # | Stage | Input to output | Crate | Typical diagnostics |
| --- | --- | --- | --- | --- |
| 1 | Parse | Text to lossless syntax tree | `dedalo-syntax` | Syntax errors, with recovery |
| 2 | Lower | Syntax tree to document model | `dedalo-model`, `dedalo-catalog` | Types, duplicate IDs, unknown references, unknown properties |
| 3 | Resolve | Document to resolved model: absolute polygons, walls, openings, footprints | `dedalo-geom` | Overlaps, unplaceable doors, disconnected corridors |
| 4 | View | Resolved model plus export settings to audience-filtered view | `dedalo-render` | None; this stage only removes and rotates |
| 5 | Paint | View plus theme to scene | `dedalo-render`, `dedalo-scene` | Plugin failures |
| 6 | Emit | Scene to SVG, then PNG or PDF; view to text map | `dedalo-export` | Output too large |

### 3.3 Rules every stage obeys

- A stage is a pure function of its input and configuration. It performs no I/O, reads no clock and uses only the seeded random stream.
- A stage's output is JSON-serialisable under a versioned schema, so `dedalo inspect --stage resolved` shows exactly what the next stage receives.
- A stage reports every independent problem it can find, then passes on the elements that did resolve. One bad door does not hide the other errors, and the web preview can show a partial map.
- Diagnostics accumulate across stages in one list with stable codes (section 10).
- Stages 5 and 6 never see data that stage 4 removed. This is the secrecy boundary.

### 3.4 Repository layout

One repository holds the core, the hosts, the specification and the tests.

```
dedalo/
  crates/
    dedalo-syntax/    lexer, parser, syntax tree, formatter, source edits
    dedalo-model/     document model, JSON Schema, DSL <-> JSON
    dedalo-catalog/   element type registry, built-in catalog
    dedalo-geom/      shapes, layout, walls, openings, geometric validation
    dedalo-scene/     scene IR, SVG serialiser
    dedalo-render/    view builder, painters, built-in theme, legend, notes
    dedalo-export/    PNG, PDF, text map
    dedalo-ops/       ID-addressed edit operations
    dedalo-plugin/    manifest, registry, pack loader, WASM host
    dedalo-core/      facade used by every host
    dedalo-cli/       the binary: commands and MCP server
    dedalo-wasm/      browser bindings
    dedalo-pdk/       plugin development kit
  web/                web editor (TypeScript)
  plugins/            first-party reference plugins
  spec/               grammar, JSON Schemas, diagnostics catalogue
  tests/corpus/       golden inputs and expected outputs
  docs/               documentation sources
  skill/              agent skill folder
```

### 3.5 Core facade

Hosts call four functions. Everything else is internal.

```rust
pub fn parse(source: &str) -> Parsed;                          // syntax tree + syntax diagnostics
pub fn compile(input: Input, reg: &Registry) -> Compiled;      // document + resolved model + diagnostics
pub fn render(c: &Compiled, export: &ExportSpec, reg: &Registry) -> Rendered;  // scene + bytes
pub fn edit(source: &str, ops: &[Op], reg: &Registry) -> EditResult;           // new source + diff + diagnostics
```

`Input` is DSL text or document JSON. `Registry` holds the catalog, themes, painters and plugins in effect; built-ins and third-party extensions are registered through the same calls.

## 4. Technology stack

The core is written in Rust because it is the one mainstream option that yields a small self-contained binary and a browser build from the same source, with sandboxed plugins running identically in both. Library choices below were checked against their current state on the as-of date; links are in Sources.

| Concern | Choice | Reason | Alternative and why it lost |
| --- | --- | --- | --- |
| Core language | Rust, stable toolchain | One codebase compiles to native targets and to `wasm32-unknown-unknown`; no runtime to ship; strict types for the model | TypeScript compiled to a binary with Bun or Deno: simpler web and plugin authoring, but the binary embeds a JavaScript runtime and plugins would run unsandboxed |
| Parser | Hand-written lexer and recursive-descent parser producing a lossless tree (`rowan`) | Multi-error recovery, comment-preserving edits, schema-aware messages | [hcl-edit](https://github.com/martinohmann/hcl-rs): format-preserving, but implements the full HCL expression and template languages that Dedalo excludes, and has no schema support yet. Kept as a fallback |
| Model and schema | `serde` with `schemars` | The Rust types are the single definition; JSON Schema is generated, never hand-written | Hand-maintained schema: drifts |
| Geometry | Fixed-point coordinates (1/1000 cell); polygon boolean operations from the `geo` / `i_overlay` family behind an internal trait | Integer arithmetic makes results identical on every platform | Floating point throughout: last-bit differences between native and WASM maths |
| SVG output | Own serialiser from the scene IR | Byte-stable output: fixed attribute order and number formatting | Generic SVG crates: no stability guarantee |
| PNG output | [resvg](https://github.com/linebender/resvg) | Pure Rust, compiles to WASM, designed for identical pixels across platforms | Browser canvas on the web: output would differ from the binary |
| PDF output | [krilla](https://github.com/LaurenzV/krilla) with `krilla-svg` | Vector PDF from the same SVG, with font subsetting | Rasterised PDF: poor print quality |
| Text | Bundled open-licence fonts loaded from memory; text converted to outlines by default | resvg has no system text rendering, and system fonts would break determinism | System fonts |
| Plugin runtime, native | [wasmi](https://docs.rs/crate/wasmi/1.0.9/source/README.md) interpreter behind a `PluginRuntime` trait | Deterministic execution, built-in fuel metering, small, no compile step at start-up, API mirrors Wasmtime | Wasmtime: faster for heavy plugins, larger binary and per-run compile cost. The trait keeps it a drop-in if profiling demands it |
| Plugin runtime, browser | The browser's own WebAssembly engine | Already present; zero added size | None |
| Plugin ABI | Dedalo ABI v1: JSON messages over four core-WASM exports (section 11) | One `.wasm` file loads in both hosts; calls are synchronous | Component Model: [no browser engine ships it natively](https://bytecodealliance.org/articles/the-road-to-component-model-1-0), so the web would need a transpile step. [Extism](https://github.com/extism/js-sdk): calls are asynchronous in JavaScript and background threads need cross-origin isolation |
| CLI | `clap` | Standard, generates help and completions | None |
| MCP | [rmcp](https://docs.rs/crate/rmcp/3.1.0/source/README.md) 3.x, the official Rust SDK, with stdio and Streamable HTTP | Tracks the 2026-07-28 revision | Hand-rolled JSON-RPC: kept as fallback (risk 6, section 19) |
| Web editor | TypeScript, Vite, CodeMirror 6, Svelte; `wasm-bindgen` for bindings | Small static bundle, no server | Monaco: heavier, harder to run offline |
| Release tooling | GitHub Actions; [dist](https://docs.rs/crate/cargo-dist/latest) for binaries and installers | Generates the release workflow and publishes to GitHub Releases | Hand-written release workflow |

Policies that apply across the stack:

- All dependency versions are pinned by the committed lockfile. Upgrading a rendering dependency is a reviewed change because it can alter output bytes.
- Core crates forbid `unsafe` code. The only exceptions are the WASM boundary in `dedalo-wasm` and `dedalo-plugin`.
- Transcendental maths (sine, cosine) uses a pure-Rust implementation on every target, never the platform library.
- Ordered maps only. No output may depend on hash iteration order.

## 5. DSL specification

A Dedalo file is a strict subset of HCL native syntax: blocks, attributes, literals, lists, objects and function calls, with no expressions and no templates. Every example in the draft already fits this subset, including `corridor "c1" using "tpl" { }`, which HCL reads as a block with three labels.

### 5.1 Files

A file is UTF-8 text with the extension `.dedalo`. It holds an optional `dedalo` header block followed by exactly one `dungeon` block. From 0.2, `template` blocks may also appear at the top level.

```hcl
dedalo {
    version = 1                # language version; defaults to the newest the tool knows

    plugin "hand_drawn" {      # plugin requirement, from 0.2
        version = "^1.2"
    }
}

dungeon "forgotten_crypt" {
    name = "La Cripta Dimenticata"

    room "entrance" {
        shape = rectangle(8, 6)
    }
}
```

### 5.2 Lexical rules

- Comments: `#` or `//` to end of line, and `/* ... */`.
- Identifiers: a letter or underscore, then letters, digits, underscores or hyphens.
- Strings: double-quoted with the escapes `\n`, `\t`, `\"`, `\\` and `\uXXXX`. Heredocs (`<<EOT` and the indented form `<<-EOT`) carry multi-line text for descriptions and notes.
- Interpolation is not supported. The sequences `${` and `%{` inside a string are rejected in version 1 so they stay available later.
- Numbers are decimal, with an optional leading minus. Booleans are `true` and `false`. `null` unsets an optional attribute.

### 5.3 Grammar

```ebnf
File      = { Block } ;
Block     = Ident { Label } "{" Body "}" ;
Label     = String | Ident ;
Body      = { Attribute | Block } ;
Attribute = Ident "=" Expr ;
Expr      = Number | String | Heredoc | "true" | "false" | "null"
          | Symbol | List | Object | Call ;
Symbol    = Ident { "." Ident } ;
List      = "[" [ Expr { "," Expr } [ "," ] ] "]" ;
Object    = "{" [ Entry { ( "," | Newline ) Entry } ] "}" ;
Entry     = ( Ident | String ) "=" Expr ;
Call      = Ident "(" [ Expr { "," Expr } [ "," ] ] ")" ;
```

Attributes and blocks end at a newline. A one-line block may hold a single attribute. Operators, conditionals, `for` expressions and splats are outside version 1; the parser recognises them and reports `SYNTAX_ERROR` with a note that the construct is unsupported.

### 5.4 Value types

| Type | Written as | Example |
| --- | --- | --- |
| Number, integer | Literal | `width = 2` |
| String | Quoted or heredoc | `name = "Ingresso"` |
| Boolean | `true`, `false` | `locked = true` |
| Symbol (enumeration value) | Bare identifier; dotted when it comes from a plugin | `type = door`, `theme = hand_drawn.sepia` |
| Reference | Quoted element ID | `from = "entrance"` |
| Point | List of two numbers | `position = [3, 2]` |
| List | Brackets | `tags = ["combat", "guarded"]` |
| Shape | Function call | `shape = rectangle(8, 6)` |

Typing is directed by the schema of each block. The reader is lenient and the formatter strict: a quoted string is accepted where a symbol is expected, and a bare identifier where a reference is expected, because the schema makes the intent unambiguous. `dedalo fmt` rewrites both to the canonical spelling.

### 5.5 Blocks

| Block | Label | Allowed in | Purpose |
| --- | --- | --- | --- |
| `dedalo` | None | File | Language version and plugin requirements |
| `dungeon` | ID | File | Root of the description |
| `grid` | None | `dungeon` | Display scale (`unit = "5ft"`) and grid type (`square` only before 1.0) |
| `settings` | None | `dungeon` | Defaults: `default_wall`, `default_floor`, `seed`, `max_stub` |
| `room` | ID | `dungeon` | A space with a shape |
| `corridor` | ID | `dungeon` | A space defined by what it connects or by a path |
| `connection` | ID | `dungeon` | A passage between two spaces |
| `feature` | ID | `dungeon`, `room`, `corridor` | Fixed element: architecture, furniture, mechanism, trap, light, terrain |
| `entity` | ID | `dungeon`, `room`, `corridor` | Creature, NPC or item |
| `note` | ID | `dungeon` and any element | Annotation, optionally positioned |
| `layout` | None | `room` | Absolute, relative or automatic placement |
| `placement` | None | `connection`, `feature`, `entity` | Wall-anchored or anchor-based placement |
| `properties` | None | Any element | Catalog-typed and custom data |
| `render` | None | `dungeon`, `export` | Theme, labels, grid and title options |
| `legend` | None | `dungeon`, `export` | Legend mode, entries and position |
| `export` | Optional name | `dungeon` | One output configuration; several may coexist |
| `template` | ID | File, `dungeon` | Reusable defaults, from 0.2 |

### 5.6 Identifiers and references

- An ID matches `[a-z][a-z0-9_]{0,63}`.
- IDs share one namespace per dungeon. A room and a feature cannot have the same ID, so any tool can address any element by ID alone.
- References may point forward. They are resolved after the whole file is read.
- An unknown reference raises `UNKNOWN_REFERENCE` and proposes the closest existing ID.

### 5.7 Nesting

A `feature`, `entity` or `note` written inside a room or corridor is equivalent to the same block at dungeon level with `room = "id"` or `corridor = "id"`. Both forms are valid in one file. Giving both a parent block and a conflicting attribute is an error.

A note attaches to anything through nesting or through `target = "id"`.

### 5.8 Structural attributes and properties

- Attributes that affect geometry, references or visibility are fixed per block type: `shape`, `width`, `from`, `to`, `type`, `subtype`, `position`, `rotation`, `visibility`.
- Game and descriptive data go in `properties { }`, typed by the catalog entry of the element's `type`.
- An unknown property raises the warning `UNKNOWN_PROPERTY` with the closest known name. Names starting with `x_` are always accepted as custom data and carried through to JSON and to plugins.

### 5.9 Defaults

A value is taken from the first source that provides it: the element itself, its template (0.2), the `settings` block, the catalog entry, then the language default.

### 5.10 Aliases

| Written | Stored in the model as |
| --- | --- |
| `type = locked_door` | `type = door` with `properties.locked = true` |
| `type = secret_door` | `type = door` with `secret = true`; `visibility` defaults to `dm` |
| `north_of = "x"` in `layout` (also south, east, west) | `adjacent_to = "x"` with `direction = north` |
| `room = "x"` on a note | `target = "x"` |

The formatter keeps the author's spelling. The JSON model always stores the normalised form.

### 5.11 Formatting

`dedalo fmt` normalises whitespace and the lenient spellings of 5.4, nothing else: four-space indentation, one blank line between sibling blocks, no alignment of `=`. It never reorders blocks or attributes, and running it twice changes nothing.

### 5.12 Language versioning

The language version is an integer. Additions keep the number. Any change that could alter the meaning of an existing file increments it, and the tool keeps reading every version since 1.

## 6. Document model and JSON form

The document model is the canonical form of a dungeon. DSL text and JSON are two spellings of it, and converting in either direction loses nothing except whitespace.

### 6.1 Shape

```json
{
  "$schema": "https://<pages-host>/schema/document-1.json",
  "dedalo": { "version": 1 },
  "dungeon": {
    "id": "forgotten_crypt",
    "name": "La Cripta Dimenticata",
    "grid": { "unit": "5ft" },
    "settings": { "default_wall": "stone", "default_floor": "stone" },
    "spaces": [
      {
        "kind": "room", "id": "entrance", "name": "Ingresso",
        "shape": { "fn": "rectangle", "args": [8, 6] },
        "layout": { "position": [0, 0] },
        "elements": [
          {
            "class": "feature", "id": "pressure_trap",
            "type": "trap", "subtype": "pressure_plate",
            "position": [4, 3], "visibility": "dm",
            "properties": { "detection_dc": 15, "disarm_dc": 14, "damage": "2d10" }
          }
        ]
      }
    ],
    "connections": [
      {
        "id": "entrance_door", "from": "entrance", "to": "guard_room", "type": "door",
        "placement": { "wall": "north", "offset": 3 }
      }
    ],
    "notes": [
      { "id": "entrance_note", "target": "entrance", "visibility": "dm",
        "text": "Esaminare attentamente il pavimento." }
    ],
    "exports": [ { "name": "player", "preset": "player", "format": "png" } ]
  }
}
```

### 6.2 Rules

- Elements live inside their space. A top-level `feature` with `room = "entrance"` lands in the same `elements` array as one nested in the room block, in file order. Elements with no space sit in a dungeon-level `elements` array and are assigned to a space by position at resolve time.
- Array order is declaration order and is meaningful: it breaks ties in layout and sets drawing order within a layer.
- Aliases are normalised (5.10). Defaults are not written out; an absent field means the default. `dedalo convert --explicit-defaults` materialises them on request.
- Symbols and references are JSON strings. The schema says which is which.
- Comments survive. Each node may carry `comments.leading` (lines above it) and `comments.trailing` (same line). A comment standing alone attaches to the next sibling, or to the parent when it is last.

### 6.3 Round-trip laws

These are tested as properties over generated documents, not only on examples.

1. `json(dsl(J)) = J` exactly, for every valid document `J`.
2. `json(dsl(json(S))) = json(S)` for every valid source `S`: regenerating the text never changes the meaning.
3. `fmt(fmt(S)) = fmt(S)` and `json(fmt(S)) = json(S)`.

Text regenerated from JSON nests each element in its space. Edit operations (13.2) do not go through JSON; they patch the syntax tree, so hand-written layout and comments stay where they were.

### 6.4 Published schemas

| Schema | Describes | Stability |
| --- | --- | --- |
| `document` | The model above | Frozen at 1.0 under the language version |
| `ops` | Edit operations | Frozen at 1.0 |
| `diagnostics` | Validation and error output | Frozen at 1.0 |
| `resolved`, `view`, `scene` | Stage outputs consumed by plugins | Public from 0.2, frozen at 1.0 under the plugin API version |
| `plugin-manifest` | Plugin packaging | Frozen at 1.0 |

Every schema is generated from the Rust types, printed by `dedalo schema <name>`, and served at a versioned URL on the Pages site. Two profiles of `document` are generated: `full`, and `authoring`, a closed and non-recursive variant sized for model structured output and function calling.

### 6.5 What the resolved model adds

| Object | Fields added at resolve time |
| --- | --- |
| Space | Absolute `polygon`, `outline` as line and arc segments, `bbox`, `level` (always `default` before multi-level support) |
| Wall | `segment`, the one or two spaces it bounds, `material`, `openings` |
| Opening | Source connection ID, `segment`, `width`, `type`, `secret`, optional stub polygon |
| Element | Absolute `position`, `footprint` polygon, `rotation`, owning space |
| Note | Absolute anchor point and running number |
| Dungeon | `bounds`, reachability graph, effective `seed` |

## 7. Geometry and layout

Every space is a closed polygon on a grid of cells, every wall is a boundary edge of a space, and every door is an opening cut into a wall by a connection. These three rules replace a general constraint solver through 1.0.

&#91;embedded content: coordinate conventions · an 8 by 6 room with one door\]

Shape vertices and corridor paths sit on grid intersections, while element positions name cells. A door at `offset = 3` on the north wall starts three cells from that wall's west end.

### 7.1 Coordinates

- The unit is one cell. `grid.unit` only labels the scale on the map ("1 square = 5 ft").
- X grows east and Y grows south. North is the top of the unrotated page. The global origin is arbitrary and negative coordinates are valid; the canvas is the bounding box of what is drawn plus a margin.
- A shape's local origin is the top-left corner of its bounding box.
- Shapes and corridor paths use lattice points: `[0, 0]` is a grid intersection.
- Elements use cells: `position = [3, 2]` puts the top-left cell of the element's footprint in column 3, row 2 of its space, counting from 0. Fractional values are allowed for off-grid placement.
- Internally all coordinates are integers in thousandths of a cell. The DSL accepts at most three decimals.

### 7.2 Shapes

| Shape | Meaning | Available |
| --- | --- | --- |
| `square(side)` | Axis-aligned square | 0.1 |
| `rectangle(width, height)` | Axis-aligned rectangle | 0.1 |
| `circle(radius)` | Circle inscribed in a `2r` by `2r` bounding box | 0.1 |
| `polygon(points)` | Simple polygon, convex or concave, three or more vertices, any winding | 0.1 |
| `ellipse(width, height)`, `l_shape(width, height, cutout)` | As in the draft | 1.0 |
| `composite(...)` | Union of shapes | After 1.0 |

A shape is invalid (`INVALID_SHAPE`) when a dimension is below one cell, a polygon self-intersects, or its area is zero. Curved shapes keep an exact outline for drawing and a polygonal approximation, within 0.01 cell, for containment and overlap tests.

### 7.3 Room placement

Placement runs in declaration order over a dependency graph and is fully deterministic.

1. A room with `layout.position` is fixed at that lattice point.
2. A room with `adjacent_to` and `direction` is placed after its target so that the two bounding boxes touch on the facing sides, separated by `gap` cells (default 0). `direction = north` means this room lies north of the target.
3. On the cross axis, `align` is `start`, `center` or `end` (default `center`, snapped to a whole cell toward the start). Start is the west edge for north and south placements and the north edge for east and west placements. `offset` then shifts the room by whole cells.
4. If a corridor connects the room to its target and `gap` is not set, the gap is the corridor's `length`, or 2 cells.
5. If no room is fixed, the first declared room is placed at `[0, 0]`.
6. A room with `mode = automatic` or no `layout` block must share a connection or corridor with an already placed space. It is placed on the side named by that connection's `placement.wall`, otherwise on the first of east, south, west, north that causes no overlap. A room with no such link raises `LAYOUT_UNDERCONSTRAINED`.
7. A dependency cycle raises `LAYOUT_CYCLE`.

After placement, the interiors of two spaces may not intersect; touching edges are fine. A violation raises `OVERLAP`, names both IDs and the overlapping area, and proposes the smallest `offset` that clears it. Automatic overlap resolution is deliberately left to the post-1.0 layout engine.

The draft's own reference example triggers this rule: with `treasure_room` (8 by 8) east of `guard_room` (6 by 6), centred, its lower row overlaps the north-east corner of `entrance`. Adding `align = end` to that layout block fixes it. The corrected file is the first entry of the golden corpus.

### 7.4 Walls

- Every boundary edge of a room or corridor is a wall unless an opening is cut into it.
- Where two spaces share a boundary segment there is one wall, drawn once.
- Walls have no thickness in the model and consume no cells. Thickness is a theme property, centred on the boundary.
- Wall material comes from the space (`properties.wall`) or `settings.default_wall`.

### 7.5 Connections and openings

A connection resolves to an opening: a segment of `width` cells (default 1) on the boundary between `from` and `to`.

| Attribute in `placement` | Meaning |
| --- | --- |
| `wall` | Side of the `from` space: `north`, `south`, `east`, `west`. On a polygon it selects the edge whose outward normal is closest to that direction; on a circle, the boundary point at that bearing |
| `edge` | Polygon edge by index, counted from the first vertex, as an alternative to `wall` |
| `bearing` | Circle boundary point in degrees clockwise from north, as an alternative to `wall` |
| `offset` | Distance in cells from the start of the wall to the start of the opening. Walls start at their west end (north and south walls) or north end (east and west walls). Omitted: centred on the shared segment, snapped toward the start |
| `width` | Opening width in cells |

Resolution has three outcomes.

1. **Shared wall.** The opening must lie entirely on the segment the two spaces share. Otherwise `DOOR_NOT_PLACEABLE` reports the valid offset range.
2. **Short gap.** If the facing walls are at most `settings.max_stub` cells apart (default 2) and overlap in projection, the engine inserts a straight stub passage of the opening's width. This also covers the sliver between a curved wall and a straight one.
3. **Neither.** `UNRESOLVABLE_CONNECTION` proposes a corridor or an adjacency.

Secret doors resolve like any door. When hidden from an audience the wall is drawn unbroken.

Vertical types (`stairs`, `ladder`, `trapdoor`) take a `position` inside `from` instead of a wall placement, and `to` is optional until multi-level support arrives: they draw as marked exits.

### 7.6 Corridors

- A corridor connects exactly two spaces (`connects`). Branches are separate corridors, or a corridor connected to another corridor.
- `width` defaults to 1 cell.
- **Automatic routing** is straight only: it succeeds when the facing walls of the two spaces overlap in projection by at least the corridor's width. The corridor is centred on the overlap, or placed at `offset`.
- **Explicit routing** uses `path`, a list of global lattice points along the centre line. Segments must be orthogonal before 1.0. Both ends must touch the spaces named in `connects`, or `CORRIDOR_DISCONNECTED` is raised.
- The corridor's polygon is the centre line widened by half the width on each side, with square corners.
- Each end opens into its space as an `open` connection unless an explicit `connection` between the corridor and that space says otherwise, for example to put a door there.
- A corridor between spaces that already share a wall has zero length and is rejected with a proposal to use a connection.

### 7.7 Element placement

An element is placed in one of three ways.

```hcl
position = [3, 2]                                # cell in the owning space

placement { wall = north, offset = 2 }           # against a wall: torches, levers, alcoves

placement { anchor = center }                    # centre of the space
```

- The footprint is `size = [w, h]` cells, defaulting to the catalog entry (usually one cell). Terrain may use `shape` for irregular areas.
- `rotation` turns the footprint in 90-degree steps. `facing` orients a creature's symbol to one of eight compass directions.
- Every footprint corner must lie inside or on the boundary of the owning space, or `OUT_OF_BOUNDS` proposes the nearest valid cell.
- Two blocking features sharing a cell raise the warning `ELEMENT_OVERLAP`. Entities may stand on non-blocking features such as traps and rugs.

### 7.8 Rotation

- A room's `rotation` is 0, 90, 180 or 270 degrees clockwise about the centre of its bounding box. Its contents, wall names and openings rotate with it. Arbitrary angles come after 1.0.
- The export option `rotation` turns the whole view by the same four angles without touching the model. Symbols rotate with the map; labels, notes and the legend stay upright; a north arrow is drawn whenever the rotation is not zero.

### 7.9 Reachability

Spaces and connections form a graph. A room that cannot be reached from the entrance (the first room tagged `entrance`, else the first declared) raises the warning `UNREACHABLE_ROOM`. The check runs on the full model; the player view gets its own graph so hidden rooms can be dropped cleanly (9.1).

### 7.10 Determinism

- Integer geometry, ordered collections, ties broken by declaration order.
- Any randomness, for example in a hand-drawn style, draws from a stream seeded by `settings.seed`. The default seed is a stable hash of the dungeon ID.

## 8. Element catalog

The catalog is a registry of element types. Each entry fixes, for one `type`, its class, footprint, property schema, default visibility and legend group. Release 0.1 ships the minimum catalog of draft section 10.3; the rest of draft section 4 arrives as packs without touching the core.

### 8.1 Entry format

Built-in entries and pack entries use the same TOML format.

```toml
[type.trap]
class = "feature"              # feature or entity: decides which block keyword is valid
category = "traps"             # legend group and export visibility toggle
subtypes = ["pressure_plate", "dart_launcher", "pit", "generic"]
footprint = [1, 1]
blocking = false
default_visibility = "dm"
layer = "markers"

[type.trap.properties]
trigger      = { type = "string" }
area         = { type = "shape" }
detection_dc = { type = "integer", min = 0, max = 40 }
disarm_dc    = { type = "integer", min = 0, max = 40 }
effect       = { type = "string" }
damage       = { type = "string", doc = "Dice expression, for example 2d10" }
damage_type  = { type = "symbol" }
linked_to    = { type = "list", of = "reference" }
```

### 8.2 Built-in catalog for 0.1

| Type | Class | Subtypes | Visibility toggle | Default visibility |
| --- | --- | --- | --- | --- |
| `architecture` | Feature | column, pillar | `features` | public |
| `furniture` | Feature | table, chair, wardrobe, altar, statue, sarcophagus, crate | `features` | public |
| `mechanism` | Feature | lever, button, generic | `mechanisms` | public |
| `trap` | Feature | pressure\_plate, dart\_launcher, pit, generic | `traps` | dm |
| `lighting` | Feature | torch, brazier, candle, glowing\_crystal | `lighting` | public |
| `terrain` | Feature | water, lava, rubble, rocks | `terrain` | public |
| `container` | Feature | chest | `treasure` | dm |
| `creature` | Entity | Named by `reference` | `creatures` | dm |
| `npc` | Entity | Named by `reference` | `creatures` | dm |
| `item` | Entity | treasure, key, weapon, armor, scroll, generic | `treasure` | dm |
| `custom` | Either | Free `symbol` and `label` | `features` | public |

Walls, doors, secret doors, archways, corridors and stairs from draft 10.3 are structural: they are spaces and connection types (section 7), not catalog entries.

### 8.3 Rules

- The class of a type is fixed. Writing `entity` with a feature type is an error whose fix swaps the keyword. This settles the draft's overlap between features and entities: a chest is a `container` feature; what it holds are `item` entities.
- An unknown `type` or `subtype` raises `UNKNOWN_TYPE` with the nearest matches. `type = custom` is the escape hatch: it draws a marker with a one-to-three-character `symbol` and adds its `label` to the legend.
- A group of creatures is one entity with `properties.count`. It draws as one token with a count badge.
- `reference` is an opaque string pointing at an external bestiary. Dedalo never interprets it.
- An item with `contained_in = "chest_id"` has no position. It appears only in the DM notes panel under its container.
- Light sources carry `radius` in cells, `intensity` (`bright` or `dim`), `color` and `active`.
- The catalog is data. `dedalo catalog --json` prints it, and validation, editor completion, hover help, legend grouping and the reference page are all generated from it.
- Types added by a pack are namespaced with the pack's ID, for example `type = scifi.console`.

## 9. Rendering and export

Rendering takes three steps: filter the resolved model down to what one audience may see, paint that view into a scene, and serialise the scene. Secrets are removed in the first step, so nothing after it can leak them.

### 9.1 Export specification

A dungeon may declare any number of named exports. An unnamed `export` block is called `default`. Command-line flags override any field.

```hcl
export "dm" {
    preset = dm
    format = pdf
    page   = a4
}

export "players" {
    preset   = player
    format   = png
    cell_px  = 70
    rotation = 90
    revealed = ["crypt_opened"]
    include  = ["secret_door"]      # the party has found it

    visibility {
        grid = false
    }
}
```

| Field | Values | Default |
| --- | --- | --- |
| `preset` | `dm`, `player`, `clean` | `dm` |
| `visibility { }` | Toggles: `grid`, `room_labels`, `features`, `mechanisms`, `traps`, `creatures`, `treasure`, `secret_doors`, `lighting`, `terrain`, `notes` | From the preset |
| `include`, `exclude` | Lists of element IDs | Empty |
| `revealed` | List of condition tags | Empty |
| `rotation` | 0, 90, 180, 270 | 0 |
| `format` | `svg`, `png`, `pdf`, `txt` | `svg` |
| `resolution` | `low`, `medium`, `high`: 32, 64, 128 pixels per cell | `medium` |
| `cell_px` | 16 to 512; overrides `resolution` | Unset |
| `bounds` | `visible`, `full` | `visible` |
| `page`, `orientation` | PDF only: `fit`, `a4`, `a3`, `letter`; `portrait`, `landscape` | `fit` |
| `legend { }`, `render { }` | Override the dungeon-level blocks | Inherited |

### 9.2 Who sees what

An element is drawn when it is not in `exclude` and either it is in `include`, or both its visibility level admits the audience and its category toggle is on.

| Visibility level | DM audience | Player audience |
| --- | --- | --- |
| `public` | Drawn | Drawn |
| `dm` | Drawn | Not drawn |
| `conditional` | Drawn | Drawn only when its `reveal_when` tag is listed in `revealed` |
| `hidden` | Not drawn | Not drawn |

| Preset | Audience | Toggles |
| --- | --- | --- |
| `dm` | DM | All on |
| `player` | Player | All on; the audience filter does the hiding |
| `clean` | Player | Geometry and doors only: every toggle, label, legend and title off |

For the player audience the filter also changes structure:

- A room with `visibility = dm` is dropped together with its contents, the connections into it, and any corridor that leads only to it. Neighbouring walls are drawn solid.
- A hidden secret door leaves its wall unbroken.
- With `bounds = visible` the canvas is cropped to what is drawn, so blank margin does not betray hidden rooms. `bounds = full` keeps DM and player maps aligned for overlays.
- IDs, names and text of removed elements appear nowhere in the output. Source annotations are emitted only for the DM audience.
- The legend lists only categories present in the view.

Visibility never deletes anything from the document.

### 9.3 Scene

The scene is a restricted vector document, close to a subset of SVG expressed as JSON.

- **Units** are cells. Pixels appear only at emit time.
- **Layers**, bottom to top: `background`, `floor`, `terrain`, `lighting`, `grid`, `walls`, `openings`, `features`, `entities`, `markers`, `labels`, `frame`.
- **Nodes**: `group` (transform, clip, opacity), `path` (lines, arcs, cubic curves; fill, stroke, dash), `use` (a symbol with a transform), `text` (string, font, size, anchor, halo), `image` (embedded raster from a theme or plugin asset).
- **Paints**: solid sRGB colour with alpha, pattern, linear or radial gradient. Blend modes are `normal` and `multiply`.
- **Excluded by design**: scripts, filters, external references, foreign objects, animation. Whatever the scene can express renders the same in a browser, in resvg and in PDF.
- Any node may carry a `source` annotation: the element ID and role it was drawn for. The editor uses it for click-to-source.

### 9.4 Painters and themes

A painter maps one view item to nodes in one layer. The registry picks the most specific painter registered for the item: kind, type and subtype; then kind and type; then kind; then a fallback marker.

A theme is data: palette, stroke widths, fonts, patterns, symbol map and label style.

```toml
[theme]
id = "classic"

[palette]
background = "#ffffff"
floor      = "#ffffff"
wall       = "#111111"
grid       = "#c8c8c8"
water      = "#a8d0f0"
light      = "#ffd27a"

[stroke]                 # in cells
wall = 0.14
door = 0.06
grid = 0.02

[symbols]
"lighting.torch"  = "symbols/torch.svg"
"furniture.table" = "symbols/table.svg"
"trap"            = { glyph = "T", frame = "diamond" }
```

The built-in theme is `classic`: black walls on a white floor, a light grid clipped to floors, letter markers for traps and creatures. It follows the draft's priority of structural correctness over artistic quality. Its conventions:

| Item | Drawn as |
| --- | --- |
| Door | Rectangle across the opening; a locked door adds a keyhole dot |
| Secret door | Letter S across an otherwise continuous wall |
| Archway, open passage | Gap with two posts; plain gap |
| Stairs | Parallel treads narrowing in the direction of descent |
| Light source | Its symbol, plus a radial gradient of `radius` cells clipped to the room |
| Terrain | Pattern fill over its footprint |
| Creature group | Token with a count badge |

### 9.5 Labels, legend, notes and frame

- **Room labels** print the room `name`, or its ID when `labels.room_ids` is on, at the polygon's visual centre, moved off element footprints when possible.
- **Legend.** `mode = automatic` creates one entry per type present in the view, grouped by category in a fixed order, each with its symbol and label. `mode = custom` uses the `entry` blocks of the draft. `position` is `right` (default), `bottom`, `left` or `top`; the legend sits outside the map and never covers it. Overriding entry labels is also how a map is localised.
- **Notes.** A note with a position or target gets a numbered marker. All visible notes are listed in a panel under the legend. Notes default to `visibility = dm`.
- **Frame.** Title from the dungeon name, a scale line from `grid.unit`, and a north arrow.

### 9.6 Output formats

| Format | Produced by | Notes |
| --- | --- | --- |
| SVG | Serialising the scene | Primary format. Byte-stable, no metadata. Text is converted to outlines by default; `text = live` keeps selectable text |
| PNG | Rasterising that SVG with resvg | Sides above 16,384 pixels raise `EXPORT_TOO_LARGE` with the largest valid `cell_px` |
| PDF | Converting that SVG with krilla | Vector, fonts subset and embedded. Tabletop print scale (one cell per inch, tiled across pages) arrives in 1.0 |
| Text map | Drawing the view with characters | An addition to the draft: cheap for agents to read, and diffable in tests |
| Stage JSON | `--emit document`, `resolved`, `view` or `scene` | For debugging and plugin development |

A text map of the draft's entrance room, DM view:

```
####+#####      + door    T trap    * light
#........#
#........#
#........#
#....T...#
#........#
#..*.....#
##########
```

## 10. Validation and diagnostics

Every problem Dedalo reports is one diagnostic record with a stable code, the ID of the element at fault, a message that states both the actual and the allowed values, and, where one exists, a fix expressed as an edit operation.

### 10.1 Format

```json
{
  "status": "error",
  "summary": { "errors": 1, "warnings": 0 },
  "diagnostics": [
    {
      "severity": "error",
      "code": "DOOR_NOT_PLACEABLE",
      "element": "entrance_door",
      "message": "Door 'entrance_door' at offset 7 on the north wall of 'entrance' lies outside the wall shared with 'guard_room'. Valid offsets are 1 to 6.",
      "span": { "file": "crypt.dedalo", "start": [98, 13], "end": [98, 23] },
      "related": [ { "element": "guard_room", "message": "shares x 1 to 7 of that wall" } ],
      "suggestion": "Set placement.offset between 1 and 6, or remove it to centre the door.",
      "fix": { "op": "update", "id": "entrance_door", "set": { "placement.offset": 3 } }
    }
  ]
}
```

`status` is `ok`, `warning` or `error`. The human-readable form printed by the CLI shows the same record with a source excerpt.

### 10.2 Codes

| Code | Severity | Stage | Draft 10.8 item |
| --- | --- | --- | --- |
| `SYNTAX_ERROR` | Error | Parse |  |
| `UNKNOWN_BLOCK`, `UNKNOWN_ATTRIBUTE` | Error | Lower | Invalid properties |
| `TYPE_MISMATCH`, `INVALID_VALUE` | Error | Lower | Invalid properties |
| `UNKNOWN_PROPERTY` | Warning | Lower | Invalid properties |
| `DUPLICATE_ID` | Error | Lower | Duplicate identifiers |
| `UNKNOWN_REFERENCE` | Error | Lower | Missing references |
| `UNKNOWN_TYPE` | Error | Lower |  |
| `INVALID_SHAPE` | Error | Resolve | Invalid shapes |
| `LAYOUT_CYCLE`, `LAYOUT_UNDERCONSTRAINED` | Error | Resolve |  |
| `OVERLAP` | Error | Resolve | Disallowed overlaps |
| `OUT_OF_BOUNDS` | Error | Resolve | Elements outside their room |
| `DOOR_NOT_PLACEABLE` | Error | Resolve | Doors that cannot be placed |
| `UNRESOLVABLE_CONNECTION` | Error | Resolve | Unresolvable connections |
| `CORRIDOR_DISCONNECTED` | Error | Resolve | Unconnected corridors |
| `ELEMENT_OVERLAP`, `UNREACHABLE_ROOM` | Warning | Resolve |  |
| `REFERENCED_ELEMENT`, `STALE_SOURCE` | Error | Edit |  |
| `PLUGIN_NOT_FOUND`, `PLUGIN_CONFLICT`, `PLUGIN_FAILED`, `PLUGIN_TIMEOUT` | Error | Any |  |
| `EXPORT_TOO_LARGE` | Error | Emit |  |

### 10.3 Rules

- Codes are permanent. A code is never renamed or reused; `dedalo explain CODE` prints its description, causes and a worked fix.
- Messages are self-contained. They name the element, the value found and the values allowed, so a reader without the file can act on them.
- A `fix` is included only when applying it is certain to remove that diagnostic. It uses the edit-operation format of section 13.2, so an agent or the editor's quick-fix can apply it verbatim.
- Suggestions for misspelt names use edit distance against the names in scope.
- Errors block rendering; warnings do not. `--force` renders whatever resolved and outlines the failed elements, which the web preview uses to stay live while the author types.
- Input limits bound resource use: 5 MB of source, 50,000 elements, nesting depth 16. Exceeding one is an `INVALID_VALUE` on the dungeon.

## 11. Plugin architecture

Dedalo has two plugin tiers: packs, which are data, and WebAssembly plugins, which are sandboxed code. Between them a third party can add element types, restyle the default output, or replace rendering and export outright. One package runs unchanged in the binary and in the browser.

### 11.1 Extension points

| Point | Tier | Runs at | Contract | Example |
| --- | --- | --- | --- | --- |
| `catalog` | Pack | Lower | Type entries in the format of 8.1 | Science-fiction props |
| `theme` | Pack | Paint | Palette, strokes, fonts, patterns, symbol files (9.4) | Blueprint look |
| `template` | Pack | Lower | `.dedalo` files of `template` blocks | Standard corridors and doors |
| `shape` | WASM | Lower | Name and arguments to an outline | `cave(seed, w, h)` |
| `validator` | WASM | After resolve | Resolved model to diagnostics | "Every room needs a light" |
| `painter` | WASM | Paint | One view item to scene nodes | Isometric crates |
| `scene_pass` | WASM | After paint | Scene to scene | Hand-drawn wobble, rock hatching |
| `renderer` | WASM | Paint | Whole view to scene, replacing every painter | A complete custom style |
| `exporter` | WASM | Emit | View and scene to bytes, media type and file extension | Virtual-tabletop file |
| `generator` | WASM, after 1.0 | Before parse | Parameters and seed to document JSON | Procedural crypt |

Graphic customisation therefore has three levels of increasing power: a theme pack changes colours, strokes and symbols; painters and scene passes change how things are drawn; a renderer takes over the whole picture.

### 11.2 Built-ins use the same seams

The extension points are Rust traits (`Painter`, `ScenePass`, `Renderer`, `Exporter`, `Validator`, `ShapeFn`). Built-in painters implement them natively. The built-in catalog and the `classic` theme are pack files embedded in the binary and read by the ordinary pack loader. A WASM plugin is wrapped by an adapter that implements the same trait, so the registry cannot tell the two apart.

### 11.3 Using plugins from a document

```hcl
dedalo {
    version = 1
    plugin "hand_drawn" { version = "^1.2" }
    plugin "uvtt"       { version = "^0.3" }
}

dungeon "forgotten_crypt" {
    render {
        theme = hand_drawn.sepia

        plugin "hand_drawn" {
            wobble = 0.6
        }
    }

    export "vtt" {
        format = uvtt.dd2vtt
    }
}
```

- Everything a plugin contributes is namespaced by its ID, so two plugins cannot collide on a name.
- Painters are chosen by specificity first, then by the order of `plugin` blocks in the header. Scene passes run in header order.
- Two active renderers raise `PLUGIN_CONFLICT`.

### 11.4 Package and manifest

A plugin is a directory, or the same directory zipped with the extension `.dedalopkg`, with `dedalo-plugin.toml` at its root.

```toml
[plugin]
id      = "hand_drawn"
name    = "Hand-drawn style"
version = "1.2.0"
api     = 1                  # plugin API version
dedalo  = ">=0.2"
license = "MIT"

[wasm]
module = "hand_drawn.wasm"

[[scene_pass]]
name = "wobble"

[[painter]]
match = { kind = "connection", type = "door" }
layer = "openings"

[theme.sepia]
file = "themes/sepia.toml"

[options]
wobble = { type = "number", default = 0.6, min = 0.0, max = 2.0 }
```

### 11.5 WebAssembly ABI, version 1

A plugin module is core WebAssembly built for `wasm32-unknown-unknown`. A module that imports anything outside the `dedalo` namespace is rejected at load, which excludes WASI and with it files, network, clock and environment.

| Export | Signature | Purpose |
| --- | --- | --- |
| `dedalo_abi` | `() -> i32` | Returns `1` |
| `dedalo_alloc` | `(len: i32) -> i32` | Gives the host a buffer in plugin memory |
| `dedalo_free` | `(ptr: i32, len: i32)` | Takes a buffer back |
| `dedalo_call` | `(ptr: i32, len: i32) -> i64` | Handles one request; returns pointer and length packed in 64 bits |

| Host import | Signature | Purpose |
| --- | --- | --- |
| `dedalo.log` | `(level: i32, ptr: i32, len: i32)` | Message for the diagnostics stream |
| `dedalo.rand_u64` | `() -> i64` | Next value of the seeded random stream |
| `dedalo.asset` | `(ptr: i32, len: i32) -> i64` | Reads a file from the plugin's own package |
| `dedalo.measure_text` | `(ptr: i32, len: i32) -> i64` | Text metrics from the host's bundled fonts |

Requests and responses are UTF-8 JSON, validated against published schemas.

```json
{ "point": "painter", "name": "door",
  "input": { "item": { }, "theme": { }, "cell_px": 64 },
  "options": { "wobble": 0.6 } }

{ "ok": { "nodes": [ ] } }

{ "error": { "message": "unsupported opening width", "element": "entrance_door" } }
```

- **Determinism.** The same request must produce the same response. The host may cache responses by request hash.
- **Limits.** 64 MiB of memory and a fuel budget per call in the binary. In the browser, where fuel is unavailable, a watchdog terminates and restarts the worker on timeout. Exceeding a limit raises `PLUGIN_TIMEOUT`.
- **Encoding.** JSON is the version 1 encoding because it is debuggable. A binary encoding can be introduced under a later ABI number if profiling of large scene passes requires it.

The development kit `dedalo-pdk` hides the ABI behind typed Rust structures, attribute macros for each extension point, scene-building helpers and a local test harness. Any other language that compiles to core WebAssembly without WASI can target the ABI directly.

In the browser the worker instantiates the same `.wasm` bytes with the standard WebAssembly API, supplies the same four imports from JavaScript, and exposes a synchronous call to the core module.

### 11.6 Loading and locking

In the binary:

- Plugins are searched in `--plugin <path>` arguments, then `./.dedalo/plugins/`, then the per-user configuration directory, then the built-ins.
- `dedalo plugin add <path-or-url>` is the only command that touches the network. It verifies the manifest and ABI and records `id`, `version` and `sha256` in `dedalo.lock` beside the document.
- Rendering never downloads. A missing plugin raises `PLUGIN_NOT_FOUND` with the install command. A hash that differs from the lockfile is an error until `--update-lock` is passed.

In the web editor:

- A plugin index file on the site lists first-party and reviewed plugins with their URL and hash; enabling one is a single click.
- A user can also load a package from a local file or a URL. Loaded packages are kept in browser storage.
- A shared link that requires a plugin outside the index shows its name, origin and hash and asks before loading. Nothing loads silently.

There is no registry service through 1.0. The index is maintained by pull request.

### 11.7 Security model

| Threat | Control |
| --- | --- |
| Plugin reads files, network or environment | No such imports exist; modules that ask for them are rejected |
| Runaway CPU or memory | Fuel and memory caps in the binary; worker watchdog in the browser |
| Malformed or hostile output | Every response is schema-validated; node and coordinate counts are capped |
| Script injection through symbols or scene | The scene cannot express scripts; SVG symbol files pass an allow-list sanitiser that strips scripts, event attributes, external references and foreign objects |
| DM secrets leaking into player maps | Painters, passes, renderers and exporters receive only the filtered view. Validators see the full model but can return only diagnostics |
| Tampered packages | Lockfile hashes; reviewed index; no automatic updates |

### 11.8 Versioning

- The manifest's `api` integer is the plugin API version. A host supports the current version and the previous one.
- The API is unstable throughout 0.2 and frozen at 1.0. Adding optional message fields does not change the number.
- `dedalo plugin check` runs the conformance suite: manifest, ABI, schema validity, determinism (each call twice) and identical results in both hosts.

### 11.9 Reference plugins shipped with 0.2

| Plugin | Tier | What it proves |
| --- | --- | --- |
| `blueprint` | Pack: theme only | A new look with no code |
| `hand_drawn` | WASM: scene pass, painters, theme | The default output can be restyled completely from outside the core |
| `uvtt` | WASM: exporter | Non-image output built from the semantic model: a [Universal VTT](https://help.roll20.net/hc/en-us/articles/41643201127831-Universal-Virtual-Tabletop-UVTT-Support) file carrying the map image, walls, portals and lights |

An exporter may declare `needs = ["png"]` or `["svg"]` in its manifest to receive the host's own rendering as part of its input.

## 12. Command-line interface

`dedalo` is one executable with subcommands. Every command takes a file or standard input, never prompts, and with `--json` prints the machine form defined in this document.

| Command | Purpose | Main flags |
| --- | --- | --- |
| `dedalo render FILE` | Compile and write a map | `-o`, `--export NAME`, `--all`, `--preset`, `--format`, `--cell-px`, `--rotation`, `--theme`, `--emit STAGE`, `--force`, `--watch` |
| `dedalo validate FILE` | Run parse, lower and resolve; print diagnostics |  |
| `dedalo fmt FILE` | Format in place | `--check` |
| `dedalo convert FILE` | Switch between DSL and JSON | `--to json`, `--to dsl`, `--explicit-defaults` |
| `dedalo edit FILE` | Apply edit operations | `--ops FILE`, `--dry-run`, `--base-hash` |
| `dedalo inspect FILE` | Print an outline of rooms, connections and counts | `--id`, `--stage`, `--text-map` |
| `dedalo catalog [QUERY]` | List types, subtypes and property schemas |  |
| `dedalo schema NAME` | Print a JSON Schema | `--profile` |
| `dedalo explain CODE` | Describe a diagnostic code with a worked fix |  |
| `dedalo guide [TOPIC]` | Print the built-in language reference |  |
| `dedalo new` | Write a starter file | `--example NAME` |
| `dedalo plugin ...` | `add`, `list`, `remove`, `new`, `check`, `test` (from 0.2) | `--update-lock` |
| `dedalo mcp` | Start the MCP server | `--root DIR`, `--http ADDR` |
| `dedalo skill export DIR` | Write the agent skill folder |  |

Conventions:

- Standard output carries only the result: bytes, or JSON with `--json`. Human-readable diagnostics and logs go to standard error.
- `-` in place of `FILE` reads standard input; `-o -` writes to standard output.
- Without `-o`, output is written beside the source as `<dungeon-id>.<export-name>.<extension>`.
- No network access except `dedalo plugin add`. No telemetry.

| Exit code | Meaning |
| --- | --- |
| 0 | Success; warnings may be present |
| 1 | Diagnostics contain at least one error |
| 2 | Invalid command-line usage |
| 3 | File or I/O failure |
| 4 | Plugin failure or internal error |

## 13. Agent interface

Agents reach Dedalo through the CLI when they have a shell and through MCP when they do not. Both expose the same operations with the same JSON. The recommendation is to ship both, plus a JSON Schema and a skill, because each serves a different kind of agent host at little extra cost.

### 13.1 Surfaces

| Surface | Serves | What it is |
| --- | --- | --- |
| CLI with `--json` | Coding agents with a shell | The commands of section 12 |
| MCP server | Chat, desktop and IDE hosts without a shell | `dedalo mcp`, inside the same binary |
| JSON Schema | Structured output and function calling where Dedalo is not installed | The `document` schema, `authoring` profile |
| Agent skill | Any host that loads skills | A folder with `SKILL.md` and reference files |

### 13.2 Design rules

- **Stateless.** Every call carries a path or the full source. Nothing is remembered between calls.
- **Addressed by ID.** An agent changes one element without regenerating the file.
- **Smallest useful feedback.** An outline instead of a dump, a text map before an image, an error with its fix.
- **Few tools.** Six, each with one job.
- **One vocabulary.** Tool results are the JSON structures of sections 6 and 10.

### 13.3 Edit operations

```json
{
  "base_hash": "sha256:9f2c...",
  "ops": [
    { "op": "add", "parent": "guard_room",
      "source": "feature \"brazier\" {\n    type = lighting\n    subtype = brazier\n    placement { anchor = center }\n}" },
    { "op": "add", "kind": "room", "id": "library",
      "set": { "name": "Library",
               "shape": { "fn": "rectangle", "args": [6, 5] },
               "layout.adjacent_to": "guard_room", "layout.direction": "west" } },
    { "op": "connect", "id": "library_door", "from": "guard_room", "to": "library", "type": "door" },
    { "op": "update", "id": "entrance_door",
      "set": { "placement.offset": 2 }, "unset": ["properties.locked"] },
    { "op": "move", "id": "skeletons", "to": { "room": "library", "position": [2, 2] } },
    { "op": "remove", "id": "wall_torch" }
  ]
}
```

| Operation | Effect |
| --- | --- |
| `add` | Inserts an element given as a DSL fragment (`source`) or as `kind`, `id` and `set`. `parent` nests it in a space |
| `update` | Sets or unsets attributes by dotted path, with values in document JSON form |
| `remove` | Deletes an element. Fails with `REFERENCED_ELEMENT`, listing the referrers, unless `cascade` is true |
| `move` | Rewrites a room's layout, or an element's space and position |
| `connect` | Shorthand for adding a connection |

- Operations apply in order to the syntax tree, and a batch is atomic.
- Text, comments and formatting outside the touched nodes are preserved byte for byte. Inserted blocks are formatted canonically.
- The result is validated. The file is written only if the batch introduces no new error, unless forced.
- The reply carries status, diagnostics, a unified diff, the changed IDs and the new content hash.
- `base_hash` is optional. A mismatch raises `STALE_SOURCE`, which protects a human's concurrent edit.
- The draft also lists `validate` and `render` as operations; here they are commands and tools, since they do not change the document.

### 13.4 MCP server

`dedalo mcp` speaks over stdio by default and over Streamable HTTP with `--http`, bound to the loopback interface only. It targets [MCP revision 2026-07-28](https://blog.modelcontextprotocol.io/posts/2026-07-28/). That revision retired the initialise handshake and protocol sessions and deprecated sampling, roots and logging. Dedalo's server relies on none of them: it offers tools, resources and prompts, and every call is self-contained. `--root` confines all path arguments to one workspace directory.

| Tool | Input | Returns |
| --- | --- | --- |
| `dedalo_guide` | `topic`: `quickstart`, `syntax`, `layout`, `catalog`, `catalog/<type>`, `errors/<CODE>`, `examples/<name>` | Markdown, each topic kept near 1,500 tokens |
| `dedalo_validate` | `path` or `source` | Diagnostics in the format of 10.1 |
| `dedalo_inspect` | `path` or `source`; `view`: `outline`, `text_map`, `element`, `resolved`; optional `id` | A compact outline, the text map, or one element's resolved data |
| `dedalo_write` | `path`, `source` | Validates, then creates or replaces the file; diagnostics and content hash |
| `dedalo_edit` | `path`, `ops`; optional `dry_run`, `base_hash` | Status, diagnostics, diff, new hash |
| `dedalo_render` | `path` or `source`; `preset` or `export`; `format`; optional `out`, `max_px` | The map as an image (PNG, long edge capped at `max_px`, default 1,024) or as text; the written path when `out` is given |

- `dedalo_guide`, `dedalo_validate` and `dedalo_inspect` are marked read-only.
- Each result has a one-line text summary and the structured JSON.
- Resources mirror the guide topics, the schemas and the catalog for hosts that surface resources. Two prompts are offered: `new_dungeon` and `review_dungeon`.
- An interactive preview inside the host, through the MCP Apps extension, is a candidate for after 1.0.

The intended loop is: read `quickstart` once, write, validate, apply the returned fixes, inspect the text map, render, then continue with small edits.

### 13.5 Schema for constrained generation

`dedalo schema document --profile authoring` prints a closed, non-recursive schema with enumerations inlined. Element `properties` are typed loosely in this profile to stay within provider limits on schema size; catalog-level checking still happens in `validate`.

### 13.6 Skill and reference

`dedalo skill export` writes a folder in the [Agent Skills](https://agentskills.io/) format: a `SKILL.md` with name, description and the working loop, and reference files for syntax, layout, catalog, error codes and examples that the agent loads on demand. Guide topics, skill references and the website reference are generated from one set of Markdown sources, so they cannot drift apart. The site also serves `llms.txt` and the reference as plain Markdown.

### 13.7 Agent evaluation

A scripted suite of about 20 tasks gates each release: author a dungeon from a written brief, repair files seeded with each error code, apply described changes through operations, and produce DM and player maps. It runs through both CLI and MCP against at least two model families. Initial targets, to be calibrated during M5, are 85% of tasks solved on the first attempt and 95% within three validate-and-fix rounds. A failed task is treated as a product defect, usually an unclear message or a missing fix.

## 14. Web editor on GitHub Pages

The web editor is a static single-page application that runs the same core, compiled to WebAssembly, inside a Web Worker. After the first load it needs no network, and no document ever leaves the browser.

### 14.1 Features

| Area | In 0.1 |
| --- | --- |
| Editing | Syntax highlighting; inline diagnostics and a problems panel; quick-fixes taken from each diagnostic's `fix`; completion of block keywords, attributes, enumeration values, element IDs and catalog types; hover help; format; outline of spaces |
| Preview | Live SVG with pan and zoom; preset and export picker; temporary toggles for grid, labels and rotation that do not modify the source; click an element to jump to its source; the last good or partial render stays visible while errors exist |
| Output | Download SVG, PNG, PDF, text map and document JSON |
| Files | Open and save local files; drafts autosaved to browser storage; examples gallery |
| Sharing | A link with the document compressed into the URL fragment, which browsers do not send to the server |
| Offline | Installable; works offline after the first visit |
| Reference | Language, catalog and error pages generated from the same sources as `dedalo guide` |

Release 0.2 adds a plugins panel: the index, loading from file or URL, and per-document consent (11.6).

### 14.2 Architecture

- The UI thread runs Svelte and CodeMirror 6. A worker hosts `dedalo-wasm`, so compiling never blocks typing.
- UI and worker exchange a small set of messages: `compile`, `render`, `export`, `complete`, `hover`, `apply_fix`. Recompiles are debounced by 150 ms and stale results are discarded by generation number.
- There are two WebAssembly modules. `core` covers parsing through SVG and loads at start. `export` covers PNG, PDF and fonts and loads on the first raster or PDF export. The boundary between them is an SVG string in and bytes out.
- CodeMirror gets only a small highlighting grammar. Diagnostics, completion and hover all come from the core, so there is one implementation of the language.
- The preview is produced by Dedalo's own serialiser from a validated scene, which cannot express scripts.
- Share links carry the source deflated and base64url-encoded. Above 8,000 characters the UI offers a file download instead.

### 14.3 Constraints of static hosting

[GitHub Pages](https://docs.github.com/en/pages/getting-started-with-github-pages/github-pages-limits) allows a published site of at most 1 GB with a soft bandwidth limit of 100 GB per month, and is not meant for running a commercial service. The consequences for the design:

- Module size is budgeted and enforced in CI: `core` at most 1.5 MB compressed, `export` at most 3 MB compressed.
- There is no server code. Sharing uses URL fragments, storage is local, and the plugin index is a static file.
- The design does not rely on custom response headers. In particular it needs no cross-origin isolation, hence no shared memory and no WebAssembly threads: the core runs single-threaded in one worker.
- A project site is served under `/<repository>/`. All asset URLs are relative and navigation uses the URL fragment, so no rewrite rules are needed.
- A GitHub Actions workflow builds both modules and the site. Tagged releases deploy to the site root; the `main` branch deploys to `/next/`. Schemas are served under `/schema/` at versioned URLs.

### 14.4 Targets

- **Browsers.** The current and previous major versions of Chrome, Edge, Firefox and Safari. On phones, preview and export work; editing is not optimised.
- **Speed.** On the 50-room sample and a mid-range laptop: first preview within 2 seconds of a cold load on broadband, and keystroke to updated preview within 100 ms.
- **Accessibility.** Full keyboard operation. The outline and the text map serve as a non-visual alternative to the preview.
- **Privacy.** No analytics, no cookies, and no requests after load other than plugin downloads the user asks for.

## 15. Quality, determinism and security

Quality rests on one corpus of example dungeons whose expected outputs are compared byte for byte on every platform, backed by property tests for the language and a leak test for player maps.

### 15.1 Test layers

| Layer | What it checks | Tooling |
| --- | --- | --- |
| Unit | Functions of each crate | `cargo test` |
| Parser snapshots | Syntax trees and diagnostics for valid and invalid inputs | `insta` |
| Fuzzing | Parse, lower and resolve never panic; printing a parsed file returns the original bytes | `cargo fuzz` |
| Properties | Round-trip laws of 6.3; formatter idempotence; edit operations leave untouched text intact; no reported overlap implies disjoint polygons; four quarter-turns equal the identity | `proptest` |
| Golden corpus | Expected diagnostics, resolved JSON, text map, SVG bytes and PNG hash for every corpus file | Custom runner |
| Cross-target | Identical SVG and PNG hashes on Linux x86-64, macOS arm64, Windows x86-64 and WebAssembly in Node and headless browsers | CI matrix |
| Leak test | No ID, name or text of a non-public element appears in any player output: SVG text, PNG text chunks, PDF text and metadata | Custom runner |
| Plugin conformance | Reference plugins give identical results in both hosts; hostile fixtures (endless loop, memory bomb, invalid output, forbidden import) are contained | `dedalo plugin check` |
| Web end-to-end | Edit, preview, export, share link, offline use | Playwright |
| MCP contract | Recorded request and response fixtures over stdio and HTTP | Custom runner |
| Agent evaluation | The task suite of 13.7 | Scripted |
| Documentation | Every code sample in the docs compiles and renders | CI |

The corpus contains the draft's reference example (corrected as in 7.3), one failing file per diagnostic code, a kitchen-sink file that exercises all 20 acceptance criteria, at least 30 layout cases, and 50-room and 100-room performance samples.

### 15.2 Determinism guarantee

Given the same source bytes, Dedalo version, plugin hashes and export settings, the SVG and PNG outputs are byte-identical on every supported platform, including the browser.

The means are integer geometry, ordered collections, pure-Rust mathematics, bundled fonts, text drawn as outlines, fixed number formatting, no timestamps or host information in any output, pinned dependencies and seeded randomness.

PDF has the same goal. It depends on fixing the document date and identifier in the PDF writer, to be verified in M3; failing that, the PDF guarantee is identical page content.

Across versions, the output of a given file under a given theme changes only in a release whose notes say so. The corpus diff is part of every release review.

### 15.3 Performance budgets

Budgets are for a 100-room, 1,000-element dungeon on a mid-range laptop, native build, unless stated. They are proposals to confirm on real hardware in M3 and are then enforced by benchmarks in CI with a 20% regression threshold.

| Operation | Budget |
| --- | --- |
| Process start to first output, small file | 30 ms |
| Validate | 50 ms |
| Render to SVG | 150 ms |
| Render to PNG at 64 pixels per cell | 1.5 s |
| Apply an edit batch | 50 ms |
| Web: keystroke to updated preview, 50 rooms | 100 ms |

### 15.4 Security

- Source files, JSON, edit operations, plugin packages and shared links are all untrusted input, bounded by the limits of 10.3.
- Core crates contain no `unsafe` code. Dependencies are checked by `cargo deny` and `cargo audit` in CI.
- The MCP server confines paths to its root, including through symbolic links, binds HTTP to loopback, checks the request origin, and never runs external commands.
- The web editor sets a strict content security policy: no inline script, and WebAssembly as the only dynamic code.
- Plugin threats and controls are listed in 11.7.
- Release artefacts carry checksums and build-provenance attestations.

## 16. Distribution and release engineering

One tagged commit produces everything: binaries on GitHub Releases, the website on GitHub Pages, and library packages for plugin authors and embedders.

### 16.1 Binary

| Target | Notes |
| --- | --- |
| Linux x86-64 and arm64 | Statically linked |
| macOS arm64 and x86-64 |  |
| Windows x86-64 | arm64 if demand appears |

| Channel | From |
| --- | --- |
| GitHub Releases: archives, checksums, shell and PowerShell install scripts | 0.1 |
| `cargo install`, and prebuilt binaries through `cargo binstall` | 0.1 |
| Listing in MCP server directories | 0.1 |
| Homebrew tap | 1.0 |
| npm wrapper package, so an MCP host can launch the server with `npx` | 1.0 |

The size target is 25 MB uncompressed, fonts included. macOS notarisation and Windows code signing need paid certificates; until that decision is taken (section 19), releases ship with checksums and provenance attestations, and the install guide explains the operating-system warnings.

### 16.2 Libraries

| Package | Registry | Audience | From |
| --- | --- | --- | --- |
| `dedalo-pdk` | crates.io | Plugin authors | 0.2 |
| Core crates | crates.io | Rust programs embedding Dedalo | 1.0 |
| WebAssembly build with TypeScript types | npm | Web applications embedding Dedalo | 1.0 |

### 16.3 Versioning

| Subject | Scheme | Promise from 1.0 |
| --- | --- | --- |
| Binary and crates | Semantic versioning | Flags and JSON output stay stable within a major version |
| Language | Integer in the `dedalo` header | Every version since 1 keeps working with its original meaning |
| Plugin API | Integer in the manifest | Current and previous versions are supported |
| Schemas | Major number in the URL | Old URLs stay served |
| Themes | Named revisions | A visual change to `classic` ships as a new revision; the old one stays selectable |

### 16.4 Release process

- The release checklist is the CI pipeline: every test layer of 15.1, the performance and size budgets, the agent evaluation and the documentation build.
- Release notes have three fixed headings: language changes, output changes (with before-and-after images from the corpus) and plugin API changes.
- Releases follow milestones until 1.0.
- Versions and tags are automatic (semantic release): Conventional Commit prefixes on `main` decide the next version once CI passes, and the release workflow builds and attaches the binaries before publishing.

### 16.5 Licensing

The recommendation is a dual MIT or Apache-2.0 licence for code, which matches the rendering and runtime dependencies of section 4, with bundled fonts under the SIL Open Font License and documentation under CC BY 4.0. The choice belongs to the product owner (section 19).

## 17. Milestones

Eight milestones lead to 1.0. M0 to M3 build the core in strict sequence, M4 and M5 put the web and agent hosts on it and close release 0.1, M6 opens the plugin platform as 0.2, and M7 freezes the interfaces for 1.0.

&#91;embedded content: roadmap · 8 milestones, 3 release gates\]

The top row runs left to right in sequence. M4 can start once M1 is done and M5 once M2 is done; both must finish, together with M3, before 0.1.

| Milestone | Outcome | Depends on | Effort (person-weeks) | Closes |
| --- | --- | --- | --- | --- |
| M0 Walking skeleton | One room renders identically from the binary and the browser |  | 2 |  |
| M1 Language | Files parse, validate statically and round-trip with JSON | M0 | 5 |  |
| M2 Geometry | Rooms, corridors, doors and elements resolve to exact geometry | M1 | 7 |  |
| M3 Rendering and export | DM, player and clean maps in SVG, PNG and PDF | M2 | 7 |  |
| M4 Web editor | The editor is live on GitHub Pages | Starts after M1, finishes after M3 | 5 |  |
| M5 Agent interface | Edit operations, MCP server, skill, evaluation | Starts after M2, finishes after M3 | 4 | 0.1 |
| M6 Plugin platform | Packs and WASM plugins run in both hosts | M3, M4 | 8 | 0.2 |
| M7 Hardening | Interfaces frozen, installers, documentation, budgets | M6 | 5 | 1.0 |

Effort is an estimate for one engineer fluent in Rust: 43 person-weeks in total, 30 of them to reach 0.1. It should be recalibrated after M1, once real velocity on this codebase is known. With two people, M4 and M5 overlap M2 and M3.

### M0. Walking skeleton

Deliverables:

- Workspace and crate skeletons; CI on three operating systems plus `wasm32`.
- Decision records for the ten decisions of section 1.
- The pipeline end to end for the smallest case: a `dungeon` with one rectangular `room`, rendered to SVG by `dedalo render`.
- A bare web page (text area and preview) deployed to GitHub Pages by workflow.
- The golden-test runner with cross-target hash comparison, and a build spike confirming that the PNG and PDF libraries compile to WebAssembly within the size budget.

Exit criterion: one file yields byte-identical SVG from the Linux, macOS and Windows binaries and from the deployed page.

### M1. Language

Deliverables:

- Lexer, recovering parser, lossless syntax tree, formatter.
- Lowering to the document model: aliases, defaults, references, one ID namespace.
- JSON Schema generation and conversion in both directions.
- Catalog for 0.1 with property typing.
- Diagnostics framework: codes, spans, suggestions, JSON and human output.
- Commands `validate` (static checks), `fmt`, `convert`, `schema`, `catalog`, `explain`, `guide`.

Exit criteria:

- The draft's section 9 example parses, formats and round-trips.
- Every lower-stage code of 10.2 has a failing fixture with a correct suggestion.
- The round-trip properties pass, and one hour of fuzzing produces no panic.

### M2. Geometry

Deliverables:

- Shapes: square, rectangle, circle, polygon.
- Placement: absolute; relative with `align`, `offset` and `gap`; automatic for the simple case; rotation in quarter turns.
- Connections with openings and stubs; corridors, straight and by explicit path; wall derivation.
- Element placement by cell, wall and anchor, with bounds checks.
- Reachability, the resolved model, `inspect`, and the text map.

Exit criteria:

- Four rooms of four different shapes, joined by doors, a secret door and corridors, resolve correctly.
- At least 30 layout cases have golden resolved models and text maps.
- Every resolve-stage code has a fixture with a correct suggestion, and a fix where one is defined.

### M3. Rendering and export

Deliverables:

- View builder: audiences, toggles, include and exclude, hidden rooms, bounds, presets.
- Scene IR, SVG serialiser, `classic` theme, symbols for the 0.1 catalog.
- Grid, labels, light halos, legend (automatic and custom), notes, frame.
- Output rotation, PNG, PDF, named exports, `render --all`.

Exit criteria:

- All 20 acceptance criteria of the draft pass on the kitchen-sink file through the binary.
- The leak test passes on the whole corpus.
- SVG and PNG are byte-identical across the CI matrix.
- The performance budgets of 15.3 are measured, then confirmed or revised.

### M4. Web editor

Deliverables:

- Worker-hosted core, message protocol, two-module split.
- Editor: highlighting, diagnostics, completion, hover, format, outline.
- Preview: pan, zoom, presets, toggles, click-to-source, partial render.
- Downloads, local files, autosave, share links, examples, offline use, reference pages.

Exit criteria:

- All 20 criteria are reproducible in the browser with the network disconnected after the first load.
- The size and speed targets of section 14 are met, and the end-to-end suite passes on three browser engines.

### M5. Agent interface

Deliverables:

- Edit-operation engine on the syntax tree, `dedalo edit`, and one-click application of diagnostic fixes in the CLI and the web editor.
- `--json` on every command.
- MCP server over stdio and HTTP with six tools, resources and prompts.
- Guide content, skill folder, `llms.txt`, the authoring schema profile.
- The agent evaluation suite.

Exit criteria:

- The evaluation targets of 13.7 are met through both CLI and MCP with two model families.
- Edit operations preserve all untouched text under property testing.
- MCP contract tests pass.

This closes release 0.1.

### M6. Plugin platform

Deliverables:

- Manifest, pack loader and registry, with the built-in catalog and theme moved onto the pack loader.
- `template` blocks and `using`.
- WASM host in the binary and the browser, ABI version 1, `dedalo-pdk`.
- Extension points: shape, validator, painter, scene pass, renderer, exporter.
- Lockfile, `dedalo plugin` commands, web plugins panel and index, symbol sanitiser.
- Reference plugins `blueprint`, `hand_drawn` and `uvtt`, and the plugin author guide.

Exit criteria:

- Each reference plugin gives identical output in the binary and the browser.
- A renderer plugin replaces the whole look with no change to any core crate.
- Every hostile fixture is contained.

This closes release 0.2.

### M7. Hardening and 1.0

Deliverables:

- Freeze review of language version 1, plugin API version 1 and all schemas.
- Remaining 1.0 scope: `ellipse` and `l_shape`; PDF print scale with page tiling.
- Install channels, signing as decided, npm and crates.io packages.
- Extended fuzzing, performance work, accessibility pass.
- Complete documentation: tutorial, reference, cookbook, plugin guide, compatibility policy.

Exit criteria:

- All budgets and test layers pass, with no open defect that produces a wrong map.
- The corpus is frozen as the compatibility contract.

This closes release 1.0.

### Traceability to the draft's acceptance criteria

| Draft criteria | Capability | First met in |
| --- | --- | --- |
| 1, 20 | Define a dungeon in the DSL; understandable errors | M1 |
| 2, 3, 4, 5 | Four shapes; placement by coordinates or relations; doors and corridors; secret doors | M2 |
| 6, 7, 8, 9 | Traps and mechanisms; creatures and objects; columns and pillars; light sources | M2 for placement, M3 for drawing |
| 10, 11, 12 | Coherent map; legend; notes | M3 |
| 13, 14, 15, 16, 17 | Hide grid, categories, rooms and secrets; DM and player versions | M3 |
| 18, 19 | Rotated output; PNG, SVG and PDF | M3 |
| All 20 | The same results in the browser | M4 |
| All 20 | The same results driven by an agent | M5 |

## 18. Post-1.0 roadmap

Each later feature of the draft has a defined entry point in this architecture, and the names it will need are reserved now so that adding it changes no existing file.

| Draft item | How it enters | Reserved by 1.0 |
| --- | --- | --- |
| 11.1 Advanced layout | New strategies behind the resolve stage, selectable per room; a `layout` extension point for solvers | `near`, `far_from` and further `mode` values parse and report that they are not yet supported |
| 11.2 Multi-level dungeons | `level` blocks group spaces; vertical connections gain a `to` on another level; one view per level | The `level` keyword and a `level` field in the resolved model |
| 11.3 Dynamic elements | A `state` attribute on connections and elements, overridable per export | The property name `state` |
| 11.4 Advanced lighting | Visibility polygons computed from wall segments in the view stage; bright, dim and dark zones as scene content | The `lighting` layer; `radius` and `intensity` on lights |
| 11.7 Other grids | Grid painter and cell addressing behind `grid.type`; isometric output as a renderer plugin | `grid.type` with the single value `square` |
| 11.8 Assisted generation | The `generator` extension point, surfaced as a CLI subcommand and an MCP tool | The extension point name |
| 11.9 Visual editor | Gestures on the preview emit the same edit operations agents use, so the text remains the source of truth | Click-to-source annotations and the operations engine |
| Section 14: 3D rendering | An exporter or renderer plugin over the resolved model, where walls are segments with heights | `ceiling_height` |
| Arbitrary rotation, `composite` shapes | Geometry work only; no interface change | The function and attribute names |
| Interactive preview inside MCP hosts | The MCP Apps extension serving the web preview | Nothing |
| More virtual-tabletop formats | Exporter plugins | Nothing |

## 19. Risks and open questions

The largest schedule risk is geometry, and the largest product risk is a secret appearing on a player map; both are addressed by tests that exist before the code they guard.

### 19.1 Risks

| # | Risk | Effect | Mitigation |
| --- | --- | --- | --- |
| 1 | Geometry edge cases cost more than planned: shared-wall detection, curved rooms, non-orthogonal polygons | M2 slips | Integer coordinates; narrow rules for 0.1; stubs for near-misses; the golden corpus written before the code |
| 2 | Byte-identical output breaks on one platform through fonts, mathematics or a dependency update | The determinism guarantee fails | Bundled fonts, outline text, pure-Rust mathematics, pinned lockfile, hash comparison in CI from M0 |
| 3 | WebAssembly modules are too large for a quick first load | Poor web experience | Two-module split, lazy export module, size budgets in CI |
| 4 | Agents place things at wrong coordinates | Low task success | Relative placement, anchors and wall placement remove arithmetic; errors carry ranges and fixes; text-map feedback; the evaluation gate |
| 5 | A secret leaks into a player map | Loss of trust in the product | Redaction before painting; plugins see only the view; the leak test on every corpus file |
| 6 | MCP keeps changing: 2026-07-28 was a breaking revision, and the Rust SDK was outside the top support tier when it shipped | Rework of the server | MCP isolated in one module; stateless tools; the CLI as primary agent surface; a minimal hand-written stdio server as fallback |
| 7 | The plugin ABI diverges from where the ecosystem settles, the Component Model | A second ABI to support later | Four exports and JSON only; a documented mapping to an interface definition; review at the 1.0 freeze |
| 8 | JSON overhead makes scene passes slow on large maps | Sluggish plugin styles | Measure in M6; binary encoding under a new ABI number; host-side geometry helpers |
| 9 | Catalog scope creep: draft section 4 lists about 130 element kinds | M3 does not end | Catalog for 0.1 fixed to draft 10.3; the `custom` type; everything else as packs |
| 10 | Trademark exposure | Forced renaming | No rules content bundled; neutral names (the style is `classic`, where the draft says "Classic D&D"); compatibility described in generic terms |
| 11 | Unsigned binaries trigger operating-system warnings | Lower adoption | Decide on signing before 1.0; document the workaround until then |

### 19.2 Open questions for the product owner

Answered by the product owner in November 2026, except staffing.

- [x] **Implementation language.** Rust.
- [x] **Y axis.** Y grows south (7.1).
- [x] **Vertical connections in 0.1.** Decided beyond the two options: multi-level dungeons are supported now. `level` blocks group spaces, each level is laid out on its own plane, and stairs, ladders and trapdoors join levels, with `to_position` marking where they arrive. This brings forward the first row of section 18.
- [x] **Default look.** Black on white for `classic`.
- [x] **Languages.** English and Italian, for the editor and for default legend labels (`render { language = it }`).
- [x] **Licence.** Apache-2.0 alone, not the dual licence proposed.
- [x] **Signing budget.** Stay unsigned.
- [x] **Names and addresses.** The owner is Francesco Magnani (`FreshMag`). The site address for the schema URLs is `https://freshmag.github.io/dedalo/next/`. The file extension is `.dedalo`.
- [ ] **Staffing.** Who builds this and at what allocation, so that person-weeks become dates.
- [x] **Evaluation gate.** 85% first-attempt and 95% within three rounds are the agent targets for 0.1.

## 20. Appendix: differences from draft v1.0

This document changes the draft in 14 places and adds 8 things it does not mention. Everything else in the draft stands as written.

### 20.1 Changes

| Draft section | Draft | This document | Reason |
| --- | --- | --- | --- |
| 3.2, 8.2 | One `export` block | Any number of named exports, plus presets | One file must yield DM, player and clean maps (draft 8.3) |
| 3.8 | `locked_door` and `secret_door` are connection types | Aliases of `door` with flags | The draft's own example sets `type = door` with `locked = true`: two spellings of one state |
| 3.9, 3.10 | Feature or entity is the author's choice | The class is fixed by the catalog type | The draft lists containers as entities and uses a chest as a feature |
| 5.1 | `relative_to = "north_wall"` | `placement { wall = north, offset = 2 }` | Same mechanism as doors, no magic strings |
| 5.2 | Relations `near`, `far_from`, `inside`, `outside`, `above`, `below` | Reserved; 0.1 has adjacency and its four shorthands | They need a solver or several levels |
| 5.4 | Arbitrary rotation angles desirable | Quarter turns through 1.0 | Keeps geometry exact and on the grid |
| 7.1 | A style named "Classic D&D" | `classic` | Trademark |
| 8.2 | `resolution = high` | Named levels mapped to pixels per cell, plus `cell_px` | Reproducibility needs a number |
| 8.3 | `hidden` and `conditional` loosely defined | Exact rules in 9.2 with `reveal_when`, `revealed` and `include` | Required to implement player maps |
| 9 | Reference example | Needs `align = end` on `treasure_room` | Two rooms overlap otherwise (7.3) |
| 10.2 | Straight corridors only in the MVP | Straight automatic routing plus explicit orthogonal paths | Draft 3.7 requires explicit paths, and they need no solver |
| 11.5 | Templates after the MVP | In 0.2 | Packs ship them |
| 12.2 | `errors` array; code `INVALID_CONNECTION` | `diagnostics` array with severity, span and fix; two precise codes | Warnings need a place, agents need the fix |
| 12.3 | `validate` and `render` listed as operations | Commands and tools | They do not edit the document |

### 20.2 Additions

- The `dedalo` header block for language version and plugin requirements.
- Dotted symbols for plugin namespaces, heredoc strings, and lenient spelling of symbols and references.
- `align`, `offset` and `gap` on relative layout; `anchor = center` for elements; stub passages.
- The `custom` element type.
- `bounds` on exports, so hidden rooms do not show as blank margin.
- The text map output.
- The plugin lockfile.
- `base_hash` on edits, protecting concurrent human changes.

## Sources

Pages consulted for the technology choices, as of the date under the title.

- [The 2026-07-28 MCP specification](https://blog.modelcontextprotocol.io/posts/2026-07-28/), Model Context Protocol blog
- [rmcp 3.1.0 README](https://docs.rs/crate/rmcp/3.1.0/source/README.md), docs.rs
- [The Road to Component Model 1.0](https://bytecodealliance.org/articles/the-road-to-component-model-1-0), Bytecode Alliance
- [Extism JS SDK](https://github.com/extism/js-sdk), GitHub
- [wasmi 1.0.9 README](https://docs.rs/crate/wasmi/1.0.9/source/README.md), docs.rs
- [resvg](https://github.com/linebender/resvg), GitHub
- [krilla](https://github.com/LaurenzV/krilla), GitHub
- [hcl-rs and hcl-edit](https://github.com/martinohmann/hcl-rs), GitHub
- [dist, formerly cargo-dist](https://docs.rs/crate/cargo-dist/latest), docs.rs
- [GitHub Pages limits](https://docs.github.com/en/pages/getting-started-with-github-pages/github-pages-limits), GitHub Docs
- [Agent Skills overview](https://agentskills.io/), agentskills.io
- [Universal VTT support](https://help.roll20.net/hc/en-us/articles/41643201127831-Universal-Virtual-Tabletop-UVTT-Support), Roll20 Help Center
