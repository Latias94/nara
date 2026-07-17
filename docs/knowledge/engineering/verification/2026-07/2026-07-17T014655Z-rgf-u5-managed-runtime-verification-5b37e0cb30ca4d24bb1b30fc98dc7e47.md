---
type: "Verification Evidence"
title: "RGF-U5 managed runtime verification"
description: "Verified sealed-App admission, sticky fault propagation, exact stepping, bounded close ownership, Winit retirement ordering, and independent reference-game consumption."
timestamp: 2026-07-17T01:46:55Z
record_id: "5b37e0cb30ca4d24bb1b30fc98dc7e47"
tags: ["rgf-u5", "verification", "runtime", "ownership", "winit", "tasks"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "ff6b111b24fe692c23e1b42ef0a2e3cd407647fc"
verified_by: "cargo-nextest,cargo-check,cargo-fmt,cargo-doc,independent-review"
---

# Verification

RGF-U5 commits one thin executable owner around the existing App. It admits a sealed, unstarted
App, preserves App-owned schedule/time/tracker/World authority, exposes generation-scoped controls
and exact fixed stepping, propagates the first typed fault, and closes explicitly registered
owners through a finite retryable protocol. Winit drives that runtime and retires its surface and
provider before native release. ADR 0084 remains Proposed.

# Result

- Implementation commit: `ff6b111b24fe692c23e1b42ef0a2e3cd407647fc`.
- Independent closure review: no open P0/P1; every original P1 has a named regression.
- Abnormal admission/start/runtime Drop retains incomplete ownership in a bounded observable
  owner-thread quarantine instead of leaking with `mem::forget`.
- Candidate, driver, and close scopes preserve canonical fallible-system/observer capture while a
  faulted driver scope remains usable for retirement.
- Required task/service outcomes fault through real domain integration systems; optional outcomes
  remain non-fatal domain results.
- Stop publishes `Stopping` before close work, preserving platform retirement authority. Terminal
  plugin shutdown failure remains failed evidence even after registered ownership reaches
  `Stopped`.

# Evidence

- Runtime instance and driver boundary: 56/56 passed.
- `nara_app`, `nara_gameplay`, `nara_tasks`, and `nara_winit`: 144/144 passed.
- Independent reference-game runtime: 1/1 passed.
- Root workspace: 778/778 passed with three declared conditional skips; workspace check passed.
- Architecture governance: 5/5 passed.
- `windowed_clear`, `windowed_sprites`, and `runtime_ui_panel` compiled under the combined desktop
  runtime/render feature ceiling.
- Formatting, rustdoc, staged and working diff checks, static boundary searches, and the final
  independent staged-snapshot review passed.
- Reviewed staged code fingerprint: `cb23536128b2cd3f16685b04f9772f1274d622dc` against source
  head `2e6b92f0773904b3ff431e52906fa6e69bae88bf`.

# Follow-up

- RGF-U24 and RGF-U25 own the permanent ordinary product action and author-facing concept budget.
- RGF-U13 owns native event-loop smoke and desktop parity.
- RGF-U12 and RGF-U29 remain dependency-ready in parallel; RGF-U26 waits for both. RGF-U28 must
  close before RGF-U6.
- Non-blocking P2 follow-up remains for terminal native-wait wake frequency, close-ledger double
  scanning, and timing-based task tests under their recorded triggers.

# Citations

- `docs/knowledge/engineering/subagents/2026-07/2026-07-17-rgf-u5-runtime-closure-review.md`
- `docs/knowledge/engineering/subagents/2026-07/2026-07-16-rgf-u5-runtime-code-review.md`
- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md`
- `docs/architecture/adr/0084-executable-runtime-ownership-and-isolation.md`
- `docs/migrations/2026-07-engine-foundation.md`
- `crates/nara_app/src/runtime.rs`
- `tests/runtime_instance.rs`
- `tests/runtime_driver_boundary.rs`
- `reference-game/tests/runtime_core.rs`
