# Dedalo

Dedalo compiles a text description of a tabletop-RPG dungeon into a map:
SVG, PNG, PDF, a text map, or a plugin's format. One Rust core does all the
work; a command-line binary, a static web editor and an MCP server are thin
hosts around it, so they cannot disagree.

```hcl
dungeon "crypt" {
    room "entrance" {
        shape = rectangle(8, 6)
    }

    room "vault" {
        shape = square(6)
        layout { north_of = "entrance" }
    }

    connection "vault_door" {
        from = "entrance"
        to = "vault"
        type = door
    }
}
```

```
dedalo render crypt.dedalo --format png                 # the DM map
dedalo render crypt.dedalo --preset player --format pdf # what the players may see
dedalo inspect crypt.dedalo --text-map                  # the map as text
dedalo validate crypt.dedalo --fix                      # apply the fixes diagnostics carry
dedalo mcp                                              # serve agents over MCP
```

Start with [the tutorial](docs/tutorial.md), then [the cookbook](docs/cookbook.md).
`dedalo guide` prints the language in one page. The full design is
[the specification](docs/specification.md).

## Status

Work follows the milestones of specification section 17.

| Milestone | State |
| --- | --- |
| M0 Walking skeleton | Done |
| M1 Language | Done. Round-trip laws are checked on the corpus and on mutated input rather than with `proptest` generators |
| M2 Geometry | Done, with 35 golden layout and feature cases. Not done: `bearing` on circles, and doors on diagonal polygon edges |
| M3 Rendering and export | Done: views, presets, the `classic` theme, text as outlines from bundled fonts, SVG, PNG, PDF, rotation, legend, notes. Not done: SVG symbol files in themes and their sanitiser |
| M4 Web editor | Done, without a build step: plain JavaScript instead of Vite, Svelte and CodeMirror (see below) |
| M5 Agent interface | Done: edit operations, `--json` everywhere, MCP over stdio and HTTP, guide, skill, `llms.txt`, authoring schema, the evaluation suite. The suite has not been run against models yet |
| M6 Plugin platform | Done: packs, templates, the WebAssembly host in the binary (wasmi) and the browser, `dedalo-pdk`, six extension points, lockfile, plugin commands, the web plugins panel, three reference plugins. Packages are folders; zipped `.dedalopkg` files and URL installs are not supported yet |
| M7 Hardening | Partly done: `ellipse`, `l_shape`, tabletop PDF printing, fuzz targets, performance budgets, release workflow, install scripts, documentation. Not done: code signing (a product decision), npm and crates.io packages, Homebrew, an accessibility audit |

## Repository

```
crates/
  dedalo-syntax/       lexer, parser, lossless syntax tree, formatter
  dedalo-catalog/      element types; the built-in catalog is a pack file
  dedalo-model/        document model, lowering, templates, diagnostics, schemas
  dedalo-geom/         shapes, layout, walls, openings, validation
  dedalo-scene/        scene IR, byte-stable SVG, bundled fonts
  dedalo-render/       audience-filtered view, painters, the classic theme
  dedalo-export/       text map, PNG (resvg), PDF (krilla)
  dedalo-ops/          edit operations on the syntax tree
  dedalo-plugin/       registry, manifests, packs, the WebAssembly host
  dedalo-core/         the facade every host calls; completion and hover
  dedalo-cli/          the `dedalo` binary and the MCP server
  dedalo-wasm/         the core module for the browser, with a plugin host
  dedalo-wasm-export/  the browser's PNG and PDF module
  dedalo-pdk/          the plugin development kit
plugins/               blueprint, hand_drawn and uvtt
web/                   the web editor, reference pages and browser checks
tests/corpus/          golden inputs and expected outputs, byte for byte
tests/perf/            50- and 100-room performance samples
eval/                  the agent evaluation suite
fuzz/                  cargo-fuzz targets
docs/                  specification, guide, tutorial, cookbook, plugin guide
```

## Building and testing

The toolchain is pinned in `rust-toolchain.toml`.

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run --release -p dedalo-core --example budgets
```

The golden corpus records, for every input, the diagnostics, document and
resolved JSON, text maps, SVG bytes and PNG, PDF and plugin-output hashes.
After a reviewed change of output: `DEDALO_BLESS=1 cargo test --workspace`.
Tests build the reference plugins themselves.

### The web editor

```
cargo build -p dedalo-wasm -p dedalo-wasm-export -p dedalo-plugin-hand-drawn -p dedalo-plugin-uvtt \
  --target wasm32-unknown-unknown --profile wasm
cp target/wasm32-unknown-unknown/wasm/dedalo_wasm.wasm web/dedalo.wasm
cp target/wasm32-unknown-unknown/wasm/dedalo_wasm_export.wasm web/dedalo-export.wasm
cp target/wasm32-unknown-unknown/wasm/dedalo_plugin_hand_drawn.wasm plugins/hand_drawn/hand_drawn.wasm
cp target/wasm32-unknown-unknown/wasm/dedalo_plugin_uvtt.wasm plugins/uvtt/uvtt.wasm
cargo run -p dedalo-cli -- plugin list --plugin plugins --json > plugins/index.json
cargo run -p dedalo-cli -- skill export web/skill --llms-txt web/llms.txt
python web/serve.py           # then open http://127.0.0.1:8731/web/
```

`bash web/run-browser-check.sh golden.html` runs the whole corpus through
the WebAssembly modules in headless Chrome and compares every output with
the native bytes; `selftest.html` drives the editor like a user. The editor
is plain JavaScript with no build step, a departure from the Vite, Svelte and
CodeMirror stack of spec section 4: it keeps the site dependency-free and
offline, and all language knowledge still comes from the core.

### Plugins

See [Writing plugins](docs/plugins.md). `dedalo plugin check PATH` runs the
conformance suite.

## Decisions waiting for the product owner

Specification section 19.2 lists ten open questions. The code follows the
document's proposals meanwhile: Rust, Y growing south, stairs as marked
exits, black-on-white `classic`, English labels, the extension `.dedalo`,
unsigned binaries. The crates declare `MIT OR Apache-2.0`, as proposed, but
the repository has no licence file until that choice is confirmed. The
bundled Noto Sans fonts are under the SIL Open Font License
(`crates/dedalo-scene/fonts/OFL.txt`).

The MCP server is hand-written, the fallback of spec risk 6, rather than
built on the rmcp SDK: it answers both the stateless requests of revision
2026-07-28 and the `initialize` handshake of earlier revisions.
