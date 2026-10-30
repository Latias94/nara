---
type: "Verification Evidence"
title: "Reference validity and current demand production replacement qualification"
description: "Verification Evidence for Reference validity and current demand production replacement qualification."
timestamp: 2026-10-30T17:23:12Z
record_id: "b16511fa33a74c2b808270394b28bf62"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-30-reference-validity-and-load-demand-plan.md"
git_branch: "refactor/foundation"
git_commit: "1ddf670"
verified_by: "serial nextest; independent public consumers; focused Windows DX12; correctness Clippy"
---

# Verification

The reference-validity/current-demand owning replacement is qualified in the bounded scope of
the [unit plan](../../../../plans/2026-10-30-reference-validity-and-load-demand-plan.md), under
ADRs0007/0011/0033/0047/0110/0122. The complete foundation Goal remains active. Target cooking,
packaged delivery, immutable pack backing/residency, rendering composition and the custom-drawn
Editor are not completed here.

The production change is a breaking replacement, with no required-only visitor alias, unit
AssetRef shape reader, compatibility conversion or new migration guide. Canonical artifacts use
their actual recipe producers; authored behavior metadata adds explicit Immediate demand.

# Source And Environment

Windows11/pwsh7, Rust1.95.0, serial Cargo with CARGO_BUILD_JOBS=1 and the root target. Main baseline
`8e175db`; selected documentation `1ddf670`; tests ran on the uncommitted owning replacement.
The final100 source/format files have normalized SHA-256
`8261e605212765f716a603b19441be7b073294b40763dec7e48028d90fc25bcd`.
Hash input is sorted repo-relative path and LF-normalized bytes, each prefixed by its little-endian
u64 length. Documentation and compiler diagnostic goldens are excluded. A local TEMP manifest
records exact paths; final integration rechecks staged source. Vendor and references are untouched.

Two positively identified NTFS LZX helpers compressed existing root target/deps while Nara Cargo
was stopped; both were stopped before resuming builds. No artifacts were deleted. Native evidence
uses hidden windows and the production DX12 backend; GPU model, scanout and other platforms are
not claimed.

# Result

- Recursive Schema and Runtime codec inventories carry Immediate/Deferred. Defaults, selected
  properties, actual Prefab property/Absent intent and identity rewrite preserve demand. Derive
  rejects zero-reference annotations through the actual sealed codec, including aliases/containers.
- Actual Product dependencies normalize with Immediate precedence and exactly match catalogue
  Required/Reference before traversal. Missing, extra and wrong-role edges reject; Soft cannot
  satisfy payload references or start a target decoder.
- Ordinary preparation follows admitted Immediate demand with canonical pending priority over
  discovered work. X revealing earlier Y, reversed recipe insertion, once-decoded typed/native
  sharing and exact/one-short overlapping costs are covered. Flat Scene data grants no travel.
- Deep Check follows both mandatory roles using the same acquisitions, disposing heavy output per
  member. Independent compact Image/TileSet/owner-parsed Tilemap facts retain CPU cross-member
  checks. Summary growth admits old/new capacity with live decoder work and preserves old values
  after refusal.
- Source observes Deferred binding/readability without claiming body/compound validity. Strict
  saved-source Check/Export follows both roles and actual importer reads, with a separate complete
  delivery receipt. Authoring captures dirty Immediate Session/Schema/revision; Deferred body
  revisions are excluded while metadata/file/parent binding changes revoke narrow observations.
- Editor compiled preparation checks actual Scene/Image/TileSet membership separately from
  resolved identity. Upgrading a Deferred-only target to Immediate refuses with
  `project.content.asset-not-prepared` until content is recaptured.

# Evidence

| Selection | Actual result and nextest identity |
|---|---|
| Reflect focused, including new derive compile-fail cases |94 passed; `b2b964ab-84fe-4574-87d1-435ab5c67a17` |
| Reflect regression excluding already executed derive UI |387 passed; `296c85ef-dab0-4c01-840e-a9a2721f3cb1` |
| Asset/Input/Image all features |464 passed,1 existing skipped; `1dad5df6-0700-4793-aec5-59631c7b3a5a` |
| Initial root content/Authoring/prepared selection |61 passed,3 failed; `75e3daa7-3d7d-404d-bd3c-089c6d4865d9` |
| Corrected Source/Authoring/catalogue and deep growth |30 passed; `2d0bda27-7708-4ff8-a04e-f6dacf1975ca` |
| Scene/Tilemap all features, including public Prefab demand |891 passed,3 existing skipped; `ae7b2084-8df2-44f5-ba4e-cfdbcde7dfe8` |
| Independent processed A-B-C/D workflow |10 passed; `e0929993-e620-418a-b83c-2a2435b5cbe3` |
| Editor prepared-membership red oracle |1 failed against old helper; `4be4beab-71d1-4a67-a1e3-15cf86852d47` |
| Final Source after native membership correction |6 passed; `89190481-d001-484c-bfad-2c6c7f0b56ea` |
| Root all-feature library broad run |1083 passed,1 obsolete budget expectation failed,4 ignored; `42cbd33b-9389-4d37-8ce3-6f180f52dc07` |
| Replacement overlap/refund and source Check/Export group |8 passed; `cffd9831-df89-49fd-8336-ca46850d5a74` |
| Independent content broad run,8 binaries |59 passed,5 failed; `2c560817-e65b-44bd-98e6-e24446ae00d2` |
| Final affected independent Scene/TileSet/Base/Host binaries |41 passed; `60bcd7d4-20e4-492e-a1ed-38c98cce6c16` |
| Root content integration,4 binaries |40 passed; `47dff287-87bb-46a8-bdf1-544bc74c2c2e` |
| Selected actual Windows/DX12 Editor trajectories |2 passed; `ff8f67a0-02fe-4bc2-8477-0d3c53ea5b7c` |

The initial root failures were two Source fixtures lacking a real declared bundle and a production
Authoring duplicate-record insertion during Deferred to Immediate upgrade. Fixtures now generate
the actual plan bundle. Owning admission borrows/reuses only exactly equal records, preserving
conflict rejection and pre-allocation accounting; exact/one-short and conflict regressions pass.
The Editor P2 was independently reported, reproduced red and fixed at the snapshot owner. Its
Scene/Image/TileSet test covers refusal, original-source reuse and final budget return. Independent
Runtime and Source rereviews found no further confirmed P1/P2 in their stated scopes.

The broad root's sole later failure assumed graph scratch never overlaps Scene decode. Actual-first
discovery deliberately keeps indices live. Its replacement proves a decoder-only cap refuses
the overlap and capacity returns for a real standalone acquisition. Six prepared owning tests
retain complete exact/one-short success/failure controls. The affected replacement and all7 source
Export tests pass; the complete broad run was not repeated after these test-only corrections.

Independent broad failures were two missing Schema-agreement observation distinctions, an old
missing-Product error expectation, a silent codec rejecting another deep root before the intended
defense, and a producer listing unused C/SECOND with old family-order counts. The producer now
declares actual references; counts follow discovered definition order. The silent fixture only
underreports its intended primary target; a correct-inventory/lax-identity fixture separately
reaches cardinality. All41 affected tests pass. The processed Host assertion panic initially
aborted on Windows; that failed attempt establishes no native memory diagnosis.

Additional completed checks:

- Workspace all-feature/all-target correctness Clippy passed. Non-correctness warnings remain;
  this is not a warning-free claim.
- Minimal non-serde Asset/Reflect/Scene libraries and root `runtime-2d,serde` library check passed.
  Root all-feature library check passed; final all-target Clippy compiled the production/API surface.
- Two-level-game's actual headless tool Check succeeded with8 members. Its current root-target
  executable completed `--verify-travel`,4 travels and1 Retry, exit0. An erroneous initial command
  selected an old nested-target executable and failed manifest parsing; the absolute root-target
  command passed without product changes.
- Actual producers regenerated Reference, image-import, collect-game, two-level and renamed-root
  bundles. Renamed-root's one-off private-recipe generation test was removed, leaving no source
  diff. The authored behavior catalog explicitly declares Immediate. All six JSON artifacts retain
  owner membership and every actual asset leaf carries demand. No legacy reader was added.
- Rust formatting and changed/new document relative links were checked during integration. Final
  staging adds whitespace, memory and source-scope checks.

Native tests were `native_retained_scene_receipt_survives_prefab_navigation_and_bound_close` and
`native_ui_canvas_priority_survives_closing_retained_scene`, ignored-only nextest with
WGPU_BACKEND=dx12. These prove retained-source/navigation/Canvas scope, not the complete native
suite, OS IME, monitor scanout or the future custom-drawn frontend.

# Follow-up

CP3 proves decode/current membership and bounded ownership, not RSS, frame-P99 or allocator
isolation gains. The eager base still captures every encoded member. Deep Check retains bounded
summary capacity, conservatively including catalogue-native members; unused metadata is not free.
Source strict validation disposes finite per-root snapshots, not streaming importer units.
Saved-source Check cannot certify all dirty Sessions. No generic validator, residency cache,
prefetch scheduler, VFS, loader service or second source interpreter was introduced.

Commit the coherent replacement and locally ff main, then continue the complete foundation plan.
CP4 backing/residency, Schema/claim/history evolution, motion/execution, target-build and custom
UI/render retain separate complete outcomes. No push, PR or reference update occurred.

# Citations

- [Selected design](../../../../architecture/reference-validity-and-load-demand-design.md)
- [Owning plan](../../../../plans/2026-10-30-reference-validity-and-load-demand-plan.md)
- [Native procedure](../../../../verification/native-editor-qualification.md)
- [Integration record](../../progress/2026-10/2026-10-30T163329Z-demand-aware-public-travel-and-editor-prepared-membership-correction-verified-fccc1404b4914d28b825e008efa969fd.md)
