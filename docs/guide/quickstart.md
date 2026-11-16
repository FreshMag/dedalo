# Dedalo quickstart

Dedalo compiles a text description of a dungeon into a map. A file has the
extension `.dedalo` and holds one `dungeon` block.

```hcl
dungeon "crypt" {
    name = "The Crypt"

    room "entrance" {
        name = "Entrance"
        shape = rectangle(8, 6)

        feature "torch" {
            type = lighting
            subtype = torch
            placement { wall = south, offset = 2 }
        }
    }

    room "vault" {
        shape = square(6)
        layout { north_of = "entrance" }

        entity "skeletons" {
            type = creature
            position = [1, 1]
            properties { count = 3 }
        }
    }

    connection "vault_door" {
        from = "entrance"
        to = "vault"
        type = door
    }
}
```

## The working loop

1. Write or change the file.
2. `dedalo validate FILE` and apply the fixes it proposes. Every diagnostic
   names the element, the value found and the values allowed.
3. `dedalo inspect FILE --text-map` to read the result as text.
4. `dedalo render FILE` to write the map. `--preset player` hides everything
   the players must not see.

## Rules worth knowing

- Coordinates are cells. X grows east, Y grows south, north is up.
- Place rooms relative to each other (`north_of`, `south_of`, `east_of`,
  `west_of`, with `align` and `offset`) rather than by absolute position:
  no arithmetic, fewer mistakes. The first room is placed at `[0, 0]`.
- A `position` inside a room counts cells from 0 at the room's top-left
  corner. `placement { wall = north, offset = 2 }` and
  `placement { anchor = center }` avoid counting altogether.
- A connection joins two rooms that share a wall. Rooms that do not touch
  are joined by a `corridor "id" { connects = ["a", "b"] }`.
- IDs are lowercase with digits and underscores, and unique in the dungeon.
- Game data goes in `properties { }`. `dedalo catalog TYPE` lists the
  properties of a type; a name starting with `x_` is always accepted.
- Traps, creatures, items and notes are visible to the DM only by default.
  `visibility = dm` hides a whole room; `type = secret_door` hides a door.

## More

- `dedalo catalog` lists element types and subtypes.
- `dedalo explain CODE` describes a diagnostic code with a worked fix.
- `dedalo guide examples/forgotten_crypt` prints a complete example.
- `dedalo schema document` prints the JSON Schema of the document model;
  `dedalo convert FILE --to json` switches between the two spellings.
