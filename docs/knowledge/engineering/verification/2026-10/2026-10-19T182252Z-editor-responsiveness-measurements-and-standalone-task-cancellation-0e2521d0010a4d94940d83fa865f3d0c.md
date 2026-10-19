---
type: "Verification Evidence"
title: "Editor responsiveness measurements and standalone task cancellation"
description: "Decision-local optimized Open Save Reopen timings, source-grounded rebuild seams, and failed-then-passing standalone pending cancellation."
timestamp: 2026-10-19T18:22:52Z
record_id: "0e2521d0010a4d94940d83fa865f3d0c"
producer_id: "codex-root"
run_id: "editor-responsiveness-20261020"
related_plan: "docs/architecture/authoring-interface-ergonomics-design.md"
git_branch: "refactor/foundation-contract-hardening"
git_commit: "b93944e"
---

# Verification

This follows the bounded G1/G2/G9 investigation in the
[authoring interface design](../../../../architecture/authoring-interface-ergonomics-design.md),
starting at `f93552c`. Three read-only research lanes examined public Open/Save, task cancellation,
and supervised build/retirement. Root added a private ignored public-consumer measurement and
owned all serialized Cargo verification. The task lane then implemented the concrete abandonment
correction in `3b70e48`; a different lane reviewed it independently. The measurement consumer and
its three retained raw populations are committed as `b93944e`. Later closure is documentation-only.

This does not reopen completed U1-U15, S1-S3 or N05, start a Goal, push, open a PR, or implement
general async preparation. The [benchmark report](../../../../benchmarks/2026-10-20-editor-responsiveness.md)
owns the measurement protocol, numerical results and concrete follow-up sequence.

# Result

Windows 11, Rust 1.95.0, shared target, `CARGO_BUILD_JOBS=1`; no concurrent Cargo invocation.
No dependency or lockfile changes. Existing feature-dependent unused/dead-code warnings remain.

| Command / selection | Actual result | Nextest run |
| --- | --- | --- |
| `cargo +1.95.0 nextest run -p nara_tasks --lib --locked -E 'test(pool_drop_cancels_all_pending_handles_before_blocked_capture_retirement)' --test-threads=1 --no-fail-fast`, regression before correction | Expected failure: a blocked first pending capture delayed the second handle's cancellation; guard released and test exited | `5a1e4f0a-52ce-47a3-8b15-f2075035dece` |
| `cargo +1.95.0 nextest run -p nara_tasks -p nara_image --lib --locked --test-threads=1 --no-fail-fast` | Final 120 passed, including logical cancellation before blocked disposal and unchanged handle Drop execution | `31e30fd1-b4b8-485e-91ff-8727ea0cca4a` |
| Root persistence: `nextest run --no-default-features --features project-workflow,tooling,runtime-ui,presentation-2d --test editor_persistence --locked -E 'not test(persistence_authority_surface_is_opaque_and_linear)' --test-threads=1 --no-fail-fast` | Final 16 passed; unchanged trybuild authority test and ignored measurement excluded | `0ad1b4cc-c591-4535-8282-7686b7fd0cca` |
| Optimized measurement command in the benchmark report, process 1 | 1 passed, 17 excluded; all 480 numeric rows retained | `afe87975-9498-4a46-a028-3eb96e5b3ddd` |
| Same optimized command, process 2 | 1 passed, 17 excluded; all 480 numeric rows retained | `9e4c8732-fa41-4778-a624-ca800ff7c5f0` |
| Same optimized command, process 3 | 1 passed, 17 excluded; all 480 numeric rows retained | `8f24df9e-0f70-40db-8d2d-b4342cb8c410` |
| `cargo +1.95.0 check --workspace --all-features --all-targets --locked` | Passed, 1m 08s | Not a nextest run |
| `cargo +1.95.0 doc -p nara_tasks --no-deps --locked`, with `RUSTDOCFLAGS='-D rustdoc::broken_intra_doc_links'` | Passed; public task-contract links resolve | Not a nextest run |
| `cargo +1.95.0 fmt --all -- --check`, `git diff --check`, affected local-document target checks | Passed; no product changes after verification | Not a nextest run |

An initial debug measurement passed (`fdee8b7d-e714-4b13-8e76-a651d26a74c7`) and an initial
optimized exploratory measurement passed. Both predate the strengthened byte/revision/canary and
physical-close assertions, so neither is part of the final retained optimized population.
The mistaken nextest `--profile release` invocation was rejected before any build; the actual
optimized commands use `--cargo-profile release`.

The optimized rows follow the strengthened measurement oracle. All three processes finish each
session's physical close before restoring source bytes. They show approximately 15 ms median
Save drive and 10 ms median Reopen/startup-closure work for 4,096 shallow empty entities, versus
less than 0.3 ms Save admission and approximately 0.3 ms idle drive. These complete-call timings
do not separate storage, validation and encoding or prove native redraw behavior. The report
selects 8 ms as a local next-investigation trigger after observing this baseline, not a pre-existing
acceptance SLA or a timing assertion. With sixteen samples, each row's nearest-rank P95 equals max.
Independent review recomputed every per-process summary from its gzip source and checked the
rounded report table: all 1,440 rows and 90 groups agree, with no missing or duplicate sample keys.
The final measurement/design review reported no findings. Both external HTML reports pass
nesting, unique-ID, fragment and local-target checks. Memory validation and derived views are
checked at closure; historical warnings are preserved rather than rewriting old records.

# Evidence

The task regression holds the first pending capture's destructor behind a release channel while
retaining both terminal handles and cancellation tokens. Both pending jobs must be cancelled with
PoolShutdown/before_start before release; no closure executes. The second characterization drops
only a handle, explicitly drives the inline test executor, and proves admitted work still runs.
That preserves Image's generation-based stale-publication policy.

The correction closes queue admission, retains the running-control snapshots while signalling
them, cancels all detached pending entries, transfers physical disposal to the existing retained
owner, then releases snapshots. Physical disposal remains serial per owner and retains the same
bounded internal lanes. First-terminal and shutdown counters remain unchanged.

Independent review identified a completed-result race in the initial documentation and running
snapshot loop. A snapshot may become the last owner of a completed result and execute its user
destructor when released. Borrowing those snapshots through cancellation and transfer fixes the
ordering; rustdoc/ADR now explicitly avoid promising arbitrary-destructor bounded latency. Final
review reported no remaining findings within the correction scope.

A separate suspected unbounded output drain was withdrawn after checking Windows and Linux
platform implementations: both already limit a poll to 64 records. Reader/queue physical
completion, explicit output loss and maintenance deadlines remain distinct. No redundant output
budget or production refactor was added.

Measurement review required three corrections before retaining final samples: label original
fixture bytes as baseline bytes; check actual Saved/Opened document/revision/digest and saved vs
unsaved entity canaries; explicitly drive every session to physical-close Complete outside the
timed span. The final static review of those corrections found no remaining measurement issue.

# Follow-up

Open phase boundaries do not preempt synchronous work inside a phase. Save/Reopen admission and
drive costs must be measured separately; a Save of an older captured revision can correctly leave
newer edits dirty, while Reopen must reject a stale base. Filesystem publication may become
uncertain after replacement and must retain its physical owner through cancellation or close.

Browser Open, actual component/Prefab closures, saved-source receipts, no-op Cargo phases,
generation directory cleanup, slow storage and native redraw interruption remain unmeasured by
this focused population. Completed-result and rejected/coalesced capture destruction can still
run on the caller thread, including older managed-close control snapshots. Those are G9 concerns,
not promises of finite arbitrary user code. D6 and the broader G1-G9 triggers remain separate.

The local Bevy/Godot comparison informs owned preparation and result publication; it does not
justify moving World/session owners or converting Plugin/component/panel contracts into Futures.
No native platform or release qualification is extended by these tests or measurements.

# Citations

- [Task cancellation contract](../../../../architecture/adr/0052-task-backpressure-cancellation-and-long-running-diagnostics.md)
- [Decision-local measurements](../../../../architecture/adr/0099-decision-local-product-evidence-and-publication-admission.md)
- [Task implementation](../../../../../crates/nara_tasks/src/runtime.rs)
- [Task regression and handle characterization](../../../../../crates/nara_tasks/src/tests/close.rs)
- [Measurement consumer](../../../../../tests/support/editor_responsiveness_measurement.rs)
