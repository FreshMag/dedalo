# Dedalo for Obsidian

Write a dungeon in a note, between ```` ```dedalo ```` and ```` ``` ````, and
Obsidian draws the map in its place, in reading view and in live preview. It
runs the same WebAssembly core as the web editor and the `dedalo` command, so
the maps are the same, and it works offline.

````markdown
## The forgotten crypt

```dedalo player
dungeon "crypt" {
    name = "The Forgotten Crypt"

    room "entrance" {
        name  = "Entrance"
        shape = rectangle(8, 6)
    }
}
```
````

## Install

From a release: download `dedalo-obsidian.zip`, unzip it into the
`.obsidian/plugins` folder of the vault (it makes `.obsidian/plugins/dedalo`),
then enable **Dedalo** under Settings, Community plugins. A tool that installs
plugins from GitHub releases, such as BRAT, can use `FreshMag/dedalo`.

From this repository:

```
cargo build -p dedalo-wasm --target wasm32-unknown-unknown --profile wasm
python integrations/obsidian/build.py --install PATH/TO/VAULT
```

## Using it

- **Options after `dedalo`** choose how a block is drawn: `player`, `dm` or
  `clean`; `level=upper`; `export=player_handout`; `rotation=90`;
  `lang=it`; `grid=false`, `labels=false`, `notes=false`. A value with spaces
  goes in quotes: `level="ground floor"`.
- **The toolbar** appears over a map: switch who it is for (DM, Players,
  Clean), pick a level, open it large (scroll to zoom, drag to move), copy it
  as an image, or save it as PNG or SVG where the vault keeps attachments.
- **Problems** are listed under the map with the note's line numbers; click a
  number to go to the line. While a block has errors, the last part that
  could be drawn is shown faded.
- **Commands**: *Insert a dungeon map*, *Format the dungeon map under the
  cursor*, and *Switch every map between DM and players*, handy before
  sharing the screen with the table.

## Settings

| Setting | |
| --- | --- |
| Default audience | DM, Players or Clean, for blocks that do not choose |
| Label language | Legend and heading language: as Obsidian, English or Italian |
| Maps in dark themes | Paper (black on white) or Inverted |
| Largest map height | Taller maps are scaled down; Open large shows them whole |
| Always show the map toolbar | Otherwise it appears on hover |
| Dedalo plugin folder | A vault folder with Dedalo plugin packages (`.dedalopkg` files or package folders), such as the `hand_drawn` theme |

## With an AI agent in the vault

The `dedalo` command and its MCP server read and write these blocks
directly, leaving the rest of a note alone, and report problems with the
note's line numbers:

```
dedalo validate "Adventures/Crypt.md"             # every block of the note
dedalo validate "Adventures/Crypt.md" --fix
dedalo render "Adventures/Crypt.md" --block 2 --format png -o crypt.png
dedalo fmt "Adventures/Crypt.md"
dedalo skill export .claude/skills/dedalo         # teach an agent the language
```

`dedalo guide notes` describes the fence options and the working loop for an
agent. With the MCP server (`dedalo mcp --root PATH/TO/VAULT`), `dedalo_write`
with a note's path adds or replaces its `dedalo` block.

## Testing

`bash web/run-browser-check.sh ../integrations/obsidian/selftest.html` runs
the built plugin in headless Chrome against a stand-in for Obsidian's API,
and checks that its maps are the golden files byte for byte.
