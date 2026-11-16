# `trap`: Trap

Written as `feature`. Category `traps`, visible to `dm` by default, footprint 1 by 1 cells.

Subtypes: `pressure_plate`, `dart_launcher`, `pit`, `generic`.

| Property | Type | Notes |
| --- | --- | --- |
| `area` | a shape such as circle(2) |  |
| `damage` | a string | Dice expression, for example 2d10 |
| `damage_type` | a symbol |  |
| `detection_dc` | a whole number | 0 to 40 |
| `disarm_dc` | a whole number | 0 to 40 |
| `effect` | a string |  |
| `linked_to` | list of an element ID |  |
| `trigger` | a string |  |
| `description` | a string |  |

Names starting with `x_` are always accepted as custom data.

```hcl
feature "my_trap" {
    type = trap
    subtype = pressure_plate
    position = [1, 1]
}
```
