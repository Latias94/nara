---
status: active
date: 2026-07-10
audience: engine contributors and early project authors
supported_baseline: unreleased source tree at the start of the engine-foundation refactor
---

# July 2026 Engine Foundation Migration Guide

This guide records deliberate breaking replacements made by the engine-foundation completion plan. nara is unreleased: the goal is one correct canonical contract, not compatibility with prototype APIs or draft file shapes.

## Policy

- Remove obsolete code, exports, fixtures, and documentation in the same implementation unit.
- Do not add deprecated aliases, compatibility wrappers, parallel `V1`/`V2` Rust types, or a second loading path.
- Give the corrected Rust API the canonical unsuffixed name.
- A superseded pre-release persistent shape is deleted and replaced by the correct canonical `format_version = 1` after every in-repository source/fixture is updated.
- Runtime readers never silently rewrite project source files. A source rewrite is explicit and performed before or outside runtime load.
- Generated caches may be rebuilt, quarantined, or deleted; they are not compatibility authorities.
- Preserve an old reader or migration chain only when an ADR names the compatibility window, owner, removal trigger, and fixtures.
- Back up external experimental projects before applying a source rewrite.

## Migration Summary

Every implementation unit that changes a public API, persistent shape, cache contract, or observable behavior adds a row. A unit with no external migration records `No external migration` in its verification evidence rather than inventing an entry.

| Migration ID | Unit | Commit | Kind | Affected contract | Required action |
|---|---|---|---|---|---|
| _Added by the owning unit_ | - | - | `rust-api`, `persistent-format`, `cache`, or `behavior` | - | - |

## Entry Contract

Each migration entry contains:

1. **Removed contract**: the exact public symbol, serialized shape, cache identity, or behavior removed.
2. **Canonical replacement or deletion rationale**: the unsuffixed replacement and why no compatibility path remains.
3. **Before/after**: required for Rust API changes; use short compilable examples.
4. **Affected examples and fixtures**: every in-repository caller/source updated by the unit.
5. **User action**: source edit, configuration edit, no action, or explicit regeneration command.
6. **Source action**: `none`, `manual-rewrite`, or an explicitly named offline tool. Runtime auto-rewrite is forbidden.
7. **Cache action**: `keep`, `rebuild`, `quarantine`, or `delete`.
8. **Compatibility window**: normally `none (unreleased canonical replacement)`; otherwise link the authorizing ADR.
9. **Rollback**: how to recover source data or revert the code commit without relying on a compatibility shim.
10. **Verification anchors**: tests, fixtures, examples, and stale-symbol searches proving the replacement is complete.

## Persistent Format Matrix

U9 and later format-owning units populate this table before writing a new shape. Rows describe only formats intentionally supported after the refactor; deleted draft formats do not remain as pseudo-legacy rows.

| Kind | Canonical written version | Readable versions | Retained migration chain | Engine minimum | Source action | Cache action |
|---|---:|---|---|---|---|---|
| _Added by the format owner_ | 1 | 1 | none | _set by owner_ | _set by owner_ | _set by owner_ |
