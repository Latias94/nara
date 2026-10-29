---
type: "Verification Evidence"
title: "Processed Runtime Scene and complete product binding qualification"
description: "Qualifies source-independent Scene wire and native/data transactions, Product-index ownership and staged new-resource publication without a source database."
timestamp: 2026-10-29T06:08:03Z
record_id: "cd202f5ff4514802a3c5f57a32b43b7a"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "refactor/foundation"
git_commit: "89d9bc8"
---

# Verification

Continue the complete active foundation Goal and ordered Runtime delivery plan from `3c62307`.
This turn is **progress**, not completion of R6/R7 or the full Goal. Production implementation is
`89d9bc8`; separate rendering research/plan refinement is `cb0a46b`. Rust 1.95.0, Windows 11,
PowerShell 7, serial Cargo, `CARGO_BUILD_JOBS=1` and the existing root target are used, including
the independent consumers and Reference Game. No remote publication or vendor/reference mutation.

# Result

Scene owns a distinct bounded `runtime_scene` version 1 JSON envelope. Cook consumes an actual
source-bound expansion, fills defaults, rewrites hierarchy/declared local references and converts
declared assets through the real database to complete Products. Existing auxiliary selection
survives. Containing source, canonical Prefab keys, lookup evidence and typed addition/source
ancestry survive encoding. Derived IDs use injective framed canonical addresses, independent of
lookup spelling or temporary compiler locators. Preserved locals and duplicate logical addresses
cannot collide silently. Ordinary strings retain their meaning.

Raw decoding/admission execute neither native codecs nor source migrations. Strict wire refuses
source fields even when null/empty, malformed provenance, noncanonical IDs, absent defaults,
wrong owner/version/shape, non-Product assets and surviving-target failures. Expected containing
source and exact definition witness accompany the move-only immutable candidate. One private
closed input selection reuses ordinary native/data preparation, occurrence/hierarchy and guarded
publication, including advanced native claims, resource writes and retry/discard. No fake source
candidate, ProjectManifest, source database or second transaction is introduced.

The initial pure-data asset consumer did not prove native handle resolution. During closure,
parent review found that Product references still always required source metadata. The owning
fix replaces primary StableId indexes with full Product indexes in `AssetServer`. Source paths
alias PRIMARY, stable-source helpers do not erase auxiliary identity, nil source records reject,
and clone/type/failure atomicity remain. Product resolution uses existing bindings only; explicit
reservation does not prove product membership, format or residency. Reverse export preserves
auxiliary and pathless primary Products. Source database routes remain primary-only.

The existing component batch and narrowed `SceneAssetBindings` can stage newly admitted Product
identities. The independent native consumer supplies actual values through an owner resource and
the existing product transaction. A final native failure or explicit discard publishes neither
new identities nor values. Successful commit publishes bindings, values and Scene together;
cancelled process IDs are not reused. This qualifies that owner-controlled native text consumer,
not a Runtime codec/catalog for every built-in Image/TileSet resource family.

Working bytes are cumulative modeled requests for the processed owner and admitted reference
work, not allocator/RSS. Existing hierarchy/default-validation/local-reference and JSON/Core
syntax/preflight scratch retain their own limits. Actual product admission and source-free base
integrity remain the content Host's obligations.

# Evidence

| Final scope | Actual result |
| --- | --- |
| Asset all-feature library | 134 passed, no skips; build9.04s/run5.718s |
| Reflect/Scene/Sprite/Tilemap/Image all-feature cohort | 1334 passed across 71 binaries, four default skips; build8m08s/run86.835s |
| Independent processed/native-asset/preparation/data consumers | 21 passed across four binaries, no skips; build15.80s/run1.345s |
| Final root startup/content delivery/Play | 70 passed across four binaries, one default skip; build2m58s/run15.207s |
| Final native Editor Windows/DX12 critical group | All three executed/passed; 283 outside-selection skips; build3m37s/run6.505s |
| Reference Game all-feature eight-binary cohort | 57 passed, no skips; build5m30s/run30.013s |
| Asset/Reflect/Scene/root/Editor all-feature/all-target correctness Clippy | Passed; 2m31s; warnings remain, including style and large-error-size suggestions |
| Scene no-default library check | Passed; 3.82s |
| Root no-default `runtime-core` library check | Passed; 7.51s |
| Root no-default `runtime-2d,serde,tooling` library check | Passed; 16.89s |
| Root, independent consumer and Reference Game formatting | All check-only invocations passed |

Nextest IDs: Asset `fa72ef11-5322-41e4-9e9d-893931667d50`; domains
`129c2302-21af-40a6-9aec-7806aafc1ef3`; independent
`e671037f-7fe7-497e-b3ad-6ebcecb32e24`; final root
`754a2400-1505-41b6-95b5-bfcd5b87e8d9`; final native
`9b7763a3-a3df-48bc-9591-b3801ae8666f`; Reference Game
`14d88341-efad-455b-9bd1-8a59fdd092c3`.

Domain package selection is `nara_reflect`, `nara_scene`, `nara_sprite`, `nara_tilemap`, `nara_image`,
all features, locked, one test thread. Independent selection is `runtime_scene_assets`,
`runtime_scene_delivery`, `scene_preparation`, `scene_data_components` in the separate
`nara-scene-module-consumer` workspace. It reads file-backed nested Prefabs/Scene, deletes source
members, round-trips cooked bytes and executes native/data/hierarchy/provenance. The three native
asset tests additionally exercise actual primary/auxiliary values, full reverse identity,
missing/wrong type, stale catalogue, private new bindings and failed/discarded/successful resource
publication. Managed App, foreign World, native claims retry, exact definitions, cancellation and
stale replacement preserve their corresponding owner invariants.

Final root selects `project_runtime_boot`, `project_content_boot`, `project_content_delivery`,
`workspace_play_runtime`. Earlier root library plus six integrations ran 997 passed/five default
skips (`ca265069-224f-4cd3-aaad-8567220b9b85`, build4m31s/run413.812s) before the complete Product
index correction. Earlier full Scene runs of 778 and independent18 likewise retain their exact
pre-correction scope; they are not reported as full final-root reruns.

Native selects the qualification procedure's three critical names: resume/redraw rebasing,
Game input/reopen/resize/Step, and independent view detach/recover/retire. This is real hidden-window
DX12 execution, not monitor scanout or Linux/macOS evidence. Reference Game selects `first_wave`,
`desktop_flow`, `desktop_parity`, `desktop_render`, `ui_product`, `ui_authoring_roundtrip`,
`project_manifest_ingest`, `plugin_composition` with all features. No complete workspace all-feature
or separate new doctest run is claimed. Changed documents passed local-target/fence and whitespace
checks; memory validation retains existing historical advisory warnings.

## Review And Corrections

Bernoulli `01a185e5-1a87-7660-b021-b79deae7dfb6` implemented processed Scene and Curie
`01a185e5-97bc-7542-b873-a67327a26c1b` integrated shared preparation in disjoint scopes. Hilbert
`01a185f4-2eae-73b0-b9ff-df4bca118793` independently found a cumulative rewrite-work boundary:
exact-size primary/subproduct values need record/path work in addition to decode and encode.
The correction derives that work allowance, preserves final value limits and aggregate admission,
and adds exact 6/7-node, one-short and shared-prefix/many-leaf regressions. The immutable candidate
and exact witness also justify deleting a redundant pure spawn gate; shared preflight still runs.

Volta `01a18618-d096-7ca2-8ac9-cf415cabbc7b` implemented the Product index after the parent found
the native gap. Hume `01a18627-1aaa-7f32-990b-3df2d10d1536` independently reviewed the frozen
identity/staging changes and found one fixture error: product configuration runs before deferred
native materialization, so a later codec failure does not undo an external callback counter.
Counts were corrected to1/2/3 while no-publication assertions remain and pass. No confirmed
production P1/P2 remained in that scoped review. Writers froze before shared fmt/Cargo; all agents
are closed. Source review is not test execution.

Rendering agent `01a185f5-5ff7-7733-b66e-730a4456d340` was created with the user's requested
`gpt-6-astra`/`max` selection; dispatch accepted it. Its unsupported inference that Astra could
not be selected was explicitly retracted, not treated as routing evidence. The research compares
pinned Bevy/Godot and keeps the initial ordered-plan recommendation distinguishable from the
parent's long-horizon graph preference. A follow-up/coherence review corrected insertion-order
authority, conditional graph participation and an accidental WESL prerequisite. `cb0a46b` updates
R8/R9 and Proposed0121; no rendering implementation/performance qualification follows.

# Follow-up

Keep the complete foundation Goal active. Next continue verified immutable Runtime-base/catalog
admission, trusted expected digest/target/recipe/Schema, actual processed resource codecs and the
same-base startup/later Scene acquisition through current Runtime/retention/native owners. The
processed Scene and explicit weak Product binding do not themselves qualify that full workflow.
Retain source-carrying delivery as its own mode; no source/metadata/cache fallback may follow
processed-base selection. Input-map representation, build-target separation, CAS/group publication,
patch/DLC/remote services, broader languages and Nara-owned UI remain separate work.

The user authorized one long-lived `refactor/foundation` branch and periodic local merges. All58
local heads were already contained in main; the only active worktree was this checkout. Renaming
the empty current feature branch and guarded deletion of56 unused merged heads leaves main and
refactor/foundation. No unique commits, working files, remote refs or worktrees were removed.
Merge this verified checkpoint locally; do not push/PR or shrink the Goal. Vendor remains untouched.

# Citations

- [Ordered delivery plan](../../../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md)
- [Full foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Processed Scene contract](../../../../architecture/adr/0006-scene-and-prefab-data-model.md)
- [Asset ownership](../../../../architecture/adr/0007-asset-identity-and-import-pipeline.md)
- [Product identity](../../../../architecture/adr/0122-scoped-imported-product-identity-and-authored-continuity.md)
- [Public Scene consumer](../../../../../tests/fixtures/module-consumer/tests/runtime_scene_delivery.rs)
- [Native asset/transaction consumer](../../../../../tests/fixtures/module-consumer/tests/runtime_scene_assets.rs)
- [Rendering review](../../../../research/2026-10-29-render-architecture-and-refactor-plan-review.md)
- [Native qualification](../../../../verification/native-editor-qualification.md)
