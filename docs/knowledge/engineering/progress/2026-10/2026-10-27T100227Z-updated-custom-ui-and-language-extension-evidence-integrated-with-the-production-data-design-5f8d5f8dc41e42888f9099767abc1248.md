---
type: "Work Progress"
title: "Updated custom UI and language extension evidence integrated with the production data design"
description: "Records source-only UI and language findings, the closed Scene representation proposal, resolved export-policy design review and next production work without declaring SDK or toolkit adoption."
timestamp: 2026-10-27T10:02:27Z
record_id: "5f8d5f8dc41e42888f9099767abc1248"
producer_id: "codex-root"
run_id: "editor-language-research-2026-10-27"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/scene-data-representation"
git_commit: "65402b9206b26555787e4da5b7f7abd1a0c048fa"
verified_by: "01a17ccb-bd08-7de2-9deb-48087c0e7778"
---

# Summary

Integrated the newly appended custom-UI and language/plugin sections of the user-supplied
architecture conversation (lines 3611-4762). The input SHA-256 was
`e18fc9ee88fc1bc5e341737f67d167807e362547a08581197e52017e5dc43fc5`.
This is a source/design checkpoint, not production implementation. The full foundation Goal
remains active; ADR 0120 remains Proposed. No official C# SDK or replacement UI toolkit was selected.

# Details

Two disjoint research writers produced the [UI](../../../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md)
and [language/extension](../../../../research/2026-10-27-language-extension-product-boundaries.md)
reports. Newly cloned ignored references are Fyrox at
`f008e4c2cdfdcda7b37bf8534d0df8de155748f6` and Xilem/Masonry at
`271a27a6d4a930f7878d404f9014e3c50a3a9b88`. Both are clean sparse checkouts; relevant omitted
source was inspected online at the same pins. Initial Git Schannel TLS failures were retried
with the OpenSSL backend through the supplied proxy, without disabling certificate verification.
Existing reference code and user/vendor content were not changed.

Important counterexamples absent from the conversation's broad recommendations:

- Fyrox's pinned ordinary input path has no IME composition grammar; its default list/tree
  construction is not a viewport-proportional virtualization implementation.
- Masonry has a real host-neutral root and caller-target rendering, but wgpu 28 differs from
  Nara's wgpu 30. External paint placeholders still require a compositor. Plain Xilem Vec
  reconciliation is positional, not proof of semantic identity preservation after reorder.
- Current Nara panel contributions are compiled Rust, not a reloadable managed tool SDK.
  A registration scope and stale-result guard do not prove callback/task/code retirement.
- Optional official C# can fit Rust-complete Strategy, but its recommendation is not adoption.
  Prior Name callbacks do not close installed component/Inspector/panel/command workflows.

The [Scene representation design](../../../../architecture/scene-component-representations-design.md)
and [production plan](../../../../plans/2026-10-27-scene-data-representation-plan.md) specify the
first closed native/data family through existing compiler, publication and live-export owners.
Hubble (`01a17cb7-adba-79c0-9e9e-cdca4ff15f91`) found a P2: native asset export policy is
implemented by codecs, whereas pure data has durable AssetRef values. The amended design and
plan explicitly lower Schema-declared asset references under both policies through selected
metadata, with missing required conversion metadata rejecting the entire output and no loads.
This resolves the design ambiguity, not the unimplemented runtime behavior.

Poincare (`01a17ccb-bd08-7de2-9deb-48087c0e7778`) independently reviewed all 11 product documents,
including targeted primary-source checks, and reported no remaining findings within that scope.
An initial local check resolved 946 local links across those 11 documents; fenced-code balance
and `git diff --check` passed. No Cargo, formatting, build, native interaction or previous trial
rerun occurred because only documents changed. Source inspection is not performance, IME,
accessibility, embedded-Viewport or unload qualification. All research/review agents were closed.

# Next Action

Implement the proposed production Scene data family with backend-free metadata admission,
strict mixed compilation, atomic carrier preparation/publication and complete live export.
Keep its public/managed consumers independent of toolkit selection. Then replace the private
preview helper in the selected language owner and evaluate one complete C# EnemyStats tool
against the existing native flow, coordinating install/lifecycle with the existing plan.

For custom UI, Masonry is the leading bounded integration experiment against incumbent Dear
ImGui, not a production adoption. Account for dependency alignment, actual Host/Viewport ownership,
logical keys, composition, virtualization, native semantics and physical retirement before
changing ADRs 0107/0118's toolkit decision. Do not grow another universal extension manager or
parallel permanent language SDKs. Local Conventional Commits and ff integration remain authorized;
no push or PR is authorized by this work.

# Citations

- [Foundation reassessment](../../../../architecture/foundation-architecture-reassessment.md)
- [Foundation execution plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0093](../../../../architecture/adr/0093-rust-authoring-hot-iteration-and-optional-scripting-adapters.md)
- [ADR 0107](../../../../architecture/adr/0107-shared-ui-foundation-and-independent-product-layers.md)
- [ADR 0120](../../../../architecture/adr/0120-schema-publications-and-execution-representations.md)
