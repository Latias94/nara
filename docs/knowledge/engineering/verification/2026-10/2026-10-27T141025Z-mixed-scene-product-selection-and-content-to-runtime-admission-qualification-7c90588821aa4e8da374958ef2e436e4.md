---
type: "Verification Evidence"
title: "Mixed Scene product selection and content-to-runtime admission qualification"
description: "Qualifies exact mixed definitions through recipes, strict content and managed publication; records remaining language and Editor work."
timestamp: 2026-10-27T14:10:25Z
record_id: "7c90588821aa4e8da374958ef2e436e4"
producer_id: "codex-root"
run_id: "mixed-scene-product-2026-10-27"
related_plan: "docs/plans/2026-10-27-scene-data-representation-plan.md"
git_branch: "refactor/mixed-scene-product-admission"
supersedes: "294f613511bb48e49fed958c05a1a469"
verified_by: ["01a17d89-e6e4-7f01-9b9d-d6d16e824380", "01a17d97-6ffb-7b71-8cef-9f00b3fc94cf"]
---

# Verification

Successor to `d58870a`, under the complete active foundation Goal and Scene data-family plan.
The commit containing this evidence owns the tested source. This is the product selection and
publication checkpoint, not a complete language SDK, new Editor frontend or all foundation waves.

# Result

Recipe contributions retain explicit owner-local data snapshots, either data-only or alongside
native providers. The generated Scene selection installs even for an empty recipe. Full-plan
preflight checks both its generated definition and its data-contributing plugins; seal verifies
actual data-owner admission identities. Canonical generated configuration includes current and
immediate predecessor generations, content fingerprints and the predecessor's own lineage link.
No synthetic native provider, native type or codec is created for a data Schema.

`SchemaValidationInput` selects the complete bindings, with the same native snapshot and original
owner snapshot clones installed in the World. The complete semantic fingerprint includes both
families; native executable fingerprints and provider receipts retain their distinct meaning.
Known inactive native owner/type/tombstone and immediate predecessor claims remain reserved.
The authoring bundle projects metadata and lineage; unselected bundle owners do not activate.

Bounded root decoding preserves declarations until the one mixed Prefab compiler selects actual
source demand. Effective asset capture reads selected Schemas from the same bindings; final
strict validation uses real native codecs plus data validation and aggregate admission. Source
upgrade flags, lazy source work-cap tightening and provenance are retained. Content and runtime
agreement compare exact complete selection through metadata-only witnesses. Shared witnesses
retain native snapshots weakly, charge owner metadata and outlive snapshots through the existing
source/assets leases without retaining native executable code.

Startup, trusted compiled Play source preparation, direct Restart/replacement, prepared content
replacement and Host safe points use that complete selection. A project-backed `SceneSource`
also retains the exact witness, so public direct startup cannot bypass the Host comparison.
Ordinary direct sources remain independently admitted against the target World's installed
definitions. Existing preview authority remains separate.

The public product fixture uses two data owners and native Name, actual JSON/RON Scene/Prefab
files, repeated occurrences, default assets and entity references, complete Check, content capture,
startup, scheduled guarded reads, asynchronous second-Scene preparation, real replacement and
old entity retirement. It proves immutable source files and no per-Schema native codec. Negative
cases cover unselected bundle data, inactive native reservations, outer selection replacement,
same-key factories returning readmitted data, divergent lineage, and public source identity.

Independent correctness review found a P1 where project-backed source dropped the binding witness;
the fixed public test shares native N, accepts original owner A clones, rejects separately admitted
equivalent B before entity/resource/activation changes, then completes startup to observe no
CurrentScene publication. The API review found two P2 gaps: outer generated-selection replacement
and omitted predecessor lineage in plan configuration. Both are fixed with public counterexamples
and independent source follow-up. No remaining blocker was reported by either scoped reviewer.

# Evidence

All Cargo invocations are serial, Rust `+1.95.0`, `CARGO_BUILD_JOBS=1`, using root `target`.
The standalone workspace explicitly uses that target. Results include:

| Scope | Actual result |
| --- | --- |
| New Scene selection/validation/source-admission public tests, all features | 9 passed; `85e99de8-20ee-4be6-b2f4-986b23458f73` |
| Root all-features library | 575 passed, 3 pre-existing ignored; part of `691dde90-4430-44e0-a282-465cde1bb5ec`; build 41.51 s |
| Root product data, plugin composition, content boot/delivery and runtime boot | 122 passed, 1 existing skipped; `b7791ecb-81f1-4576-96df-73bf8b976de0`; build 47.49 s, tests 18.221 s |
| Final complete product-data consumer after fixture hardening and formatting | 10 passed; `57ee88d2-3725-48d0-9f87-b9bb5c33b322`; build 14.17 s, tests 1.739 s |
| Root content boundary including private-owner compile rejection | 23 passed; part of `691dde90-4430-44e0-a282-465cde1bb5ec` |
| Full Scene and Reflect all-features nextest | 994 passed, 3 pre-existing ignored; `c6a3116c-290e-4d7b-b31a-cb8f81bf0ef4`; build 6m13s, tests 65.367 s |
| New Scene selection/validation tests without default features | 7 passed; `4b5b7394-9e6a-492e-9587-361cc4b1601b`; build 19.76 s, tests 0.311 s |
| Root no-default runtime-core library check | Passed, 20.85 s; existing feature-dependent warnings |
| Standalone module consumer: scene_data_components and scene_preparation | 14 passed; `ff0c4e45-0d0f-420c-8fdb-7897ecf62d5f`; build 1m22s, tests 0.899 s |
| Windows/DX12 critical Editor trajectories plus trusted Play source-close association | 4 passed, 268 filtered out; `bbddda61-423c-47cc-b9aa-2e03cfa21197`; build 3m00s, tests 8.509 s |
| Root/Scene/Reflect all-features/all-targets Clippy, correctness denied | Passed, 1m58s; existing non-correctness warnings remain |
| Formatting, changed-document relative links and engineering-memory validation | Passed; legacy memory warnings remain unchanged |

The root combined 608-case run had one new product fixture failure after its 575 library and
23 boundary cases passed. The fixture had declared a data-bearing second Scene while expecting
native-only complete Check to succeed. The final product run fixes that setup. Earlier fixture
corrections use a shared root capability for candidate/plan/loader, declare the second Scene,
preserve `CommittedPreflightRejected` attribution, and install the actual provider plugins for
a preloaded native snapshot. New AssetRef test metadata explicitly declares that capability.
None of these failures was hidden with production limit increases or weaker admission.

The final 10-case consumer run additionally makes the unselected-data negative use the same
root capability as its plan/loader, checks the actual owner-admission diagnostic, and uses
native-only content in both declared roots for its positive control. The full root library and
boundary result precede only these test-fixture/formatting refinements, not later product changes.

The native group ran the three critical trajectories from the owning procedure plus
`native_unrelated_prefab_close_preserves_presented_play_source_until_source_close`. They use
hidden Windows windows, real DX12 submission/presentation and owned input. The source-close
trajectory specifically protects the changed trusted Play source preparation path. This is
selected development qualification, not the complete native or OS-input matrix.

Two previous boundary expectations changed deliberately: surviving invalid components now fail
in the shared declaration compiler before asset capture. The tests assert its actual component
or migration diagnostic as well as zero active reservations. They do not reintroduce early
native-only root canonicalization merely to preserve an obsolete stage label.

# Follow-up

Keep the full foundation Goal active. Replace the language fixture's private preview/post-commit
carrier and cached export with public Scene compilation/preparation/access/export. Preserve actual
collectible .NET context and held callback evidence separately from data materialization. Existing
Lua regression coverage may share the corrected harness without selecting another production SDK.
Use the existing metadata-based window-free Session for source edits/history and Save/Reopen.
Empty adapter receipts do not turn known-unbound fields into editable records.

Then implement the concrete limited EnemyStats semantic proposal consumer, retaining source and
execution-publication basis in the Rust Host. Nara's own custom-drawn frontend should consume the
same semantic owner. Install/update/disable, task return, code retirement and independent delivery
remain complete workflow obligations. ADR 0120 remains Proposed beyond the adopted storage subset.

The compiled Editor's native preview and native Apply Changes exporter remain distinct consumers;
mixed Runtime startup does not qualify editable data records or mixed Apply Changes in that lane.
Owner catalog migration, dependency cohorts, Stop-first and in-flight executable replacement are
not implemented by this checkpoint. No official C# SDK, toolkit embedding, complete native Editor
suite, other platform, OS IME or monitor-scanout qualification is claimed.

Planck's bounded next-seam review (`01a17d8f-c27d-7350-b1d4-8cabdccec211`) found that migrating only
C# does not immediately delete `behavior_trial.rs`: representation correctness tests and the Rows
cost/orphan comparison still use it. Migrate correctness consumers and retain only indispensable
storage-only comparison evidence before removing the superseded materialization/export harness.

# Citations

- [Scene plan](../../../../plans/2026-10-27-scene-data-representation-plan.md)
- [Scene design](../../../../architecture/scene-component-representations-design.md)
- [ADR 0098](../../../../architecture/adr/0098-schema-owner-lineage-and-active-runtime-composition.md)
- [Product contribution](../../../../../src/product.rs)
- [Composition](../../../../../src/project_host/composition.rs)
- [Content and retained witness](../../../../../src/project_content.rs)
- [Public startup](../../../../../src/startup_scene.rs)
- [Public complete product consumer](../../../../../tests/product_data_components.rs)
- [Public Scene identity consumer](../../../../../crates/nara_scene/tests/component_binding_selection.rs)
- [Native procedure](../../../../verification/native-editor-qualification.md)
