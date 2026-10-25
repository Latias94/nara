---
type: "Verification Evidence"
title: "Exact persistence attempts and terminal observation qualification"
description: "Records owner-issued attempt correlation, exact observation acknowledgment, unchanged physical custody, public workflow evidence and scoped review corrections; complete R1 remains open."
timestamp: 2026-10-25T08:14:43Z
record_id: "332f36f3c0f94f93ae1eb2721e981359"
producer_id: "codex-root"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "8b6ae91378b7d1f0f1c8b690f9b4c3b41c145d72"
---

# Verification

This records only foundation R1/F2's exact persistence-attempt owner replacement on `8b6ae91`
plus the final 58 Rust files. It does not complete the semantic tool, structured compiled-tool
diagnostics, finite child-process experiment, R1/R2 or the foundation Goal. ADRs 0047/0118 adopt
the contract; the [design](../../../../architecture/editor-persistence-attempt-design.md)
distinguishes it from file receipts, content identity and physical retirement.

# Result

- One concrete Idle/Pending/Terminal state replaces independently writable pending work,
  operation and result slots. The admitted attempt contains session provenance and a checked
  monotonic sequence; pending work, published terminal and dispatch use the same identity.
  Frame generation is not an acknowledgment-freshness requirement. This ID is neither persistent
  source identity, a format version nor a security capability.
- `Accepted { attempt }`, published `Rejected { attempt, reason }`, nonpublishing `Refused(reason)`
  and exact `Acknowledged { attempt }` are distinct outcomes. Busy is not intrinsically a refusal:
  a capture or admitted worker can publish it. Nonpublishing refusal and checked-ID exhaustion
  preserve existing state. Acknowledgment allocates nothing and consumes only the exact observed
  terminal, not the latest result selected later by a callback.
- Permitted retries explicitly supersede a terminal with a new attempt under the existing policy.
  Read-only optional-target normalization precedes that policy, but NoActiveDocument publication
  cannot overwrite a retained success. Identical failure payloads remain distinct attempts.
  Old/foreign/duplicate acknowledgments cannot clear successors; an older frame can acknowledge
  its still-current exact terminal after unrelated redraws.
- Workspace Saving carries its admitted ID. Matching Close success retains the terminal;
  matching Exit success consumes only that attempt before advancing its document cursor.
  Failure/uncertainty remains available for another decision. Private Saving extraction is typed,
  not a phase test followed by an impossible optional-ID branch.
- The existing physical-return-aware Save/Reopen polls, work mutex, captured document/Host,
  irreversible Save outcome, Completed-only Reopen publication and independent cleanup/task
  owners remain. Consuming a terminal observation does not reconcile uncertain bytes, release
  a borrowed file Host or certify complete physical shutdown. No result bus, observer registry,
  compatibility alias, migration guide or persistent-format conversion is added.

# Evidence

## Discriminating Failure And New Proof

The public Session action path accepts persistence acknowledgments independently of frame
freshness. A retained first frame's payload-free acknowledgment cleared the second identical
NotDirty result. The corrected-fixture baseline is actual behavior red
`1c4d7f2a-6c28-4a56-aa4a-57d02672a5f4`: one failed assertion, original terminal versus Idle.
The first fixture needed missing imports; that compile failure is not behavioral evidence.
The first reached assertion also triggered Runtime-drop abort during panic cleanup. The fixture
then captures the result, explicitly closes the Session and asserts afterward, producing the
ordinary failed run above. No OS abort is presented as a separate engine vulnerability.

The initial seven owner tests pass in `312d5c2e-2dc7-493b-80c8-80dbd33ffe8a`. They cover delayed
old-frame confirmation, valid old-frame confirmation, identical retries, foreign sessions with
equal local sequences, duplicate confirmation, held Save/Reopen, targetless requests, exhaustion
and Close-retains/Exit-consumes. Original file/dirty/source/cleanup tests retain their assertions.

The independently compiled Application consumer exercises two observers in non-active Scene
and Prefab sources. It compares each immediate dispatch ID with the actual terminal, preserves
owned earlier results through subsequent submissions, refuses the old confirmation over its
successor, and retains exact navigation, revisions, clean state and unchanged source bytes.
Existing default-first-write workflows now bind dirty Accepted through exact Saved and bind
clean NotDirty to its own attempt; acknowledgment uses that captured ID, not a fresh sample.
The earlier actual dirty Save/Reopen and complete JSON preservation proof is not weakened.

## Reviews And Corrections

The bounded frozen-core review first reports no actionable defect and traces the temporary
`mem::take` Idle state: no reachable completion helper reads that projection, reenters admission
or publishes a frame before the owner restores Pending/Terminal. It does not invent a generic
Transitioning state without a consumer. Two mechanical private-phase compiler errors are corrected
separately; that limited pass is not a final compilation or complete review claim.

Actual complete CE review `20261025-072428-56ec12b3` runs in the parent orchestration context,
not an isolated child asked to launch unavailable nested agents. Eight structured personas cover
correctness, project standards, tests, maintainability, API contract, reliability, security and
adversarial behavior; the ninth checks native/window-free SDK parity. All nine returns, merge
and report leaves are collected and closed. The report has `status: complete`, zero actionable
findings and two advisory proof gaps. The merge selects zero findings, so validator execution
is an explicitly recorded empty batch, not an invented validation pass.

The observed active-agent cap is six; queued reviewers launch only after collected handles are
closed. All inherit the configured session model. User instructions select Codex-native review;
no external peer job or different-model independence is claimed. Root AGENTS.md is the standards
fallback. Read-only reviewers run no Cargo or mutation tests; parent execution remains separate.

Four production-only simplification rubrics run against the substantive owner files: reuse,
quality, efficiency and comments. Tests are outside that mutation scope and protected from
pruning. Reuse/quality identify the same small cleanup: bind `save_attempt()` at the Saving guard,
compare the exact ID directly and consume it directly for Exit. This one deduplicated change
preserves restoration, document lookup and publication order. Efficiency finds no worthwhile
change; the four new public-contract comments remain. No line-count objective is used.

The two retained testing gaps are addressed without new production handling:

1. The existing queued Save case still retires the original Host and admits a replacement for
   both Scene and Prefab. It now retains the actual Accepted ID and requires the actual Busy
   terminal to carry it, while preserving exact failure payload, no-write bytes and dirty state.
   This distinguishes published worker Busy from the separate pending-admission Refused(Busy).
2. A real two-document Exit saves a dirty Scene and Prefab. The first ordinary drive consumes its
   exact attempt before the second prompt. The second attempt differs; holding the private owner
   publication/progression cut lets a delayed first acknowledgment prove it cannot consume the
   second terminal. Exit then consumes only that second attempt. Both files contain the captured
   edits. The test explicitly does not claim public frames expose that private intermediate cut.

The correction cohort `833d79c8-6913-4bf0-a49d-047e86d38273` passes nine tests, including the
strengthened existing Busy case. Only three of the 58 reviewed Rust files change after the full
review: one production cleanup and two proof files; the other 55 raw hashes remain unchanged.
An independent targeted correction reviewer confirms both gaps closed, the private cut's truthful
scope and behavior-equivalent cleanup, with no actionable issue. It independently checks 58/58
final raw hashes and 55/55 unchanged hashes; it runs no tests and does not repeat the full CE round.
All review/simplification/correction handles are collected and closed.

## Serial Qualification

All Cargo runs use explicit Rust 1.95.0, locked manifests, jobs=1 and the shared root `target`,
without concurrent Cargo in this checkout. Initial broad evidence precedes the three-file
correction, rather than being relabeled as final-source execution:

| Initial scope | Actual result |
| --- | --- |
| Root/Editor/Tooling all-feature libraries | 666 passed; 28 skipped |
| Independent prepared-panel consumer | 7 workflows passed; one timing diagnostic skipped |
| Independent component/reference/property consumers | 11 passed |
| Reference-game Editor journey and UI refresh | 2 passed |
| Selected actual Windows/DX12 | 4 passed; 219 excluded |
| Workspace and independent-consumer all-feature/all-target checks | Both exit 0 |
| Scoped correctness/suspicious Clippy and Tooling no-default check | Both exit 0 |
| Root, independent-consumer and reference-game format checks | All exit 0 |

The first reference-game command incorrectly selected its package from the root workspace and
failed before test compilation. It is corrected to `--manifest-path reference-game/Cargo.toml
--target-dir target`; the corrected two-test run passes. This command-scope error is not counted
as a product failure, successful skipped qualification or new behavior red.

The entire affected serial qualification is repeated after the production cleanup and proof
corrections. All 12 final `postfix-` command groups exit 0 between 16:10 and 16:30 on 2026-10-25
(Asia/Shanghai). The five test groups report:

| Final-source scope | Result | Nextest run ID |
| --- | --- | --- |
| Root/Editor/Tooling all-feature libraries | 667 passed; 28 skipped | `d5ce69ff-e4a7-4281-99ec-8a7dbf7e89d9` |
| Independent prepared-panel consumer | 7 passed; one timing diagnostic skipped | `15ae6c6c-90ca-40d0-a165-4fd070512ea6` |
| Independent component/reference/property consumers | 11 passed | `2213d12d-bbe0-46a9-9bde-a4a0121aa987` |
| Reference-game Editor journey and UI refresh | 2 passed | `8f6d3b2a-269c-4aed-b023-ebaec57c3a46` |
| Selected actual Windows/DX12 | 4 passed; 219 excluded | `ee808962-a4ba-4642-9155-076b890c3809` |

The other seven groups are workspace all-feature/all-target check, independent-consumer
all-feature/all-target check, Tooling/root/Editor Clippy with correctness and suspicious groups
denied, Tooling no-default-features check, and root/consumer/reference-game format checks.
Existing unrelated warnings remain; no warning-free Clippy claim is made.

The selected native cases are `native_extension_document_widgets_save_reopen_and_retire_panels`,
`native_ordinary_structure_widgets_arrange_delete_undo_save_and_reopen`,
`native_canvas_widgets_pick_move_resize_and_save_reopen_real_source`, and
`native_retained_scene_receipt_survives_prefab_navigation_and_bound_close`. They qualify those
actual Windows/DX12 workflows, not the full native group, Linux/macOS, IME, accessibility,
monitor scanout or performance.

The [normalized source manifest](../../../../verification/data/2026-10-25-persistence-attempts-source.json)
contains the 58 final Rust paths. For each file, replace CRLF with LF before SHA-256; sort rows
by path, encode them as UTF-8 compact JSON with sorted object keys and separators `(',', ':')`,
then hash that encoding. The manifest digest is
`6204afeef59e117c77e504723124fbf87f3750aa769e0f44722e1d244b64cc8e`.
All 58 raw hashes also match the final local verification snapshot. No Rust source changed
between final qualification and this documentation closure. Local raw command arrays, terminal
exit codes and timestamps remain in the run's `verification-results.json`; the five log run IDs
above are read from their final terminal summaries, not inferred from earlier runs.

Documentation closure checks pass: engineering-memory render/check and validation, all 994 local
Markdown links across the ten affected documents/derived views, and `git diff --check` under the
checkout's normal line-ending policy. Existing legacy-ID, local-path and large-ledger memory
warnings remain historical; no unrelated history is rewritten. These document checks are not
another product acceptance cycle.

# Follow-up

Resume R1's useful concrete semantic tool and structured compiled-tool diagnostic transport,
followed by the explicitly enabled finite child-process experiment. Source-built panels and
exact persistence attempts do not qualify install/update/disable/remove, inline Inspector,
Behavior/module replacement, render composition or the remaining delivery waves.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Exact persistence design](../../../../architecture/editor-persistence-attempt-design.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Document guide](../../../../guides/editor-documents.md)
- [Normalized source manifest](../../../../verification/data/2026-10-25-persistence-attempts-source.json)
