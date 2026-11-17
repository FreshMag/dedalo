# Dungeons in Markdown notes

A dungeon can live inside a Markdown note, such as a page of an Obsidian
vault, as a fenced code block whose language is `dedalo`. The Dedalo plugin
for Obsidian draws each block as a map where it stands. The `dedalo` command
and the MCP server read and write the block and leave the rest of the note
alone.

````markdown
## The forgotten crypt

The air smells of wet stone.

```dedalo player
dungeon "crypt" {
    name = "The Forgotten Crypt"

    room "entrance" {
        name  = "Entrance"
        shape = rectangle(8, 6)
    }
}
```

Read-aloud text and room descriptions go here, in Markdown.
````

## Fence options

Words after `dedalo` choose how the block is drawn. Flags on the command
line and tool arguments win over them.

| Option | Values | Effect |
| --- | --- | --- |
| `dm`, `player`, `clean` | | The audience; the same as `preset=...` |
| `preset` | `dm`, `player`, `clean` | The audience |
| `export` | an export name | Draw that `export` block of the dungeon |
| `level` | a level ID | Draw one level of a multi-level dungeon |
| `rotation` | `0`, `90`, `180`, `270` | Turn the map clockwise |
| `lang` | `en`, `it` | The language of default legend labels |
| `grid` | `true`, `false` | The grid |
| `labels` | `true`, `false` | Room labels |
| `notes` | `true`, `false` | The notes panel |

A value with spaces goes in double quotes: `level="ground floor"`.

## Several blocks in one note

Blocks are counted from 1 in reading order. Choose one with `--block N` on
the command line, or `block` in an MCP tool. `validate` and `fmt` take every
block when none is chosen; the other commands need a choice when the note
has more than one block. Diagnostics give the note's own line numbers.

A second block can show another part of the same adventure, but each block
is a whole dungeon: blocks do not share rooms. To show one dungeon to the DM
and to the players, use one block and switch the audience in Obsidian (each
map has a switch), or write two blocks with the same dungeon and different
options.

## Working in a note

1. Write the dungeon into the note. MCP: `dedalo_write` with the note's
   path replaces the note's only block, or the one given as `block`, and
   adds a block at the end when the note has none; `fence` sets the new
   block's options, such as `player`. By hand: write the fence and the
   dungeon with any editor.
2. `dedalo validate Note.md --json` (MCP: `dedalo_validate`) checks every
   block.
3. Fix: `dedalo validate Note.md --fix`, or edit by ID with
   `dedalo edit Note.md --block 2 --ops -` (MCP: `dedalo_edit` with `block`).
4. Look at it: `dedalo inspect Note.md --text-map`, or render a picture to
   check the layout: `dedalo render Note.md --format png -o map.png`.
5. `dedalo fmt Note.md` lays the blocks out canonically.

Keep the story in Markdown around the block, and inside the dungeon only
what belongs on the map: names, short `description`s, and `note` blocks for
numbered call-outs.
