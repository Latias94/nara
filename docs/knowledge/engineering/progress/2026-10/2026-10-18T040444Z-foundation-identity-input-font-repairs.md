---
type: Work Progress
title: Foundation identity residency, input endpoint and font ownership repairs
description: U5, U11 and U14 code and consumer proof, remaining recursive-schema work, serial test evidence and limits.
timestamp: 2026-10-18T04:04:44Z
record_id: foundation-u5-u11-u14-20261018-040444
producer_id: codex-root
run_id: foundation-contract-refactor-20261018
git_branch: refactor/foundation-contract-hardening
related_plan: docs/plans/2026-10-18-1105-refactor-foundation-contracts-plan.md
---

# Landed Contracts

U11 is `8a975db`: physical button holds retain endpoint membership, with a private distinction
between direct synthetic mutations and normalized timeline events. Action outcomes retain event
and original press routes/sequences. Independent review found a synthetic/default-driver collision
in the first implementation; a private enum and a two-direction mixed-input regression closed it.
PointerState remains a single-pointer convenience position. Canonical timeline observations retain
endpoint-stamped positions; this unit does not claim multi-pointer aiming or gamepad actions.

U5 is `5d42599`: namespace/UUID durable references resolve logical residency, while both runtime
reference variants and mutation tokens carry a checked process-global incarnation. Public
identity-only retirement revokes markers after full lifecycle/topology preflight. Scene prepared
retirement retains the existing exclusive guaranteed-despawn tail. Deleted is explicit and
terminal; unload updates a bounded logical record that can reactivate without history growth.
Scene history claims and arbitrary-order restore remain intact. Settings now take three independent
ItemLimits and are infallible; the obsolete cross-budget error was removed. Old command targets
are stale until the complete restore/fork remap rewrites them. Cross-process replay still requires
timeline admission/remapping; the process-local allocator is not a durable save identity.

U14 is `47d449d`: existing immutable FontFace owns the shared shaping/raster source. Configured
UI/presentation plugin definitions include face content ID. Bundled defaults remain available;
foreign glyph runs reject before atlas publication and retain planned/presented last-good packets.
Ordinary adapter and real WGPU readback are complementary tests, not one native Editor journey.
The root facade also exposes the new exact identity types and font configuration entry.

# Verification

All checks ran serially with CARGO_BUILD_JOBS=1 and reused root target. Windows, Rust 1.99.0.

| Scope | Result and run |
| --- | --- |
| U11 red | Both seat and pointer aggregation tests failed: `78369b5b-7bcd-4fb2-8ccc-3e665c489e77`. |
| U11 serde package | 51 passed, including mixed synthetic/timeline: `d84c0dc0-9be0-4df6-b459-20f15138efca`. |
| U5 red | Same UUID rebind rejected and same-Entity token reused: `3fbf460b-3934-4b68-87d8-4238d65b19b8`. |
| U5 identity/gameplay serde | 89 passed: `b3fc895c-c2d7-40d3-9e9d-0409492ed24c`. An earlier 86/87 run retained one obsolete persistent Tombstoned expectation; it now asserts Unloaded while preserving both Scene tombstone checks. |
| Independent module consumer | 6 passed: `a5a9a504-24b5-4e2e-9148-7fb68194caae`. A saved NPC state completes 32 residency cycles under one fixed Scene owner; logical record and Scene claim counts remain fixed. |
| Scene serde package | 169 passed: `a452815e-e8b8-4626-857f-8150b88a8d0c`. |
| Startup, project host and identity facade | 56 passed: `c4f71e59-22a8-4330-a9e8-4df533e92730`, features runtime-2d,serde,project-tooling,tooling. |
| Reference-game first_wave | 21 passed: `ceb5af28-26dc-4618-9594-35617ddbb413`. |
| U14 red | Existing compiler custom face failed raster presentation: `11047982-81c7-419c-b0d7-03d46b0b5e6a`. |
| Text | 14 passed: `7d428a72-5fa7-46fa-b067-83461cf50333`. |
| UI/planner | 114 passed: `55958beb-4561-4e24-9ff7-147bb8bbf6c0`. Added existing nara_image as a test-only dependency to install the ordinary image/presentation stack; lock delta is one dependency edge. |
| Real WGPU ui-submitter | 96 passed: `cd5139bb-ca84-493d-9157-34b81ea038be`, including custom-face nonblank GPU pixels and shared-cache isolation. WGPU_BACKEND=dx12 was set; the readback helper chooses its adapter through the existing WGPU default instance. |
| Compile breadth | cargo check --locked --workspace --all-targets --all-features passed. |
| Formatting | Root, excluded reference-game and module-consumer cargo fmt completed; final diff checks passed after normalizing one whitespace-only license line and its provenance hash. |

Early invocations found compile-only test wiring/API errors (a missing image dev dependency,
misnamed input test driver function, nonexistent raw command test method, and a required tooling
feature). They were corrected without widening runtime authority. No full native Editor, MSRV,
other-platform or external audio SDK qualification is claimed.

# Remaining Work And Recovery

The goal remains active. Completed required units are U1, U5, U6, U11 and U14. U2/U3/U4/U13 still
require recursive schema, effective defaults, consistent reference closure and final publication.
U7–U10/U15 retain their concrete consumer gates; U12 needs actual scale measurements.

Read ignored research `target/foundation-refactor-plan-research/u2-design.md` and
`u2-migration.md` before locking the next interface. One shape authority should replace stored
kind/domain duplication. Root field stable identity remains; concrete child paths use existing
Field/Index segments. Record members are present and closed; Optional only expresses explicit Null.
Existing optional fields must mechanically lift to Optional(old shape), even where a native codec
currently rejects Null. Root required/default remain independent. Old generic Map/List schemas
must remain explicit opaque shapes during mechanical migration; silently changing them to closed
Records would tighten persistent semantics without component-version migration.

U2 also needs present-value visitors for its real nested-reference consumer. U3 adds effective
defaults and target-existence semantics. Finite field codec traits should let Rust resolve aliases
and containers; do not build a proc-macro Rust resolver. Catalog and authoring bundle both reset
to v2, while Scene/Prefab/value wire and unrelated envelopes stay unchanged. Preserve historical
IDs, tombstones and component versions; recalculate fingerprints with Rust and rewrite predecessor
links through explicit old/new mappings. Samples/collect-game and generic fixtures also contain
catalog/bundle history and cannot be missed.

The external HTML remains in Downloads and now shows seven fixed findings. Its 119 links and
16 finding IDs pass structural checks; visual browser qualification remains unavailable.
Pre-existing untracked myeasylog.log and vendor remain untouched. No remote publication is authorized.
