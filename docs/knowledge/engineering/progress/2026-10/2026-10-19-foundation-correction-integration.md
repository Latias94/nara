---
type: Work Progress
title: Foundation correction integration and authoring acceptance
description: U1-U9 focused progress, U10-U15 implementation, independent findings and remaining goal gates.
timestamp: 2026-10-19T09:23:00Z
record_id: 838594d8ea674ba8911b5e1f9e760b6e
producer_id: codex-root
run_id: foundation-correction-20261019
related_plan: docs/plans/2026-10-19-1335-refactor-foundation-correction-plan.md
git_branch: refactor/foundation-contract-hardening
---

# Active Scope

The goal remains active for all U1-U15 in the correction plan. User authorization includes local
Conventional Commits and parallel subagents; no push/PR or Tile tools. Fresh press after context
restoration is settled. The later interface request asks where asynchronous preparation belongs,
not for blanket async traits. The interface/module audit adds G9 measurements and private
extraction guidance, and U10 removes the unusable public ComponentCodec implementation trait.

# Committed Boundaries

- `8b3e26b`: U2 superseded Image generation retirement.
- `ebbe005`: U3 live-device texture admission.
- `0b078a0`, `964d25c`: U4 cumulative reservations, demand counters and real Runtime/Editor scope
  proof. WGPU 122 passed with three native cases ignored; the new two-target case passed separately.
- `2713396`: U1 prompt-bound close decisions; core, Editor and persistence consumers passed.
- `eff95c2`: U8 Unix nonblocking leaf admission. Linux behavior proof passed 64 cases using original
  sources through a temporary offline standalone manifest; this is not locked-workspace Linux
  qualification. Windows locked crate cohort passed 64 with three platform tests skipped.
- `1ff253a`: U9 text content geometry; kernel/render 123 passed. The amended neutral layout gallery
  subsequently passed at scales 1.0 and 1.25; its verification record includes that result.
- `5419b10`: U6 installed-graph admission and U7 explicit managed custom schedules. Public/renamed
  root consumers passed. The complete App/Input/Gameplay/Tilemap cohort passed 242 tests, including
  consecutive custom schedule change detection.

These focused results do not replace the final workspace gate or close the entire goal.

# Current Integration

U5 associated Scene candidates passed Scene 194, root/content/runtime 138 and public boundary
tests. Candidate rustdoc, independent module consumers and Reference Game runs remain.

U10 closed Vec2, explicit owner v5/component v2 migration, historical v1-v4 preservation and private
codec trait are implemented. Reflection/data-authoring focused cohort passed 68. Bundle generation
first exposed duplicate inherited tombstones; the obsolete redeclaration was removed without
weakening registry validation. The canonical bundle now generates successfully. Strict saved-v1
Check/Save/Reopen and independent closure-codec/math consumers still need runs.

U11 recursive Inspector and U13 paired stateful native panel composition are implemented. Editor
ordinary/native acceptance has not yet completed. Independent review found duplicate ImGui IDs on
List/Map Remove buttons: the child ID scope ends before the button and its path is only recorded
as test metadata. Fix and second-item removal evidence remain required.

U12 typed profile-default font selection and common composition owner are implemented. The first
root tests used the wrong facade for advanced packet facts; imports are being corrected without
opening a new public surface. Shared ordinary-recipe Headless/Check, Editor Play, native Desktop
and actual Export/startup proof is being integrated.

U14 semantic held-state implementation passed its initial five normalized-input regressions, but
independent review found paused synthetic presses adopting resume-time identities and restoration
of old cloned/moved mappings reviving expired state. New regressions are ready for a red run;
production is frozen until that run. The proposed correction centralizes non-reverting observed
lifetimes at StageActions and stages normalized/synthetic events through that owner.

U15 concrete TileSetContent joins are implemented for Runtime, Editor and TaskUpdate; its four new
domain cases passed in the 242-test cohort. Post-migration root Editor/startup cases remain.

# Execution Constraints And Next Gates

All Cargo is serialized by root with Rust 1.95.0 and CARGO_BUILD_JOBS=1, reusing target. A previous
all-features link hit MSVC PDB LNK1140 and the target volume ran out of space. Cargo clean removed
345.1 GiB of build artifacts; subsequent cohorts rebuild cold. No source was removed. Preserve
unrelated untracked myeasylog.log and vendor; repo-ref remains read-only.

Finish U14 red/fix/green and U11 removal identity fix; complete root/product, Reference Game,
external consumer, candidate rustdoc and affected Windows/DX12 native trajectories. Then perform
serialized workspace all-features validation, simplification and the actual ce-code-review
workflow. Ad-hoc reviewers so far are independent findings, not that final review receipt. Update
ADR ledger, audit and external HTML with actual evidence and commit only intended paths/hunks.
