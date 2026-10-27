---
type: "Verification Evidence"
title: "Code-free local finite tool release capture and lifecycle owner design"
description: "Qualifies immutable declared artifact capture before installation, records pinned placement findings and the remaining local Package ownership and activation boundaries."
timestamp: 2026-10-27T20:14:03Z
record_id: "1f12c7706bd44248adec1203402d167e"
producer_id: "codex-root"
run_id: "local-source-tool-releases-2026-10-28"
related_plan: "docs/plans/2026-10-28-local-source-tool-release-plan.md"
git_branch: "refactor/local-source-tool-releases"
verified_by: "01a17eed-5685-7ae2-819a-b969ee8735a1"
supersedes: "c47581dc5dfa45038a38a6aea5b2d4be"
---

# Verification

Continue the complete active foundation Goal after `e376752` on
`refactor/local-source-tool-releases`. Entry had only the original untracked `vendor/`; all other
changes in the containing commit belong to this preparation unit. No push, PR, separate target
or vendor access. The local installation lifecycle plan remains in progress. This commit is its
first dependency, not complete R1b, installation, execution approval or full Goal qualification.

# Result

`PreparedEditorSourceToolRelease::capture` lives under the existing Project Host source-tool
owner. It consumes an explicitly selected raw manifest digest and strict source capability,
admits bounded code-free declarations, verifies every declared artifact/content member and
retains immutable owned bytes. It performs no write, command construction, probe, metadata
installation or document mutation. Exact requirements remain declarations, not a solved or
installed graph. Fixed ceilings are 64 KiB manifest, 64 files, 16 MiB member and 64 MiB payload;
Reflect supplies its existing catalog parsing, Schema and lineage rules. Catalog generation one
is the initial supported form. There is no predecessor input yet, including for code-only updates
over an evolved catalog. Package identity, Schema owner and selected component remain distinct.

The release reads only declared members and rejects case/file/directory aliases before payload
opens. The current Host must match one of the explicit Windows x64 MSVC / Linux x64 GNU targets.
Source contents are digest-checked into owned bytes, so later changes do not alter the candidate.
Unlisted source siblings cannot be carried into an installed artifact's implicit dependency
closure. The external launcher/runtime is explicitly outside this capture; it still needs exact
Host approval and custody before installed execution can claim an immutable complete identity.

The actual finite tool component admission is extracted once and reused by both the raw trusted
definition and release capture. No second Schema validator, package executor, source resolver,
document owner, protocol compatibility alias or migration guide is added. The raw command API
continues to mean trusted local development configuration, not installed release identity.

## Independent Source Findings And Design

Researcher `01a17eed-2aa8-7083-95a3-cd069941291f` pinned Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558` and Bevy
`85503941e488152711a4695bbe248e9ed9f453fc` against the actual health facet/panel consumer.
Godot separates plugin discovery/enablement and Inspector/dock registration, but its per-file ZIP
extraction does not establish owned-file update/removal. Bevy's source plugin build/group disable
and pre-schedule cleanup are not runtime unload. Select the existing finite child placement for
the ordinary tool; retain source-built composition and defer privileged native ABI until a named
authority or measured workload needs it. No new placement latency result follows from source.

Reviewer `01a17eed-5685-7ae2-819a-b969ee8735a1` established that the phased Package document is
requirements-only beyond adopted slices, with no landed Product Package installation owner.
Existing Session owns process/proposal/history and filesystem supplies primitives, not deletion
policy. The new plan orders capture, ownership/publication, approved activation, dependent-safe
removal and complete public/native workflow without a general Package kernel. P1's historical
Rust-only example wording is corrected to the selected concrete C# child; no second tool is
implemented to satisfy a stale example. Existing Rust composition remains complete and separate.

The reviewer found a P2 directory-tree counterexample: full-file casefold dedup still accepted
`Bin/a` with `bin/b`. The manifest now compares every lowercase directory prefix's original
spelling. Reversed order, nested aliases and valid shared directories have regression assertions.
A P3 root-test portability issue is corrected by gating capture tests to the two supported Hosts;
pure manifest tests remain portable. Final static recheck closed both findings. Source-root
identity is explicitly only an observation, not persistent trust or installed-file authority.

Two future publication choices are retained explicitly: fresh-project metadata must pass through
the actual authoring bundle/readmission owner, and successful probe versus durable selected
release publication needs one honest Session/coordinator cut. A catalog copied to disk solves
neither. Installed/enabled/active, last-good, modified/adopted content and physical cleanup must
remain independently observable. A disabled installed required dependent still blocks removal.

# Evidence

Serial Rust `+1.95.0`, `CARGO_BUILD_JOBS=1`, root `target`, Windows 11. No concurrent Cargo.
Writers froze before formatting. Actual .NET artifacts are independently rebuilt by the existing
language runner. Test assertions below are backed by executed results, not just source review.

| Scope | Actual result |
| --- | --- |
| New manifest/capture root tests | 23 passed, 600 excluded; `78e1d856-5cf7-47fe-b8b4-a606461f0b3e`; 2.308 s |
| Complete source-tool owner selection, including new capture and prior exact activation/physical Close | 45 passed, 578 excluded; `d9f712ac-7dd6-4a2b-8d05-26228881eb40`; 46.727 s |
| Actual rebuilt independent language/Session/release consumers | 14 passed, 0 skipped; `0687891b-98de-47a4-a6b8-8d8b369ea412`; 18.144 s |
| Explicit actual C# Application consumer | 1 passed, 278 excluded; `77c2d7ca-ac9a-4e41-be60-66500adc77de`; 0.808 s |
| Root no-default, tooling-only library | check passed |
| Root all-feature, all-target correctness Clippy | passed; existing warnings remain; 49.65 s |
| Independent language workspace all-target correctness Clippy | passed; dependency warnings remain; 4.52 s |

The new external release tests capture the real DLL, deps manifest, runtimeconfig, catalog and a
content sample through the public Host API. They compare every payload/digest/kind and the actual
admitted catalog, mutate only temporary source copies and prove both old-candidate immutability
and new-capture digest refusal. Manifest drift fails the originally selected raw digest. No child
is needed to inspect the release and no installed workflow is implied. Root tests additionally
prove strict capability admission, wrong target/cost rejection before payload opens, invalid
Schema rejection and hard-link refusal even when contents match.
The final candidate Debug formatter summarizes identity and counts without dumping captured
manifest/payload or full catalog data; final checks compile that presentation-only refinement.
Formatting, changed-document relative links and engineering-memory validation/render checks
complete the integration evidence without expanding runtime or native scope.

This unit changes no native UI/platform path and does not rerun or extend predecessor DX12
qualification. Linux execution and unsupported-target capture are not newly qualified. Bounded
logical input/copy costs are not an RSS sandbox, deadline or measured responsiveness claim.

# Follow-up

Continue the full local lifecycle plan. Before implementing removal, establish exact deletion
and ownership facts through the filesystem owner: `nara_fs` currently reports relative unlink
Unsupported on Windows, and `DirectoryPublicationTransaction::abort` only proves cleanup for an
empty staging directory. Do not infer nonempty recursive cleanup authority from directory
publication. The private supervised-Play store's path/marker-scoped `remove_exact_generation_tree`
is not automatically a capability-proven Package removal primitive; do not copy it into the new
untrusted/adopted-content boundary or widen its existing evidence.

Next implement recoverable installation publication/receipt over this captured input, with
explicit preview/confirm/cancel and no self-authorized execution. Then connect the actual Session
probe at a declared durable-selection/routing cut, bind external runtime custody, add required
closure and modified/adopted ownership, and complete actual UI installation/update/removal. The
whole foundation Goal remains active and unchanged. No narrower completion claim is intended.

# Citations

- [Local release/lifecycle plan](../../../../plans/2026-10-28-local-source-tool-release-plan.md)
- [Finite source tools guide](../../../../guides/finite-source-tools.md)
- [Foundation rebaseline plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Phased Package requirements](../../../../plans/2026-07-20-001-feat-package-extension-product-contract-plan.md)
- [Prior inline tool qualification](2026-10-27T193225Z-finite-source-tool-property-facets-and-shared-native-review-qualification-c47581dc5dfa45038a38a6aea5b2d4be.md)
