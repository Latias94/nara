---
type: "Verification Evidence"
title: "Shared Windows process image evidence and explicit installed tool runtime trust partition"
description: "Records extraction of exact native image checks from Play, actual process-image and generation tests, and the bounded machine-runtime trust decision without claiming installed execution."
timestamp: 2026-10-27T21:37:08Z
record_id: "814f24baa3964ef5ae5708602aaca655"
producer_id: "codex-root"
run_id: "installed-tool-launch-custody-2026-10-28"
related_plan: "docs/plans/2026-10-28-local-source-tool-release-plan.md"
git_branch: "refactor/installed-tool-launch-custody"
verified_by: "01a17f4e-6f59-7560-92a0-9acf7b24588f"
supersedes: "4443d7b18348430884f7015082c9e304"
---

# Verification

Continue the complete active foundation Goal after artifact publication `e8f3a59`. Entry had
only the original untracked `vendor/`. The containing commit extracts one concrete native
primitive and reconciles the external runtime trust partition. It does not complete installed
execution, durable enablement, local Package lifecycle or the full foundation rebaseline.
No push, PR, vendor access or separate Cargo target. All Cargo runs are serial with one job.

# Result

The root project Host now owns the doc-hidden Windows `ExactProcessImage`. It consumes two
capability-issued restrictive leases, retains them without Clone/raw ownership export, checks
the final executable/immediate-directory relationship and compares a borrowed process's mapped
image with the guarded executable's underlying file stream. It exposes a path observation for
native startup without allowing that path to replace the retained authority. Final-path sizing
rejects more than 32,768 UTF-16 buffer units before allocation and performs one retrieval only.

The existing Play consumer now uses this primitive and deletes its duplicate native path and
`NtQueryInformationProcess` implementations. It still owns its generation/manifest custody,
contained suspended spawn, image check before primary-thread resume, rollback, bootstrap and
physical retirement. Native image comparison is not a transitive dependency attestation,
`FileObject` pointer equality, immutable namespace or protection from pre-existing writable
mappings. The primitive has no spawn method, process supervisor, Package resolver or cleanup.

## Design Correction And Independent Review

The [runtime authority study](../../../../research/2026-10-28-installed-tool-runtime-authority.md)
records primary .NET v10.0.0 resolver/servicing source and current official command/loading
documentation. `--fx-version` does not alone fix hostfxr; environment clearing does not remove
machine servicing, and directory sharing does not freeze namespace additions. The first
framework-dependent tool will therefore use an explicitly Host-approved machine runtime
baseline, with configuration/observations separate from immutable package inputs. It will not
promise immutable transitive platform code or add an ACL/security kernel or a runtime resolver.
Package-controlled siblings remain outside that trust baseline.

The proposed artifact handoff is a concrete trusted bootstrap receiving already captured managed
images before the unchanged source-tool request. It will load images from memory, check metadata
and declared references before loading, explicitly stop unknown dependency resolution and reuse
the existing Session/process/proposal owner. The actual private synchronous EnemyStats entry is
compatible with that bounded experiment. An ALC resolver returning null or FileNotFoundException
allows fallback; its simple-name cache also accepts an already loaded higher version, so callback
checks alone cannot enforce exact package identity. Those findings shape the next implementation;
they are not passing evidence for a bootstrap or installed execution in this commit.

Three independent read-only design lanes challenged runtime closure, Accepted contract scope
and the captured-image alternative. The final source reviewer
`01a17f4e-6f59-7560-92a0-9acf7b24588f` checked actual root/Editor diffs, bounded path sizing,
move-only custody, cfg/features, suspended Play rollback and the scoped ADR wording, with no
material findings. It did not count the parent's then-running tests as completed evidence.

# Evidence

Windows 11, Rust `+1.95.0`, `CARGO_BUILD_JOBS=1`, root `target`, no concurrent Cargo. All Rust
writers froze before formatting. The native tests run real Windows file/image/process operations,
not a fixture assertion that the implementation follows a particular branch.

| Scope | Actual result |
| --- | --- |
| Root native current-image match and identical-byte different-stream rejection | 2 passed, 641 excluded; `a52625bd-953f-48ea-9b01-7fab9c4399a0`; 0.305 s |
| Editor generation/store, native Windows process helpers and supervised lifecycle selection | 31 passed, 248 excluded; `694201b9-8905-4769-8774-b54553d7808a`; 5.473 s |
| Root no-default `runtime-core` library | check passed; 20.53 s |
| Root and Editor all-feature/all-target correctness Clippy | passed; existing warnings remain; 1 m 28 s |

The Editor selection includes the actual suspended exact-generation child, restrictive directory
rename refusal, source/digest revalidation, process output, bootstrap endpoint, Job descendant
reaping and physical cleanup observations. Rust formatting, changed-document relative links,
diff whitespace and engineering-memory validation/render checks complete integration. Historical
memory warnings remain historical. This evidence does not requalify DX12 presentation, the three critical
graphics trajectories, Linux process authority or managed installed execution.

# Follow-up

Implement and qualify the actual captured-image managed bootstrap and Host-approved launcher
policy. Reuse the Session's existing candidate probe, review, history and physical return;
do not create a second installed executor. Every pending request must retain original captured
bytes through cancellation and late spawn. Prove actual installed V1/V2 failure preservation,
same-byte source drift independence and forbidden default dependency fallback.

Then implement owner-local durable selection with actual filesystem publication facts before
routing, including truthful disk-V2/route-V1 reconciliation and physical retirement debt. The
artifact ready receipt and Scene persistence lanes cannot serve as that store. Metadata adoption,
evolved catalog lineage, required dependents, content adoption/removal and full UI lifecycle
remain in progress. The entire foundation Goal remains active.

# Citations

- [Local lifecycle plan](../../../../plans/2026-10-28-local-source-tool-release-plan.md)
- [Runtime authority study](../../../../research/2026-10-28-installed-tool-runtime-authority.md)
- [ADR 0116](../../../../architecture/adr/0116-standalone-supervised-editor-play.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Predecessor artifact publication](2026-10-27T210523Z-local-tool-artifact-publication-with-a-final-ready-receipt-and-continuous-file-custody-4443d7b18348430884f7015082c9e304.md)
