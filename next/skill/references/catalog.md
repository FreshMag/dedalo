# Dedalo catalog

The class is the block keyword: `feature` or `entity`. Use `type = custom` for anything else, with `properties { symbol = "Al", label = "Altar of bones" }`.

| Type | Class | Category | Default visibility | Subtypes |
| --- | --- | --- | --- | --- |
| `architecture` | feature | features | public | column, pillar |
| `container` | feature | treasure | dm | chest |
| `creature` | entity | creatures | dm | free |
| `custom` | either | features | public | free |
| `furniture` | feature | features | public | table, chair, wardrobe, altar, statue, sarcophagus, crate |
| `item` | entity | treasure | dm | treasure, key, weapon, armor, scroll, generic |
| `lighting` | feature | lighting | public | torch, brazier, candle, glowing_crystal |
| `mechanism` | feature | mechanisms | public | lever, button, generic |
| `npc` | entity | creatures | dm | free |
| `terrain` | feature | terrain | public | water, lava, rubble, rocks |
| `trap` | feature | traps | dm | pressure_plate, dart_launcher, pit, generic |

Read `catalog/<type>` for the properties of one type.
