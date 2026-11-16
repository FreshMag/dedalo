# 7. The scene is the only rendering contract

Status: accepted, 2026-11-16. Source: specification section 1, decision 7.

## Context

Output formats multiply, and a style that must be re-implemented per format will not be.

## Decision

Painters emit a restricted vector scene in cell units. SVG is serialised from it by our own byte-stable writer; PNG and PDF will be derived from that SVG. The scene cannot express scripts, filters, external references or animation.

## Consequences

Replacing painters replaces the look in every format at once. Anything the scene cannot express cannot be drawn. Text is live `<text>` until the bundled fonts of milestone M3 allow outlines.
