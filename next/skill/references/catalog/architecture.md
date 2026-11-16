# `architecture`: Column

Written as `feature`. Category `features`, visible to `public` by default, footprint 1 by 1 cells, blocking.

Subtypes: `column`, `pillar`.

| Property | Type | Notes |
| --- | --- | --- |
| `material` | a symbol |  |
| `description` | a string |  |

Names starting with `x_` are always accepted as custom data.

```hcl
feature "my_architecture" {
    type = architecture
    subtype = column
    position = [1, 1]
}
```
