# `container`: Chest

Written as `feature`. Category `treasure`, visible to `dm` by default, footprint 1 by 1 cells, blocking.

Subtypes: `chest`.

| Property | Type | Notes |
| --- | --- | --- |
| `key` | an element ID |  |
| `lock_dc` | a whole number | 0 to 40 |
| `locked` | true or false |  |
| `trapped` | true or false |  |
| `description` | a string |  |

Names starting with `x_` are always accepted as custom data.

```hcl
feature "my_container" {
    type = container
    subtype = chest
    position = [1, 1]
}
```
