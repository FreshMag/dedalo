# `item`: Item

Written as `entity`. Category `treasure`, visible to `dm` by default, footprint 1 by 1 cells.

Subtypes: `treasure`, `key`, `weapon`, `armor`, `scroll`, `generic`.

| Property | Type | Notes |
| --- | --- | --- |
| `magical` | true or false |  |
| `quantity` | a whole number | at least 1 |
| `value` | a string |  |
| `description` | a string |  |

Names starting with `x_` are always accepted as custom data.

```hcl
entity "my_item" {
    type = item
    subtype = treasure
    position = [1, 1]
}
```
