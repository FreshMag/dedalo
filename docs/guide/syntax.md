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
| `room` | ID | `dungeon` | A space with a `shape` |
| `corridor` | ID | `dungeon` | Joins two spaces: `connects = ["a", "b"]` |
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
and may omit `to`.

```hcl
connection "vault_door" {
    from = "entrance"
    to = "vault"
    type = door
    placement { wall = north, offset = 3, width = 2 }
}
```

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
    revealed = ["pit_found"]
    include = ["vault_door"] # shown although secret
    visibility { grid = false }
}
```

`dedalo render FILE --all` writes every export. A PDF export with
`scale = tabletop` prints one cell per inch, tiled across `page`-sized sheets.

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

`dedalo plugin add PATH` installs a plugin package and records it in
`dedalo.lock`; `--plugin PATH` loads one for a single command.
