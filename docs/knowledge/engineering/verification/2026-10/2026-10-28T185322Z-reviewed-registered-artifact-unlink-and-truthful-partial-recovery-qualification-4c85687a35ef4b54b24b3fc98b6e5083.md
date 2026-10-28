---
type: "Verification Evidence"
title: "Reviewed registered artifact unlink and truthful partial recovery qualification"
description: "Qualifies actual ready-artifact intent, per-member unlink, cancellation, result publication debt and exact Session/Application retention; incomplete publications remain open."
timestamp: 2026-10-28T18:53:22Z
record_id: "4c85687a35ef4b54b24b3fc98b6e5083"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
git_branch: "feat/reviewed-artifact-reclamation"
git_commit: "2089f76"
related_plan: "docs/plans/2026-10-28-managed-source-tool-artifacts-plan.md"
---

# Verification

Continue from local `main@65e6a24`. The current unit implements registered ready-artifact
reclamation under the amended ADRs 0070/0118. All tests use Rust 1.95.0, Windows 11, serial Cargo,
`CARGO_BUILD_JOBS=1` and the existing root target. The independent consumer shares that target.
This evidence qualifies the ready-artifact unit, not incomplete installation recovery or the full
foundation Goal. No remote publication is authorized or performed.

# Result

The public Session/Application prepares a move-only exact review without writes, confirms its
original artifact and selection observations, and publishes bounded intent through the existing
file publisher. Actual IO return and successful cleanup precede first unlink. Revalidate original
selection binding/digest, graph absence, source owners and held objects after the asynchronous
wait. Graph absence includes ID, canonical directory and actual directory object; another ready
identity at the same location still blocks disposal.

Use actual `DirectoryCapability::unlink_file` per selected member. Removed requires its real
receipt; Missing is an absence observation. Retain each result before another member. First
failure or cancellation leaves later members Pending. Unknown siblings, project-owned imported
copies and unrelated installations survive. Fresh Open only reads Intent/Results and never
restores approval, historical native identity or deletion authority.

Results publication failure retains the actual mixed record and debt. Explicit reconcile plus
retry only republishes results. Close revokes admission immediately, drains owned removal and one
results attempt before shutting its publisher, and can finish physically with the exact unpublished
record/error in its failure receipt. Application retains all failed Session receipts keyed by
actual entry generation, preserving its first-error getter for native exit reporting.

Keep the existing immutable-launch lease unchanged. A separate opaque `DirectoryNamespaceLease`
reopens the exact directory with read/write sharing and no delete sharing on qualified Windows;
other platforms refuse it. Child publication/unlink remain possible. Completed installation hands
its creation directory over to a read/traverse open while both live objects are compared; failure
retains the original and PublishedUnverified. The generic unlink primitive is unchanged.

Tasks exposes read-only sticky cleanup failure without driving or closing execution. All admitted
root/activity work publishes failure evidence before CleanupFailed becomes visible. Activity
failure stays local while the physical facility aggregates its real tasks and worker cleanup.

# Evidence

| Scope | Actual result |
| --- | --- |
| Root/Tooling/Editor/Filesystem/Tasks all-feature libraries, final | 1,316 passed, 38 explicit skips; build 1.42s, execution 339.822s |
| Final related persistence/artifact/reclamation cohort | 63 passed, 1,291 outside-selection skips; 17.521s |
| Tasks focused full library after ordering correction | 97 passed, no skips; 7.984s |
| Filesystem focused full library | 47 passed, no skips; 1.169s; final combined run includes later receipt assertions |
| Actual independent language/tool consumer | 30 passed, no skips; 33.285s; template/history/reclamation case 0.760s |
| Actual managed-child cancellation | 1 passed; 0.355s |
| Actual C# Application workflow | 1 passed; 2.214s |
| Selected Windows/DX12 native source-tool workflow | 1 passed; 9.250s |
| Root/Tooling/Editor/Filesystem/Tasks all-feature/all-target Clippy, `-D clippy::correctness` | Passed; 1m34s; warnings remain |
| Root no-default `runtime-2d,serde,tooling` library | Passed; 12.68s; warnings remain |
| Workspace and independent fixture `cargo fmt -- --check` | Passed |
| Affected Markdown local links | 11 files, 1,190 links passed |
| Engineering-memory render/validate | Passed with retained historical warnings |

Final libraries: `b9c26244-357c-45a6-b1b8-9d8a5480eaa6`:

```text
cargo +1.95.0 nextest run -p nara -p nara_tooling -p nara-editor -p nara_fs -p nara_tasks
  --all-features --locked --lib --test-threads=1 --no-fail-fast
  --status-level fail --final-status-level fail
```

Final related cohort: `777dfb5f-c1dc-4046-9e4c-1917c0da750c`.
Tasks: `9ba6037d-5bad-4670-b86d-3773e42d5ba6`.
Filesystem: `192fe564-7553-4372-97ae-d3fd48db8334`.

Actual filesystem tests prove directory DELETE-access exclusion, child publication/rename/unlink,
and release of the namespace restriction. Root tests retain the installation Arc while reviewing,
then exercise full real deletion, equal-byte object replacement, unavailable graph, post-intent
selection drift, locked later member, cancellation after one unlink, surviving renamed aliases,
failed intent, failed results/reconcile/retry and physical Close with exact mixed results.
The two-Session Application regression obtains actual trusted Runtime close failures and retains
both receipts through manager navigation/replacement, preserving the first failure.

`python tests/fixtures/behavior-languages/run.py --native-editor` completed successfully:

- Independent: `9b7084c2-f641-4349-b7f2-dcaf65ffe0b7`.
- Managed cancellation: `8b76b223-f36d-48e6-9657-c98d7278b1ba`.
- C# Application: `a148ddcf-8c33-4e52-a7c0-dba15d135691`.
- Selected native: `a7008b20-ea74-43d8-8364-422d939dfaaf`.

The independent consumer installs two versions plus an exact dependent, imports/edits/Undo/
Save/Reopens a project copy, withdraws/cascades, actually reclaims the old artifact and observes
Results after fresh Open. Unknown siblings, the copy and the remaining installation survive.
The window-free Application consumer separately exercises explicit review and actual deletion.
The selected native case remains
`native_source_tool_inline_property_and_panel_share_review_history_and_persistence`: hidden
Windows/DX12 tool/facet/selection presentation. It does not qualify new graphical reclamation
controls, native-format template remaps, other platforms or Nara's future custom-drawn UI.

The final minimal command is `cargo +1.95.0 check -p nara --no-default-features
--features runtime-2d,serde,tooling --lib --locked`. Workspace and independent-fixture format checks
pass. All source/code changes are committed locally as `227b215` (Tasks), `e44c8d4` (namespace
lease/ADR 0070) and `2089f76` (ready-artifact workflow). The linked results test that exact source
state; later documentation closure does not require another product acceptance cycle.

## Review And Corrections

Independent reviewer `01a18398-124e-7893-bb15-5bdcd7f45078` identified unavailable graph absence,
new persistence/conversion IO during removal, endless terminal-debt Close, lost surviving alias
custody and later Application receipt loss. Owning guards, physical/debt separation, exact Close
evidence and all-session retention address them. Reviewer `01a1839d-643e-7dc2-b641-72199d33c6f7`
identified the root task cleanup publication ordering race; a deterministic gate verifies failure
before running-accounting release. No remaining source-collector defect was reported.

Independent final reviewer `01a183bc-bd18-72f3-ba60-495c5b823eca` found no confirmed issue in the
namespace lease, exact directory handover, original leaves and Close drain. Its final recheck also
accepted same-directory/different-ready-ID refusal. Reviews were read-only; parent owns actual
verification. All agents were closed after handoff/review.

The first real tests exposed retained directory-creation DELETE access and reuse of an overstrong
launch lease, blocking healthy review or the caller's own publication. Fix ownership/sharing;
do not bypass busy checks or drop actual custody. Earlier test-only private record access and a
format-2 fixture missing the required reclamations array were corrected. The initial complete
library run passed 1,313/1,315. A worker had changed an unrelated rejected-Open Exit test into a
trusted shutdown-failure fixture, then asserted receipt retention before Application acceptance;
restore that original fixture and keep the actual two-failing-Session test separately. The new
fresh-Open review test must wait for actual source work. It retains original bases and retries only
Busy; other refusals remain failures. The final related and full runs above pass. Assertion-unwind
abort codes are not treated as independent evidence of a native buffer overflow.

# Follow-up

Incomplete publication remains outside the ready inventory. The linked progress record maps a
concrete artifact-publication owner replacement: actual Ready/Incomplete facts, actual subset and
receipt-temporary paths, retained cleanup-only objects, exact reader/directory handover and the
same bounded review/publisher/reclaimer. No fake ready receipt, serialized disposal authority,
recursive removal or second executor is permitted. `TemporaryFileCleanup::discard` cannot produce
an unlink receipt. Native Scene/Prefab/asset template identity/sidecar/reference remaps, Schema
evolution, executable gameplay, rendering/resources/delivery and Nara-owned custom UI remain
separate full-Goal work. No full lifecycle, speed, cross-platform unlink or crash-durability claim
follows from this unit. Unknown siblings and empty directories are intentionally preserved.

# Citations

- [Artifact plan](../../../../plans/2026-10-28-managed-source-tool-artifacts-plan.md)
- [Ownership design](../../../../architecture/source-tool-artifact-ownership-design.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Integration and incomplete successor](../../progress/2026-10/2026-10-28T182559Z-reviewed-real-artifact-unlink-integration-and-incomplete-publication-follow-up-1de5c56631a146318b4b1098eb0fdfd8.md)
- [Finite-tool guide](../../../../guides/finite-source-tools.md)
- [ADR 0070](../../../../architecture/adr/0070-capability-oriented-filesystem-substrate.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Native qualification](../../../../verification/native-editor-qualification.md)
- [Microsoft sharing contract](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)
