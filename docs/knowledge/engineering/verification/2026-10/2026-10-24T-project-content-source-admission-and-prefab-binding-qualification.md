---
type: "Verification Evidence"
title: "Project content source admission and canonical Prefab binding qualification"
description: "ProjectContentLoader admits explicit Scene/Prefab source keys, retains metadata closure members, and lends canonical Prefab bindings through expansion."
timestamp: 2026-10-24T08:44:57Z
record_id: "04269ce355174c9e9eb94dd7fa5a29ff"
resource: "tests/project_content_boot.rs"
tags: ["architecture", "identity", "scene", "prefab", "verification"]
status: "qualified"
producer_id: "codex"
run_id: "session-01a1629b-source-admission"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_base: "e652aa8"
verified_by: "serial nextest; focused project content and Scene cohorts; workspace check; scoped Clippy correctness; cargo fmt"
---

# Verification

# Result

The production project-content owner now admits one canonical `SourceDocumentKey` for the
selected Scene and every loaded Prefab before expansion. `project_location` binds the authored
source universe and complete project-relative path. `stable_metadata` requires the exact sidecar,
validates its relative path and Scene/Prefab kind, retains the sidecar in the content closure, and
rejects missing or mismatched metadata without minting a replacement identity.

The Prefab resolver lends the canonical key and immutable document as one `ResolvedPrefabSource`.
Prefab projection steps retain the key, and the active cycle stack compares canonical keys when
available. Existing path and stable-asset lookup still belong to the admitted project catalogue;
the loader does not create a second source registry.

Location policy keeps semantic source identity separate from catalogue lookup evidence. Metadata
for every loaded, catalogued Prefab is reconciled after all Prefab/domain/TileSet/Image discovery,
then revalidated and retained in the exact closure. Unreferenced adjacent sidecars do not silently
promote location identity. Stable Prefab records use the existing duplicate-ID catalogue admission.
The finite delivery union checks selected Scene and loaded asset stable IDs against full project
paths; it neither retains decoded snapshots nor becomes a permanent source registry.

Canonical provenance has a separate 64 MiB default logical allocation budget. Active ancestry,
each copied parent chain, keys, requests, map entries and identifiers are charged before copying.
The root content work plan reserves this category before expansion. Exact-name opens also reserve
their modeled buffer and Linux three-handle peak. These models do not claim allocator or RSS bounds.

This checkpoint is intentionally narrower than complete source/occurrence adoption. Runtime
`SceneSource` and `SceneEntitySource`, logical occurrence claims, Browser/Editor admission,
Runtime/Canvas UI keys and source-carrying Export remain follow-up owner boundaries.

# Evidence

- Final root cohort: `cargo nextest run -p nara --all-features --lib --test project_content_boot
  --test project_content_limits --test project_content_delivery --test project_scene_content
  --test project_workflow_creation --test project_workflow_cli --locked -j 1 --no-fail-fast
  --failure-output immediate --status-level fail --final-status-level fail`: 440 passed,
  6 skipped, run `b76c53f6-2710-487c-9aec-59437e912d66`. It includes all 384 enabled root-library
  tests, exact content/retention/budget failure boundaries, late Prefab and late TileSet metadata
  discovery, source spelling rejection, duplicate native identity refusal and independently built
  generated project creation/CLI behavior. Existing ignored tests remain ignored.
- Final Scene/FS cohort: `cargo nextest run -p nara_scene -p nara_fs --all-features --locked
  -j 1 --no-fail-fast --failure-output immediate --status-level fail --final-status-level fail`:
  381 passed, 4 skipped, run `3f35acfd-8985-416e-82bf-ecea30082b47`. This includes canonical alias
  cycles, repeated siblings, wrong-kind refusal, exact/one-less provenance limits, long keys and
  nested-chain accounting, and public spawn refusal preserving World/identity/hierarchy.
- [Exact spelling evidence](2026-10-24T095418Z-exact-stored-spelling-through-filesystem-capabilities-79d51b36d2944aa0bc70384eb2d11b86.md):
  Windows FS 83 passed/3 skipped; final WSL Linux FS 89 passed/0 skipped, run
  `0526c880-3060-4f2f-bdcf-cc7dfce32b08`. Linux filesystem tests do not qualify native product
  loading or a casefold-volume deployment.
- Final serial integration gates passed with `CARGO_BUILD_JOBS=1`:
  `cargo check --workspace --all-features --all-targets --locked`,
  `cargo clippy -p nara -p nara_scene -p nara_fs --all-features --all-targets --locked --
  -D clippy::correctness`, and `cargo fmt --all -- --check`. Existing non-blocking warnings
  remain; this does not claim a warning-free workspace or additional native qualification.

## Counterexamples And Review Disposition

| Finding | Evidence and owning correction |
| --- | --- |
| Metadata read swallowed budget failures | The new file-budget test first failed with `AssetMeta` instead of `BudgetExceeded`; native reads now preserve existing typed I/O/budget errors. |
| Duplicate path-loaded stable Prefabs | The new test first returned a successful snapshot for two different paths with one stable ID; both now enter existing catalogue admission. |
| Partial catalogue treated as complete | Initial integration rejected a legal path Image after indexing only its Prefab; complete-catalogue admission is now explicit. |
| Wrong-kind canonical Prefab key | Public expansion first accepted a Scene key; it now rejects before publishing provenance. |
| Late stable reference omitted earlier Prefab sidecar | The A/C path-then-stable-reference regression first failed on the missing closure member. |
| TileSet stable Image discovered metadata after expansion | Its separate regression first failed on the missing Prefab sidecar; final reconciliation now follows all discovery and changes digest/revision when that metadata changes. |
| Duplicate stable Scene roots in delivery | The new delivery test first succeeded incorrectly; a bounded full-project-path identity union now rejects. Selected Scene/loaded-asset identity conflict is also tested. |
| Windows lookup aliases created distinct location identities | Native exact spelling is enforced before native source admission, including directory prefixes; existing opens retain their semantics. |
| Unbounded copied canonical provenance | Static review demonstrated long root paths multiplied by depth and leaf count; independent pre-copy logical limits and root work reservation now bound that added work. No red-before-production run is claimed for this static finding. |
| Linux entry/name race and extra descriptor | Independent review found missing dirent-inode association and a three-versus-two-handle peak; deterministic replacement, scan-limit and actual fixture-descriptor tests now execute on WSL. |

Independent correctness and adversarial agents reviewed the owning source/closure/retention and
filesystem boundaries. The final bounded follow-up (`01a16d68-ed55-7513-95b9-83b2302a1991`)
found no new in-scope defect and statically confirmed the four final corrections: pre-copy
provenance budget, post-asset reconciliation, exact inode/handle association and delivery identity
union. It executed no tests; execution evidence above belongs to the main integration owner.

Code review: skipped (ce-code-review unavailable). The actual delegated portable review attempt
(`01a16d66-2ed6-74e0-a4f1-8bf6d731e277`) returned `status: failed` because its client lacked the
nested dispatch/collection facilities required by that workflow. No completed portable receipt is
claimed. The independent native scoped reviews and final manual diff scan remain recorded, rather
than being relabelled as that workflow's completion.

# Follow-up

Carry the key through `SceneSource`/runtime publication, add logical occurrence claims inside the
same exclusive publication cut, then migrate Runtime/Canvas UI identity and Browser/Editor source
admission. Keep source admission and occurrence identity separate; do not infer occurrence from a
source key or from runtime instance counters.

The next unit must replace identity-free materialization with required source-bound candidates and
canonical Prefab bindings, not retain an optional-key fallback. Scene owns active namespace/slot
claims with source and exact activation as values, including empty instances, and releases them
inside exact publication/end/epoch cuts. Ordinary no-entity prepare/cancel and shared Runtime/Canvas
identity/refreeze follow. The independent pinned Bevy/Godot owner review is integrated into the
source/occurrence design; it is not evidence that those downstream units are implemented.

# Citations

- [Source key](../../../../../crates/nara_scene/src/source.rs)
- [Prefab resolver binding](../../../../../crates/nara_scene/src/prefab.rs)
- [Project content owner](../../../../../src/project_content.rs)
- [Source/occurrence design](../../../../architecture/scene-source-and-occurrence-identity-design.md)
- [ADR 0038](../../../../architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md)
- [Foundation rebaseline plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
