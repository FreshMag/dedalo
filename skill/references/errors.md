# Dedalo diagnostic codes

Read `errors/<CODE>` for causes and a worked fix.

- `SYNTAX_ERROR` (Error): The text is not valid Dedalo syntax.
- `UNKNOWN_BLOCK` (Error): A block keyword is not valid at this place.
- `UNKNOWN_ATTRIBUTE` (Error): An attribute name is not defined for this block.
- `TYPE_MISMATCH` (Error): A value has the wrong type.
- `INVALID_VALUE` (Error): A value has the right type but is not allowed.
- `UNKNOWN_PROPERTY` (Warning): A property is not defined by the catalog for this element type.
- `DUPLICATE_ID` (Error): Two elements share one ID.
- `UNKNOWN_REFERENCE` (Error): A reference names an element that does not exist.
- `UNKNOWN_TYPE` (Error): An element type or subtype is not in the catalog.
- `INVALID_SHAPE` (Error): A shape cannot be built.
- `LAYOUT_CYCLE` (Error): Rooms are placed relative to each other in a loop.
- `LAYOUT_UNDERCONSTRAINED` (Error): A room has no layout and nothing links it to a placed room.
- `OVERLAP` (Error): The interiors of two spaces intersect.
- `OUT_OF_BOUNDS` (Error): An element lies outside its room.
- `DOOR_NOT_PLACEABLE` (Error): A connection's opening does not fit on the wall the two spaces share.
- `UNRESOLVABLE_CONNECTION` (Error): The two spaces of a connection neither share a wall nor face each other closely.
- `CORRIDOR_DISCONNECTED` (Error): A corridor does not reach the spaces it connects.
- `ELEMENT_OVERLAP` (Warning): Two blocking features share a cell.
- `UNREACHABLE_ROOM` (Warning): A room cannot be reached from the entrance.
- `REFERENCED_ELEMENT` (Error): An element cannot be removed because others refer to it.
- `STALE_SOURCE` (Error): The file changed since the edit was prepared.
- `PLUGIN_NOT_FOUND` (Error): The document requires a plugin that is not installed.
- `PLUGIN_CONFLICT` (Error): Two active plugins claim the same exclusive role.
- `PLUGIN_FAILED` (Error): A plugin returned an error or invalid output.
- `PLUGIN_TIMEOUT` (Error): A plugin exceeded its time or memory budget.
- `PLUGIN_RULE` (Warning): A plugin validator reported a problem.
- `EXPORT_TOO_LARGE` (Error): The output image would exceed 16,384 pixels on one side.

## SYNTAX_ERROR

Dedalo is a strict subset of HCL: blocks (`keyword "id" { }`), attributes (`name = value`), literals, lists, objects and shape calls. Operators, conditionals, `for` expressions, splats and `${...}` templates are not part of version 1.

Fix: read the message for the construct expected at that position. Typical causes are a missing `}` or `"`, a missing `=`, or an expression such as `width = 2 + 1`; write `width = 3`.

## UNKNOWN_BLOCK

Each block is allowed only inside certain parents (spec 5.5): for example `layout` inside `room`, `placement` inside `connection`, `feature` or `entity`.

Fix: use the keyword proposed in the message, or move the block to a parent that accepts it. Example: `layout { }` written directly in `dungeon` belongs inside the `room` it places.

## UNKNOWN_ATTRIBUTE

Structural attributes are fixed per block type. Game and descriptive data go in a `properties { }` block instead.

Fix: use the closest name proposed in the message, or move the value into `properties`. Example: `detection_dc = 15` on a trap becomes `properties { detection_dc = 15 }`.

## TYPE_MISMATCH

The schema of each block fixes the type of every attribute and property: number, string, boolean, symbol, reference, point, list or shape.

Fix: write the value in the expected form. Example: `position = 3` should be a point, `position = [3, 2]`.

## INVALID_VALUE

Raised for values outside an enumeration or range, malformed IDs, attributes set twice, conflicting attributes, and input over the limits (5 MB of source, 50,000 elements, nesting depth 16).

Fix: the message lists the allowed values. Example: `rotation = 45` on a room must be one of 0, 90, 180, 270.

## UNKNOWN_PROPERTY

Properties are typed by the catalog entry of the element's `type`. Unknown names are kept but flagged, because they are usually typos.

Fix: use the closest name proposed in the message, or prefix the name with `x_` to declare custom data: `x_origin = "homebrew"`. Run `dedalo catalog <type>` to list the properties of a type.

## DUPLICATE_ID

IDs share one namespace per dungeon: a room and a feature cannot have the same ID, so that any tool can address any element by ID alone.

Fix: rename one of the two. Example: a room `crypt` and a note `crypt` become `crypt` and `crypt_note`.

## UNKNOWN_REFERENCE

References are quoted element IDs (`from = "entrance"`). They may point forward, and are resolved after the whole file is read.

Fix: use the closest existing ID proposed in the message, or declare the missing element.

## UNKNOWN_TYPE

The catalog fixes the valid types and subtypes. Run `dedalo catalog` to list them.

Fix: use one of the nearest matches in the message. For something the catalog does not cover, use `type = custom` with a one-to-three-character `symbol` and a `label` in `properties`.

## INVALID_SHAPE

A shape is invalid when a dimension is below one cell, a polygon has fewer than three vertices, self-intersects or has zero area, or the shape function is unknown. The shapes of 0.1 are square(side), rectangle(width, height), circle(radius) and polygon(points).

Fix: correct the arguments. Example: `shape = rectangle(8, 0)` becomes `shape = rectangle(8, 6)`.

## LAYOUT_CYCLE

Relative placement needs a starting point: `a` north of `b` and `b` south of `a` leaves both undefined.

Fix: give one room of the loop an absolute position, `layout { position = [0, 0] }`, or remove its `adjacent_to`.

## LAYOUT_UNDERCONSTRAINED

A room without a `layout` block is placed automatically next to a space it shares a connection or corridor with. A room with no such link cannot be placed.

Fix: add `layout { position = [x, y] }` or `layout { adjacent_to = "other", direction = east }`, or connect the room to a placed room.

## OVERLAP

After placement, two spaces may touch along an edge but may not overlap. Automatic overlap resolution is left to a later release.

Fix: move one of the rooms. The message proposes the smallest `layout.offset` that clears the overlap; `align = start` or `align = end` often does the same.

## OUT_OF_BOUNDS

Every corner of an element's footprint must lie inside or on the boundary of the owning space. Positions count cells from 0 at the top-left corner of the space.

Fix: use the nearest valid cell proposed in the message. Example: in an 8 by 6 room, valid positions for a one-cell element run from [0, 0] to [7, 5].

## DOOR_NOT_PLACEABLE

The opening must lie entirely on the segment shared by `from` and `to`. `placement.offset` is measured in cells from the west end of a north or south wall, or the north end of an east or west wall.

Fix: set `placement.offset` inside the range given in the message, or remove it to centre the opening.

## UNRESOLVABLE_CONNECTION

A connection needs a shared wall, or facing walls at most `settings.max_stub` cells apart (default 2), where a short stub passage is inserted.

Fix: make the rooms adjacent (`layout { adjacent_to = ... }`), or join them with a `corridor` instead of a connection.

## CORRIDOR_DISCONNECTED

An automatic corridor is straight: the facing walls of the two spaces must overlap in projection by at least the corridor's width. A corridor with an explicit `path` must start and end on the walls of the spaces named in `connects`.

Fix: move the rooms so their walls face each other, or give the corridor an explicit orthogonal `path` of lattice points.

## ELEMENT_OVERLAP

Blocking features such as columns and furniture cannot occupy the same cell. Entities may stand on non-blocking features such as traps.

Fix: move one of the two features to a free cell.

## UNREACHABLE_ROOM

Spaces and connections form a graph. The entrance is the first room tagged `entrance`, else the first declared room.

Fix: add a `connection` or `corridor` that joins the room to the rest of the dungeon, or ignore the warning if the room is meant to be sealed.

## REFERENCED_ELEMENT

Removing an element that connections, notes or other elements reference would leave dangling references.

Fix: remove or update the referrers listed in the message first, or pass `cascade: true` to remove them together.

## STALE_SOURCE

An edit batch may carry `base_hash`, the content hash of the source it was computed against. A mismatch means someone else changed the file.

Fix: read the file again, recompute the edit, and send it with the new hash.

## PLUGIN_NOT_FOUND

Rendering never downloads. Plugins are looked up in `--plugin` paths, `./.dedalo/plugins/`, the per-user configuration directory and the built-ins.

Fix: install it with `dedalo plugin add <path-or-url>`, or remove the requirement from the `dedalo` header block.

## PLUGIN_CONFLICT

Only one renderer may be active for an export.

Fix: remove one of the two `plugin` blocks, or select the renderer explicitly in the export.

## PLUGIN_FAILED

Every plugin response is validated against a published schema.

Fix: check the plugin's message. `dedalo plugin check` runs the conformance suite against a plugin.

## PLUGIN_TIMEOUT

Plugins run with a fuel budget and 64 MiB of memory per call.

Fix: simplify the input, or report the problem to the plugin author.

## PLUGIN_RULE

Validator plugins add their own checks after layout, such as "every room needs a light". The message ends with the plugin and rule that raised it; the plugin may mark a problem as an error.

Fix: follow the message, or remove the plugin from the `dedalo` header block if its rules do not apply to this dungeon.

## EXPORT_TOO_LARGE

The pixel size is the map size in cells times `cell_px` (or `resolution`).

Fix: lower `cell_px` to the largest valid value given in the message, or export SVG or PDF, which have no pixel limit.
