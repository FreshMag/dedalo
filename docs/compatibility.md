# Compatibility policy

What Dedalo promises to keep stable, from release 1.0 (spec 16.3).

| Subject | Version | Promise |
| --- | --- | --- |
| The `dedalo` binary and the crates | Semantic versioning | Within a major version, command-line flags and JSON output stay compatible: fields may be added, none removed or renamed |
| The language | An integer in the `dedalo` header | Every version since 1 keeps working with its original meaning. Additions keep the number; any change that could alter the meaning of an existing file increments it |
| Diagnostic codes | Permanent | A code is never renamed or reused |
| The plugin API | An integer in the manifest | The current and the previous version are supported |
| JSON Schemas | A major number in the URL | Old URLs stay served |
| Themes | Named revisions | A visual change to `classic` ships as a new revision; the old one stays selectable |
| Output | The golden corpus | The output of a file under a given theme changes only in a release whose notes say so, with before-and-after images |

## Determinism

Given the same source bytes, Dedalo version, plugin hashes and export
settings, SVG and PNG output are byte-identical on every supported platform,
the browser included. PDF output is byte-identical too: it carries no date
and its document ID is derived from its content.

## What may change in a minor release

- New attributes, blocks, element types, shapes, presets and output formats.
- New diagnostic codes, and better messages, suggestions and fixes for
  existing ones.
- Output that differs only where the release notes say so.

## Before 1.0

Releases 0.x make no promise beyond the golden corpus: the plugin API in
particular is unstable throughout 0.2.
