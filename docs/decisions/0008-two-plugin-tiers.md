# 8. Two plugin tiers

Status: accepted, 2026-11-16. Source: specification section 1, decision 8.

## Context

Most customisation is data: colours, symbols, element types. Code plugins are powerful but need a sandbox that works in a browser.

## Decision

Declarative packs cover themes, symbols, catalogs and templates with no code. Sandboxed WebAssembly plugins cover painters, scene passes, renderers, exporters, validators and shapes, through a small JSON-over-four-exports ABI that runs in the binary and in the browser.

## Consequences

One plugin file serves both hosts. The ABI is deliberately plain, at some cost in speed for large scenes. Nothing of this tier exists before milestone M6.
