---
type: "Verification Evidence"
title: "Per-document persistence custody qualification"
description: "Typed document file custody, captured Save/Reopen Hosts, exact cleanup isolation and bounded native/public evidence."
timestamp: 2026-10-23T16:39:42Z
record_id: "30cc4d7af3a445d18d9adf5b795886e2"
producer_id: "codex-foundation-integration"
run_id: "foundation-r5c-custody-20261024"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "83eb9f39a96af5d9d255e41d7dcd39b31daadb93"
verified_by: "codex-root; standards review 01a16997-192c-7b91-8567-3e1ba9e2b9f2; spec review 01a16996-ca82-7943-89b6-c51dca4ef683"
---

# Verification

Qualified implementation: `83eb9f39a96af5d9d255e41d7dcd39b31daadb93`, on top of `aadb7b5`.
This is the first bounded custody step of the R5c design, not completion of R5c, R2, delivery
wave 1 or the full foundation rebaseline. Subsequent ledger/memory closure is documentation-only.
Both pre-existing untracked `vendor/dear-imgui-wgpu` files remain outside the change.

# Result

- Scene and Prefab retain concrete file lanes in separate maps keyed by `EditorDocumentId`.
  The existing Host-custody registry still owns physical retirement; there is no erased store,
  extra publication protocol or Runtime per document.
- Save captures the exact concrete Host together with its checkpoint/readmission at admission.
  The worker and acknowledgement retain that Host. A rejected Workspace receipt marks the
  original published Host uncertain, not a newly selected or rebound same-kind Host.
- Reopen retains the concrete Host and checks shared-handle identity, availability, document,
  revision, binding and Schema before native admission. Workspace publication and baseline
  reconciliation borrow that same Host; an unavailable Host cannot leave a published document
  followed by a panic or a false successful Prefab result.
- Close checks cleanup and removes custody by exact document. A held failed temporary cleanup
  preserves its slot/lane while another same-kind owner remains editable and saveable.
- Extension source exclusion traverses all retained Scene/Prefab lanes and remains fail-closed
  on unavailable custody. Current-target identity still comes from retained parent capabilities.
- Scene preview has a private explicit document binding rather than map-order selection.
  It preserves the existing singleton Scene view through Prefab/extension activation. The
  public same-kind Open/retarget policy is unchanged; its unique-predecessor assertion is not
  additive Open. The future multi-Scene selection policy remains Proposed.

# Evidence

## Environment And Scope

Windows, Rust 1.95.0 (`59807616e`, 2026-04-14), root `target`, `CARGO_BUILD_JOBS=1`, serial
Cargo and one nextest test thread. Native qualification sets `WGPU_BACKEND=dx12`. No remote
push, publication, extra target directory or automatic Clippy fixes were performed.

The new private owner fixture deliberately installs multiple concrete lanes to exercise the
custody contract. It is not an independent/public demonstration of additive same-kind Open.
Six new tests cover fourteen Scene/Prefab case combinations, including same-document/path
Host replacement, retired Hosts, non-active persistence, receipt refusal and held cleanup.

An initial feature selection omitted `serde` and selected zero tests; it is not acceptance.
The corrected pre-migration selection passed 26 existing custody cases in
`31d9450b-a629-4bde-8229-957555413b15`. The first mapped implementation passed those cases in
`bb73e020-9245-424a-beb1-c6d9766321ed`. The first new fixture failed because it omitted the
required authoring bundle; adding that fixture declaration fixed the setup. That failure is
not evidence of a production defect or a red-before-fix ownership regression. The first five
new owner tests then passed in `495cc12f-6dc6-4c03-ada7-365ce1f4f0c2`.

## Final Executed Qualification

| Scope | Actual result | Nextest run |
|---|---|---|
| Root, Reflect, Scene, Tooling and Editor libraries, final production code after simplification | 831 passed, 23 ignored | `e311567e-1ab9-4ae4-b4b9-99885e5784f1` |
| Four selected actual Windows/DX12 trajectories | 4 passed, 197 other cases filtered/ignored | `37cff340-f1cb-4ff3-a146-9adeeb81d60e` |
| Independently compiled Editor panel/document consumers | 30 passed across 10 binaries, 2 ignored | `0b7eb32f-dc30-4812-a0b3-27420e0198d5` |
| Final affected custody/owned Save/Reopen cohort, including the later held-cleanup test | 34 passed, 821 other cases filtered/ignored | `9ef2ae3e-01ae-4d5d-8a55-3ce69cf365d6` |

The supplemental held-cleanup test was added after the 831-case run. No production source
changed afterward; all six new owner tests execute in the final 34-case cohort. This record
does not claim that all 832 non-ignored final library cases ran together in one final command.
The earlier complete pre-simplification run also passed 831 cases in
`1de9bef9-1fdc-44db-9a5e-4cdcaa987106`.

The actual native trajectories were:

- `native_ordinary_scene_and_prefab_widgets_save_and_reopen_preserved_source`
- `native_resume_redraw_rebases_paused_idle_before_the_owner_drive`
- `native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime`
- `native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts`

These qualify the selected hidden-window authoring, preview, Game input and retirement paths;
they do not establish monitor scanout, accessibility parity, Linux/macOS or the complete native
release matrix. No ignored test is counted as a pass.

The whole-library and independent-consumer commands were:

```powershell
cargo +1.95.0 nextest run -p nara -p nara_reflect -p nara_scene -p nara_tooling -p nara-editor --lib --locked --test-threads 1 --no-fail-fast
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-editor-panel-module-consumer --tests --locked --test-threads 1 --no-fail-fast
```

The independent workspace also sets `CARGO_TARGET_DIR` to the existing root `target`. The final
custody filter selects `project_host::runtime::editor::persistence` and the runtime
`owned_save`, `owned_reopen` and `host_custody` modules under the same five-package feature union.

`cargo +1.95.0 clippy -p nara -p nara-editor --lib --locked -- -D clippy::correctness` passes.
It is not warning-free: non-correctness warnings, including large layouts and unrelated
Editor warnings, remain. `cargo +1.95.0 fmt --all -- --check` passes after formatting the
supplemental test; the intermediate formatting check correctly rejected its unformatted draft.
`git diff --check` and 105 local link targets across the four implementation documents pass.

Raw logs remain outside the repository: `nara-r5c-final-libraries.log`,
`nara-r5c-final-native.log`, `nara-r5c-final-consumers.log`, `nara-r5c-final-clippy.log` and
`nara-r5c-final-custody.log` in the local temporary directory.

## Independent Review And Limits

Two initial read-only lanes inspected exact custody and the Accepted contracts/pinned engines.
They identified migration hazards that were not being presented as newly reproduced public
single-lane defects: delayed Save Host lookup, uncertainty routing, same-locator Reopen Host
replacement, cleanup isolation and implicit preview/predecessor selection.

Five independent simplification lenses covered reuse, quality, efficiency, test value and
comments. The implementation removes redundant success checks already guaranteed by the
publication owner, flattens result handling, avoids an allocated whole-Workspace predecessor
scan and removes two narrative comments. Proof tests and trust-boundary checks remain intact.

Code review: harness-native fallback -- the dedicated `ce-code-review depth:full` attempt
`01a16993-b6b7-7c21-81d9-485f00cb281e` terminated with `status: failed` because its delegated
context lacked native subagent launch/collection tools. It produced no completed receipt.
Separate native standards and correctness/spec reviews then completed:
`01a16997-192c-7b91-8567-3e1ba9e2b9f2` and
`01a16996-ca82-7943-89b6-c51dca4ef683`. Neither found a confirmed actionable defect in this
bounded step. They were static reviews, not independent test execution. No CE full, merge,
validator or release gate is claimed. The root also manually inspected the final diff.

The spec review noted two combined coverage gaps. Held cleanup among multiple same-kind owners
was subsequently added and passed in the final cohort. An alias that matches only a later
retained same-kind map entry remains an explicit test gap for the source-exclusion convergence
step; it is not claimed as qualified by the independent public consumer.
All ten review-owned handles, including the failed orchestration handle, were closed.

# Follow-up

The global foundation goal remains active. Next bounded work follows the existing design:

1. Centralize exact-source and current-file alias admission across Browser, anchored Prefab and
   extension paths, including a non-first owner, atomic Save, directory rename and unavailable
   Host cases. Preserve source scopes and fail-closed behavior.
2. Adopt the multi-Scene preview policy and its native navigation/input behavior explicitly.
   Retained last-explicitly-selected Scene is recommended; neither a map's first entry nor
   active Prefab/extension implicitly chooses a new Scene or executable startup.
3. Replace same-kind predecessor Close with additive Open and exact failed-publication
   navigation restoration; delete displaced Open phases and UI text rather than keep aliases.
4. Normalize Prefab source-observation keys and retain every source's pending/failed refresh
   obligation. The current single obligation can still be displaced by a later source.
5. Retain trusted Play's source through cancelling and physical-retirement states before using
   source-local Close isolation there. Keep the current global Scene Reopen exclusion meanwhile.
6. Qualify the public two-Scene/two-Prefab workflow, non-active persistence, Close isolation,
   Exit/replacement and native presentation. Broader R5c projection/history/epoch work and the
   remaining R2 transaction-local/property/query/ergonomics work are not closed by this unit.

The pinned Godot edited-scene collection and Bevy asset/instance mappings support separating
retained sources from live selection/incarnations; neither supplies Nara's exact file-custody
contract. No ECS-substrate replacement was selected from these ownership findings.

# Citations

- [Bounded custody design](../../../../architecture/editor-document-persistence-custody-design.md)
- [Foundation rebaseline plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Native qualification scope](../../../../verification/native-editor-qualification.md)
- [Owner conformance tests](../../../../../src/project_host/runtime/editor/persistence/tests.rs)
- [Retained Reopen admission](../../../../../src/project_host/runtime/editor/persistence/reopen.rs)
- [Unchanged physical custody registry](../../../../../src/project_host/runtime/editor/persistence/retirement.rs)
