---
type: "Work Progress"
title: "Selected capability admission and legal cross-layer unavailable controls"
description: "Separates known Scene capability from missing Adapter and replaces false-positive same-layer Properties refusal fixtures."
timestamp: 2026-11-02T17:51:53Z
record_id: "a24faed38ecd429abad2ebbd8b3e774c"
producer_id: "codex-foundation-main"
run_id: "extraction-context-20261103"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "88fd728"
---

# Summary

Continuation of the [physical contributor/traversal checkpoint](2026-11-02T174336Z-complete-physical-contributor-mapping-and-bounded-extraction-traversal-d1926e8c625c40b3b66654d70756b2ff.md).
The complete foundation Goal remains active. Main and `refactor/foundation` remain `88fd728`;
the authorized typed-root/extraction WIP is unstaged and uncommitted. This final correction closes
the scoped authoring capability P2 and replaces misleading Properties refusal fixtures. No
complete CP2, project publication/recovery, public extraction or native qualification is claimed.

# Details

## Admission Boundary

Goodall's independent follow-up found that complete unavailable overlay preservation could hide
a known Scene-capability refusal. A matching current owner/version with no Adapter became
KnownUnbound; omission removed it before final validation. The red public test actually observed
a forbidden PrefabAddition returning a candidate and KnownUnbound omission. Run
`4d3c7372-8278-46b0-878c-c0fcfa103bb0` executed2: the corrected legal cross-layer control passed,
and the capability control failed. This is a reproduced selected-authoring regression, not a
Native/Mixed execution bypass.

`materialize_program` now checks KnownUnbound's actual current Schema with the same authority's
existing `validate_current_schema_capabilities`, before cloning or accepting an opaque record.
The shared helper is crate-visible; no duplicate validator, Adapter requirement or general proof
API was added. The check applies equally to selected Source, Addition and Set. Unknown, future,
old-version and wrong-owner payloads retain their original interpretation-free paths. Dormant
inventory classification is unchanged. ADR0090 and the CP2 plan now state that missing implementation
and known declaration capability are distinct facts.

## Test Validity Correction

The earlier public test placed Properties in the same overlay as its complete record. Set cases
could refuse as DuplicateIntent, and direct same-layer addition targeting could refuse before
unavailable interpretation. Its previous passing run therefore did not prove that branch.
The replacement uses real Scene-to-U-to-V composition: U owns an Addition or complete Set in its
V instance; the Scene's outer Properties routes through U:u at V. With the same legal structure,
the Available control yields value9 without a pin and11 with it. Five unavailable reasons preserve
plain observation and specifically refuse a pin as Component, not DuplicateIntent. Actual reads,
raw source equality, typed omissions and Native preview filtering are checked.

The separate no-Scene/no-Adapter test covers Addition, Set/Add and Set/Replace. Exact current
owner/version must refuse as Component. Old/future versions and wrong owners preserve uninterpreted
raw values/defaults instead of applying a different Schema's constraints. Goodall's final scoped
static review closes the unavailable-observation P2, capability P2 and test-validity gap, with no
remaining assigned P1/P2. Review was read-only and did not independently execute Cargo.

## Final Verification

Serial Windows/MSVC Rust1.95.0, jobs1, root target, default thread stacks:

- Final complete Scene library plus `authoring_schema_lookup`, `prefab_deferred_values` and
  `prefab_conversion_basis`: **745/745**, no skips,42.684s;
  `f1cee1ae-44dd-4240-9d4f-6778c0623807`.
- Final Reflect/Scene all-feature/all-target Clippy with `-D clippy::correctness`: pass,26.53s.
  Existing general warnings remain; this is not `-D warnings`.
- Final minimal Scene library: pass,1.31s. Root all-feature library: pass,6.56s.
- Final `cargo +1.95.0 fmt --all -- --check` and `git diff --check`: pass.
- The plan, supplemental reference study and ADR0090 have23 checked local file links, none missing.

The earlier195/195 Reflect and1085/1085 Scene full-integration results retain the scopes recorded
in the predecessor. The final745 run covers the last capability change; it does not rerun every
unrelated integration binary or establish Native Editor/fresh-process acceptance. Session34424
is terminal0; no Nara Cargo or helper remains from this lane. All seven agents started or resumed
in this continuation are closed. Other project processes and original vendor/reference trees
were not modified. No artifact cleanup, remote push or PR occurred.

# Next Action

Continue the complete CP2 plan at real Workspace capture/preparation/late binding. Share private
immutable source/classification capture and source/history checkpoint installation with CP1 only
when CP2 is the concrete second consumer. Keep nominal proofs distinct and basis unchanged.
The planned source checkpoint seam must preserve complete open membership, both history branches,
actual allocation/revision/content observations and one allowance. Guard-only participants do not
implicitly lose history; Q initially remains unopened. Then integrate real all-profile/dirty-source
inventory, borrowed Review, observed-absent Q/sidecar, extraction-specific Pending/recovery and
bind-all-before-Ready. Qualify independent/public and fresh-process/native workflows before precise
commit/local ff main. Do not repeat the completed broad research receiving review or manufacture
a CP1 Schema successor. Foundation remains active.

# Citations

- [Complete CP2 plan](../../../../plans/2026-11-02-prefab-extraction-plan.md)
- [Pinned context/traversal comparison](../../../../research/2026-11-03-prefab-context-and-bounded-traversal.md)
- [Schema capability authority](../../../../../crates/nara_scene/src/schema_authority.rs)
- [Selected value admission](../../../../../crates/nara_scene/src/prefab/overlay/compile/staged/values.rs)
- [Public authoring controls](../../../../../crates/nara_scene/tests/authoring_schema_lookup.rs)
- [Lossless authoring contract](../../../../architecture/adr/0090-unavailable-schema-and-lossless-authoring.md)
