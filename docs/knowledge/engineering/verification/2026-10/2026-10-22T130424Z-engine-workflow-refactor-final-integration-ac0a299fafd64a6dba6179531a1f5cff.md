---
type: "Verification Evidence"
title: "Engine workflow refactor final integration"
description: "Final U1-U9 owner, public-consumer, Editor and compilation evidence with bounded review outcomes."
timestamp: 2026-10-22T13:04:24Z
record_id: "ac0a299fafd64a6dba6179531a1f5cff"
producer_id: "codex-root"
run_id: "engine-workflow-2026-10-22"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-22-1100-refactor-engine-workflow-boundaries-plan.md"
git_branch: "test/compiled-asset-domain-export"
---

# Verification

Integration of the [U1-U9 workflow plan](../../../../plans/2026-10-22-1100-refactor-engine-workflow-boundaries-plan.md)
on `test/compiled-asset-domain-export`. The receiving session
`01a1629b-6c28-7203-87c5-85328214df4a` is authoritative after recovery; the earlier
`01a15d8d-ceda-7543-8053-2aca71290dfd` rollout remains historical evidence.
The initial source baseline for U7/U8 integration was `86f6650`; final commits are listed below.

# Result

U1-U9 are complete within the selected workflows, with independent review corrections applied
and no unresolved correctness finding in the changed contracts. The final code is committed
through `0633602`; the subsequent integration commit contains documentation and derived memory.
Existing passed cohorts retain the exact source and feature scope below. Cache deletion does not
invalidate an executed test, but those results do not qualify subsequent behavioral changes.

| Unit / responsibility | Local commit |
| --- | --- |
| U1 ordinary nonmodal routing | `e72fb5b` |
| U2 instance tint and complete target image demand | `af8ffcb` |
| U3 close evidence and responsibility-based runtime extraction | `69082c0` |
| Shared document incarnation/revision primitives | `0e7d33c` |
| U4 safe format diagnostics and U9 delivery catalogue | `86f6650` |
| U5 managed native-domain travel progress | `3e73b0d` |
| U6 source-local EntityRef authoring | `a186507` |
| U7 independent document owner, persistence and source exclusion | `a765fec` |
| U8 ordinary product launch and cleanup | `0633602` |
| U9 unpressured queue admission | `42875e3` |
| Editor polling/reference test correction | `0dd1bb5` |

# Evidence

All Cargo commands are serialized, set `CARGO_BUILD_JOBS=1`, and use the root `target` unless an
exception is identified. The recorded development toolchain is Rust 1.99.0 on Windows 11.

| Scope | Result | Nextest run ID |
| --- | --- | --- |
| App, Tasks, UI, Input, Sprite, WGPU, Scene, Tooling and Winit libraries, all features | 689 passed, 8 excluded | `c2741b33-e352-497d-9b9d-26a6045baae5` |
| Root content/Browser/launch/Scene request selection | 63 passed | `41cabb16-fd68-4ad1-aa68-94d23b99c325` |
| Complete Editor persistence, Scene workspace, content boundary/delivery/runtime boot binaries and desktop terminal lowering | 68 passed | `66be6cc3-c116-403d-b464-3f1b8b00f206` |
| Public App runtime close, simulation and owner-thread integration | 13 passed | `3f9a8023-ec7c-4246-98e3-40fdc2fca3bf` |
| External managed native-domain travel and nested/optional source-local EntityRef workflow | 19 passed | `b87f3255-cc9f-413d-bb47-22fa8b24aad7` |
| External dialogue documents/panels and source import, before later file-alias correction | 16 passed | `ceeff5f5-c131-4f50-9d00-9721654fb76a` |
| Editor library, generic component composition, first-game workflow and authoring loop | 188 passed, 25 excluded | `df5d2355-1635-45d3-9eb5-665161ae9ee4` |
| Hidden native source-local EntityRef and independent dialogue document workflows | 2 passed, 198 excluded | `4198877c-d10c-4343-8f05-81c5b716a4ac` |
| Ordinary product preparation and live-cleanup subprocess | 3 passed | `ad581e3f-621d-4db8-ab91-84c8df6ec9b6` |
| Product launch/bootstrap/source receipts, Browser preparation, Scene requests, import refresh and diagnostic privacy | 50 passed | `88d280dc-f799-42d2-ae0b-6beadf834118` |
| Collect-game gameplay | 4 passed | `a68acadb-4415-495c-b4ce-0662a21628c1` |
| Two-level-game checkpoint and gameplay, including fresh-process save/restore | 16 passed | `a977ae75-3cb4-4b29-8cde-5de9cd7382c4` |
| Final external dialogue documents/panels and source import, including file-alias and retained-parent correction | 27 passed | `731300e3-fd14-440e-8ee4-fef257146d5a` |
| Final Browser/Host custody, owned Save/Reopen, complete persistence/workspace binaries and generated starter Cargo product | 42 passed, 264 excluded | `3b66c5e5-db76-4673-bfe9-df39752a545a` |
| Final hidden native EntityRef and dialogue document trajectories after file-alias correction | 2 passed, 198 excluded | `39ebd387-364e-4ba6-97c0-b304f42e92c4` |
| Existing source-carrying export agreement/revalidation and project creation | 15 passed, 277 excluded | `065b701f-ea5c-454a-ad9e-74f90154e83c` |

The first eight rows ran before the later U7 file-alias correction. They include the corrected
U3 pending-participant/final-failure semantics, U4 public diagnostic privacy, U5 retirement/discard
and retry diagnostics, and the original U7 source-namespace checks. The two sample test commands
used their existing sample-local targets; subsequent verification explicitly reuses the root target.
Both sample desktop/all-target checks also passed before the build-cache cleanup; collect-game's
lockfile was verified with `--locked`, while the two-level lock refresh used `--offline`.

The full locked/all-features/all-targets workspace check passed before the file-alias correction.
Native execution used `WGPU_BACKEND=dx12`, actual graphics, hidden windows and owned Context input.
It qualifies the two named widget/save/reopen/panel trajectories, not OS input delivery, monitor
scanout, Linux/macOS, the complete Rust 1.95 release gate, or a new export/platform claim.

The Editor observation regression initially failed because a post-Save image preparation
completion was incorrectly treated as an unchanged persistence-only poll. The test now compares
all observed work facts and waits for independent preparation before requiring a quiet poll.
The recursive-reference rejection test now uses a missing target instead of a supported local
reference. The final 188-test row follows both test corrections; product notification semantics
were not changed to satisfy the old timing assumption.

The ordinary cleanup subprocess initially exited before driving a retained owner with a zero
deadline. Both product loops now drive the owner once before checking the deadline. Terminal
failure and ordinary Pending cases report the actual unfinished participant; an outer
`Ok(report)` still requires inspection of the product outcome.

Late read-only review found that U7 compared source strings, allowing case aliases on Windows
to create multiple publication owners for one file. The correction keeps exact path reservations
and compares current file identities through retained publication capabilities. Scene/Prefab
also expose a private read-source capture, avoiding stale root-relative resolution after a
parent-directory rename. Tests cover aliases, source namespaces, atomic Save replacement, retained
parent directories and distinct files on case-sensitive filesystems. All 27 selected tests passed
after the correction. Case-specific tests return early when the required filesystem semantics are
absent: this Windows run does not establish execution of the distinct-case-file branch or Linux
qualification. The alias checks observe Open admission, not a filesystem-wide lock against
arbitrary later external mutations. Existing exact source and publication conflict checks remain.
An additional read-only correctness reviewer found no blocking issue in the corrected flow;
phase-specific external alias races were inspected but not separately fault-injected.
The final two native trajectories pass after that correction and formatting. No later Rust
behavior change follows their run.

The starter is an embedded source template, not a standalone Cargo workspace. Direct checks of
nonexistent template manifests and a nextest invocation without `project-workflow-cli` did not
run tests and provide no evidence. A correctly selected generated-product test was then stopped
by `rustc-LLVM ERROR: ... no space on device` before execution. Two sample build caches were
compressed without deleting files; the user subsequently ran `cargo clean` and restored space.
No source, vendor tree, or unrelated change was removed.
The correctly feature-selected generated-product test subsequently passed in the final 42-test
row. It materializes the ordinary starter, checks independent Cargo targets, and runs an offline
all-target/all-feature build against the actual checkout using the root target. The same cohort
also passes the negative compilation tests protecting opaque, linear persistence authority.

# Follow-up

`cargo fmt --all -- --check`, focused external/template formatting checks and final diff checks
passed. Independent U3/U5, U7 source-alias and U8 launch reviews have no remaining blocking
finding. Ordinary product cleanup deadlines are cooperative: the owner reports retained pending
participant evidence and exits on expiry between drives, but cannot interrupt a blocking callback.
An outer launch `Ok(report)` still requires inspection of the product outcome.

ADRs and their implementation ledger describe the resulting owners. The bounded architecture
review identifies six opportunities requiring an ordinary consumer or attributed measurement;
none authorizes a universal framework or a speculative rewrite. Broader native platforms,
Rust 1.95 release qualification, production physics/audio, cross-source references and dynamic
project-code loading retain their existing separate scopes. No remote publication was performed.

# Citations

- [Workflow plan](../../../../plans/2026-10-22-1100-refactor-engine-workflow-boundaries-plan.md)
- [Prior correction record](2026-10-22T114555Z-workflow-refactor-recovery-and-review-corrections-43dfe5955a684108a8a65dab7aee5ed3.md)
- [Post-refactor review](../../../../audits/2026-10-22-post-refactor-ownership-and-ergonomics-review.md)
- [Catalogue measurement](../../../../benchmarks/2026-10-22-stable-id-delivery-catalogue.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
