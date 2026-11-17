# `terrain`: Terrain

Written as `feature`. Category `terrain`, visible to `public` by default, footprint 1 by 1 cells.

Subtypes: `water`, `lava`, `rubble`, `rocks`.

| Property | Type | Notes |
| --- | --- | --- |
| `damage` | a string |  |
| `damage_type` | a symbol |  |
| `depth` | a number | at least 0 |
| `difficult` | true or false |  |
| `description` | a string |  |

Names starting with `x_` are always accepted as custom data.

```hcl
feature "my_terrain" {
    type = terrain
    subtype = water
    position = [1, 1]
}
```
