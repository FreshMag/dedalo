# `creature`: Creature

Written as `entity`. Category `creatures`, visible to `dm` by default, footprint 1 by 1 cells.

| Property | Type | Notes |
| --- | --- | --- |
| `count` | a whole number | at least 1; A group of creatures is one entity with a count |
| `disposition` | a symbol | hostile or neutral or friendly |
| `faction` | a symbol |  |
| `hp` | a whole number | at least 0 |
| `description` | a string |  |

Names starting with `x_` are always accepted as custom data.

```hcl
entity "my_creature" {
    type = creature
    position = [1, 1]
}
```
