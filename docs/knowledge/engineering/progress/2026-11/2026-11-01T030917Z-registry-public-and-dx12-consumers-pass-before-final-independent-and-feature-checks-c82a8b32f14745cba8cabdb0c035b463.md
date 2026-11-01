---
type: "Work Progress"
title: "Registry public and DX12 consumers pass before final independent and feature checks"
description: "Root 1668, selected native4, owning and independent doctests and thirty direct consumers pass; renamed-root qualification and final checks remain active."
timestamp: 2026-11-01T03:09:17Z
record_id: "c82a8b32f14745cba8cabdb0c035b463"
producer_id: "codex-foundation-main"
run_id: "registry-ownership-20261101"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-11-01-component-registry-ownership-plan.md"
git_branch: "refactor/foundation"
git_commit: "5acab02"
---

# Summary

The previous goal turn made real source/document corrections and yielded domain/research evidence;
it was progress. Continue the complete active foundation Goal, with the uncommitted registry
ownership replacement still in qualification on `refactor/foundation`, base/main `5acab02`.

# Details

- Root/Editor and thirteen public integration targets passed all 1,668 tests, with43 existing
  skips, run `f8fac87d-98ee-40e7-ac2a-f4026a5c53ce`. Build12m38s, execution523.609s.
  Exec58573 is terminal0; do not restart it.
- All four selected native Windows/DX12 trajectories passed, run
  `00eb7883-ab30-48dd-ab83-820bad33a3d1`,13.981s. Env selected DX12, fallback0, required actual
  adapter. Exact root/editor all-feature library union reused the compiled targets in1.65s.
  Coverage: Game input/reopen/pause/refocus/resize/step; retained Scene/Prefab navigation/close;
  Canvas priority through Scene close; complete conversion/edit/Undo/Save/Reopen. This is the
  current frontend's scoped qualification, not the future editable Nara-drawn UI or all native tests.
- Reflect/Scene doctests passed7+9 compile-fail cases; one existing inventory-template example
  remains ignored. The independent module-consumer's three Builder/Snapshot misuse docs also pass.
- Independent module-consumer workspace all-target strict Clippy passed19.33s after one fixture
  correction. `scene_preparation::explicit_projection_definition_preserves_the_distinct_managed_host_basis`
  had retained an old default-object projection argument; it now explicitly supplies an empty
  database resolver under ROOT. No production API or authority guard was relaxed.
- The direct independent consumer passed30/30 across inventory, manual codec, Scene preparation,
  data components and property authoring, run `b5dd1fe3-5ff9-4ea1-b7af-f05b5c69307f`,3.943s
  after3m19s build. This includes failed-binding/migration repair and original prepared-definition
  retention. Other feature-specific Root dependency warnings are unchanged.
- Renamed-root initially refused `--locked`: its existing lock omitted the prior Input module's
  serde/serde_json dependency changes. Offline minimal resolution adds only itoa/serde_json/zmij
  and updates that path package's dependencies. A metadata invocation without target filtering
  tried uncached js-sys and failed offline; it did not run tests. Pinning the added serde_json and
  zmij to existing Root versions1.0.150/1.0.21 preserves the remaining locked graph. Nextest's
  normal filtered metadata succeeds offline. Exec57552 is now compiling/running the renamed-root
  fixture with `--locked --offline`; TEMP log:`nara-registry-owner-renamed-root-tests.log`.
- A separate read-only next-R5 design lane produced the EnemyStats value/claim upgrade consumer.
  Its proposed [unit plan](../../../../plans/2026-11-01-authored-schema-upgrade-plan.md) selects
  reviewed dirty-input saving and a new history checkpoint, complete declaration-tree conversion,
  exact source/Schema/Adapter bases and operation-owned process recovery. It remains Proposed,
  outside the registry commit. Author Bohr `01a194ff-5556-7820-9d08-eca54192d320` is closed.
  Independent adversarial reviewer Epicurus `01a1950c-d0e3-73c2-a68c-fc84fa489b4f` is read-only
  and active, challenging actual inventory, pending/readiness and recovery proof. No R5 production
  code or Accepted contract has changed for this draft.

# Next Action

1. Poll exec57552 before another Cargo. Preserve jobs1, root target, no push/PR/vendor/reference
   changes. Other system Cargo can belong to unrelated Baozi projects; do not stop it.
2. Finish changed independent fixtures/Reference consumers, minimal/native-wasm compile checks,
   correctness Clippy and final fmt. Root/selected native results stay valid: subsequent source
   changes affected only the independent resolver fixture and its separate lock.
3. Inspect/stage the exact registry code/contracts and commit the completed replacement. Then add
   final evidence/ledger/unit status, commit documentation and ff local main. Keep the new R5 draft
   separate until reviewed. Full foundation Goal remains active; no module/research scope completes it.
4. Collect Epicurus's decision-ready counterexamples and reconcile the R5 draft before selecting
   its exact publication/inventory contracts and implementation. Do not weaken current creation-only
   or generic authoring guards to implement a structural upgrade.

# Citations

- [Registry ownership unit](../../../../plans/2026-11-01-component-registry-ownership-plan.md)
- [Prior progress](2026-11-01T024221Z-supplied-research-coverage-rechecked-and-registry-public-qualification-running-a9175ee5fe6a461f8a4de88657cfe000.md)
- [Next proposed unit](../../../../plans/2026-11-01-authored-schema-upgrade-plan.md)
