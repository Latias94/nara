---
type: "Work Progress"
title: "Canonical UI source creation now lowers recipes through actual templates and declared properties"
description: "UI baseline tests116 pass; corrected Reference readiness passes but source Create exposes an omitted recipe route, now compiled through the real source construction owner."
timestamp: 2026-11-01T19:55:22Z
record_id: "ccb4164c44af4baaacdf1f9787982ad8"
producer_id: "codex-foundation-main"
run_id: "authored-upgrade-ui-construction-20261102"
related_plan: "docs/plans/2026-11-01-authored-schema-upgrade-plan.md"
supersedes: "4ade1b7c592d4408842cc351ab494274"
---

# Summary

Full foundation Goal active; complete authored-upgrade WIP remains uncommitted at ed21b230 on
refactor/foundation. The Reference qualification exposed a second missing source-authority seam
after canonical UI cleanup: specialized UI creation still produced raw records, but source
Workspace routing only allowed empty drafts. Parent now lowers finite recipes through the existing
source construction owner. It does not restore Native whole-record authority or add a UI bypass.

# Details

Canonical UI baseline owner run `1b5a8a61-1cbf-41aa-91b0-6b1b06951db5` passes116/116 with no
skips, before adding creation templates. It removes the prototype Visual/Text migration chains,
synthetic catalog predecessors and tombstones, keeping typed Image/deferred codec and exact F32
size constraints. Independent Carver `01a198a5-694e-7a91-b405-90a710b126f8` found no assigned P1/P2,
verified exactly14 record versions with identical payloads and two UI-only catalog updates, and is
closed. That review does not cover the new construction bridge/templates below.

Focused Reference run `93ee599a-0486-4f6b-a9a5-1ab4a56e1302` runs2 tests: project_content_boot now
passes with the exact Files7 formula, while editor_ui_refresh gets past canonical source Open and
fails at UI Create (`prefab_composition_outcome=None`). Its original clean/unchanged source checks
passed. Windows0xc0000409 follows the assertion/fixture teardown, not a new diagnosed stack issue.
Source routing intentionally rejected nonempty PrefabCompositionEntityDraft; current UI schemas
also lacked actual creation templates. Native loading fallback would conceal this owning gap.

Parent source-ready production patch in crates/nara_tooling/src/workspace/construction.rs and
workspace.rs adds apply_authoring_prefab_creation. It begins the actual bounded construction,
requests each real source-owned template, writes only declared scene/edit property roots and
checks cumulative authoritative field limits before payload clones. It prepares under the exact
source basis, then requires the complete created component payload/version and any supplied owner
to match the requested finite recipe. Unknown/extra payload, read-only differences, wrong versions
and missing templates cannot be silently dropped. A shared private commit_selected_construction
keeps exact local mapping, selection, Created outcome and one existing source history transaction.
Standalone Native-only composition retains its genuine registry path. Tooling all-feature lib
check passes3.83s; no owning tests on this patch yet.

Accepted ADR0113 now records this lowering/authority boundary. Parent also adds the full Workspace
report to the Reference Create failure message. These latest files are not yet formatted.

Current agents and disjoint write sets:

- Epicurus `01a198ab-2355-7592-b84c-3819685ac4e0`: writes ONLY nara_ui/src/codec.rs and
  component_schema_tests.rs. It adds real complete creation templates for Node/Visual/Text/Button/
  ButtonVisuals through current native encode helpers, no new schema history/version. Root/Theme/
  Semantics are not part of the recipe and gain no speculative template. The code is visible, but
  wait for source-ready before fmt/affected compile. No Cargo/Git allowed.
- Ohm `01a198ac-0b78-70f1-9ce3-ffa9665723bc`: owns ONLY NEW
  crates/nara_tooling/src/workspace/construction/source_composition_tests.rs. Parent must add its
  mod include once ready. Tests use actual Data owner/adapter and empty Native registry to prove
  public recipe creation/history/selection and atomic owner/version/template/extra/readonly refusal.
  It must not edit parent construction.rs/workspace.rs or run Cargo/fmt/Git.
- Archimedes `01a198ad-5dea-7aa0-8b39-29999f3b2d54`: read-only independent review of parent helper
  and current Scene construction boundaries. No assigned writes or Cargo. Await final findings.

There is no known current Nara Cargo. Session99770 (Reference2) is terminal100,54153 (UI116)0,
and the focused Tooling library check is terminal0. Unrelated Baozi Cargo may run; do not stop it.
All previous CP1/handoff/process evidence remains at its documented feature/source scope; no-UI
Root392 and process22 do not exercise UI creation. Need rebuild actual bundle producers now that
templates participate in UI authoring identity.

# Next Action

Freeze Epicurus/Ohm, receive independent review, add tests module include and inspect diff→fmt.
Run Tooling source-composition tests and nara_ui current baseline/templates tests, fixing actual
failures without bypassing source template/field authority. Use root target/jobs1, strict serial.
Then rebuild/run reference-game authoring_bundle --write through its actual producer. Parent has
already consolidated current resource records/catalogs, but new UI template values are not yet in
the two checked-in bundles. Copy only the resulting exact UI owner into generic-editor/image-import
bundle, preserving its other owners; verify Reference --check and current generic Editor admission.
No migration guide or compatibility helper is needed.

Rerun Reference editor_ui_refresh/project_content_boot on final source. Continue final Root+Editor
all-feature library/related consumer coverage, correctness Clippy/minimal and selected actual DX12
critical/retained Scene/Canvas/Prefab/stock-effect trajectories. Then complete honest unit evidence,
ledger, exact Conventional Commit/local ff main. No push/PR. Do not call all159 passing: original
Reference157/159 plus focused1/2 are the observed results so far. The complete transformation,
recovery, source preservation and UI structural workflow must all hold together before delivery.

Full Goal and separate CP2/motion/render/material/custom editable Editor work remain open.
CP2 study is recorded and not adopted; branches already only main/refactor/foundation. Preserve
vendor and concurrent work. No permission question is pending.

# Citations

- [Previous UI baseline checkpoint](2026-11-01T194254Z-reference-consumer-exposes-unreleased-ui-migration-history-and-source-readiness-accounting-4ade1b7c592d4408842cc351ab494274.md)
- [Complete unit](../../../../plans/2026-11-01-authored-schema-upgrade-plan.md)
- [ADR0113](../../../../architecture/adr/0113-editor-ui-prefab-authoring-and-canvas-authority.md)
- TEMP logs: nara-ui-canonical-baseline-owner-tests.log,
  nara-ui-canonical-reference-consumer-tests.log,
  nara-source-recipe-construction-check.log.
