# Extension Contract Kernel Design History

**Status**: Dormant proposal; no production kernel is admitted (2026-10-17).

The July 2026 appendix explored a shared leaf for cross-domain package declarations, typed bindings
and resolution receipts. Its `ContractSupport` and `ContractSlice` sketches were not implemented.
This proposal has not been replaced by an equivalent general kernel: current compiled Rust
composition is deliberately narrower.

## Current Sources and Revisit Trigger

- [Rust Product Recipes](../guides/rust-product-recipes.md) covers runtime plugins and persistent
  schema contributions through the current compiled product.
- [OQ-045](open-questions.md#oq-045-plugin-package-contribution-and-official-product-recipe-ergonomics)
  records the remaining package ergonomics question and its real-consumer trigger.
- [OQ-031](open-questions.md#oq-031-product-package-contribution-and-trust-topology) retains the wider
  package, installation and trust questions.
- [Domain extension decisions](adr/0016-extension-seams-for-backends-and-domain-modules.md) and
  [ADR 0117](adr/0117-2d-foundation-ownership-and-authoring-contracts.md) constrain current owners.

Revisit shared machinery only when a concrete package needs more than runtime plugins plus
persistent schema. Preserve typed domain ownership and explicit Host authority when evaluating
that need; the old proof carriers and proposed stages are not implementation obligations.

## Historical Text

The full candidate design, alternatives and scenario references remain in Git. The old section
names and illustrative types belong to that historical proposal:

```text
git show 9d50cf9661514b29f3429e1ba5bd7fafaa2d1502:docs/architecture/extension-contract-kernel-interface-design.md
```
