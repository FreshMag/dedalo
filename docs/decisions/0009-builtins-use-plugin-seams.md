# 9. Built-ins use the plugin seams

Status: accepted, 2026-11-16. Source: specification section 1, decision 9.

## Context

Extension points that the core itself does not use are untested promises.

## Decision

The built-in catalog and the `classic` theme are pack files (`builtin.toml`, `classic.toml`) read by the ordinary loaders, and the built-in painters register through the same painter registry a plugin will use.

## Consequences

The seams are exercised by every render. A missing capability in a seam shows up in the core first. The pack loader for third-party packages is still to come.
