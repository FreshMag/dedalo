# Changelog

Each release lists language changes, output changes and plugin API changes
(spec 16.4).

## Unreleased

### Language

- `template` blocks and `using "name"` on rooms, corridors, connections,
  features and entities; packs can provide templates as `pluginid.name`.
- Shapes `ellipse(width, height)` and `l_shape(width, height, cutout)`.
- Export attribute `scale = tabletop`: PDF at one cell per inch, tiled.
- Plugin requirements in the `dedalo` header are enforced, with version
  ranges such as `^1.2`.
- New diagnostic code `PLUGIN_RULE`, for problems reported by validator
  plugins.

### Output

- Text is drawn as outlines from the bundled Noto Sans fonts, so maps look
  the same everywhere; `render { text = live }` keeps selectable text.
- PNG output (resvg) and vector PDF output (krilla).
- Room labels avoid the elements and stairs inside the room.

### Agents and tools

- Edit operations addressed by element ID: `dedalo edit`, and
  `dedalo validate --fix` to apply diagnostic fixes.
- The MCP server, `dedalo mcp`, over stdio and Streamable HTTP.
- `dedalo guide` topics, `dedalo skill export`, `llms.txt`, and the
  `authoring` profile of the document schema.
- `dedalo render --watch`.
- The web editor: completion, hover, diagnostics with quick fixes, outline,
  pan and zoom, click-to-source, downloads in every format, share links,
  offline use, and a plugins panel.

### Plugin API (version 1, unstable)

- Packs: themes, catalogs and templates.
- WebAssembly plugins: painters, scene passes, renderers, exporters,
  validators and shapes, with the `dedalo-pdk` development kit.
- `dedalo plugin add | list | remove | new | check`, and `dedalo.lock`.
- Reference plugins `blueprint`, `hand_drawn` and `uvtt`.

## 0.0.1

- The language, geometry, SVG rendering, text maps and the command line of
  milestones M0 to M2.
