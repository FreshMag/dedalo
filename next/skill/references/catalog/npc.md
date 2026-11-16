# `npc`: NPC

Written as `entity`. Category `creatures`, visible to `dm` by default, footprint 1 by 1 cells.

| Property | Type | Notes |
| --- | --- | --- |
| `disposition` | a symbol | hostile or neutral or friendly |
| `faction` | a symbol |  |
| `role` | a string |  |
| `description` | a string |  |

Names starting with `x_` are always accepted as custom data.

```hcl
entity "my_npc" {
    type = npc
    position = [1, 1]
}
```
