---
type: "Verification Evidence"
title: "Durable installed source-tool selection and independent paired attempt qualification"
description: "Qualifies code-free desired selection, approved installation enablement and physical lifetime through Session, Application and native Editor consumers."
timestamp: 2026-10-28T01:22:06Z
record_id: "3a4a90d3cc3042e5b89e8fa20e314c22"
producer_id: "codex-root"
run_id: "durable-source-tool-selection-2026-10-28"
related_plan: "docs/plans/2026-10-28-local-source-tool-release-plan.md"
git_branch: "feat/durable-source-tool-selection"
verified_by: "01a17feb-7cd3-76c1-ad4f-21527f5d075c,01a17ffc-d602-7c31-8027-bed178b2da75"
supersedes: "9621f4a6d44249569e458a0e4006fa4a"
---

# Verification

Continue the complete active foundation rebaseline from `b77c445`. The containing source commit
binds this selected single-slot installed-selection unit; it does not complete the local Package
lifecycle or foundation Goal. Windows 11 x64, Rust `+1.95.0`, .NET SDK 10.0.400 and framework
10.0.11; actual native Editor execution uses DX12 and hidden windows with owned Context input.
All Cargo invocations are serial with one build job and the existing root target. No push or PR;
the original untracked `vendor/` is preserved.

# Result

The Session owns bounded `.nara/source-tools/selection.json` format-1 desired state independently
of installed ready receipts, native object identities and executable approval. Open/reconciliation
only observe data. Missing means unselected; corrupt/unsupported/oversized/inaccessible data blocks
blind overwrite. A strict Host-issued project capability binds preparation to the Session's actual
live root; `DirectoryCapability::same_object` preserves independent sessions, rights and trust.
Managed approval retains actual inspected directory provenance and captured-image custody.

The existing probe produces a private ready candidate. Background IO verifies the selected
installation, publishes desired selection through the shared file Host, and records actual file
facts before Session-only route promotion. Promotion rechecks the original basis, predecessor,
attempt, admitted Schema and open Session. Published-but-refused activation remains truthful;
there is no automatic disk rollback. Post-publication admission checks the current named parent,
retained leaf and digest. Disable revokes execution/review immediately and persists Disabled
separately; failed writes do not restart code. Close retains physical IO/child/cleanup custody.

Observe/Enable/Reconcile keep one named `operation` attempt, and Disable keeps its own `disable`
attempt. Each carries optional terminal outcome and survives until explicitly replaced by a new
operation in its own lane. Co-terminal IO/Disable outcomes remain independently observable.
Duplicate pending Disable rejects. Unified probe cancellation retains `ProbeCancelled` by original
attempt, while genuine failed probing is `ProbeFailed`, not an invented missing-approval error.
The old single-result type is removed without an alias.

Application, immutable Tooling actions and the incumbent native panel consume these same owners.
Actual C# consumers cover V1 enablement, rejected update, V2 switch, stale review refusal, durable
Disable, Save/physical Close/fresh Open and explicit re-enable. Fresh Open observes enabled desired
state without activation. The native panel additionally persists Disable for both an active origin
and an unapproved saved selection. The actual Application can disable during a dirty-source Close
decision; cancelling that Close does not recover the revoked review.
The independent installed consumer also cancels a real durable probe and verifies both named
terminal outcomes. After successful preparation, exact receipt-byte drift and declared DLL drift
each refuse durable publication as `Installation` while the captured probe still succeeds; V1's
route and selection bytes remain unchanged. Restoring the fixture bytes permits the later V2 update.

## Independent Review Corrections

The first adversarial review found lost co-terminal outcomes, cancellation losing its durable
attempt, and omitted parent identity after publication. The owning corrections and deterministic
tests cover all three. The second review found nested public probe admission could consume another
returned probe after validating the caller's basis. Enable now observes returned work before original
basis validation and predecessor capture; shared private probe admission never drives the owner.
A real returned-but-unconsumed child regression rejects the stale request before installed IO.
The focused second recheck found no remaining concrete issue in these assigned paths. Reviews
are static evidence, distinct from the executed tests below.

Integration also corrected a fixture arithmetic error at the 512-byte path boundary; production
accepts <=512, with independent exact 512/513 assertions. A Windows directory-custody test wrongly
assumed a sharing violation mapped to `PermissionDenied`. It now checks actual
`ERROR_SHARING_VIOLATION`; exclusive open fails while the inspected handle lives and succeeds after
its drop. No handle was discarded to satisfy the old assumption.

# Evidence

| Scope | Actual result |
| --- | --- |
| Final root all-feature library and public `editor_persistence` | 702 passed, 6 skipped; `e1fca809-c95d-4ace-82fe-6aaaf3396543`; 252.412 s |
| Focused selection/store/public conformance and deterministic lifetime/namespace cases | 28 passed, 660 excluded; `b2cfca40-b4d9-41f0-93d9-d27b8f24faa4`; 18.501 s; included in final broader source cohort |
| Final actual installation/inspection tests after Windows error-code correction | 14 passed; `e424e393-d1bc-4239-96a0-3cea6132a2bd`; 1.733 s; included in final broader cohort |
| Filesystem all-feature suite | 87 passed, 3 skipped; `48b50fa7-7cc0-49a5-bde5-7ef15db993e9`; 2.486 s |
| Tooling all-feature suite | 86 passed, 1 skipped; `c2d930dd-d4b2-467e-95e0-b38fd2fb4b58`; 2.129 s |
| Editor all-feature ordinary library suite | 245 passed, 34 skipped; `a262eb23-030e-4742-bc2e-a7c2ddda1a28`; 42.546 s |
| Final rebuilt language/installed/bootstrap/Session runner including receipt/DLL drift | 24 passed, 0 skipped; `cfaaeda3-7988-4123-ba6d-d4b3c9f7f9c0`; 24.230 s |
| Final actual managed-child cancellation/custody | 1 passed; `c6429ecf-6b3c-4a6c-96e0-b08e3b1a86a8`; 0.320 s |
| Final actual installed C# Application including Close decision | 1 passed; `03331fd7-1589-4e98-a369-becea7654d81`; 1.593 s |
| Final actual durable installed native property/panel/history/Save/physical Close/fresh Open | 1 passed; `6af0dd06-e84a-4796-af29-e1282e258c40`; 8.483 s |
| Three critical actual Windows/DX12 input/view/lifetime trajectories | 3 passed; `0f5bbed7-d24b-4587-8fed-9e5b98b31ff8`; 9.192 s |
| Feature-minimal root `runtime-2d,serde,tooling` library | check passed; 6.54 s, existing feature-conditional warnings |
| Root/Editor/Filesystem/Tooling all-feature/all-target correctness Clippy | passed with `-D clippy::correctness`; 1 m 16 s, existing non-correctness warnings |

The runner command was `python tests/fixtures/behavior-languages/run.py --native-editor`; it supplies
the complete module/bootstrap/probe environment and root target. An earlier manual full invocation
with incomplete environment passed 18/24 and failed six setup prerequisites; the formal run above
supersedes that invocation. The first broad root run passed 701 and failed only the Windows error
kind assertion; the final 702-case run above includes its exact-code correction. Skips and filtered
tests are never counted as executed. Cohorts overlap and must not be summed as unique coverage.

Deterministic tests use real code-free installation and filesystem publication around internal
post-probe Ready boundaries. They qualify IO ordering, paired outcomes, exact namespace admission
and abnormal-Drop cleanup, not synthetic managed approval. The independent actual C# tests cover
successful approval/execution separately. Held cleanup survives incomplete Close and runtime
quarantine until release; original selection bytes stay unchanged and the actual Host retires
only after temporary cleanup completes.

The final single-run Debug consumer observed V1 enable 208.5913 ms, rejected update 182.0335 ms,
proposal-to-review 173.804 ms, V2 enable 186.6126 ms and selection Disable 6.4302 ms. These include
polling/scheduling and process IO where relevant. They are no distribution, tail-latency, optimized
benchmark or comparison against a different Host. Each operation uses the existing finite probe
or proposal invocation; no second executor or proposal interpreter was added.

# Follow-up

The complete foundation Goal remains active. This unit assumes the project's generation-one
metadata is already admitted and rejects unresolved installed requirements. Fresh-project metadata
adoption, evolved owner lineage/migration, installed dependent closure, modified/adopted content
preservation, exact removal/recovery and the full Package UI remain unfinished. Source-free delivery,
executable gameplay publications and Nara's own custom-drawn Editor runtime are separate waves.
Current native evidence uses the incumbent Dear adapter and does not transfer its capabilities
to a future frontend or qualify Linux/macOS/native sandboxing. Read handles do not freeze bytes;
shared replacement remains detect-only and Windows directory-sync limits remain.

The accidentally generated nested fixture target remains after automatic policy rejected its
deletion; all subsequent fixture commands explicitly use the root target. No rejection bypass
was attempted. Document/memory integration checks bind this evidence to the containing commit.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Local lifecycle plan](../../../../plans/2026-10-28-local-source-tool-release-plan.md)
- [ADR 0035](../../../../architecture/adr/0035-project-manifest-and-runtime-settings-authority.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0070](../../../../architecture/adr/0070-capability-oriented-filesystem-substrate.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Finite source-tool guide](../../../../guides/finite-source-tools.md)
- [Integration checkpoint](../../progress/2026-10/2026-10-28T010718Z-durable-installed-tool-selection-and-paired-attempt-integration-in-progress-9621f4a6d44249569e458a0e4006fa4a.md)
