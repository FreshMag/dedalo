# `mechanism`: Mechanism

Written as `feature`. Category `mechanisms`, visible to `public` by default, footprint 1 by 1 cells.

Subtypes: `lever`, `button`, `generic`.

| Property | Type | Notes |
| --- | --- | --- |
| `effect` | a string |  |
| `linked_to` | list of an element ID |  |
| `description` | a string |  |

Names starting with `x_` are always accepted as custom data.

```hcl
feature "my_mechanism" {
    type = mechanism
    subtype = lever
    position = [1, 1]
}
```
