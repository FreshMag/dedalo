# Writing plugins

A plugin changes what Dedalo knows or how it draws. There are two tiers:

- **Packs** are data: themes, element types and templates. No code.
- **WebAssembly plugins** are sandboxed code: painters, scene passes, a
  renderer, exporters, validators and shape functions.

One package works unchanged in the binary and in the web editor. The plugin
API is version 1; it is unstable until Dedalo 1.0.

## The package

A plugin is a folder with `dedalo-plugin.toml` at its root.

```toml
[plugin]
id = "parchment"          # lowercase, digits and underscores
name = "Parchment"
version = "1.0.0"
api = 1
dedalo = ">=0.2"
license = "MIT"

[theme.default]           # used as `theme = parchment.default`
file = "themes/default.toml"

[catalog]                 # types used as `type = parchment.NAME`
file = "catalog.toml"

[[template]]              # templates used as `using "parchment.NAME"`
file = "templates.dedalo"

[wasm]                    # code, when the plugin has any
module = "parchment.wasm"

[[scene_pass]]
name = "age"

[[painter]]
match = { kind = "opening", type = "door" }
layer = "openings"
name = "door"

[[exporter]]
format = "foundry"        # used as `format = parchment.foundry`
extension = "json"
media_type = "application/json"
needs = ["png"]           # the host renders a PNG and passes it in

[[validator]]
name = "lights"

[[shape]]
name = "cave"             # used as `shape = parchment.cave(7, 12, 9)`

[options]                 # set in `render { plugin "parchment" { age = 2 } }`
age = { type = "number", default = 1.0, min = 0.0, max = 3.0 }
```

Everything a plugin adds is namespaced by its ID, so two plugins never clash.
A document activates a plugin by naming it in its header; loaded plugins it
does not name have no effect on it.

`dedalo plugin new parchment --kind theme|catalog|wasm` writes a starter.

## Themes

A theme is the format of `crates/dedalo-render/classic.toml`: palette,
stroke widths, text sizes and marker symbols. Whatever a theme leaves out
comes from `classic`.

## Code plugins

A code plugin is core WebAssembly for `wasm32-unknown-unknown`. It may import
only these host functions, from the `dedalo` namespace; a module that imports
anything else, WASI included, is refused:

| Import | Signature | Purpose |
| --- | --- | --- |
| `dedalo.log` | `(level: i32, ptr: i32, len: i32)` | A message for the log |
| `dedalo.rand_u64` | `() -> i64` | The next value of the seeded random stream |
| `dedalo.asset` | `(ptr: i32, len: i32) -> i64` | A file from the package |
| `dedalo.measure_text` | `(ptr: i32, len: i32) -> i64` | Text metrics from the bundled fonts |

It exports `memory`, `dedalo_abi` (returning 1), `dedalo_alloc`,
`dedalo_free` and `dedalo_call`. A call receives one JSON request and returns
one JSON response. With the plugin development kit you write only the
handler:

```rust
use dedalo_pdk::{Request, Response, json, plugin};

fn handle(request: Request) -> Response {
    match (request.point.as_str(), request.name.as_str()) {
        ("scene_pass", "age") => {
            let mut scene: dedalo_pdk::Scene = dedalo_pdk::serde_json::from_value(request.input["scene"].clone())
                .map_err(|e| e.to_string())?;
            // ... change the scene ...
            Ok(json!({ "scene": scene }))
        }
        (point, name) => Err(format!("no {point} named `{name}`").into()),
    }
}

plugin!(handle);
```

### Requests and responses

| Point | `input` | `ok` |
| --- | --- | --- |
| `painter` | `item` (with `kind`), `theme`, `cell_px`, `legend`, `audience` | `{ "nodes": [...] }` |
| `scene_pass` | `scene`, `cell_px`, `audience` | `{ "scene": ... }` |
| `renderer` | `view`, `theme` | `{ "scene": ... }` |
| `exporter` | `view`, `scene`, and `svg` or `png` (base64) when declared in `needs` | `{ "text": ... }` or `{ "base64": ... }` |
| `validator` | `resolved` | `{ "diagnostics": [{ "message", "element", "severity", "suggestion" }] }` |
| `shape` | `args` | `{ "points": [[x, y], ...] }`, in cells |

A failure is `{ "error": { "message": "...", "element": "id" } }`.

Scene nodes are `path`, `group` and `text`, in cells; see
`crates/dedalo-scene/src/lib.rs`. Responses are validated: malformed nodes,
more than 200,000 nodes, or coordinates beyond a million cells are refused.

### Rules

- **Deterministic.** The same request must give the same response. Use
  `rand_u64`, never a clock; each call runs in a fresh instance.
- **Bounded.** A call has a fuel budget and 64 MiB of memory in the binary;
  in the browser a watchdog stops calls that take too long.
- **No secrets.** Painters, passes, renderers and exporters receive only what
  the export's audience may see. Element IDs reach player maps nowhere: the
  host removes them from plugin output too.

## Checking and installing

```
dedalo plugin check path/to/parchment      # the conformance suite
dedalo plugin add path/to/parchment        # into .dedalo/plugins, recorded in dedalo.lock
dedalo render map.dedalo --plugin path/to/parchment
```

`dedalo plugin check` loads the package, runs every theme and exporter on an
example dungeon twice, and fails on any difference or on element IDs in a
player map. The lockfile records each plugin's hash; a changed package is
refused until `--update-lock`.

In the web editor, the Plugins panel loads reviewed plugins from the site's
index, or a package folder from your disk after you confirm.

## Reference plugins

`plugins/` holds three: `blueprint` (a theme pack), `hand_drawn` (a scene
pass, a door painter and a theme) and `uvtt` (an exporter to Universal VTT).
