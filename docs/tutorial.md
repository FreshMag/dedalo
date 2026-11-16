# Tutorial: your first dungeon

This tutorial builds a three-room crypt, checks it, and prints a map for the
game master and one for the players. It takes about fifteen minutes. You need
the `dedalo` binary, or the web editor, where everything below works the same.

## 1. One room

Create `crypt.dedalo`:

```hcl
dungeon "crypt" {
    name = "The Crypt"

    room "entrance" {
        name = "Entrance"
        shape = rectangle(8, 6)
    }
}
```

Check it and look at it as text:

```
dedalo validate crypt.dedalo
dedalo inspect crypt.dedalo --text-map
```

The text map draws one character per cell: `#` for walls, `.` for floor.
Coordinates count cells; X grows east and Y grows south.

## 2. A second room, placed by relation

Rather than computing coordinates, say where a room lies relative to another.
Add a guard room north of the entrance and a door between them:

```hcl
dungeon "crypt" {
    name = "The Crypt"

    room "entrance" {
        name = "Entrance"
        shape = rectangle(8, 6)
    }

    room "guard_room" {
        name = "Guard Room"
        shape = square(6)
        layout { north_of = "entrance" }
    }

    connection "entrance_door" {
        from = "entrance"
        to = "guard_room"
        type = door
    }
}
```

The door is centred on the wall the two rooms share. To move it, add
`placement { wall = north, offset = 2 }`: the offset counts cells from the
west end of the wall.

## 3. Things in rooms

Elements live in rooms. A trap, a torch on the south wall, and three
skeletons:

```hcl
dungeon "crypt" {
    name = "The Crypt"

    room "entrance" {
        name = "Entrance"
        shape = rectangle(8, 6)

        feature "plate" {
            type = trap
            subtype = pressure_plate
            position = [4, 3]

            properties {
                detection_dc = 15
            }
        }

        feature "torch" {
            type = lighting
            subtype = torch
            placement { wall = south, offset = 2 }
        }
    }

    room "guard_room" {
        name = "Guard Room"
        shape = square(6)
        layout { north_of = "entrance" }

        entity "skeletons" {
            type = creature
            reference = "srd:skeleton"
            placement { anchor = center }

            properties {
                count = 3
            }
        }
    }

    connection "entrance_door" {
        from = "entrance"
        to = "guard_room"
        type = door
    }
}
```

`dedalo catalog` lists the types; `dedalo catalog trap` lists the properties
of one. A typo such as `detecton_dc` is reported with the name it was
probably meant to be.

## 4. A secret

Add a vault east of the guard room, hidden from the players, behind a secret
door:

```hcl
dungeon "crypt" {
    name = "The Crypt"

    room "entrance" {
        name = "Entrance"
        shape = rectangle(8, 6)
    }

    room "guard_room" {
        name = "Guard Room"
        shape = square(6)
        layout { north_of = "entrance" }
    }

    room "vault" {
        name = "Vault"
        shape = square(5)
        visibility = dm
        layout { east_of = "guard_room", align = start }

        feature "chest" {
            type = container
            placement { anchor = center }
        }
    }

    connection "entrance_door" {
        from = "entrance"
        to = "guard_room"
        type = door
    }

    connection "vault_door" {
        from = "guard_room"
        to = "vault"
        type = secret_door
    }
}
```

## 5. Two maps

```
dedalo render crypt.dedalo --preset dm --format png
dedalo render crypt.dedalo --preset player --format png
```

The player map has no vault, no secret door, no trap and no skeletons: the
filtering happens before anything is drawn, so nothing hidden can leak into
the file. To keep both versions in the file, declare exports and render them
all at once with `dedalo render crypt.dedalo --all`:

```hcl
dungeon "crypt" {
    room "entrance" {
        shape = rectangle(8, 6)
    }

    export "dm" {
        preset = dm
        format = pdf
        page = a4
    }

    export "players" {
        preset = player
        format = png
        cell_px = 70
    }
}
```

## 6. When something is wrong

Change the door to `placement { wall = north, offset = 9 }` and validate. The
error names the door, the offset you gave and the offsets that work, and it
carries a fix. `dedalo validate crypt.dedalo --fix` applies it.

## Next

- [The cookbook](cookbook.md) has recipes for common layouts.
- `dedalo guide layout` explains placement in full.
- [Writing plugins](plugins.md) shows how to change the look.
