# 4. JSON is a first-class equal of the DSL

Status: accepted, 2026-11-16. Source: specification section 1, decision 4.

## Context

Agents and programs produce structured data more reliably than text, and people prefer text.

## Decision

The document model has a published JSON Schema generated from the Rust types. Conversion in both directions preserves all semantics and comments, and the round-trip laws of specification 6.3 are tested on every corpus file.

## Consequences

The model, not the text, is canonical. Comments must have a place in the model, which costs a `comments` field on every node. Defaults are never written out.
