---
type: "Work Registration"
title: "Reference-game runtime authority and delivery: RGD-U10 local preparation"
description: "Records the locally verified standalone candidate preparation while keeping hosted candidate execution and unit closure blocked on U8."
timestamp: 2026-07-23T04:34:28Z
record_id: "de22429723d549779edb5f2835ae25c3"
tags: ["rgd-u10", "release-candidate", "packaging", "active"]
status: "active"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "412002eba1a41448aa6622f0f78dca522e1fd968"
supersedes: "bfaa6144b525432986066ddd3f6e8577"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/verification/2026-07/2026-07-23T043428Z-rgd-u10-local-candidate-preparation-verification-668ca80db5e7483f80a3f25fe4ee0301.md"
---

# Scope

RGD-U10 local preparation is now committed: the reference-game candidate layout, bounded package
and no-checkout transport tools, consumer smoke path, policy tests, licenses, and protected-main
candidate workflow are available for the final hosted revision.

# Current Claim

Active: local preparation completed at
`412002eba1a41448aa6622f0f78dca522e1fd968`. A Windows local mechanics smoke passed, but its archive
was created before this commit and is not U10 candidate evidence. U8 must first provide the final
hosted Windows/Linux matrix on the integrated revision; only then may U10 execute, bind exact
artifact identities and retention records, and close. No publication or public-release claim is
authorized.

# Latest Links

- `docs/knowledge/engineering/verification/2026-07/2026-07-23T043428Z-rgd-u10-local-candidate-preparation-verification-668ca80db5e7483f80a3f25fe4ee0301.md`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md`
- Commit `412002eba1a41448aa6622f0f78dca522e1fd968`

# Handoff

Do not change executable inputs, package layout, verifier helpers, policy tests, or candidate
workflow definitions between U8's qualifying hosted revision and U10 execution. After U8 passes,
build the exact Windows/Linux candidates, verify them before extraction, run the no-checkout
consumer from randomized external roots, preserve bounded workflow/artifact identity and retention
records, and record the hosted result separately. U9's baseline execution remains independently
blocked on the same U8 boundary.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u10-build-and-consume-standalone-release-candidates`
- Superseded registration `bfaa6144b525432986066ddd3f6e8577`
