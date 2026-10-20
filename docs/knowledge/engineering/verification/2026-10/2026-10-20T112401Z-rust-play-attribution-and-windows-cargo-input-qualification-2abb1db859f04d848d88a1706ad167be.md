---
type: "Verification Evidence"
title: "Rust Play attribution and Windows Cargo input qualification"
description: "Public Play/Restart, private native Cargo attribution, exact CLI cache interoperability and compiler failure recovery at the corrected Windows path boundary."
timestamp: 2026-10-20T11:24:01Z
record_id: "2abb1db859f04d848d88a1706ad167be"
status: "verified"
producer_id: "codex-root"
related_plan: "docs/plans/2026-10-20-rust-play-attribution-plan.md"
git_branch: "main"
git_commit: "2c26c92a42229d4b80f18ee43628ad1225d81a38"
verified_by: "independent source and numeric reviewers"
---

# Verification

Implementation `2c26c92a42229d4b80f18ee43628ad1225d81a38`; final headless diagnostics and
workspace checks include the unrelated-to-Play platform test correction at `da76a21`.
Rust/Cargo 1.95.0 on Windows; root alone runs Cargo, one build job and the existing shared target.
The disposable Collect Game is outside the checkout and retained. No user source is discarded.

# Result

L4's bounded public consumer, attribution and Windows input correction are qualified. Public
Editor application actions drive a real external desktop game through validated first presentation;
this is not a native Editor widget/input-replay qualification. The same-parameter ordinary CLI
build reuses all three game artifacts. Compiler failure, last-good custody, repaired fresh
generation, Stop/physical close and original source-byte restoration all pass.

Baseline/final cohorts are sequential and uncontrolled. Body-edit and Restart medians increase;
the report retains every group rather than claiming a general build-speed gain. The game's
dependency graph, build profiles, linker settings and immutable generation authority are unchanged.
The new Windows-only Editor `dunce` dependency stabilizes Cargo path spelling after actual
canonicalization. All temporary production probes are removed.

# Evidence

| Check | Result and identity | Retained diagnostic or source |
| --- | --- | --- |
| Public Play baseline | 24 windows plus three semantic oracles and failure recovery; nextest `6edb2a6d-15d9-4e82-87f9-ed0c28f66fd5`, 445.986 s total | `target/rust-play-locality-1.log`; archived raw JSON and collector-source hash in the report |
| First temporary phase attribution | Eight windows, six nested/partial stages per Play; `826d04c0-a8fc-4edd-aef2-5448fb1570d6`, 142.217 s | `exploratory-phases.json`; ten supervised native Cargo HTML reports plus a labelled ordinary CLI control |
| Second temporary attribution | Eight windows, including an all-Fresh no-edit counterexample; `d5e7eeeb-da3f-43a8-9b17-6e3c9427f1a6`, 142.387 s | `exploratory-phases-second.json` and two matching native Cargo HTML reports |
| Matched Windows path control | Unchanged repeated inputs expose `PathToSourceChanged` with ordinary/verbatim path spellings | `windows-path-freshness.json`; diagnostic, not public timing acceptance |
| Final public Play | 24 windows, three semantic results, all-three-Fresh CLI reuse and complete failure/recovery; `9e46d91e-36b0-451d-9a5a-58e2dd3b3705`, 228.825 s | `target/rust-play-locality-final.log`; `public-play-path-correction.json` |
| Editor library | 170 passed, 20 native/measurement tests explicitly ignored; `8ac9133c-4066-4ef4-87bc-0c7ddd728cb0` | `target/play-editor-owner-final.log` |
| Headless native Cargo control | Two builds, first 165 fresh/3 dirty after source restoration; unchanged second 168 fresh/0 dirty. Both real games pass 62 ticks/three goals/victory/x 108 | `headless-cargo-diagnostics.json`, two native HTML reports; `target/play-headless-timings-[1..2].log` |
| Final workspace build | Locked/offline all-feature/all-target workspace check passed, 1m18s; test-only dead-code warnings remain | `target/locality-ci-workspace-final.log` |
| Formatting and diff | Root, reference-game and module-consumer checks pass; `git diff --check` passes | Serial Cargo fmt and Git checks |

Independent review recomputes both public cohorts' 32 displayed statistics, generation/source
hashes and all game/recovery/CLI oracles. Thirteen desktop diagnostic HTML hashes/static fields
and two headless diagnostic hashes are verified. The archived baseline collector is reconstructed
from the committed final collector after removing the later CLI oracle; its retained hash matches.
Cargo HTML static fields are read directly; its internal JavaScript is not a machine protocol.
No phase costs are assigned by subtracting independent percentiles or summing nested intervals.

# Follow-up

L1-L4 are complete within the selected locality plan. Complete encoders/clone/O(N) scalar work
and seconds of compilation/startup remain measured pressure. Profile/linker experiments need
controlled A/B and explicit debugging tradeoffs. No hot-patch ABI, second language, generic async
owner trait, Tile tools or browser runtime is selected. Hosted CI qualification proceeds separately
under the CI repair plan; the earlier intermediate CI record is historical, not current green CI.

# Citations

- [Complete report, reproduction and raw data](../../../../benchmarks/2026-10-20-rust-play.md)
- [Completed L4 plan](../../../../plans/2026-10-20-rust-play-attribution-plan.md)
- [Locality plan and remaining measured costs](../../../../plans/2026-10-20-measured-work-locality-plan.md)
- [Separate CI repair](../../../../plans/2026-10-20-ci-portability-repair-plan.md)
