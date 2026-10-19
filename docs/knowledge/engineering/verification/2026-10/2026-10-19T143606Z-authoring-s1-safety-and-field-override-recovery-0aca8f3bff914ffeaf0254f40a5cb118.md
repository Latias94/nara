---
type: "Verification Evidence"
title: "Authoring S1 safety and field override recovery"
description: "Owner-local Prefab/render rejection fixes and complete data-only Reset/Remove history with explicit test and platform limits."
timestamp: 2026-10-19T14:36:06Z
record_id: "0aca8f3bff914ffeaf0254f40a5cb118"
producer_id: "codex-root"
run_id: "authoring-s1-20261019"
related_plan: "docs/architecture/authoring-interface-ergonomics-design.md"
git_branch: "refactor/foundation-contract-hardening"
---

# Verification

Scope is S1 / E1-E2 of the [authoring design](../../../../architecture/authoring-interface-ergonomics-design.md):
N01, N02, N08 and N09 from the [post-correction audit](../../../../audits/2026-10-19-foundation-next-stage-review.md).
The user selected implementation after the workflow-order refinement. The completed U1-U15 plan
remains closed; S2/S3 and G1-G9 are outside this correction. No Goal, push, PR or release was started.

| Finding | Owning correction | Code source |
| --- | --- | --- |
| N01 | Budgeted resolved Prefab sources receive pure structural admission before overrides/projection indexing; no extra native decode or public wrapper | `9f53aba` |
| N08 | Texture and view allocations have separate expected-OOM scopes; typed deferral preserves the prior viewport owner state | `6c2e31c` |
| N09 | Ready/rejected pipeline state retains deterministic content failure under the full key; cleanup removes the device namespace; allocation/device failures stay distinct | `18fcf4f` |
| N02 | Stored-presence UI, data-only Inspector admission and reversible Set/Remove lowering form one complete existing document transaction | `bfbd2b8` |

S1 is complete within the evidence and platform limits below. The final behavior source is
`bfbd2b8`; subsequent report/ledger updates do not change that tested code.

# Result

Windows, Rust 1.95.0, repository lockfile, shared target and `CARGO_BUILD_JOBS=1` throughout.
All Cargo invocations were serialized. Renderer runs selected `WGPU_BACKEND=dx12` and required
an adapter with `DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1`.

| Command / cohort | Actual result | Nextest run |
| --- | --- | --- |
| `cargo +1.95.0 nextest run -p nara_scene --test prefab_topology --features serde --locked --test-threads=1 --no-fail-fast` | 7 passed | `b0e16540-2faf-401c-a8c6-3d25a8b4d807` |
| Same package/features, `--lib -E 'test(prefab)'` | 34 passed, 99 excluded | `04d7518c-b167-4aa6-8499-2e7b329a140b` |
| `cargo +1.95.0 nextest run -p nara_scene --features serde --locked --test authoring_field_overrides --test authoring_decode --test data_authoring --test-threads=1 --no-fail-fast` | 23 passed | `cccd13a1-d1e5-4068-87ac-a3d7456d6e74` |
| `cargo +1.95.0 nextest run -p nara_render_wgpu --features editor-imgui,sprite-submitter,ui-submitter --lib --locked --test-threads=1 --no-fail-fast` | 125 passed, 3 ignored cases excluded; 38.572s | `7ad916a1-5952-4667-8529-ffb77a70fb28` |
| Same renderer configuration, three new failure selectors only | 3 passed, 125 excluded; overlaps the full renderer cohort | `d65e5a77-b82d-4690-a220-f52bbc4c42c7` |
| `cargo +1.95.0 check -p nara_render_wgpu --no-default-features --all-targets --locked` | Passed; existing feature-dependent unused/dead-code warnings remain | Not a nextest run |
| `cargo +1.95.0 nextest run -p nara-editor --lib --locked -E 'test(frontend::imgui::interaction_tests::field_overrides::)' --test-threads=1 --no-fail-fast` | 3 passed, 154 excluded | `17b322ce-4f32-4e71-9d47-da7938ed6538` |
| `cargo +1.95.0 nextest run -p nara_tooling --lib --locked --test-threads=1 --no-fail-fast` | 46 passed | `9da26e6e-7dbd-4ba4-a9f4-6b50847821bc` |
| Editor lib, `-E 'test(frontend::imgui::interaction_tests::)'` | 9 passed, 148 excluded; includes the three field-control tests above | `d3608750-f605-4165-9cdf-ec71a49d416f` |
| Editor lib, `--run-ignored only -E 'test(native_field_reset_and_remove_round_trip_authored_scene_and_prefab_overrides) \| test(native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts)'` | Both executed and passed; 155 excluded; 9.775s | `594a8471-e989-48cc-b974-c39dc8c1427f` |
| `cargo +1.95.0 fmt --all -- --check`; `git diff --check` | Passed after final Rust changes | Not a nextest run |

The two native trajectories use the same serialized `cargo +1.95.0 nextest run -p nara-editor
--lib --locked --test-threads=1 --no-fail-fast` prefix as the ordinary Editor cohorts, plus the
ignored/filter arguments shown. Their hidden windows and owned Context input exercise actual
registered WGPU presentation, not global pointer or keyboard injection.

# Evidence

- The seven public Prefab tests exercise in-memory and custom resolvers, nested sources,
  dangling roots/children, duplicate IDs/cycles, Asset DB and non-DB spawn/expand, preexisting
  identity and fresh Worlds. Entities, sentinel component, hierarchy roots, identity statistics
  and change ticks remain unchanged on rejection. Default/override and bounded retained
  diagnostic loss accounting remain intact.
- Renderer tests invoke the real invalid WGSL acquisition four times with one compile attempt,
  then draw a valid material with the same ID and different source, reading back green pixels.
  Device cleanup/replacement and a fresh backend revalidate. Injected allocation deferral does
  not poison the content cache; device loss takes its namespace path.
- N08 checks texture and view allocation-result failure separately, old target stamp/receipt and
  generation retention, no new view publication on failure, successful smaller retry and normal
  retirement. The test uses controlled result injection, not actual VRAM exhaustion. It does not
  claim a native resize with injected external-registration failure or a global memory budget.
- Actual N02 control tests cover Scene and Prefab required/default, optional/default,
  optional/no-default and explicit Null, exact RemoveField target/revision, disabled repeated
  removal, Undo/Redo, raw key omission after Save/Reopen and unavailable-record preservation.
  Root-value/list-index controls remain disabled; required/no-default remains unremovable.
- Lowering keeps public `AuthoringFieldEdit` write-only. A private Option-valued field change
  uses the existing patch owner, final classification, complete manifest comparison and bounded
  forward/inverse validation. Setting an absent key records inverse Remove; removing an authored
  key records inverse Set. Required/readonly/unavailable target and oversized inverse failure
  reject without a document/history publication. ADR 0090 records this finite refinement.
- Independent correctness reviewed N01/N02, including the expanded owner chain and root/index
  control restriction. Adversarial review checked N08/N09 and the locked WGPU error scopes/key
  and device lifecycle. Maintainability review found no new authority or unnecessary framework.
  All returned zero material findings; no cross-model review is claimed.
- Documentation checks passed for nine changed/new Markdown files and 772 local links, with
  balanced fences. Both external HTML reports passed nesting, unique anchors and local/internal
  link checks with no external resources. Engineering memory validates with existing legacy
  metadata/path advisories, and its derived views are current. No browser visual test is claimed.

Initial failed probes were resolved rather than suppressed: Prefab diagnostic assertions used
`len()` (observed entries) instead of bounded `retained_len()`; the first UI Reset correctly exposed
the data-only Set-only gates and missing inverse-absence behavior; the extended tooling fixture
needed the current complete Sprite creation template rather than an incomplete hand-written
record. Runtime schema validation was not weakened. An initial feature build also caught the
new allocation-error enum arm, now mapped consistently through the existing deferred path.

# Follow-up

The future correlated submit/draft lifecycle, component inventory/range metadata and broader
async/frame-job/multi-document work remain separate. Native evidence is limited to the actual
selected Windows trajectories. No Linux/macOS qualification, OS pointer/IME input, compositor
scanout, shader timing or real OOM behavior is established by these checks. No full workspace
suite or release/export cycle is claimed. Existing dead-code/feature warnings were not expanded
into unrelated cleanup.

# Citations

- [Prefab public-input regressions](../../../../../crates/nara_scene/tests/prefab_topology.rs)
- [Data-only field override regressions](../../../../../crates/nara_scene/tests/authoring_field_overrides.rs)
- [Actual Inspector control regressions](../../../../../editor/src/frontend/imgui/interaction_tests/field_overrides.rs)
- [Native field override trajectory](../../../../../editor/src/platform/imgui/authoring_workflow_tests/field_overrides.rs)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
