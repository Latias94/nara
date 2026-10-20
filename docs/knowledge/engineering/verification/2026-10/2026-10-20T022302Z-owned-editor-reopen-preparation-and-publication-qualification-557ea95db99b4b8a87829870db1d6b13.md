---
type: "Verification Evidence"
title: "Owned Editor Reopen preparation and publication qualification"
description: "Bounded read preparation, owner native codec affinity, exact stale admission, task terminal policy and responsiveness evidence."
timestamp: 2026-10-20T02:23:02Z
record_id: "557ea95db99b4b8a87829870db1d6b13"
producer_id: "codex-root"
run_id: "editor-owned-reopen-20261020"
related_plan: "docs/plans/2026-10-20-owned-editor-reopen-plan.md"
git_branch: "refactor/foundation-contract-hardening"
git_commit: "8f5003b9c8f6ce044e5fbac473845675f3d9e6f9"
---

# Verification

Baseline `4356001`, Windows 11, Rust 1.95.0, shared target, serialized Cargo with
`CARGO_BUILD_JOBS=1`. Root alone builds, formats, stages and commits. Lower-owner, native/consumer
and adversarial lanes have disjoint write ownership or remain read-only. Unrelated `vendor/`
and `myeasylog.log` are preserved. No new Goal, push, PR or release qualification is performed.
Final product/measurement source is `8f5003b`; tests and measurements execute that pre-commit
tree, with later changes limited to formatting import order, comments and documentation closure.

| Check | Actual result | Nextest run |
| --- | --- | --- |
| Minimal-feature root lib/public persistence/runtime cohort, excluding unchanged trybuild authority case | Initial 114 passed | `b1c79c5b-bbe1-49e6-98ff-1f244099106c` |
| Same cohort including all five private Reopen worker tests after first-terminal correction | 119 passed, 81 excluded | `9e044c5d-3b87-4ca9-a9e8-3874c4d14fed` |
| Editor lib + application_contract / authoring_loop / first_game_workflow / generic_component_composition / generic_open_recovery | 189 passed, 20 ignored native tests excluded | `66774be7-8bf6-4867-904a-68c601833920` |
| Final same Editor cohort after cancellation policy correction | 189 passed, 20 ignored native tests excluded | `86e641ec-bff4-4cdc-ac09-b5a695b7dba6` |
| Windows/DX12 native ordinary Scene/Prefab Save/Reopen, Canvas Save/Reopen and paused-resume redraw | 3 executed and passed, 180 excluded, actual adapter and hidden native windows | `cb1c2fa4-4437-4328-a18c-3adc3d65b421` |
| Optimized v3 measurement process 1 / 2 / 3 | Each passed; 672 numeric rows per process | `4e792315-9926-4c17-8b61-2f0371481bca` / `dfa596a3-7e4f-458e-b2ae-049f28b1ce48` / `0b57d544-4a26-4ef2-b11f-4af5b3ca6deb` |
| Independent module-consumer manifest, `component_inventory`, shared target | 3 passed | `90c2999c-5036-41fa-b3ad-02c0c5f4cb78` |
| Reference-game manifest, `--features editor --test editor_journey --test editor_ui_refresh` | 2 passed | `716eb7f4-fb1d-4aa8-8b49-e57790aca46b` |
| `cargo +1.95.0 check --workspace --all-features --all-targets --locked` | Passed, 50.01s | Not a nextest run |
| `cargo +1.95.0 fmt --all -- --check`, `git diff --check`, affected local Markdown links | Passed after import-order formatting; 628 local targets checked | Not a nextest run |

Root minimal-feature checks use `--no-default-features --features
project-workflow,tooling,runtime-ui,presentation-2d --locked`, root/public and Editor ordinary
checks use `--test-threads=2 --no-fail-fast`. Selected ignored native tests run serially under
`WGPU_BACKEND=dx12` following the stable qualification procedure. The measurement command and
full metric definitions are in the linked report. Cohorts overlap; their counts are not a unique
combined test total.

# Result

Reopen now captures a read-only source and exact publication facts at admission. A bounded worker
read/parse/pure-authoring path prepares data without the original Host, Workspace or native
registry callbacks. Owner admission rechecks document kind, source incarnation/revision, binding,
lane path, schema basis and the existing trusted Scene runtime exclusion before invoking native
code or replacing a document. Only successful Workspace publication reconciles the Host baseline.

The native maintenance, Close/Exit and source-replacement behavior introduced for Save already
uses non-Idle persistence and covers Opening. Three application tests prove queued-but-not-driven
Reopen -> CloseProject / Exit / same-project replacement and completion observation without
repainting unchanged polls. Existing Opening UI policy remains unchanged.

# Evidence

Real-thread lower tests cover Scene and Prefab requests. Native v1 -> v2 migration and codec
callbacks record zero worker invocations, then only owner-thread invocations during `admit()`.
Pure authoring preparation preserves unavailable manifests and leaves uncertain original Hosts
unchanged until explicit owner commit. Direct old synchronous Reopen methods and obsolete
duplicate decode paths are removed. Browser Prefab opening still performs its existing
synchronous source-open workflow using the same narrow preparation primitive; it is not claimed
as asynchronous Browser Open.

Deterministic after-preparation tests prove that later edits survive stale rejection with history,
source and saved checkpoint intact. A Host-baseline oracle changes disk to candidate A before
preparation, restores the original bytes before rejection and then successfully Saves later edits;
an incorrectly advanced baseline would reject that Save. Another test replaces A with B after
preparation: owner publication uses A's digest/identity and leaves B untouched; the next Save
rejects TargetChanged. This is DetectOnly, not CAS or a consistent snapshot claim.

Held pre-read tests reject new Play / Restart / RunLastGood through trusted, frame-bound and
external routes and keep close Incomplete until release. Already-open tab activation remains
permitted, with publication bound to the captured document. Existing Prefab-running and already
running external-child Reopen semantics remain unchanged. Content-runtime replacement continues
to defer for any pending persistence operation.

Independent review caught a copied Save cancellation policy: the initial Reopen poll could admit
prepared bytes after Cancelled/Failed won the logical terminal. The final poll waits for physical
mutex return and admits only `TaskTerminal::Completed(())`. Tests cover queue rejection, queued
cancellation, held post-preparation cancellation, a real post-preparation unwind with poisoned
mutex and retained data, and normal Completed admission. Cancelled/failed data is discarded without
changing document or Host baseline. No public Cancel interface is introduced.

Successful read abandonment retains one runtime quarantine owner until its held closure returns;
source bytes and directory entries are unchanged. This proof does not include pre-existing failed
Save cleanup on the original Host. Source review confirms that separate gap below.

Independent numerical review recomputes every median, nearest-rank P95 and max from all 2,016
v3 rows, validates 126 unique groups and agrees with every summary/table value. Each process has
42 groups of sixteen samples with exact metadata and no retained warm-ups. Independent HTML
review preserves v1/v2 history and checks nesting, unique IDs, local links and all twelve v3
table cells against the report. Source review confirms the first-terminal correction and the
separate cleanup gap without claiming it is repaired here.

# Follow-up

**Post-terminal Host cleanup custody remains a pre-existing gap.** Once Save returns its Host,
operation-scoped Save custody no longer retains that authority. Dropping a session can abandon
failed `TemporaryFileCleanup`; its failure owner explicitly does not retry during Drop. Read-task
quarantine owns no original Host and cannot fix it. Normal explicit close/retry remains covered.
Prioritize one private full-lane-lifetime retirement owner, including source replacement and an
already-Stopped runtime, before more startup/source latency work. Do not fabricate idle SaveWork
or expand the generic task executor to carry a separate file-ownership obligation.

Native codecs, owner projection/history disposal, Prefab refresh and bound external Play receipts
retain separate synchronous costs. Startup closure and Browser Open remain separately scoped.
No journal, strong concurrent-writer CAS, generic async operation service, Future executor or
async plugin/codec/panel trait is introduced. Native evidence remains the three executed Windows/
DX12 development paths, not Linux/macOS, accessibility, OS IME, scanout or full-release evidence.

Memory validation and derived-view checks run at closure. Historical warnings remain unchanged;
no legacy record is rewritten to silence them. Standalone HTML stays outside docs.

# Citations

- [Plan](../../../../plans/2026-10-20-owned-editor-reopen-plan.md)
- [v3 measurements](../../../../benchmarks/2026-10-20-owned-editor-reopen.md)
- [Native qualification](../../../../verification/native-editor-qualification.md)
