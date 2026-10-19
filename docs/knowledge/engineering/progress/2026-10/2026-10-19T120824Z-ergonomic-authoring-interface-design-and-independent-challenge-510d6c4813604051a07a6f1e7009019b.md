---
type: "Work Progress"
title: "Ergonomic authoring interface design and independent challenge"
description: "Proposed caller-first field editing, correlated dispatch, component inventory and bounded validation; design-only outcome."
timestamp: 2026-10-19T12:08:24Z
record_id: "510d6c4813604051a07a6f1e7009019b"
producer_id: "codex-root"
run_id: "authoring-ergonomics-design-20261019"
git_branch: "refactor/foundation-contract-hardening"
git_commit: "f988852"
---

# Summary

The user authorized first-principles, ergonomic interface design following the post-correction
foundation audit. The new [design](../../../../architecture/authoring-interface-ergonomics-design.md)
is Proposed: no production API, Accepted ADR or completed U1-U15 Goal changed. Source baseline
is `f988852`. The standalone HTML is `nara-authoring-interface-design-2026-10-19.html` in the
user's Downloads directory, outside the repository as requested. The withdrawn reference is excluded.

# Details

- Three independent lanes compared a single-pending callback, owned polling tickets and a
  complete default field helper. The recommendation is the helper over a small correlated
  submit/result callback, with no arbitrary one-action-per-panel limit and no generic tool inbox.
- Keep Component data, persistent schema, behavior installation and Editor tools distinct.
  Ordinary consumption retains `ProductRecipe` plus the package helper. Generate a paired initial
  catalog/native registration and reusable current schema list from one component inventory;
  IDs, predecessor snapshots, migrations and tombstones remain explicit.
- Stable widget identity differs from exact mutation basis. Retain invalid/conflicting input;
  collapse or temporary unselection is not retirement. Missing projection coverage is unresolved,
  not proof of field deletion. Capacity rejects new input without evicting dirty drafts.
- Common-caller challenge closed exact batch routing with an opaque move-only frontend candidate,
  original-position mapping, post-drive lifetime reconciliation and one private recipient per
  origin. Native presentation must seal the final candidate after canvas provenance is complete.
  Dropping a candidate runs no callbacks; supersession/retirement bounds observation bookkeeping.
- Separate contract review clarified F32 materialization, explicit UiText domain succession,
  all bound changes requiring component succession, exact integer comparisons, absent-metadata
  fingerprints and diagnostic field membership at the bridge. Data-only range errors do not
  execute project codecs or synthesize arbitrary native codes.
- An independent adversarial document reviewer found no direction-blocking issue. Its current
  inventory-after-v2 and multiple-same-basis-action concerns are explicit in the final proposal.
  Multiple actions remain ordered; Apply All uses an existing single atomic patch when needed.
- The document defines E1-E7 with owners and observable criteria: owner safety, correct
  Reset/Remove, dispatch correlation, default field lifecycle, inventory, constraints/errors and
  external/native workflow qualification. Async preparation and further module extraction remain
  tied to real consumers and measurements.
- Documentation checks passed for three Markdown files, 75 local links and balanced code fences;
  the standalone HTML passed nesting, six unique anchors and nine local/internal links, with no
  external resources. This is structural validation, not browser visual or native UX evidence.
  No Cargo command or production-code test was needed or run for this design-only change.

# Next Action

Use the proposal to select implementation scope. Start with independent N01/N08/N09 owner fixes
and N02 authored-state repair; E3 precedes E4. Reconcile affected ADRs when adopting durable
contracts, then prove the independently compiled component/panel consumers and native behavior.
Do not treat proposed interfaces or design acceptance as implemented functionality. No new Goal
was requested in this turn. Preserve unrelated `myeasylog.log` and `vendor/` changes.

# Citations

- [Authoring interface design](../../../../architecture/authoring-interface-ergonomics-design.md)
- [Post-correction source audit](../../../../audits/2026-10-19-foundation-next-stage-review.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- Review lanes: `/root/ergonomics_minimal_interface`, `/root/ergonomics_flexible_interface`,
  `/root/ergonomics_common_caller` and `/root/ergonomics_design_challenge`.
