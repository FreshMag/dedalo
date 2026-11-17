# `custom`: Custom

Written as `feature or entity`. Category `features`, visible to `public` by default, footprint 1 by 1 cells.

| Property | Type | Notes |
| --- | --- | --- |
| `label` | a string | Legend label |
| `symbol` | a string | One to three characters drawn in the marker |
| `description` | a string |  |

Names starting with `x_` are always accepted as custom data.

```hcl
feature "my_custom" {
    type = custom
    position = [1, 1]
}
```
