# 6. Redaction happens before rendering

Status: accepted, 2026-11-16. Source: specification section 1, decision 6.

## Context

A secret door or a trap on a player map destroys trust in the tool. Filtering at draw time leaves many places for a leak.

## Decision

A player map is painted from a filtered model. The view stage only removes and rotates; painters, the scene, exporters and plugins never see what it dropped. Source annotations are emitted for the DM audience only.

## Consequences

A leak requires a bug in one small stage, and the leak test checks every corpus file. Hidden rooms take their corridors with them, and the canvas is cropped so blank margin does not betray them.
