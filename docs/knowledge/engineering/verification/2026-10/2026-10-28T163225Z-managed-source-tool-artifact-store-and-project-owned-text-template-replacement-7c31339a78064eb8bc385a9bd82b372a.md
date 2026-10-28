---
type: "Verification Evidence"
title: "Managed source-tool artifact store and project-owned text template replacement"
description: "Qualifies fixed managed artifact publication/inventory and explicit project-owned text import while removing mixed-content source protection and retaining future real reclamation."
timestamp: 2026-10-28T16:32:25Z
record_id: "7c31339a78064eb8bc385a9bd82b372a"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
git_branch: "refactor/managed-source-tool-artifacts"
related_plan: "docs/plans/2026-10-28-managed-source-tool-artifacts-plan.md"
---

# Implemented Replacement

Continue from `main@1b29a5c` and the selected ownership design. ADRs 0020/0035, 0047/0118 and
0070 are reconciled before the production ownership replacement. The full foundation Goal
remains active; reviewed reclamation is not implemented by this unit.

Installation preparation receives a strict read-write project root and one ASCII destination,
writes nothing, and retains the actual project association. Explicit publication creates only
the fixed `.nara/source-tools/artifacts/` parents, reserves an absent installation, and preserves
the existing finite receipt-last/no-replace publication and unready custody. Graph admission
accepts only that prefix plus one component. Exact requirements and disabled reverse dependents
retain their existing owner. Store path existence never grants disposal of unknown files.

The separate format-1 `.nara/source-tools/artifacts.json` inventory retains finite installation
facts, member roles, baselines and current observations. Registration retains actual member
objects and refuses an ordinary source owner. Explicit observation/reconciliation does not
recover created-file or deletion authority. Tasks, FilePublicationHost and HostCustody remain
the actual execution/publication owners. Accepted publication followed by executor cleanup
failure retains both public facts through `PublishedWithCleanupFailure` and `TaskCleanupFailed`.

Text import prepares captured UTF-8 `.txt` bytes from an exact installed graph member, original
selection basis, actual project root/source directory and absent destination. It verifies the
complete exact parent again before publication. Actual publication, uncertain/admission-refused
publication and executor cleanup are separate observations. Cancellation cannot erase entered
IO; the private publisher retires after physical return/cleanup so the copy can open through its
ordinary source owner. The independent consumer exercises edit, Undo/Redo, Save, Reopen, fresh
Session and withdrawal preservation. Package origin never regains deletion rights over that copy.

Remove the mixed content record/reader, public management/adoption/protection APIs, entire old
code/test subtree, five source-touch hooks, lane flags and optional-record Save/Reopen/Close
gates. Remove the Workspace content-state getter that had only served that retired interception.
No old reader, migration helper, compatibility alias or migration guide is added. Canonical
formats remain 1. Generic `unlink_file` and the filesystem crate are unchanged.

## Qualification

All Cargo commands ran serially in this checkout with Rust 1.95.0, one build job and the existing
root target. The independent fixture reused the root target. Other observed Cargo work belonged
to a different checkout; none was killed or modified.

| Scope | Actual result |
| --- | --- |
| Root/Tooling/Editor all-feature libraries, final run | 1,149 passed, 38 explicit skips; build 2m28s, execution 332.494s |
| Actual independent language/tool consumers | 30 passed, no skips; text import/history/persistence/withdrawal case 0.599s |
| Actual approved managed-child cancellation | 1 passed, 0.371s |
| Actual C# Application workflow | 1 passed, 1.879s |
| Selected Windows/DX12 native source-tool workflow | 1 passed, 9.289s |
| Root no-default `runtime-2d,serde,tooling` library | Passed, 5.61s; warnings remain |
| Root/Tooling/Editor all-feature/all-target Clippy, `-D clippy::correctness` | Passed, 1m18s; not a zero-warning claim |
| Root and independent fixture `cargo fmt -- --check` | Passed; independent fixture formatting corrected after its successful behavior run |
| Affected Markdown local links | 15 files, 1,028 links passed |
| Engineering memory render/validate | Passed with retained historical warnings |

Final library run: `5c89380f-0192-4ae2-b935-fbd352f3e00a`:

```text
cargo +1.95.0 nextest run -p nara -p nara_tooling -p nara-editor --all-features
  --locked --lib --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
```

`python tests/fixtures/behavior-languages/run.py --native-editor` completed successfully:

- Independent: `0a3e5d73-19b8-4660-a74d-1029391b4e82`, 30 passed in 37.887s.
- Managed cancellation: `7171ef40-b9a0-452d-b497-f23faeed5462`.
- C# Application: `d6bc1f7f-baad-4dcd-9285-9235ebef02d0`.
- Selected native: `a6a14c4a-d4b6-4407-89ca-c0ad623c3830`.

The native selection remains
`native_source_tool_inline_property_and_panel_share_review_history_and_persistence`.
It qualifies the existing Dear tool/facet/selection path against the new store, not new graphical
import/reclamation controls or Nara's future custom-drawn frontend.

## Failures, Review And Corrections

The first focused build found a missing test-only Duration import. The next focused run
`bbcca8ac-f772-446a-9144-6f596f4a9ce1` executed 201: 200 passed, one failed because the new
inventory/Reopen test omitted Save-result acknowledgment. Keep the existing ResultPending
contract and fix the test. Four import/inventory owner tests then passed under
`a289407c-f85e-4feb-8d8f-f8f5259a79fc`.

Initial full libraries `3aa523ed-96f5-4bd0-b915-69f60ac30905` executed 1,147: 1,145 passed,
two failed. Strict new-source validation had lost the retained Prefab directory after namespace
rename; restore that exact owner's strict leaf validation only on root NotFound and exact
configured prefix. The old alias test also created hardlinks before initial source admission;
update it to admit the original first, then create the alias and refresh the index. Preserve
its AlreadyOpen and unchanged unsaved-history assertions. Four focused source guard tests passed
under `01e89571-19fb-4f72-951b-da1ac64af43c`, before the final full-library success above.

The first independent run `af2fe12b-794b-4259-91c9-c69a3b589614` passed 29/30. Its missing
installation test still used an obsolete arbitrary path, receiving InvalidPath before the
intended missing-directory check. Use a valid store path whose directory is absent; retain the
Installation error assertion. The actual new text workflow already passed in that first run.
Assertion-unwind aborts are not interpreted as independently established native buffer overflow.
The unused Workspace getter removal occurred after the final library binary was built; no
consumer used it. The final minimal check and all-target Clippy compile the resulting sources.

Independent correctness reviewer `01a1833c-cae0-72f0-82e1-1ec2b529dcea` requested a source
directory replacement counterexample. Add actual equal-byte receipt/manifest/member replacement
without changing selection basis; confirm it reports Changed and creates no copy. It passed
in the final full libraries. No remaining correctness finding was reported.

Independent security reviewer `01a1832e-d9ae-72a3-8610-c8187fae18a0` identified native short-name
source/import bypasses and TrustedLocal startup hardlinks. Common source admission now proves
exact stored spelling and strict single-link traversal; startup uses strict exact full-path
opens. Import uses exact parent components before preparation/publication and portable path
separators outside `.nara`. Existing retained owners are checked before strict new-source
admission. Native case/spelling, destination-parent replacement and both startup modes have
focused regressions. Actual 8.3 short-name provisioning was not separately run. The reviewer
closed all three findings and the retained Prefab fallback after static recheck.

Workers owned disjoint publication, inventory, consumer and guide paths. A template lane was
closed without delivered source, then the parent implemented that critical path locally.
All agents were closed; none ran Cargo or workspace formatting. Existing `vendor/` remained
untracked/untouched. No destructive filesystem maintenance or remote publication was attempted.

## Remaining Goal Work

Continue the artifact plan's original-basis disposal review, graph absence and actual-user
quiet, pre-deletion intent publication, per-member real unlink, partial results and fresh-Open
recovery. Do not replace this with withdrawal alone, quarantine or recursive unknown-file deletion.
Native Scene/Prefab/asset templates still require format-owned identity/sidecar/reference remaps.
Existing-owner Schema evolution, executable gameplay publications, rendering/resources/delivery
and the custom-drawn Editor remain separate foundation work. No full local lifecycle, foundation
completion, all-stream unchanged-content proof, performance gain or new platform unlink claim
follows from this unit.

## Owning Sources

- [Managed artifact plan](../../../../plans/2026-10-28-managed-source-tool-artifacts-plan.md)
- [Ownership design](../../../../architecture/source-tool-artifact-ownership-design.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Finite-tool guide](../../../../guides/finite-source-tools.md)
- [Native qualification scope](../../../../verification/native-editor-qualification.md)
