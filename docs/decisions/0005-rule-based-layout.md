# 5. Layout is deterministic and rule-based

Status: accepted, 2026-11-16. Source: specification section 1, decision 5.

## Context

A constraint solver is powerful but hard to predict, to explain in an error message and to keep identical across platforms.

## Decision

Rooms are polygons on a cell grid, walls are derived from space boundaries and openings are cut by connections. Placement follows fixed rules in declaration order. Coordinates are integers in thousandths of a cell. There is no solver before 1.0.

## Consequences

Results are exact and identical everywhere, and every failure can say what the valid values are. Authors resolve overlaps themselves, helped by a proposed fix. Coordinates are limited to 100,000 cells so that integer arithmetic stays far from overflow.
