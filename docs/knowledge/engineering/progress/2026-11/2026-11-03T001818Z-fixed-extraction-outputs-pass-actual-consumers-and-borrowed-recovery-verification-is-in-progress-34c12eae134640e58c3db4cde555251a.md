---
type: "Work Progress"
title: "Fixed extraction outputs pass actual consumers and borrowed recovery verification is in progress"
description: "Canonical output identity and late refusal custody are verified; stored P/Q and dependant admission is being integrated without candidate reconstruction."
timestamp: 2026-11-03T00:18:18Z
record_id: "34c12eae134640e58c3db4cde555251a"
producer_id: "codex-foundation-main"
run_id: "extraction-fixed-outputs-stored-admission-20261103"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "88fd7289c0d680481e0f964561650de05f90cbad"
---

# Summary

The full foundation Goal remains active. The complete current unit is Prefab extraction, with
actual Session/Application, Pending/Ready, new-member publication and fresh-process recovery
still unfinished. `main` and `refactor/foundation` remain at88fd728; the large authorized WIP
is uncommitted and nothing was pushed. Root target, serial Cargo/jobs1 and vendor protection remain.

# Details

## Verified fixed output owner

- Scene now issues immutable `EncodedPrefabExtractionSource` from the actual verified corpus,
  selected by `PrefabExtractionSourceOutput::{Existing, Extracted}`. Bytes cannot be replaced
  by a caller, and equal bytes from another nominal Work are refused. Raw traversal, fourfold
  canonical-copy/serializer storage and pre-growth buffers use the original Work. Tooling's
  closed encoder rejects open or unchanged sources; open documents use real saved checkpoints.
- Root `prefab_extraction/outputs.rs` owns one explicit output per published physical source
  plus Q and metadata, while Guards remain only in the complete Review/inventory. It retains
  original facts, exact encoding/key/path and digest. Later failure preserves completed payloads
  and position; retry does not reencode them. Releasing encodings returns actual Taken checkpoints,
  preserves Accepted evidence and refunds metadata's exact Host capacity after buffer drop.
  There is no Pending or file authority. Parent removed the unused mutable Review getter so
  retained payloads cannot be detached from their original Review.
- The shared checkpoint encoder now debits the exact encoded length immediately before hashing.
  Hash refusal returns the same capture. This replaces the Root worker's initial worst-case
  per-file byte-limit reservation. Actual canonical/serializer input admission stays separate.
- Independent Core and final Root adversarial reviews have no remaining scoped P1/P2. All those
  reviewers and the Root output writer are closed.

Actual serial evidence before the newer stored-verifier WIP:

- Public source encoder JSON/RON/exact/one-short/foreign Work test1 passed, run
  `3f424cff-1f0b-437e-af46-536a5a475d54`.
- Scene/Tooling owning and independent saved-generation cohort35 passed, run
  `6df75da7-a3d5-4ca0-9d41-f2f8a2add373`,1.547s after1m03s build.
- Tooling persistence after the exact hash change17 passed, run
  `6a7aa71c-19f2-404c-b6b1-baabe292c80d`,0.976s.
- Root actual fixed-output8, shared checkpoint hash, extraction/inventory/leaf persistence and
  selected old upgrade cohort140 passed, run `c53f8ca3-1bf4-424a-b572-694f3948cc11`,17.252s
  after2m00s build. Session2207 is terminal0. The one late recoverable failure is a test-only
  once gate; real exhausted budgets are never renewed to make retry succeed.
- Scene no-default check passed1.37s and Scene/Tooling all-target correctness Clippy passed45.26s
  before the later shared hash change. Final fmt preceded Root140. No native/process extraction
  qualification is claimed. TEMP logs use nara-extraction-fixed-source-encoding-tests,
  fixed-output-domain-tests, fixed-checkpoint-hash-tooling-tests, fixed-output-root-tests,
  fixed-output-scene-minimal and fixed-output-domain-clippy, with .log suffix.

## Stored recovery admission now in progress, uncompiled

The recovery study confirmed that Pending binds exact data but does not remove pure semantic
verification. A fresh process must validate the stored working/output pair, not call the live
candidate constructor again. Canonical entity record order may differ from the live iteration
that assigned extracted input names, so names must be selected from stored arguments. The plan
and profile/saved-generation research now record this requirement and the concrete package
versus neutral transport boundaries. The read-only research agent is closed.

Parent has made uncompiled changes to:

- `relocation/moved.rs`: shared local/outward reference-meaning and argument comparison, plus
  borrowed `StoredMovedReferenceInputs` matching actual stored argument names and tracking use.
- `construction/verification.rs`: selected P/Q stored validation, one entity fragment at a time,
  exact anchor and arguments, original inputs and shared coupled placements. No full candidates.
- `ownership.rs`: existing bounded `RecordIndex` and its new/claim/finish are now extraction-private
  shared helpers instead of duplicate ID indices.
- `corpus.rs`: shared source-context and occurrence verification; `ExtractionRequest::admit_outputs`
  retains provided source/Q allocations and checks supplied metadata/authority before selected
  and dependant stored verification. The new helper still depends on the active worker below.
- `authoring/prefab_extraction.rs` and hidden exports: new `PrefabExtractionRequest::admit_outputs`
  and `PrefabExtractionStoredAdmissionFailure`; refusal returns actual request, sources, Q,
  original Work and diagnostics. These latest edits are not compiled or fmt-qualified.

Only active writer is Heisenberg the3rd, agent `01a19ebc-8892-7d92-aaab-dd40aa852b50`, assigned
`dependants.rs` and `dependants/tests.rs` only. It received EDITS GO after Root140 terminal.
Agreed private API is `verify_dependant(before, source_key, after, relocation, basis,
preserve_unavailable, work) -> Result<bool, ExtractionError>`. It shares existing fragment kernels,
adopts ownership::RecordIndex, checks both raw sides before clone/equality, accepts record-order
permutations and preserves input/placement order. It must not Cargo/fmt/commit/spawn. Parent owns
all other changed Core paths and will add independent stored-admission consumers next.

# Next Action

1. Receive worker source-ready before fmt/Cargo. At this checkpoint no Cargo or cleanup helper is
   live. Do not compile transitional missing verify_dependant or overwrite its two files.
2. Add independent public stored-admission tests through the real corpus: fresh basis, decoded
   original/output allocations, reversed entity records with two outward dependencies, exact
   stored input-name bindings, P/Q/dependant tampering, malformed association and same-owner
   failure/retry/exact/one-short capacity. Use the existing compiler, not a test evaluator.
3. Compile/fix and independently review stored admission, then qualify relevant Core/Tooling/Root
   scope. Final fixed-output evidence above predates this WIP and cannot qualify it.
4. Continue concrete recovery AST and shared code-free package/manifest transport, then actual
   extraction publication/future-project/all-owner Ready binding and full public/process/native
   unit. Keep complete extraction and foundation active; no partial-unit commit or remote push.

# Citations

- [Complete extraction plan](../../../../plans/2026-11-02-prefab-extraction-plan.md).
- [Stored recovery comparison](../../../../research/2026-11-03-profile-resolution-and-saved-generations.md#stored-extraction-recovery).
- [Previous neutral inventory qualification](../../verification/2026-11/2026-11-02T234852Z-neutral-source-inventory-preserves-strict-authority-and-both-real-consumers-fdd43928e9ab444bac78ed2c16d92bf9.md).
