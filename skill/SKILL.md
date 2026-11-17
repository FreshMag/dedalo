---
name: dedalo
description: Author, validate, edit and render tabletop-RPG dungeon maps with the Dedalo DSL and the `dedalo` command or MCP server. Use when asked to design a dungeon, draw a battle map, or change a `.dedalo` file.
---

# Dedalo

Dedalo compiles a text description of a dungeon into a map: SVG, PNG, PDF or a text map. Prefer relative room placement (`north_of`), wall and anchor placement for elements, and edit operations over regenerating files.

## Working loop

1. Read `references/quickstart.md` once.
2. Write the file, then run `dedalo validate FILE --json` (MCP: `dedalo_validate`).
3. Apply the `fix` of each diagnostic: `dedalo validate FILE --fix`, or send it to `dedalo edit FILE --ops -` (MCP: `dedalo_edit`).
4. Read the result as text: `dedalo inspect FILE --text-map` (MCP: `dedalo_inspect` with `view = text_map`).
5. Render: `dedalo render FILE --preset player --format png` (MCP: `dedalo_render`).
6. Make further changes with small edit operations addressed by element ID rather than rewriting the file.

## References

- `references/quickstart.md`: the language in one page
- `references/syntax.md`: values, blocks, connections, visibility, exports
- `references/layout.md`: coordinates, shapes, placement, doors, corridors
- `references/catalog.md`: element types; `references/catalog/<type>.md` for properties
- `references/errors.md`: diagnostic codes and fixes
- `references/examples/<name>.dedalo`: complete examples
