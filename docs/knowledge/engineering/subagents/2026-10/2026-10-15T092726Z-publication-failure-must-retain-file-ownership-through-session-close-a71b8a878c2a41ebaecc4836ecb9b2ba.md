---
type: "Subagent Finding"
title: "Publication failure must retain file ownership through session close"
description: "U4 source trace shows filesystem failure ownership must propagate through Scene, Prefab, image metadata, repair, retry, and close."
timestamp: 2026-10-15T09:27:26Z
record_id: "a71b8a878c2a41ebaecc4836ecb9b2ba"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
---

# Finding

U4 cannot end at a new cleanup field inside FilePublicationHost. Low-level consuming errors and
short-lived callers can still drop the only temporary-file owner. This is an existing reachable
ownership defect, not a request for a generalized filesystem transaction system.

# Evidence

- `src/project_host/persistence/file_publication.rs:211-307` checks a static platform table,
  creates/writes/syncs temporary state, then consumes it through `replace_temp`.
- `crates/nara_fs/src/capability.rs:448` rejects an untrusted Unix name-bound replacement only
  after creation. `platform/unix.rs:327` cannot perform exact temporary deletion in that mode.
- `crates/nara_fs/src/replace.rs:58` consumes `discard` and returns only FsError on failure.
- `runtime/editor/image_import/transaction.rs:552-599` uses a local publication owner; a failed
  outcome returns while the owner is dropped. `project_workflow/repair.rs:340` shares the low-level seam.
- Editor document close clears persistence lanes; Runtime cleanup and authoring task retirement
  currently do not own failed file publication cleanup.

# Recommendation

Preflight the actual capability instance before avoidable effects. Retain the temporary candidate
in low-level failure results and pass cleanup ownership to the existing long-lived session/image
owner. Use one bounded pending slot per existing lane, expose a value-only pending state to tooling,
and keep ownership through retry/document close/session close. A permanently unsupported cleanup
must report residue honestly, never delete a replacement path or claim retry is guaranteed.

Also handle creation followed by failed handle-fact validation without upgrading an unverified
handle to FileCapability or deleting through an unverified name. Reuse the established unverified
directory-publication pattern narrowly. Migrate repair and image metadata callers with the low-level
signature, not just the ordinary Scene save path.

# Disposition

Admitted into the already-authorized U4 ownership repair. No implementation or native Linux test
has been performed for this finding yet. Preserve the user's existing filesystem/SPL modifications.
Required evidence is native Linux untrusted pre-effect rejection, Windows failed-replace cleanup
retry, verified-name replacement safety, and one session/image close trajectory retaining the owner.

# Citations

- `docs/verification/2026-10-15-foundation-correctness-audit.md` F04
- `docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md` U4
- Read-only source trace by Codex subagent `/root/toolchain_positioning`, 2026-10-15
