# 2. A compiler pipeline with inspectable stages

Status: accepted, 2026-11-16. Source: specification section 1, decision 2.

## Context

Layout and rendering bugs are hard to locate when the tool is one opaque step, and plugins need stable seams to attach to.

## Decision

Source text becomes a document model, then a resolved model with absolute geometry, then an audience-filtered view, then a scene, then bytes. Every stage is a pure function whose output serialises to JSON, and reports every independent problem it finds.

## Consequences

`dedalo inspect --stage` and `render --emit` show exactly what the next stage receives. A stage may not perform I/O, read a clock or use unseeded randomness.
