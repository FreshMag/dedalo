# `lighting`: Light

Written as `feature`. Category `lighting`, visible to `public` by default, footprint 1 by 1 cells.

Subtypes: `torch`, `brazier`, `candle`, `glowing_crystal`.

| Property | Type | Notes |
| --- | --- | --- |
| `active` | true or false |  |
| `color` | a string |  |
| `intensity` | a symbol | bright or dim |
| `radius` | a number | at least 0; Radius of the light in cells |
| `description` | a string |  |

Names starting with `x_` are always accepted as custom data.

```hcl
feature "my_lighting" {
    type = lighting
    subtype = torch
    position = [1, 1]
}
```
