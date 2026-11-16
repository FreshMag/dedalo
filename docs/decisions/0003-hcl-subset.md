# 3. The DSL is a strict subset of HCL native syntax

Status: accepted, 2026-11-16. Source: specification section 1, decision 3.

## Context

A new syntax costs editor support and model familiarity. Full HCL brings expressions and templates the product does not want.

## Decision

Every `.dedalo` file is valid HCL: blocks, attributes, literals, lists, objects and function calls. Dedalo has its own error-tolerant, lossless parser (hand-written, on `rowan`) so that it can report all errors at once and edit files without disturbing comments.

## Consequences

Existing HCL highlighting works. Operators, conditionals, `for` expressions and templates are recognised and rejected with a clear message. The parser is ours to maintain; `hcl-edit` remains the fallback.
