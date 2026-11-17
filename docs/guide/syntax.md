# Dedalo syntax

A file holds an optional `dedalo` header block and one `dungeon` block. The
syntax is a strict subset of HCL: blocks, attributes, literals, lists,
objects and shape calls. There are no expressions and no templates.

```hcl
dedalo {
    version = 1
}

dungeon "crypt" {
    name = "The Crypt"

    grid {
        unit = "5ft"
    }

    settings {
        default_wall = stone
        seed = 7
    }

    room "entrance" {
        name = "Entrance"
        shape = rectangle(8, 6)
        tags = ["entrance"]

        properties {
            description = <<-EOT
                Cold air rises from the stairs.
                EOT
        }
    }
}
```

## Values

| Type | Written as | Example |
| --- | --- | --- |
| Number | Decimal, at most three decimals | `width = 2`, `offset = 1.5` |
| String | Double quotes, or a heredoc | `name = "Ingresso"` |
| Boolean | `true`, `false` | `locked = true` |
| Symbol | Bare word; dotted when from a plugin | `type = door`, `wall = north` |
| Reference | Quoted element ID | `from = "entrance"` |
| Point | Two numbers | `position = [3, 2]` |
| List | Brackets | `tags = ["combat"]` |
| Shape | A call | `shape = circle(3)` |

`null` unsets an optional attribute. Comments are `#`, `//` and `/* */`.
Escapes in strings: `\n`, `\t`, `\"`, `\\`, `\uXXXX`. The sequences `${` and
`%{` are not allowed. A one-line block holds one attribute:
`layout { north_of = "entrance" }`.

## Blocks

| Block | Label | Inside | Purpose |
| --- | --- | --- | --- |
| `level` | ID | `dungeon` | One floor of a multi-level dungeon: a `name`, rooms and corridors |
| `room` | ID | `dungeon`, `level` | A space with a `shape` |
| `corridor` | ID | `dungeon`, `level` | Joins two spaces: `connects = ["a", "b"]` |
| `connection` | ID | `dungeon` | A door or passage: `from`, `to`, `type` |
| `feature` | ID | `dungeon`, `room`, `corridor` | Fixed element: furniture, trap, light, terrain |
| `entity` | ID | `dungeon`, `room`, `corridor` | Creature, NPC or item |
| `note` | ID | anywhere | Annotation: `text`, optional `target`, `position` |
| `layout` | none | `room` | Placement of the room |
| `placement` | none | `connection`, `feature`, `entity` | Wall or anchor placement |
| `properties` | none | any element | Typed game data from the catalog |
| `render`, `legend` | none | `dungeon`, `export` | Theme, labels and legend |
| `export` | optional name | `dungeon` | One output configuration |

## IDs

An ID is lowercase letters, digits and underscores, starting with a letter,
at most 64 characters. All IDs share one namespace per dungeon: a room and a
feature cannot share an ID. References may point forward.

## Connections

`type` is one of `door`, `archway`, `open`, `stairs`, `ladder`, `trapdoor`.
`locked_door` and `secret_door` are accepted and mean `door` with
`properties { locked = true }` or `secret = true`. A secret door is hidden
from players. Stairs, ladders and trapdoors take a `position` inside `from`
and may omit `to`. With a `to`, `to_position` is the cell in that room where
they arrive, drawn as a second marker.

```hcl
connection "vault_door" {
    from = "entrance"
    to = "vault"
    type = door
    placement { wall = north, offset = 3, width = 2 }
}
```

## Levels

A dungeon on several floors groups its rooms and corridors in `level`
blocks. Each level is laid out on its own: placement, corridors and doors
stay inside one level, and only stairs, ladders and trapdoors join two. A
dungeon with levels keeps every room and corridor in one of them;
connections, notes and exports stay in the `dungeon` block.

```hcl
dungeon "tower" {
    level "ground" {
        name = "Ground floor"

        room "hall" {
            shape = rectangle(8, 6)
        }
    }

    level "upper" {
        name = "Upper floor"

        room "study" {
            shape = circle(3)
        }
    }

    connection "spiral" {
        from = "hall"
        to = "study"
        type = stairs
        position = [6, 1]
        to_position = [1, 3]
    }

    export "upstairs" {
        preset = player
        level = "upper"
    }
}
```

A map shows every level side by side, each under its name. An export with
`level`, or `dedalo render --level upper`, draws one. A level with nothing
an audience may see is left out of that audience's map, name included.

## Visibility

`visibility` is `public`, `dm`, `conditional` (with `reveal_when = "tag"`) or
`hidden`. Traps, creatures, items, chests and notes default to `dm`. A room
with `visibility = dm` disappears from player maps together with its doors.

## Exports

```hcl
export "players" {
    preset = player          # dm, player or clean
    format = png             # svg, png, pdf or txt
    cell_px = 70
    rotation = 90
    level = "upper"          # one level of a multi-level dungeon
    revealed = ["pit_found"]
    include = ["vault_door"] # shown although secret
    visibility { grid = false }
}
```

`dedalo render FILE --all` writes every export. A PDF export with
`scale = tabletop` prints one cell per inch, tiled across `page`-sized sheets.

## Language

The words a map prints by itself are English or Italian: the legend labels
of the built-in types, the headings `Legend` and `Notes`, and the scale
line. Names, notes and custom labels are yours and are never translated.

```hcl
dungeon "cantina" {
    render {
        language = it        # en (default) or it
    }

    room "cantina" {
        name = "Cantina"
        shape = rectangle(6, 4)
    }
}
```

`render` inside an `export` overrides it for that export, and
`dedalo render --lang it` for one command. A legend `entry` block still
replaces any single label.

## Templates

A template holds defaults for blocks that use it. Values the block sets win;
`properties` merge. Templates sit at the top of the file or in the dungeon,
and packs can provide them as `pluginid.name`.

```hcl
dungeon "barracks" {
    template "iron_door" {
        type = door

        placement {
            width = 2
        }

        properties {
            locked = true
        }
    }

    room "west" {
        shape = rectangle(6, 4)
    }

    room "east" {
        shape = rectangle(6, 4)
        layout { east_of = "west" }
    }

    connection "between" using "iron_door" {
        from = "west"
        to = "east"
    }
}
```

## Plugins

A plugin adds themes, element types, templates, painters, scene passes,
exporters, validators or shapes. Require it in the header and use its names
with its ID as prefix:

```hcl
dedalo {
    version = 1

    plugin "hand_drawn" {
        version = "^1.2"
    }
}

dungeon "sketch" {
    render {
        theme = hand_drawn.sepia
    }

    room "hall" {
        shape = rectangle(6, 4)
    }
}
```

`dedalo plugin add SOURCE` installs a plugin package and records it in
`dedalo.lock`. The source is a package folder, a zipped `.dedalopkg` file,
or the address of one; `--sha256 HASH` refuses a package with another hash.
`--plugin PATH` loads a folder or a `.dedalopkg` for a single command.
