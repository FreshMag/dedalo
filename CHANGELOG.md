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
- Multi-level dungeons: `level "id" { }` blocks hold the rooms and corridors
  of one floor, each laid out on its own. Stairs, ladders and trapdoors join
  levels, and `to_position` marks where they arrive. An export draws one
  level with `level = "id"`.
- Doors on the slanted edges of polygons, with `offset` and `width` measured
  along the edge.
- `placement { bearing = 70 }` puts a door anywhere on a round wall, in
  degrees clockwise from north.
- `render { language = it }` prints default labels in Italian.
- Text that contains `${` or `%{` survives conversion from JSON to the DSL:
  it is written with an escape. Before, the value was lost.

### Output

- Text is drawn as outlines from the bundled Noto Sans fonts, so maps look
  the same everywhere; `render { text = live }` keeps selectable text.
- PNG output (resvg) and vector PDF output (krilla).
- Room labels avoid the elements and stairs inside the room.
- A map of a multi-level dungeon shows the levels side by side, each under
  its name.
- The text map shows doors on round walls, which a stub passage used to
  cover.
- The legend sample of a trapdoor is a trapdoor; it was drawn as a door.
- Stairs, ladders and trapdoors with a `to` appear among the links of the
  resolved model.

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
- The web editor in English and Italian, with a level picker, and usable
  from the keyboard throughout; see `docs/accessibility.md` for the audit.
  Example and download lists now act through a button beside them.
- `dedalo render --level ID` and `--lang en|it`; the same options on the MCP
  `dedalo_render` tool.
- Edit operations know levels: a level is added, renamed and removed like
  any element, and a room is added to a level with `parent`.
- The round-trip laws are tested on generated documents.
- Published schemas carry their address as `$id`.

### Plugin API (version 1, unstable)

- Packs: themes, catalogs and templates.
- WebAssembly plugins: painters, scene passes, renderers, exporters,
  validators and shapes, with the `dedalo-pdk` development kit.
- `dedalo plugin add | list | remove | new | check`, and `dedalo.lock`.
- Reference plugins `blueprint`, `hand_drawn` and `uvtt`.
- Themes can map a type to an SVG symbol file of the package. The file
  passes a sanitiser that keeps plain geometry only.
- Zipped packages: `dedalo plugin pack` writes a `.dedalopkg`, and
  `dedalo plugin add` installs one from a file or an address, optionally
  checked against `--sha256`. The web editor loads them from a file or an
  address after showing name, origin and hash.
- `dedalo plugin list --json` lists the files of each package, and the web
  editor loads exactly those, so the hash it loads is the hash the index
  states.
- The `view` passed to plugins has `levels` and, when not English,
  `language`; openings, elements and notes carry their `level`.

### Project

- The licence is Apache-2.0.

## 0.0.1

- The language, geometry, SVG rendering, text maps and the command line of
  milestones M0 to M2.
