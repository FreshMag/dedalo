# 10. Agents get four surfaces

Status: accepted, 2026-11-16. Source: specification section 1, decision 10.

## Context

Agents reach tools through shells, through MCP, through structured output, or by reading instructions. No single surface serves them all.

## Decision

Every command prints machine-readable JSON with `--json`; edit operations are addressed by element ID; an MCP server lives inside the binary; a packaged skill carries the language reference. All four use the same JSON structures.

## Consequences

Diagnostics are designed for agents from the start: stable codes, self-contained messages, and a `fix` as an edit operation. Today only the first surface exists; fixes are emitted but the engine that applies them is milestone M5.
