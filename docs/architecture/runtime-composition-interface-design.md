# Runtime Composition Design History

**Status**: Retired design draft (2026-10-17).

The July 2026 workbench explored pure plugin planning, fresh runtime candidates and concrete Host
ownership. Its staged U4/U5/U24 sketches and outstanding questions no longer describe the current
product. Keeping those sketches beside the implementation contract obscured which interface to use.

## Current Sources

- [ADR 0117](adr/0117-2d-foundation-ownership-and-authoring-contracts.md) defines the current
  composition and runtime ownership boundaries.
- [ADR 0082](adr/0082-process-host-authority-and-runtime-construction-topology.md) and
  [ADR 0084](adr/0084-executable-runtime-ownership-and-isolation.md) retain the construction and
  isolation decisions.
- [Rust Product Recipes](../guides/rust-product-recipes.md) explains the supported authoring path.
- [Implementation status](adr/implementation-status.md) links current evidence and remaining gaps.

The useful distinction survives in those owners: a reusable definition is not an active runtime,
planning precedes mutation, and each concrete Host owns publication and retirement. The retired
examples do not establish additional APIs or acceptance gates.

## Historical Text

The original sections, scenario IDs and alternatives remain in Git. Historical records referring
to caller journeys, trade-off budgets or RGF units refer to that version, not this short note:

```text
git show 9d50cf9661514b29f3429e1ba5bd7fafaa2d1502:docs/architecture/runtime-composition-interface-design.md
```
