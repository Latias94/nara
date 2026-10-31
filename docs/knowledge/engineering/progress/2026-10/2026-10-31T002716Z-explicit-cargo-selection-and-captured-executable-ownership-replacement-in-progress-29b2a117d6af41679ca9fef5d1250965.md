---
type: "Work Progress"
title: "Explicit Cargo selection and captured executable ownership replacement in progress"
description: "Replaces fixed desktop selectors through native build, source Export and supervised Editor consumers; independent and foreign artifact qualification pending."
timestamp: 2026-10-31T00:27:16Z
record_id: "29b2a117d6af41679ca9fef5d1250965"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-explicit-cargo-build-selection-plan.md"
git_branch: "refactor/foundation"
git_commit: "0c36715"
---

# Summary

The next R7 replacement is in progress on `refactor/foundation` after qualified local builds
at `0c36715`. The complete foundation Goal remains active. This unit selects explicit trusted
Cargo builds and captured executable ownership; complete target-content/package/activation
work remains separate. No part of this new unit has full qualification yet.

# Details

The pure Cargo layer now uses explicit package/bin/profile/triple/default-feature/feature
selection and full Cargo metadata. Actual resolved node features establish feature closure;
the old fixed Desktop/Headless decoder types and selectors are removed. The current supervised
Editor, starter tool bootstrap and source Export are being migrated as actual consumers.
Source Export now holds captured executable bytes and per-artifact provenance rather than
reopening mutable Cargo output in staging. Its exact native product agreements remain separate.

Parent owns `project_tooling::cargo_build`: selected actual tools, declared source inventory,
bounded metadata/build output, original input revalidation and captured target bytes. Early
independent review found actual gaps in cross-session directory equality, rustup proxies,
tool path binding, callback/reentry output attribution, Windows spelling, allocation accounting
and direct-child error custody. Corrections use same-object checks, resolved actual tools,
before/after path/digest verification, one cooperative target-directory lock through capture,
bound output before callbacks and explicit cleanup-incomplete child/lease retention. No global
budget/lock registry, target execution or universal process kernel was added.

The first two root checks exposed parent module-path and RelativePath accessor mistakes;
owning paths now use explicit component conversion. The final project-tooling-only library
check passed. No tests are claimed from that compilation. The first independent actual native/
foreign-build consumer is compiling now with the existing root target and serial Cargo. Its
new external lockfile was created offline through Cargo. A reviewer is rereading production
corrections; Editor/native and source Export acceptance still need execution.

Pinned reference study confirmed Bevy's Host-tool versus wasm output and optional runner split,
and Godot's preset/template/export versus launch distinction. Neither source comparison proves
Nara build success or hermeticity. The selected finite per-file input envelope explicitly leaves
ambient trusted Cargo/native/config/environment inputs outside its guarantee.

# Next Action

Finish native-owner/core/receipt independent reviews, run actual custom-workspace native/WASI
build consumers and existing processed-input physical cases, then owning source/Editor and
affected native qualification. Freeze writers before formatting. Reconcile guides/implementation
ledger only with actual results, remove superseded assumptions, commit coherent qualified units
and locally fast-forward main. Preserve vendor and read-only refs; no push/PR.

# Citations

- [Unit plan](../../../../plans/2026-10-31-explicit-cargo-build-selection-plan.md)
- [Accepted workflow refinement](../../../../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md#explicit-cargo-selection-and-captured-build-results)
- [Independent consumer](../../../../../tests/fixtures/cargo-build-consumer/README.md)
