---
type: "Subagent Finding"
title: "Complete UI key representation constraints and bounded experiment"
description: "Mixed-context UI trees rule out implicit local-source inheritance; exact-sharing storage needs a bounded comparison before refreeze."
timestamp: 2026-10-24T16:27:30Z
record_id: "cf1460fb992640a7a0d9addfd0f2de54"
producer_id: "codex"
run_id: "session-01a1629b-ui-representation"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
subagent_id: "01a16eb4-4f55-7ed3-b3a6-ee15951327bf"
review_subagent_id: "01a16ebc-3a16-7290-adec-b3c970a81797"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
---

# Finding

UI root scope and Scene source context are different axes. Hierarchy mutation admits cross-Scene
reparenting without rewriting provenance. Runtime projection traverses those children and accepts
persistent/runtime mixtures; only a nested root or absent node stops its subtree. An implicitly
relative local address would therefore omit legal child identity. Public local-key extraction,
comparison and Button reconstruction also make that representation's contract observable.

# Evidence

Bernoulli inspected the live hierarchy writer, Runtime lowering, Canvas/cache inputs, kernel
authoring/compiler and Button consumers. The main owner checked the source/occurrence text
contracts and existing lowering against those findings. Source names and occurrence coordinates
reject NUL but accept other controls, whereas `UiKeyAtom` rejects controls; a required binary
context needs its own bounded carrier, not Display conversion or a string-atom workaround.

The lane completed without writes, Cargo or descendants and was closed after collection. Its
recommendation is source-based research, not a storage or performance result.

The independent storage challenge (Socrates) confirms a bounded two-segment local could preserve
extraction/recomposition without a recursive address graph, but would require content-based
Eq/Ord/Hash and larger handles or another allocation. Existing authoring and compiled keys clone
the same local Arc: a compilation-only suffix allocation increases retained payload while original
locals stay live, and validation also calls `scoped`. Runtime later drops the temporary snapshot;
Button construction can begin with a scoped key. Measure those actual retention phases rather
than assuming compression is universally worse or better. The follow-up also completed read-only
without Cargo/descendants and was closed.

# Recommendation

Keep complete root/local semantics. Compare independently retained addresses, genuine relative
addresses with explicit foreign/runtime cases, and exact shared context/prefix storage. The lane
prefers the third, but the main owner has not selected a format: byte-prefix machinery must earn
its complexity against a simpler typed context. Inspect retention across original authoring and
compiled output before attributing memory savings to a compressed pair.

The owning design records exact-coordinate equality, content-defined budget accounting, mixed
trees, canonical aliases, cost boundaries, Runtime/Canvas parity and a small/4096-node experiment.
An illustrative prefix framing costs `7 + R + L - P`; it is not the accepted encoding. The 256-byte
ceiling is unchanged, and 32 projection steps do not guarantee admission of every path length.
No hash-only identity, truncation, global interner or unexplained ceiling increase is authorized.

Prefer the simpler complete typed address with shared opaque context and allocation-free scoping
as the initial replacement candidate; canonical prefix encoding need not dictate in-memory
segmentation. Begin with the small clone/lifecycle probe without Scene/layout/render integration.
Segmented storage must beat typed context across real retention phases, including its favorable
long-prefix case, before it earns additional comparison/recomposition machinery.

# Disposition

Research and bounded experiment specification only. ADR 0102 remains unchanged and not refrozen;
ordinary Scene preparation qualification remains a separate unit. The two read-only lanes changed
the next implementation decision from direct segmented storage to a simpler candidate plus a
falsifiable experiment. There are no live subagents. The complete foundation goal remains active.

# Citations

- [Owning design and experiment](../../../../architecture/scene-source-and-occurrence-identity-design.md)
- [Hierarchy writer](../../../../../crates/nara_hierarchy/src/writer.rs)
- [Runtime projection](../../../../../crates/nara_ui/src/runtime/projection.rs)
- [Kernel key representation](../../../../../crates/nara_ui_kernel/src/identity.rs)
- [Button reconstruction](../../../../../crates/nara_ui_widgets/src/button.rs)
- [Canvas source cache](../../../../../src/project_host/runtime/editor/ui_preview.rs)
