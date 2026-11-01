---
type: "Work Progress"
title: "Source-owned UI construction and actual creation templates pass all owning controls"
description: "Tooling/UI251 pass after canonical source recipe integration; real bundle producer is building before final Reference, feature/lint and native checks."
timestamp: 2026-11-01T20:00:06Z
record_id: "2b5140bf02e3460a9a8e072ae65d3514"
producer_id: "codex-foundation-main"
run_id: "authored-upgrade-ui-construction-20261102"
related_plan: "docs/plans/2026-11-01-authored-schema-upgrade-plan.md"
supersedes: "ccb4164c44af4baaacdf1f9787982ad8"
---

# Summary

The full foundation Goal remains active and the authored-upgrade unit uncommitted at ed21b230 on
refactor/foundation. The source recipe routing gap and real UI creation templates are implemented,
formatted and pass all owning tests. Current source is frozen; no writer remains. The actual
Reference bundle producer is still building, so do not start another Cargo or claim final
Reference/native qualification. No push/PR or complete-unit commit has occurred.

# Details

Source-safe Prefab recipe creation lowers through actual source templates and declared property
writes. It verifies complete prepared payload/version/optional owner equality before the ordinary
source commit, retaining mapping, selection and one history transaction. It admits cumulative
replacement costs before cloning and retains the original source candidate/inverse limits. No
Native fallback, second history or new public API is introduced.

Five real complete UI templates are now part of the canonical version-one catalog: UiNode from
UiStyle::default, Panel Visual with no image, Text("Text",16), Button("Button","activate",16),
and default ButtonVisuals. Existing encoders produce each template. Root/Theme/Semantics gain no
template. Typed Image, deferred/context decode, opaque ID refusal and F32 `(0,256]` semantics
remain. Current ADR0102/0081/0113 and ergonomics prose are reconciled; historical evidence remains.

Epicurus and Ohm are source-ready and closed. The new Tooling module is included via cfg(test)
mod source_composition_tests in construction.rs. Its real Data-owner fixture uses an empty frozen
Native registry, a preserved unrelated opaque record and exact authored wire-field mapping. Three
controls cover exact two-component creation/history, empty creation/history, and eight categories
of atomic refusal preserving both history branches/selection/manifest. The read-only component
case is rejected by ordinary composition eligibility, not solely the final payload equality.

Run `fb059a4b-1857-4967-9867-b1de606d9463` passes251/251 across nara_tooling and nara_ui complete
all-feature library suites, zero skipped,6.822s execution after19.93s build. This covers the new
templates, copies/no-AssetServer preflight/roundtrip and invalid-value refusal, new source recipe
creation and all existing owning tests. Prior UI116 and focused Reference1/2 retain their earlier
scope; Reference must still rerun against the new source and actual updated catalog.

Independent Archimedes `01a198ad-5dea-7aa0-8b39-29999f3b2d54` found no remaining assigned P1/P2
in the parent construction helper, actual Scene owner boundaries and source route. It checked
source authority, precopy gates, exact result equality, stale/history/selection and preserved UI
Session gates. That reviewer is closed. Einstein `01a198b1-6e4c-7572-ae58-9ef5ca6d4551` is the
only active agent, read-only final review of the added UI templates and complete source recipe
coherence. It has no write/Cargo/fmt/Git permission. Receive/close it when complete.

Current serial Cargo session66050 is running:
cargo +1.95.0 run --manifest-path reference-game/Cargo.toml --locked --target-dir target
--bin authoring_bundle -- --write
with CARGO_BUILD_JOBS1 and root CARGO_TARGET_DIR. Log:
TEMP/nara-canonical-ui-authoring-bundle-write.log. It uses the producer's default non-Editor
feature graph, requiring fresh dependency compilation despite previous editor-feature tests.
No error is known; latest output is compiling nara_text around local04:00. Do not cancel/restart
or launch another Cargo. No other Nara build is known; unrelated Baozi builds must not be stopped.

# Next Action

Poll session66050 to its actual terminal result. After producer success, copy ONLY its exact
nara.ui.components owner into tests/fixtures/generic-editor/image-import/schema/authoring.bundle.json,
preserving every other owner. The14 checked-in UI record payloads were already unchanged/versionONE.
Both current bundles need the newly produced five authoring_initial_value templates; do not invent
their JSON manually. Run Reference producer --check, then focused Reference editor_ui_refresh and
project_content_boot with features editor/root target. It must finish Create/Move/Delete/history/
Save/Reopen/Restart, not only clean source Open. Generic Editor fixture admission also needs final
source coverage. Reference full159 must not be called passed: the actual previous result is157/159.

Then complete Root+Editor all-feature library/appropriate UI/source tests, correctness Clippy
all-targets and minimal checks, independent formatting, and selected hidden actual Windows/DX12
tests under docs/verification/native-editor-qualification.md: three critical trajectories, retained
Scene and UI Canvas navigation/close, complete Prefab conversion Save/Reopen and stock effects.
The final no-UI Root392, Application35, independent process22, public100 and module49 evidence is
recorded in preceding shards and retains its exact source/feature scope. No need repeat unrelated
expensive nested builds merely for documentation closure, but new source/feature behavior needs
fresh relevant checks.

After complete results, create final verification, update ledger/selected plan/guides honestly,
inspect exact new+modified files, Conventional Commit and local ff main. No push/PR. Complete
foundation Goal stays active: CP2, motion/execution, material generations and Nara-drawn editable
Editor remain. Local branches only main/refactor/foundation; no extra branch cleanup is needed.
Preserve vendor and concurrent edits. No permission question is pending.

# Citations

- [Construction predecessor](2026-11-01T195522Z-canonical-ui-source-creation-now-lowers-recipes-through-actual-templates-and-declared-properties-ccb4164c44af4baaacdf1f9787982ad8.md)
- [Current complete unit](../../../../plans/2026-11-01-authored-schema-upgrade-plan.md)
- TEMP/nara-source-safe-ui-construction-owner-tests.log.
