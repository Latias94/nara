---
type: "Work Progress"
title: "Reference consumer exposes unreleased UI migration history and source readiness accounting"
description: "Reference157/159 reveals an obsolete Native-open migration assumption and exact readiness-file overlap; canonical first-party UI baseline cleanup is in progress."
timestamp: 2026-11-01T19:42:54Z
record_id: "4ade1b7c592d4408842cc351ab494274"
producer_id: "codex-foundation-main"
run_id: "authored-upgrade-ui-baseline-20261102"
related_plan: "docs/plans/2026-11-01-authored-schema-upgrade-plan.md"
supersedes: "dfbbb0a8998a433090d17aa6fad56b93"
---

# Summary

The full foundation Goal is active and the authored-upgrade unit remains uncommitted at
`ed21b230` on refactor/foundation. Final-source Reference run found157/159 passed and two failures.
One exposes obsolete unreleased UI loading history, which conflicts with lossless source Open.
The user and foundation plan explicitly authorize consolidating such first-party history at1;
the selected correction removes it rather than restoring Root's Native reader. Do not claim the
complete unit qualified or commit until this correction and remaining checks pass.

# Details

Reference run `7828fdff-22cb-48cf-8232-39afd7da6f16` compiled all editor-feature lib/tests in10m30,
then ran159 tests across32 binaries in408.044s. It performed a real nested desktop build with
explicit target/profile/features.157 passed, including the ordinary broader consumers. Failures:

- editor_ui_refresh::saved_ui_prefab_refreshes_project_content_and_explicit_restart seeds an old
  brush and expects source Open to migrate and become dirty. It fails at prefab_tab.dirty; the
  subsequent Windows0xc0000409 abort is fixture teardown after that assertion, not a separately
  diagnosed native stack overflow.
- project_content_boot::committed_project_content_boots_from_random_process_environment expects
  Files.high_water6 but observes7. All closure-member comparisons passed before the assertion.

Read-only attribution by James `01a1989d-34ff-7630-9a22-f7f99c8818af` finds the exact peak:
SourceReadiness::capture retains one Files reservation for the actual nara.toml observation while
the six content-file charges coexist. At into_lease, those six Files end; the final readiness
reread overlaps only the retained first manifest reservation. Thus max(1+6,1+1)=7, not eight
cumulative reads. DependencyEdges remains4 Prefab occurrences +4 Sprite references=8; closure
members remain6. Parent updates only the exact assertion and explanatory formula, without raising
any cap. The reviewer is closed. Dynamic rerun is pending.

Actual production nara_ui still contained Visual1→2, Text1→2→3 migration callbacks, legacy brush
schemas/helpers, synthetic catalog generations/predecessors and historical reference tombstones.
Accepted ADR0102 now selects the current canonical first-party component/catalog/binding/codec1
baseline with current typed Image and bounded logical-size semantics, no prototype reader. Parent
also reconciles current ADR0081/0113 and authoring-interface-ergonomics prose while preserving
historical qualification records. Generic project evolution, including actual EnemyStats v1→v2,
is retained and must not be mechanically reset.

Active bounded writer Rawls `01a1989d-0ef4-79f3-8dc9-cb2f92925ba2` owns ONLY crates/nara_ui/src
(and crate tests as needed). It removes obsolete migrations/predecessor plumbing and updates
current baseline/bounds tests, typed Image and deferred/contextual decoding unchanged. No Cargo,
fmt or Git is authorized to that writer. Parent must not edit or globally format those files until
source-ready. There is no current Nara Cargo running; Reference session50577 is terminal100.

External inventory Hegel `01a1989e-10a2-77c1-935c-9e6589850646` is closed. It found one external
positive Rust version literal: runtime/tests.rs::ui_visual_record. Parent changes it2→ONE; its
image/metadata failure controls remain. authoring/mod.rs's intentional forged owner/fingerprint
negative remains untouched. No other external UiText3/UI generation/binding3 hardcoding found.

Parent-owned current data update changes14 UI records and two catalog baselines in exactly:
reference-game/prefabs/reference-ui.prefab.json, reference-game/schema/authoring.bundle.json,
tests/fixtures/generic-editor/image-import/prefabs/ui.prefab.json and its schema bundle. Payloads
are already current typed brushes/logical-size; only first-party versions/history are consolidated.
Both UI catalogs drop their old predecessor copies and reference tombstones. This removes about
6700 lines of obsolete definitions. Authoritative producer validation is still required after the
writer freezes; these are not yet qualified bundles.

Parent-owned editor_ui_refresh now seeds canonical typed image data, checks no source upgrade,
versionONE in source/effective output, source Open clean checkpoint and original bytes preserved,
then continues the existing image Create/Move/Delete/Undo/Save/Reopen/Restart consumer. It no
longer constructs a legacy reference string or relies on Native Open rewriting it. Source helper
source_brush uses encode_asset_reference_value. Not compiled/formatted since these changes.

# Next Action

Wait for Rawls source-ready; inspect final diff, then format. Validate nara_ui owning/current
baseline/native codec/authoring tests and authoritative checked-in bundle output. Existing
reference-game authoring_bundle binary supports --check/default and --write through its real
producer; rebuilding after writer freeze must use root target/jobs1. Generic-editor Image bundle
can be checked against current built-in providers through existing fixture/producer consumers.
No new migration guide or compatibility helper is needed.

Run focused Reference editor_ui_refresh and project_content_boot on final source before extending
qualification. Do not rerun all159 with the expensive nested build merely to check two fixture
corrections unless their coverage is materially affected. UI baseline changes require the relevant
Editor/UI and independent consumers to be rebuilt; no-UI Root392 and process22 keep their original
feature scope. Finish all-feature correctness Clippy/minimal checks and selected actual DX12
trajectories. Record accurate results, complete unit evidence/ledger, exact local commit and ff main;
no push/PR. All earlier independent handoff/recovery reviews remain scoped to their source.

CP2 research is recorded separately and remains unimplemented. It is not an added CP1 condition.
Local branches remain only main and refactor/foundation. Preserve original vendor and all concurrent
work. An unrelated Baozi Cargo may run; never stop it. No permission question is pending.

# Citations

- [Previous process/consumer checkpoint](2026-11-01T192930Z-fresh-process-recovery-and-independent-source-consumers-pass-while-product-qualification-continues-dfbbb0a8998a433090d17aa6fad56b93.md)
- [Owning plan](../../../../plans/2026-11-01-authored-schema-upgrade-plan.md)
- [ADR0102](../../../../architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md)
- TEMP/nara-authored-upgrade-final-reference-game-tests.log.
