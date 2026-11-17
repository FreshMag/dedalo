# Dedalo layout

Coordinates count cells. X grows east and Y grows south; north is the top of
the page. Rooms are polygons on the grid, walls are their edges, and doors
are openings cut where two rooms touch.

## Shapes

| Shape | Meaning |
| --- | --- |
| `square(side)` | Square |
| `rectangle(width, height)` | Rectangle |
| `circle(radius)` | Circle in a `2r` by `2r` box |
| `ellipse(width, height)` | Ellipse in a `width` by `height` box |
| `l_shape(width, height, cutout)` | Rectangle with its north-east corner cut away; `cutout` is a side or `[width, height]` |
| `polygon([[x, y], ...])` | Simple polygon, any winding, edges of any direction |

Doors go on any edge of a polygon, slanted ones included. Circles and
ellipses take doors at their four compass points, or anywhere with a
`bearing`. See Doors below.

`rotation = 90` turns a room clockwise in quarter turns; its contents and
wall names turn with it.

## Placing rooms

Prefer relative placement: no arithmetic.

```hcl
room "vault" {
    shape = square(6)
    layout {
        north_of = "entrance"   # also south_of, east_of, west_of
        align = start           # start, center (default) or end
        offset = 1              # shift along the shared side, in cells
        gap = 0                 # space between the two rooms
    }
}
```

- `align = start` lines up the west edges (north or south placement) or the
  north edges (east or west placement).
- `layout { position = [x, y] }` fixes the top-left corner on the map.
- The first room is placed at `[0, 0]` unless some room has a position.
- A room with no `layout` goes next to a placed room it has a connection or
  corridor with, on the side its door names, else the first free side.
- Rooms may touch but not overlap. An `OVERLAP` error proposes the smallest
  `offset` that clears it; `align` often fixes it too.

## Doors

A connection needs the two spaces to share a wall. `placement.wall` is the
side of `from`; `offset` counts cells from the west end of a north or south
wall, or the north end of an east or west wall. Without `offset` the door is
centred on the shared part. Walls up to `settings.max_stub` cells apart
(default 2) get a short stub passage automatically.

On a polygon, `wall` picks the edges that face that way, and
`placement { edge = 3 }` picks one edge by its index, counted from the first
vertex. A door on a slanted edge needs the other room to share that edge.
Its `offset` and `width` are measured along the edge, from its west end, or
from its north end when the edge is steeper than 45 degrees.

On a circle or an ellipse, `placement { bearing = 70 }` puts the door at the
point of the wall 70 degrees clockwise from north; 0 is north and 90 is
east. The bearing turns with the room when the room is rotated. The passage
runs straight out of the nearest compass side, so the other room must face
that part of the wall; when it does not, the error gives the bearings that
work.

```hcl
dungeon "rotunda" {
    room "hub" {
        shape = circle(4)
    }

    room "east" {
        shape = rectangle(3, 6)
        layout { east_of = "hub" }
    }

    connection "side_door" {
        from = "hub"
        to = "east"
        type = door
        placement { bearing = 70 }
    }
}
```

With levels, each level has its own plane: `position` and corridor `path`
points are counted on the level's own map, and the first room of each level
is placed at `[0, 0]`.

## Corridors

```hcl
corridor "hall" {
    connects = ["entrance", "vault"]
    width = 1
}
```

An automatic corridor runs straight between facing walls; when the rooms are
placed relative to each other the gap defaults to the corridor's `length`, or
2. An explicit `path` lists lattice points along the centre line, with
horizontal and vertical segments only; with an odd width use half cells
(`[4, 2.5]`). Both ends must touch the two spaces.

## Elements

```hcl
feature "table" {
    type = furniture
    subtype = table
    position = [3, 2]                     # top-left cell, from 0, in the room
}
feature "torch" {
    type = lighting
    subtype = torch
    placement { wall = south, offset = 2 } # against a wall
}
feature "altar" {
    type = furniture
    subtype = altar
    placement { anchor = center }          # also north, south_east, ...
}
```

In an 8 by 6 room, one-cell positions run from `[0, 0]` to `[7, 5]`. `size`
overrides the footprint; `rotation` turns it. Two blocking features may not
share a cell; creatures may stand on traps and rugs.

## Checking the result

`dedalo inspect FILE` prints each room's box on the map; `--text-map` draws
it: `#` wall, `.` floor, `+` door, `S` secret door, letters for elements.
