# 1. One core, three hosts

Status: accepted, 2026-11-16. Source: specification section 1, decision 1.

## Context

A dungeon must render the same from the command line, in the browser and through an agent. Logic duplicated across hosts drifts.

## Decision

All language, geometry and rendering logic lives in one Rust workspace compiled to native code and to WebAssembly. The binary, the web page and the MCP server are thin hosts that call the facade in `dedalo-core` and contain no map logic.

## Consequences

Hosts cannot disagree. Every feature must fit the facade. The MCP host does not exist yet; the binary and the WebAssembly module do, and the golden corpus proves they agree byte for byte.
