---
type: "Verification Evidence"
title: "Reopen preparation failure fidelity before native source admission"
description: "Records the source-confirmed prior Reopen error-priority defect, bounded Scene/Prefab correction, unchanged public failure assertions and source-gated native callback ordering."
timestamp: 2026-10-25T22:01:13Z
record_id: "9e51c9e94971464ab4876bdf2ff2ff07"
producer_id: "codex-root"
run_id: "foundation-reopen-preparation-20261026"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/editor-private-lazy-tasks"
git_commit: "f69ac8785947cd693a25e14e8601f09932ee88f7"
---

# Verification

This bounded correction is discovered by the private-lazy Editor phase's extra public integration
cohort. The failing public test, file preparation owner, source policy and Session Reopen route
were unchanged against `f69ac87` before this correction. Source tracing establishes that the
defect path already exists at that revision; no clean-baseline execution is claimed. All actual
runs below use the current branch, Rust 1.95.0, jobs1, serial Cargo and the existing F: `target`.

The retained file owner already distinguishes actual target deletion, open failure, bounded read,
decode and pure validation errors. An independent source probe erased those failures by returning
SourceBindingMismatch before the Session inspected preparation. Mapping every probe NotFound to
TargetDeleted would be wrong: a missing sidecar or unreachable project alias is not necessarily
deletion of the file read through the retained parent capability.

The correction queries the existing stored preparation result without native callbacks or a new
state machine. After current document/revision/binding/schema/Host/runtime admission, pure failure
retains its original result. Only a successful candidate proves admitted/read/current identity
before native codecs/migrations. Existing final source validation, Workspace publication and
Host-baseline commit ordering remain unchanged. ADR 0047 clarifies that existing contract.

# Result

The original unchanged public test and the complete selected public72/three skipped pass after
the correction. Deletion, directory and retry/source-preservation assertions are not relaxed.
Additional owner, review and broad results are recorded below with their actual scope.

# Evidence

- Public72 at `998fd785-55d1-4816-813e-7b3a203375af`:71 pass/one failed/three skipped. The only
  failure is `failed_reopen_preserves_the_dirty_workspace_and_can_be_retried`, receiving
  SourceBindingMismatch instead of TargetDeleted after deletion. Its assertion precedes the
  subsequent abort; that is not independently diagnosed memory corruption.
- Independent read-only diagnosis `01a1750c-5cb0-72f2-80fc-d307b7d3f1b1` traces the same Scene and
  Prefab priority bug to pure preparation versus independent probe ordering. It rejects moving
  complete native admission ahead of source proof, because that could execute callbacks for an
  unadmitted candidate. It recommends reusing the existing exact three-way source validator.
- Focused unchanged public test: `f9b08693-6491-45a1-890e-48182ea9e288`, one pass. The private
  `preparation_identity` query borrows stored identity or copies the stored failure; it performs
  no read, decode, registry behavior or publication. Scene and Prefab use the same ordering.
- Corrected complete selected public72/three skipped: `e8a4b534-1eb1-410c-8c14-bf7ca04ca856`, using
  the same five integration binaries, including public Editor persistence/Play, Runtime boot,
  Host boundary and composition. Compile-fail authority probes execute in this cohort.
- Independent source-only review `01a17515-c3d8-7401-b10f-8d0773f124dc` finds no actionable defect
  in the three changed owner/Session files and ADR 0047. It independently traces the original
  failure and successful focused rerun, exact source/sidecar identity before callbacks, repeated
  final admission and publish-before-baseline ordering. This is separate from the earlier
  seven-persona Tasks/Session review; it does not claim a new full review or strong filesystem CAS.
- Owner/provenance6: `668ab493-3b55-4da1-b281-7d14cdc76972`. Three new tests exercise both Scene
  and Prefab deletion/directory/Decode/Validate, complete document/history/selection/saved-state
  preservation, old Host baseline under subsequent Save and successful Reopen retry. They also
  hold actual AfterPreparation, retain the predecessor file under another name to prevent identity
  reuse, replace the target and require SourceBindingMismatch. Stable-metadata tests distinguish a
  missing sidecar on a successful candidate from an already failed Decode. Existing exact-Host,
  later-edit and same-key metadata-byte/file replacement tests remain unchanged and pass.
- Actual Windows/DX12 native12: `70c8e2dc-495e-43a4-8953-7ba1494ff388`, after this correction.
  It includes ordinary Scene/Prefab widget edit/Save/Reopen, critical3, all five graphics retirement
  trajectories, retained Scene/Canvas priority and unrelated Prefab/Play source Close. It is not a
  held-native quiet-cycle or OS-permission failure matrix.
- Independent test review `01a1751d-0cdd-7ed0-ae49-063062b11f76` confirms the actual failure,
  history/baseline, retry, non-reused file identity and sidecar-priority assertions. It identifies
  one material test gap, not a production defect: an empty replacement candidate proves final
  publication rejection but cannot distinguish native callbacks before versus after source proof.
  The production order remains correct. A separately counted real native-callback negative and
  successful-retry positive control is required to close that narrowly identified gap.
- The added real `SchemaContribution`/plugin/provider probe registers native decode/encode and a
  component migration. Both Scene and Prefab show zero callbacks at AfterPreparation and after
  rejecting an explicitly different physical file identity. Restoring the retained original
  object then succeeds with actual decode and migration on the owner thread, value7 preserved
  and disk bytes unchanged. Its v1-to-v2 component is a test of authored migration callbacks, not
  a new engine file-format history or release compatibility promise.
- Callback-order cohort4 passes at `ed6d048b-13aa-49fb-9324-0dc44c421752`. Independent closure
  `01a17515-c3d8-7401-b10f-8d0773f124dc` marks that test gap Closed after checking the real
  negative/positive controls and terminal log. The reviewer runs no mutation test; the source
  ordering is unchanged and correctly rejects before callbacks, not merely before publication.
- Complete root446/three skipped: `4bfef26e-dcba-401e-a22d-c0da5ff7c35a`, after the production
  correction and first three owner tests. The later callback-order test is qualified separately
  by the focused4 above. Expanded Editor250/33 skipped subsequently passes at
  `b45e66e7-6fdf-4247-af53-6cd77ddcb812`, including real generated-starter Ready/Stop/clean Exit.
- Final workspace all-feature/all-target, minimal root and headless/tooling library/test checks,
  affected correctness/suspicious Clippy and formatting check pass on the combined source. These
  are serial runs in the existing target, with retained non-fatal warnings; the accompanying
  Editor deployment evidence records the exact command/log scope.

# Follow-up

This is not new persistence compatibility, cross-file rollback or a different document protocol.
Source rejection for a successful candidate remains distinct from an already failed read/decode.
No OS-permission matrix, global filesystem snapshot or arbitrary codec side-effect rollback is
claimed. The separate Tasks/private-lazy qualification retains its own scope and review lineage.

# Citations

- [Persistence contract](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [Master rebaseline plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
