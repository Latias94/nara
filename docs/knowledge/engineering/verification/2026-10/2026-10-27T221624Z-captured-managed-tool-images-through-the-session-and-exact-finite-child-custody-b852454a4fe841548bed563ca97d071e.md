---
type: "Verification Evidence"
title: "Captured managed tool images through the Session and exact finite child custody"
description: "Records approved machine runtime and immutable image execution, strict managed graph admission, native cancellation custody, actual installed V1 V2 consumers and qualification limits."
timestamp: 2026-10-27T22:16:24Z
record_id: "b852454a4fe841548bed563ca97d071e"
producer_id: "codex-root"
run_id: "captured-managed-tool-execution-2026-10-28"
related_plan: "docs/plans/2026-10-28-local-source-tool-release-plan.md"
git_branch: "feat/captured-managed-tool-execution"
verified_by: "01a17f65-2222-7f83-9029-adb19de5731d, 01a17f67-5a55-7a61-81fb-94ad4a88cc46"
supersedes: "814f24baa3964ef5ae5708602aaca655"
---

# Verification

Continue the full active foundation Goal from `3011d6e`. The branch carried three uncommitted
bootstrap files from its authorized predecessor worker; they are part of this unit, not user
changes. The original untracked `vendor/` remains untouched. All other changes in the containing
commit belong to the captured-managed execution workflow. No remote push/PR, separate Cargo
target or concurrent Cargo. The local release lifecycle plan remains in progress.

# Result

`EditorSourceToolManagedHost::new` performs explicit, code-free approval of a selected Windows
x64 MSVC launcher/bootstrap distribution, four expected member digests, exact .NET 10 framework
version and finite timeout. Restrictive native leases retain launcher/bootstrap identity and
ordinary write/delete exclusion; before each child acquisition the IO worker rechecks bootstrap
names, live identities and digests. Runtime framework/servicing/OS dependency selection remains
an explicitly trusted machine baseline, not a transitive immutable snapshot or sandbox.

`approve_installation` consumes a strict receipt-selected inspection, validates the admitted
runtimeconfig/deps subset and creates one bounded owned image envelope. The definition retains
installation/manifest association and those exact bytes. Each invocation uses a fresh child and
streams the envelope before the unchanged proposal request through the same stdin worker, then
closes that writer for EOF. Captured images are at most 64 members, 16 MiB each and 64 MiB in
aggregate; prefix framing has a separate bound from the ordinary 64 KiB request. Pending Tasks
retain the same approved input and native custody, including after logical cancellation.

The concrete Host-owned .NET bootstrap performs exact unbuffered envelope reads, inspects all
supported managed PE metadata and complete reference identities before package loading, and
uses a private ALC with explicit captured/package and actual-framework bindings. Unknown
dependencies throw FileLoadException, never a null/FileNotFound fallback. Exact package identity
checks occur before the ALC simple-name cache can unify a higher version. Supported entry is a
private or public synchronous static `int Main(string[])` with no arguments. Native/mixed/R2R
images, external AssemblyFiles/resources, async/STA entry semantics, unresolved references and
unsupported runtime/probing metadata reject. Package content does not become a filesystem for
implicit execution dependencies. Tool code remains explicitly trusted ambient process code.

The existing finite process owner now acquires either the development std child or the approved
Windows exact child. Native acquisition uses an explicit three-pipe inherited-handle list, a
constructed environment and suspended startup. Every post-create mapping/resume failure occurs
inside the same Custody; it retains stop/reap/pipe join through physical return and unwind.
Cancellation observed after held acquisition terminates the suspended child without resuming it.
The shared Windows argv encoder replaces the Editor's private copy. No second executor, proposal
interpreter, history, process supervisor or compatibility reader is introduced.

Actual Session activation still requires the existing complete catalog/facet probe. Failed
replacement preserves V1 activation and review; successful replacement revokes old review and
uses the same source preparation/Undo/Save. The external consumer removes the entire installation
directory after approval and still executes the original images. The actual native fixture now
publishes/inspects/approves an installation and removes its DLL before native probing, replacing
its development command setup while retaining the control/panel/history/persistence trajectory.
Fresh Open does not execute a tool merely because an artifact or receipt exists.

## Findings And Corrections

The first bootstrap prototype built cleanly but failed actual EnemyStats execution. Temporary
local diagnostics identified a repeated identical CoreLib TPA record. The final admission merges
only the same normalized path and complete identity; another path or identity still rejects.
Temporary exception dumping was removed before final build/tests. Bootstrap failure output is
the fixed bounded stderr line with empty protocol stdout.

The actual no-default `runtime-2d,serde,tooling` build exposed a missing dependency declaration:
earlier artifact receipt/staging code used getrandom, accidentally supplied by project-workflow
in broader feature unions. Tooling now declares that dependency itself. The corrected no-UI
combination passes; no prior tooling-only build is misrepresented as coverage of this combination.

The independent managed/config review found no material defect and asked whether the shared
serde preflight rejects duplicate map keys. The parent verified its explicit DuplicateMapKey
branch and added a real artifact with duplicate config keys; Host approval rejects it. The
independent native/Host review found no material defect in approval, guarded input lifetime,
suspended startup, cancellation, cleanup, cfg boundaries or exports. Neither reviewer ran Cargo
or counted then-running tests as completed evidence. Native fixture changes preserve its existing
callback/assertions and were checked separately before the actual DX12 run.

# Evidence

Windows 11 x64, OS build 26300, Rust `+1.95.0`, .NET SDK 10.0.400, explicitly selected
Microsoft.NETCore.App 10.0.11, DX12 for the native Editor trajectory. Root target reused,
`CARGO_BUILD_JOBS=1`, all Rust and .NET builds serial. Writers froze before formatting/builds.

| Scope | Actual result |
| --- | --- |
| Bootstrap project | rebuilt with zero warnings/errors; package code not needed to compile the Host |
| Initial root all-feature library | check passed; 34.64 s |
| Initial actual installed V1/invalid/V2 + Undo/Save/Open | 1 passed, 15 excluded; `f3cf53fb-24b0-48a5-bd84-930abf6f53b2`; 1.434 s |
| Root source-tool selection plus shared image tests, including the ignored actual C# held-acquisition/IO case | 66 passed, 578 excluded; `6bce1056-e8be-4cec-9ee7-9864672d80d6`; 49.165 s |
| Complete rebuilt language/Session/artifact/bootstrap consumers before the additional config test | 21 passed, 0 skipped; `8fb2be41-6cab-4259-9ef5-6e8bff01ecf8`; 21.679 s |
| Actual managed cancellation custody through the fixture runner | 1 passed, 643 excluded; `c1fd6823-5df6-468b-9676-b863f1f4daa0`; 0.290 s |
| Actual C# Application consumer with the existing development configuration | 1 passed, 278 excluded; `42df75ea-9a86-4a5f-9fc0-7587d0586107`; 0.857 s |
| Final installed Session and probing/native/duplicate-config admission tests | 2 passed, 20 excluded; `dd956afe-e59d-44c2-ac65-319b55b5e647`; 1.539 s |
| Corrected no-default runtime-2d/serde/tooling library without UI features | check passed; 4.08 s |
| Final Editor generation/store, Windows argv/native helpers and supervised process selection | 31 passed, 248 excluded; `51e16524-6454-4472-96f0-e63622b91263`; 5.866 s |
| Actual installed-image native Inspector/panel/history/Save/physical Close/fresh Open | 1 passed, 278 excluded; `b548d33c-17f9-41e8-abfe-c77993a483f9`; 7.190 s |
| Root and Editor all-feature/all-target correctness Clippy | passed; existing warnings remain; 1 m 21 s |
| Independent language workspace all-target correctness Clippy | passed; dependency warnings remain; 4.44 s |

The five direct bootstrap tests execute real compiled private Main/dependency images, including
missing and same-name higher-version dependencies, duplicates, async/STA entry points, malformed
and truncated envelopes, and actual EnemyStats request correlation. These are loading failures
before stdout, not Rust source-text checks of the intended resolver. Config admission separately
rejects package probing, native assets and duplicate JSON keys in real captured installations.

The held Tasks test distinguishes logical completion from physical return at before-spawn and
before-IO-return gates and observes the approved Host custody still alive until return. It does
not imply descendant containment or a hard cleanup deadline. Native UI uses the existing hidden
windows/owned input and asserts both DX12 presentation scopes. No OS pointer/keyboard injection,
framebuffer readback, scanout, complete critical native group or other-platform claim follows.
The installed DLL deletion is solely the qualified fixture's owned test mutation, not an installer
reclamation operation. Formatting, relative links, diff checks and memory render/validation finish
integration without expanding those product claims.

# Follow-up

Continue the complete lifecycle plan. Separate a probe-ready candidate from route publication
before adding owner-local durable installed selection/desired enablement. Actual disk publication
must precede route promotion with truthful post-publication reconciliation; a persisted selection
does not recover Host execution approval. Required installed dependents, adoption, evolved catalog
lineage and metadata readmission require their actual owners. Do not use Scene persistence or the
artifact ready receipt as an installed selection store.

Update/disable/remove still need dependency closure and exact content reclamation after physical
return, with modified/adopted/ambiguous content preserved. The first managed adapter supports
already admitted project Schema and no unresolved requirements; it is not an official C# SDK,
gameplay executable publication or complete local Package UI. The user-selected Editor destination
remains Nara's own custom-drawn runtime. This phase neither completes R1b nor the foundation Goal.

# Citations

- [Local lifecycle plan](../../../../plans/2026-10-28-local-source-tool-release-plan.md)
- [Finite tool guide](../../../../guides/finite-source-tools.md)
- [Runtime authority study](../../../../research/2026-10-28-installed-tool-runtime-authority.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Shared image predecessor](2026-10-27T213708Z-shared-windows-process-image-evidence-and-explicit-installed-tool-runtime-trust-partition-814f24baa3964ef5ae5708602aaca655.md)
