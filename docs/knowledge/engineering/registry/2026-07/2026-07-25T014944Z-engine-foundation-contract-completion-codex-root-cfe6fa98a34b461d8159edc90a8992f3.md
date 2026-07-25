---
type: "Work Registration"
title: "Reference-game runtime authority and delivery: RGD-U12 local preparation"
description: "Records locally verified immutable-release preparation while keeping hosted execution and publication blocked on U8, U11, and separate authorizations."
timestamp: 2026-07-25T01:49:44Z
record_id: "cfe6fa98a34b461d8159edc90a8992f3"
tags: ["rgd-u12", "release", "immutable", "active"]
status: "active"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "58bbf6a3652a2f730b865dad2cfd4818f0bfc622"
supersedes: "de22429723d549779edb5f2835ae25c3"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/verification/2026-07/2026-07-25T014944Z-rgd-u12-local-immutable-release-preparation-verification-5186939a5dec4103bfa1c702061af655.md"
---

# Scope

RGD-U12 local preparation is committed: the immutable pre-release workflow, pinned verifier,
credential-bound policy tests, and local preparation documentation are available for the final
hosted revision.

# Current Claim

Active: local preparation completed at `58bbf6a3652a2f730b865dad2cfd4818f0bfc622`. This record
does not claim that U12 is complete. U8 must first supply the qualifying hosted Windows/Linux
matrix. U11 must then produce a final, pinned `Publish` decision over exact candidate identities.
Only four separately authorized operations may then initiate and finalize publication: protected
tag creation, draft-upload environment approval, release-finalize environment approval, and
Release mutation.

# Latest Links

- `docs/knowledge/engineering/verification/2026-07/2026-07-25T014944Z-rgd-u12-local-immutable-release-preparation-verification-5186939a5dec4103bfa1c702061af655.md`
- `docs/benchmarks/reference-game-release-preparation.md`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md`
- Commit `58bbf6a3652a2f730b865dad2cfd4818f0bfc622`

# Handoff

Do not use local readiness to run hosted verification or publication. After U8 and U11 close on
the integrated executable revision, obtain each required authorization separately. The dispatch
must bind the exact approval revision/blob/SHA, publisher-definition digest, tag, candidate run,
and platform artifact identities. Any failure after tag creation requires a new U11/U12 version;
do not retry or mutate that version in place.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u12-publish-the-evidence-approved-immutable-github-pre-release`
- Superseded registration `de22429723d549779edb5f2835ae25c3`
- Commit `58bbf6a3652a2f730b865dad2cfd4818f0bfc622`
