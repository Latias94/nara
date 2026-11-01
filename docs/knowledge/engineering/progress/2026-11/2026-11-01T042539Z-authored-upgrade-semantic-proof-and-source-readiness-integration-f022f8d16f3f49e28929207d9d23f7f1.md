---
type: "Work Progress"
title: "Authored upgrade semantic proof and source readiness integration"
description: "The supplied research still matches; actual invalid payload regressions drove two-sided Schema validation and source-owned fresh history checkpoints, while project recovery remains in progress."
timestamp: 2026-11-01T04:25:39Z
record_id: "f022f8d16f3f49e28929207d9d23f7f1"
producer_id: "codex-foundation-main"
run_id: "authored-upgrade-20261101"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-11-01-authored-schema-upgrade-plan.md"
git_branch: "refactor/foundation"
git_commit: "ed21b23"
---

# Summary

Continue the complete active foundation Goal. Main and refactor/foundation still equal
`ed21b23`; this entire authored-upgrade replacement remains uncommitted. The supplied research
is the same archive already reconciled into the foundation plan. Its full SHA-256 remains
`99589eef3287cb9e353501bf1d27d1f4287ec825dbaa45c97abee38033ed2c36`; all91 manifest
members match and all10 imported session documents are byte-identical. No replacement omnibus
roadmap or new reference checkout is needed. Current development remains the selected R5
authored-value/claim upgrade, with full recovery and complete-set adoption still unfinished.

# Details

- Hegel's independent review exposed two actual semantic admission gaps. Six newly added tests
  reproduced all of them: changed versions could retain an invalid old-shaped addition or Set,
  invalid old addition payloads could pass, a remapped claim could contain an invalid payload,
  unchanged armor payloads could be invalid, and unchanged unknown selectors could hide in a
  version-advanced batch. Actual red run `94b03e89-3e76-4e5d-9244-4385283d0759` had4 original
  passes and6 failures. Parent fixed the owning private upgrade proof, not ordinary inventory.
- New `UpgradeSourceValidation` validates every affected complete record on both sides under its
  own admitted Schema, including default semantics, and resolves/validates every affected claim,
  including unchanged claims. One document-wide selector allowance and the operation's cumulative
  callback cover old/new traversal. Required root Absent/default/binding checks follow the
  existing compiler policy. Exact caller refusal survives. Unknown out-of-scope data remains
  structurally equal. Native/effective source closure remains the eventual Host's responsibility;
  this private slice does not qualify arbitrary transformation semantics or project publication.
- Hume added a narrow Reflect `__private` selected-payload validator. Parent rejected a duplicated
  raw-value walker; the final code reuses the existing `MaterializationBudget` admitted walker,
  registry shape/value/error scratch preflight and shared replacement-shape validator. No fake
  component Schema or second semantic interpreter. Six focused helper tests cover successful and
  failing payloads, exact borrowed caller errors, finite ceilings and opaque raw work. Worker
  unexpectedly ran stable Cargo despite its no-Cargo assignment; it was stopped from starting
  further commands and reached terminal before parent Cargo resumed. Do not reuse that worker
  result as the required +1.95 qualification; parent serial owner runs below cover the code.
- Archimedes implemented private Scene/Prefab prepared upgrade and exclusive bind/commit values
  in `authoring/schema_upgrade.rs`, with6 owner tests exercising both nominal source kinds.
  They pin original Arc/source key/revision/content state/nominal basis, prepare future source and
  classification, and mint a new history incarnation with original limits and zero Undo/Redo.
  Preparation/cancellation/guard drop retain both old branches. Commit installs only the source
  checkpoint, setting Scene live_dirty; it mints no saved token or file evidence. Parent wired
  module visibility, basis/kind observations and bounded Prefab projection through existing
  classification after raw admission. Actual source-key association and whole Workspace/persistence
  binding remain to be supplied by their complete-set owner.
- First fixed cohort passed31/31, run `b3217f43-0d13-41ab-9b7e-3a00b87403f0`. Scene,
  Reflect and Project full all-feature library cohort passed763/763,0 skips, run
  `edb45d86-eafd-4dac-8c02-f85ae1b064bf`. Subsequent adversarial review found three further
  P2s not covered by that green cohort: selected-field tombstones could broaden, duplicate
  Properties paths remained ambiguous, and broader candidate decode limits could install an
  unsaveable successor under the original encoded file policy.
- Three new regressions reproduced those defects:0/3 passed, run
  `8ed9ff72-9fe2-408c-b8ef-e36184812522`. Parent fixed exact selected-field tombstone
  equality, reused the unchanged compiler `validate_claim_paths` with prior pointer-index and
  comparison-work admission, and applied existing Scene/Prefab publication-shape validation under
  original file limits before classification/checkpoint storage. Hegel's targeted rereview found
  no remaining P1/P2 in these source fixes. Latest Scene full library run passed512/512,0 skips,
  `dbba74b7-6d89-48e7-a866-707d63056c36`. Reflect/Project source did not change after their
  full cohort. Do not attribute the earlier763 result to the later three fixes.
- Fermat's fixture/producer migration is frozen and reviewed:56 files,6 static manifests and82
  embedded/generated TOML inputs now explicitly Ready. No parser-side compatibility insertion.
  Parent added the missing canonical ADR0035 Ready example. All writers are now closed; no file
  reservation survives their old memory entries.
- Mendel implemented actual Source readiness capture and final revalidation in
  `project_content/source_readiness.rs`, ordinary loader and delivery. The actual on-disk
  manifest is read through bounded capabilities and real content budget reservations at entry
  and final publication. One retained observation spans delivery startup/all Scenes/final union.
  Old retained Ready candidates cannot bypass observed Pending. A legitimate startup repair
  proposal may differ from actual Ready disk bytes; entry/final observations compare actual disk
  identity/digest, not proposal equality. Ten real temporary-project tests cover gates, final
  changes, retained startup, union completion, budgets and source-free Runtime independence.
- Parent added Authoring prepare's Pending refusal before bundle loading and its real-file test,
  with an explicit Editor Open diagnostic. Existing authoring source validation and Export final
  complete-member digest revalidation already include the manifest; no duplicate parser added.
  Parent fixed Windows startup repair's ownership order: prepare ordinary replacement source
  before taking the exclusive manifest write lock, then lock and revalidate original identity/
  digest before expected-target publication. ADR0110 records this; normal readiness is never
  bypassed for a Windows read-lock failure. Independent Root review found no actionable P1/P2 in
  this bounded Source slice and cooperative-writer tier.
- Root focused command used no defaults plus `project-workflow-cli,tooling,presentation-2d`,
  library and `project_workflow_check`, selecting project_content/authoring and the whole Check
  binary. Run `9800ad9b-f6a2-49ab-bb4b-e3a0cd17a39a`:238 passed/2 failed,240 selected,
  866 excluded. All new gates and actual Windows startup repair passed. The two failures were
  exact Files high-water expectations that omitted the newly retained manifest: catalogue
  refusal0->1; shared Scene/P/R closure3->4. Parent updated only those expectations and rationale;
  no budget limit or accounting rule loosened. Focused rerun2/2 passed,
  `70fb6a30-b987-4240-bc3e-b00ff6381eb5`; the other238 were not rerun without a behavior change.
- Updated selected plan and ADR0090 to make inventory-versus-validity, batch ambiguity and
  original publication limits explicit. The operation freezes only its owned persistence lanes:
  drain actual Save returns before Pending, then fence Save/Reopen/Close-initiated Save.
  Independent Editor writes remain external conflicts, not a claimed cross-process lock.
- `cargo +1.95.0 fmt --all` completed after writers froze; diff inspection and whitespace check
  passed. Existing feature-specific warnings remain, and new private upgrade code is not yet
  called by production Workspace/Host, so it also emits dead-code warnings until integration.
  Do not suppress those or claim final API/lint/native acceptance. No Cargo process is known live.

# Next Action

- Keep the complete Goal active. This selected R5 unit is not ready for commit/ff main. Do not
  reduce its acceptance to these source proof and readiness tests. No push/PR or vendor changes.
- Wire concrete public/independent Session/Application proposal preparation through the private
  source proof and complete Workspace bind-before-install. Bind new actual saved content/digest,
  every persistence Host's new leaf and the lane's authoring_source, instead of carrying old
  checkpoint tokens. Mint no saved state from classification alone.
- Implement bounded all-profile participant inventory, deterministic recovery package, package
  existence/digest/semantic admission, Pending-first/Ready-last file publication, owner-local
  execution/persistence fences and fresh-process interrupted recovery. Current Pending gates
  refuse safely but do not yet inspect the referenced package or provide a complete recovery UI.
- Complete the independent EnemyStats dirty Scene/Prefab/nested/dormant consumer, exact new
  Adapter and separate v2 Runtime agreement, ordinary follow-up edit/Undo/Save/Reopen/Check/Play,
  interruption before/between/after file publication and external-edit controls. CP2 occurrence
  remap remains a separate successor, as do motion/executor, material and self-drawn Inspector.
- Current sessions23757/42941/93096/61111 and earlier focused tests are terminal. Root target only,
  +1.95.0/jobs1/strict serial Cargo. Reuse focused evidence; no broad reruns solely for doc closure.
  Before next changes inspect live diff. Preserve all authored WIP and original vendor; never
  restore the large working tree to the last registry checkpoint to make it look clean.

# Citations

- [Selected authored upgrade](../../../../plans/2026-11-01-authored-schema-upgrade-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Research disposition](../../../../research/2026-10-30-engine-direction-reconciliation.md)
- [Predecessor continuity](2026-11-01T035642Z-authored-upgrade-contracts-selected-with-first-source-and-manifest-owner-tests-passing-e7344c68ecf54870afeb437aeed117b2.md)
