# Cookbook

Short recipes, each a complete file you can render.

## Rooms around a hub

A circular hub with a room on each side; circles take doors at their compass
points.

```hcl
dungeon "hub" {
    room "hub" {
        shape = circle(3)
    }

    room "north" {
        shape = rectangle(4, 3)
        layout { north_of = "hub" }
    }

    room "east" {
        shape = rectangle(3, 4)
        layout { east_of = "hub" }
    }

    connection "to_north" {
        from = "hub"
        to = "north"
        type = archway
    }

    connection "to_east" {
        from = "hub"
        to = "east"
        type = door
    }
}
```

## A long corridor with a bend

Path points are lattice points along the centre line; a corridor one cell
wide runs along half-cell coordinates.

```hcl
dungeon "bend" {
    room "a" {
        shape = square(4)
        layout { position = [0, 0] }
    }

    room "b" {
        shape = square(4)
        layout { position = [10, 8] }
    }

    corridor "hall" {
        connects = ["a", "b"]
        path = [[4, 1.5], [6.5, 1.5], [6.5, 9.5], [10, 9.5]]
    }
}
```

## Rooms a short step apart

Facing walls up to two cells apart get a stub passage under the door, so a
gap needs no corridor.

```hcl
dungeon "gap" {
    room "a" {
        shape = rectangle(6, 4)
    }

    room "b" {
        shape = rectangle(6, 4)
        layout { east_of = "a", gap = 1 }
    }

    connection "door" {
        from = "a"
        to = "b"
        type = door
    }
}
```

## A trap the party has found

`conditional` elements appear on player maps only in exports that list their
tag in `revealed`.

```hcl
dungeon "pit" {
    room "hall" {
        shape = rectangle(8, 4)

        feature "pit" {
            type = trap
            subtype = pit
            position = [3, 1]
            visibility = conditional
            reveal_when = "pit_found"
        }
    }

    export "before" {
        preset = player
    }

    export "after" {
        preset = player
        revealed = ["pit_found"]
    }
}
```

## A map without labels or grid

The `clean` preset draws geometry and doors only; `visibility` toggles turn
single categories off in any preset.

```hcl
dungeon "plain" {
    room "a" {
        name = "Hall"
        shape = rectangle(8, 6)

        feature "table" {
            type = furniture
            subtype = table
            position = [3, 2]
        }
    }

    export "bare" {
        preset = clean
    }

    export "no_grid" {
        preset = player

        visibility {
            grid = false
        }
    }
}
```

## A legend in another language

Legend entries override labels, which is how a map is localised.

```hcl
dungeon "cripta" {
    legend {
        title = "Legenda"

        entry "door" {
            label = "Porta"
        }

        entry "trap" {
            label = "Trappola"
        }
    }

    room "ingresso" {
        name = "Ingresso"
        shape = rectangle(6, 4)

        feature "piastra" {
            type = trap
            position = [2, 2]
        }
    }
}
```

## Printing at table scale

One cell per inch on A4 sheets, for miniatures:

```hcl
dungeon "table" {
    room "arena" {
        shape = rectangle(12, 9)
    }

    export "print" {
        preset = player
        format = pdf
        page = a4
        scale = tabletop
    }
}
```

## Editing from a script

Edit operations change one element by ID and leave everything else as it
was, comments included:

```
echo '[{ "op": "update", "id": "door", "set": { "placement.offset": 2 } }]' \
  | dedalo edit gap.dedalo --ops -
```

`--dry-run` shows the diff without writing; `--base-hash` refuses the edit if
someone else changed the file in the meantime.
