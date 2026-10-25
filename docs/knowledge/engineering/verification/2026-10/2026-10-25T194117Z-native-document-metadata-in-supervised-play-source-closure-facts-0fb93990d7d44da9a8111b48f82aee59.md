---
type: "Verification Evidence"
title: "Native document metadata in supervised Play source closure facts"
description: "Records the actual generated-starter handshake refusal, its source-fact owner correction and matched Editor/Runtime metadata-closure regression evidence."
timestamp: 2026-10-25T19:41:17Z
record_id: "0fb93990d7d44da9a8111b48f82aee59"
producer_id: "codex-root"
run_id: "foundation-native-source-facts-20261026"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/authoring-open-custody"
git_commit: "a1ff454"
---

# Verification

The expanded initial-Open qualification runs the generated-starter supervised Play consumer in
addition to ordinary Editor library cases. At `48d6bb1b-3fad-45c9-b21d-0c4f23b62ef7`, 244 tests
pass and one fails: the child rejects `external-play.handshake` / `runtime-facts`, followed by
test-process abort during panic. The test creates a `stable_metadata` startup Scene. Its Runtime
closure includes the `.meta` member but the Authoring source receipt omits that admitted member.
The digest's encoded length alone must therefore differ. These source paths are unchanged from
`a1ff454`; this is not an independently rerun baseline comparison or a diagnosed memory fault.

Native admission already retains exact metadata identity, digest and read accounting. The
correction shares that source fact into the admitted Scene/Prefab closure, including dependency
depth, retained accounting and identity-kind conflict admission. Stable-reference catalogue
verification reuses the same admitted observation rather than reading/charging another copy.
The existing Editor receipt, strict Runtime closure builder and exact handshake comparison retain
their authority. This implements an existing source-closure contract, not a new digest protocol.

# Result

The new two-test receipt/strict-Runtime comparison passes as part of the focused eleven-test
lifecycle/source cohort. The full root library cohort passes 425 tests with three skipped.
Expanded Editor247/33 skipped passes, including the actual generated-starter Ready/Stop/clean
Editor Exit that previously failed. Independent source review finds no actionable defect.
Final minimal/workspace checks, affected Clippy, formatting, local links and memory validation pass
within the linked integrated qualification scope, retaining existing warnings.

# Evidence

- First focused compile reports E0432 for test imports from `project_host`; the test now imports
  the three existing functions from their owning `project_host::composition` module. No production
  export is added to accommodate the test.
- `566f9678-4918-4801-ab8b-66de8889a13c`: eleven focused tests pass, including the two new
  `external_play::source_fact_tests` cases. The first compares startup Scene sidecar membership,
  exact bytes and aggregate digest. The second compares a shared Prefab through repeated path
  references and a mixed path/stable-ID reference, requiring each metadata fact exactly once.
- Both tests use public encoded Editor source receipts and actual `ProjectContentLoader` strict
  startup closure evidence for the same project. The Editor is physically closed immediately
  after capturing its owned receipt, before semantic assertions can unwind.
- Full root425/three skipped passes in `target/r4b-initial-authoring-custody-root-final.log`.
  The ordinary Editor library203/30 native skipped cohort also passes after the intent correction.
- All Cargo is serial, Rust 1.95.0, jobs1, existing F: checkout `target`. The original generated
  project test's supervised Cargo work also stays under that target; no C:/D: build tree is added.
- Root425: `b715de36-31de-4358-977c-2d679411316f`. Expanded Editor247:
  `c62e3d1a-1585-482a-ba81-42002917455b`; generated-starter Play completes in that run.
  Independent reviewer `01a17494-0776-7c10-91d1-b5baeff41941` inspects exact fact sharing,
  identity/path conflicts, partial admission, depth, source/retained budgets and catalogue reuse
  without finding a defect. This is a source review, not an independent test rerun.

# Follow-up

Do not treat matching two metadata closures as full source-free delivery, artifact/CAS or package
qualification. The original child-handshake failure and its eventual rerun retain separate facts.
No independent confirmation of the abort call site is available; the retained Runtime quarantine's
abort-on-thread-exit path is consistent with the symptom without proving it was the exact cause.

# Citations

- [Source-fact owner](../../../../../src/project_host/authoring/project.rs)
- [Native metadata admission](../../../../../src/project_host/authoring/native_source.rs)
- [Matched receipt/Runtime regression](../../../../../src/project_host/runtime/editor/external_play/source_fact_tests.rs)
- [Supervised Play contract](../../../../architecture/adr/0116-standalone-supervised-editor-play.md)
- [Initial authoring qualification](2026-10-25T191539Z-initial-authoring-construction-and-failed-open-physical-custody-qualification-ad41f2c430ac4e56bdaf88fbb457f1cd.md)
