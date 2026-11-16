# Dedalo

Dedalo compiles a text description of a tabletop-RPG dungeon into a map. One
Rust core does all the work; a command-line binary and a static web page are
thin hosts around it, so they cannot disagree.

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
cargo run -p dedalo-cli -- render crypt.dedalo                  # DM map, SVG
cargo run -p dedalo-cli -- render crypt.dedalo --preset player  # what the players may see
cargo run -p dedalo-cli -- inspect crypt.dedalo --text-map      # the map as text
cargo run -p dedalo-cli -- validate crypt.dedalo --json         # diagnostics with fixes
```

The full design is in [docs/specification.md](docs/specification.md). Start
with `dedalo guide` for the language in one page.

## Status

Work follows the milestones of specification section 17.

| Milestone | State |
| --- | --- |
| M0 Walking skeleton | Done, except that CI and the Pages deployment have not run yet (no remote), and the PNG and PDF build spike is open |
| M1 Language | Done: lossless parser with recovery, formatter, document model, JSON both ways, catalog, diagnostics, `validate`, `fmt`, `convert`, `schema`, `catalog`, `explain`. `guide` has the quickstart only; property tests over generated documents and the `cargo fuzz` targets are open |
| M2 Geometry | Mostly done: four shapes, absolute, relative and automatic placement, quarter-turn rotation, doors, stubs, straight and path corridors, walls, elements, reachability, `inspect`, text map. Open: `bearing` and non-orthogonal polygon edges for openings; the corpus has 4 layout cases of the 30 asked for |
| M3 Rendering and export | Started: audience filter, presets, toggles, scene, SVG, `classic` theme, grid, labels, light halos, legend, notes, rotation. Open: PNG, PDF, text as outlines (needs bundled fonts), theme symbol files, `render --watch` |
| M4 Web editor | Not started. `web/` holds the bare page of M0 |
| M5 Agent interface | Not started. Edit-operation types exist and diagnostics carry fixes; the engine, `dedalo edit` and the MCP server are open |
| M6, M7 | Not started |

## Repository

```
crates/
  dedalo-syntax/    lexer, parser, lossless syntax tree, formatter, text edits
  dedalo-catalog/   element type registry and the built-in catalog (TOML)
  dedalo-model/     document model, diagnostics, JSON Schema, DSL <-> JSON
  dedalo-geom/      shapes, layout, walls, openings, geometric validation
  dedalo-scene/     scene IR and the byte-stable SVG serialiser
  dedalo-render/    audience-filtered view, painters, the classic theme
  dedalo-export/    text map (PNG and PDF to come)
  dedalo-plugin/    registry of catalog, themes and painters
  dedalo-core/      the facade every host calls
  dedalo-cli/       the `dedalo` binary
  dedalo-wasm/      the core as a WebAssembly module
  dedalo-ops/       edit operations (skeleton)
  dedalo-pdk/       plugin development kit (skeleton)
web/                the bare web page and the cross-target golden check
spec/               grammar and generated JSON Schemas
tests/corpus/       golden inputs and expected outputs
docs/               specification, guide, decision records
```

## Building and testing

The toolchain is pinned in `rust-toolchain.toml`.

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The golden corpus under `tests/corpus/` records, for every input, the
diagnostics, the document and resolved JSON, the text maps and the SVG bytes.
Tests compare them byte for byte. After a reviewed change of output:

```
DEDALO_BLESS=1 cargo test --workspace
```

### The web page and the cross-target check

```
cargo build -p dedalo-wasm --target wasm32-unknown-unknown --profile wasm
cp target/wasm32-unknown-unknown/wasm/dedalo_wasm.wasm web/dedalo.wasm
python web/serve.py            # then open http://127.0.0.1:8731/web/
```

`/web/golden.html` runs the whole corpus through the WebAssembly build in the
browser and compares every output with the bytes the native build recorded;
`node web/golden.mjs` does the same in Node. This is how identical output
across targets is enforced.

## Decisions waiting for the product owner

Specification section 19.2 lists ten open questions. Until they are answered
the code follows the document's proposals: Rust, Y growing south, stairs drawn
as marked exits, black-on-white `classic`, English labels, the extension
`.dedalo`. The licence fields say `MIT OR Apache-2.0`, as proposed; no licence
file is included until that choice is confirmed.
