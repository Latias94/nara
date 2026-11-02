---
type: "Work Progress"
title: "Inline authored reference and contextual default production replacement in progress"
description: "Identity and Reflect owners pass focused tests while Scene source, input compiler, native gates and retention are integrating without compatibility tables."
timestamp: 2026-11-01T23:46:57Z
record_id: "b823aeb26f164fcea2842bc51af8f129"
producer_id: "codex-foundation-main"
run_id: "inline-authored-reference-20261102"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "0c3fe4491bf564edc9498e2859612f69f05cec5b"
supersedes: "fdb7dce644a84dcbb5b8b81fee9ec514"
---

# Summary

Full foundation Goal active. Main and refactor/foundation are at 0c3fe44 after the preceding
final-reference-demand qualification. The next CP2 production replacement is a large uncommitted
WIP. None of the new Scene/input/default/Native integration is qualified yet. No compatibility
binding table, reader or conversion helper is retained. Preserve all WIP and the original vendor.

# Details

Identity owns the sole PrefabEntityOrigin/Hop/Target types and adds validated ReferenceInputId,
AuthoredReferenceTarget, AuthoredEntityReference and EntityReference::Authored. Actual occurrence
scope is interpreted only by Scene. Identity tests pass102/102 in
ec3cb543-5c29-4e2a-a02b-822e8efbb7f2. Euler is closed.

Reflect reuses its raw value walker for complete typed authored-reference rejection after
defaults/before codecs, including opaque contents and encoding. Its default planner now accepts
borrowed contextual overrides through internal budget and owned helpers. Source shape/default
paths retain authored values; no alternate value tree or evaluator. Reflect lib tests pass179/179
in dece9c7a-7d9d-4039-9778-cd54bd53208c. Helmholtz is closed. Independent Hilbert review found
two actual corrections: Persistent logical UUID cost was16 at baseline, not the36 incorrectly
assumed by parent; the new shared cost must restore16 while traversal keeps its distinct36.
Hilbert is now active only on this accounting fix; do not overwrite its Identity reference and
Reflect accounting/test edits. The same review identifies external RON discriminants lost when
an outer ComponentValue's content precedes its tag. Parent has added a real full-value test in
Reflect tests/authored_value_wire.rs; it has not run. Resolve the canonical compositional wire,
not only direct EntityReference deserialization. No old reader is required.

Scene source data now owns PrefabDocument.inputs, PrefabInstance.arguments with exact base and
typed values, and SceneComponentRecord.contextual_defaults by stable root field ID under its
record version. Source-dependent Schema defaults require explicit context even when hidden;
canonical Null/Persistent/scalar defaults retain fallback. Parent owns document.rs validation,
source model and properties/references helpers. Old durable PrefabReferenceBinding types and
addition/Set/property tables are removed. The old grammar-only binding tests are deleted; their
mechanism is displaced, while new public inline consumer tests are being added.

Kuhn implemented Source/Shape versus Native admission, complete effective-set checking before
the first codec, processed/data setter checks and public gate tests; it is frozen and closed.
Source returns diagnostics, not PreparedScene. This is source-ready, uncompiled. Accepted
ADR0006/0026 were reconciled for source intent versus executable eligibility.

Locke implemented and froze staged.rs plus staged/values,reference_program,output and new inputs.
It is closed. Origins follow lexical occurrences; terminal argument environments govern visibility;
current-chain occurrence/input keys detect cycles; missing arguments demand only final leaves.
Opened occurrences validate exact argument base and provided names. Absent retains the original
record default environment. Final lowering counts complete authored reference costs and strips
interpreted contextual metadata. Parent wired root input declarations through compile.rs and
standalone Prefab expansion without instantiate() erasure. These files are uncompiled.

Chandrasekhar completed root/Tooling source retention and consumer integration, then closed.
It preserves hidden defaults/input/argument costs, source metadata and new input literals within
src and Tooling. Parent must inspect it before checks. It reported Prefab schema-upgrade input
preservation as a coupling; Pasteur owns that source upgrade work. A blanket contextual-default
upgrade refusal can be an intermediate safety state, not the complete CP1 successor outcome.

Still-active writers at this capture: Pasteur01a19979-b9ee-7be0-9c48-10f0dec2b32e owns Scene
format/migration/authoring_decode/patch/schema-upgrade and overlay cost; Herschel
01a1997e-c716-73a0-9b15-eb0ab49a8c1b owns complete Prefab-copy owner; Hilbert
01a19981-6428-7111-a00a-5e8d287157f3 owns the reviewed accounting correction. No Cargo is
known active. Do not fmt globally or compile their changing owners before source-ready.

# Next Action

Finish real RON full-value reproduction and correct string discriminants at the canonical wire;
restore Native logical cost with exact controls. Receive remaining source-ready writers, inspect
diff, fmt once frozen, then serial Scene library checks and new public Native/input tests. Expect
many old source literals/fixtures to need intentional source rewrites; do not restore bindings or
invent compatibility. Parent tests/inline_authored_references.rs covers lexical versus selected
input scope, repeated occurrence distinction, self-input cycles, overwritten undemanded inputs
and hidden defaults/Absent. It has not compiled. Native and authoring complete controls, whole
copy/remap, source/history/CP1 defaults and ordinary workflows remain required before commit.
Opaque source data and the separate flat compiled output boundary still need exact integration;
do not mislabel a raw SceneDocument wrapper as compiled proof. Complete CP2 extraction/publication
and recovery, root/placement replacement and the full foundation remain open.

# Citations

- [CP2 plan](../../../../plans/2026-11-02-prefab-extraction-plan.md)
- [Prior qualification](../../verification/2026-11/2026-11-01T231259Z-final-reference-demand-and-preallocated-property-paths-qualification-fdb7dce644a84dcbb5b8b81fee9ec514.md)
- TEMP logs: nara-inline-reference-identity-tests.log and nara-inline-reference-reflect-tests.log.
