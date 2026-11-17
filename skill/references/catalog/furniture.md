# `furniture`: Furniture

Written as `feature`. Category `features`, visible to `public` by default, footprint 1 by 1 cells, blocking.

Subtypes: `table`, `chair`, `wardrobe`, `altar`, `statue`, `sarcophagus`, `crate`.

| Property | Type | Notes |
| --- | --- | --- |
| `material` | a symbol |  |
| `movable` | true or false |  |
| `description` | a string |  |

Names starting with `x_` are always accepted as custom data.

```hcl
feature "my_furniture" {
    type = furniture
    subtype = table
    position = [1, 1]
}
```
