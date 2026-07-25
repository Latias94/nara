---
type: "Verification Evidence"
title: "RGD-U12 release security hardening verification"
description: "Records local verification of publisher approval binding and immutable-release workflow hardening without claiming hosted publication."
timestamp: 2026-07-25T06:12:12Z
record_id: "50b3c1491dce415e8f7e0fe508c59276"
tags: ["rgd-u12", "release", "security", "verification"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "ba03a97c5b40d9a9beb6ce709ac9dc46a9818c49"
---

# Verification

This evidence covers the local U12 release-security corrections in
`fee339f11776eb837653bc0ca62e7db7b90eb396` and
`ba03a97c5b40d9a9beb6ce709ac9dc46a9818c49`. It verifies no hosted workflow,
tag, draft release, public release, immutable-release setting, or anonymous
public artifact.

# Result

- The canonical U11 approval now binds the publisher workflow path, digest, and source revision.
  `verify_release.py` rejects a trusted publisher definition or revision that differs from that
  approved triplet.
- The release workflow derives the live publisher workflow digest from protected `main`; it no
  longer accepts that digest as a dispatch input. Its pinned verifier and approval schema point at
  the preceding reviewed helper commit.
- Repeated attempts fail explicitly. Run-history and tag-ruleset inspection are bounded and
  paginated. A qualifying tag ruleset must be active, match without an exclusion, prohibit all
  bypass actors, and contain creation, update, and deletion rules.
- Windows receipt steps explicitly use Bash. Publisher inputs, the manifest, and smoke helpers are
  retained for fourteen days. Static policy coverage rejects credential aliases and unexpected
  verifier environment values.

# Evidence

- `python -B reference-game/tools/verify_release.py verify-policy --approval-schema docs/benchmarks/data/approvals/v1/reference-game-pre-release.schema.json --manifest-schema docs/benchmarks/data/approvals/v1/publication-manifest.schema.json --trusted-input-schema docs/benchmarks/data/approvals/v1/release-trusted-input.schema.json` passed.
- `cargo fmt --all -- --check` passed.
- `cargo nextest run --locked -p nara --test release_verification --test release_workflow_policy --test-threads=1` passed: 10 tests.
- `actionlint -no-color .github/workflows/reference-game-release.yml` passed.
- All four inline `actions/github-script` bodies passed non-executing Node syntax checks after an
  async wrapper was added by the checker.
- `git diff --check` and exact staged-path review passed before the two commits. `architecture_docs`
  was intentionally not run because this change does not alter architecture-governance authority.
- `wiki_memory.py validate --root docs/knowledge/engineering` found no defect in these new shards,
  but remains nonzero because of a pre-existing duplicate 2026-07-23 record and legacy rollup
  warnings outside this change.

# Follow-up

- U12 remains active and blocked on U8 hosted Windows/Linux evidence, U11's final `Publish`
  approval, and separate authorizations for tag creation, draft upload, finalization, and release
  mutation.
- The exact permission-bearing workflow revision still requires a final independent security and
  bug/regression review before any external publication authorization is requested.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u12-publish-the-evidence-approved-immutable-github-pre-release`
- `docs/knowledge/engineering/verification/2026-07/2026-07-25T014944Z-rgd-u12-local-immutable-release-preparation-verification-5186939a5dec4103bfa1c702061af655.md`
