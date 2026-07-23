---
type: "Decision"
title: "RGD-U7 Runtime and Host independent decision matrix"
description: "Records the refreshed independent ADR 0084 Runtime verdict, ADR 0082 Host verdict, compatible-pair scope, and remaining delivery gates."
timestamp: 2026-07-23T07:40:18Z
record_id: "e2e5ea1ed4cf4e28860cedb32f0e7e48"
tags: ["rgd-u7", "runtime", "host", "compatibility", "adr"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "5ebc45e287c94dac99f194aa921adaf5086cc8a2"
verified_by: "Independent Runtime review and independent Host/compatibility review"
---

# Decision

RGD-U7 reviewed ADR 0084 and ADR 0082 in dependency order against the refreshed U2-U6 evidence.
The Runtime review completed first. The Host and compatible-pair review ran only after that review
accepted ADR 0084.

| Scope | Verdict | Product consequence |
|---|---|---|
| ADR 0084 executable Runtime | **Accepted** | One thin lifecycle owner around one `App` is the accepted runtime boundary for the reviewed scope. |
| ADR 0082 outer Host | **Accepted** | Concrete Headless, Desktop, Editor, and code-first embedding paths use the accepted authority and lifetime graph. |
| Combined topology | **Compatible Accepted Pair** | The Host owns project/process authority and publication; the Runtime owns one generation's execution, fault, and close state. |
| U11 admission | **Runtime/Host gate passed** | This decision removes only the Runtime/Host authority blocker. U8-U10 and their required hosted evidence remain separate prerequisites. |

Neither decision creates a universal `EngineHost`, service hub, `RuntimeFactory`, or Runner trait.
The accepted scope is limited to already-compiled, Host-trusted code. Project data still does not
authorize Cargo resolution, build scripts, proc macros, native packages/importers, or in-process
Play.

# Trust Scope

The scope covers the concrete Rust/code-first, Headless, Winit-backed Desktop, and Editor paths
that the reviewed Host explicitly constructs. It does not accept a general package activation
topology, a replacement Render Host, a universal Platform/Runner Adapter, arbitrary external
native-code activation, or a shared process-service model.

A concrete product Host publishes only through its owned `RuntimePublicationSlot`. A direct
code-first owner may call `ReadyRuntimeCandidate::promote()` and receive a faulted runtime if a
fault wins the final race; that preserves observation and close ownership but is not a
Host-published runnable session. A `CloseIncomplete` child retains its concrete Host parent
authority. Direct embedding has no implied `ProjectHost` parent and must continue its own bounded
close driving.

# Evidence Revisions

| Evidence | Exact revision | Role in this decision |
|---|---|---|
| RGF-U23 historical matrix | `f7e5ee283e06ff156224b0f11fcc1df0c31284a3` | Records the earlier Proposed/Proposed verdict and the gaps that this review must not shrink. |
| RGF-U24 | `5ddbf186712b6c829dc0134a9f41a0ac8250fa3e` | Concrete Host start/publication and reversal evidence. |
| RGF-U26 | `a2d695d6c58b8f8f21bb33aaf18fa27e18661a79` | Independently frozen manual raw-`App` counterfactual. |
| RGD-U2 | `5c06cdeb015612bfbe0feb93f21d8fe3e7603116` | One frozen executable component behavior authority. |
| RGD-U3 | `6c6813848ea6335ef0a3eb40c16a9e6bbfa9ce39` | Per-runtime Bevy fault routing and truthful route retirement. |
| RGD-U4 | `549d5c25a4091585c8cca3dc51e1f7748fd2cd9d` | Fresh mutable service/backend/identity reconstruction. |
| RGD-U5 | `0c2dadd849e08d5b3f22dcedac963bbfccf08595` | Public Headless/Desktop/Editor semantic command parity. |
| RGD-U6 | `12696d45b167cb4b8f0cb9ed61060f8b9cda7dae` | Renamed-root external managed-runtime Runner fixture. |
| U7 review baseline | `5ebc45e287c94dac99f194aa921adaf5086cc8a2` | Exact reviewed source revision after U2-U6 evidence and before this documentation-only decision. |

# ADR 0084 Runtime Metrics

| Metric | Result | Evidence |
|---|---|---|
| Startup publication | Pass | `RuntimePublicationSlot` compares the current candidate fault under the reporter lock before exposing one Host-owned runtime. |
| Ownership handoff | Pass | The sealed `App` and `RuntimeObligationLedger` move once through candidate, publication failure, or published retirement paths. |
| App admission | Pass | Unsealed, already-started, raw-runner, and undeclared-obligation paths reject before candidate admission. |
| Play execution | Pass | Headless, Desktop, and Editor drive scheduled `App` execution through `RuntimeInstance`, not a bare `World` owner. |
| Driver parity | Pass | RGD-U5 runs one bounded semantic command stream through public Headless, Winit Desktop, and Editor Hosts. |
| Driver authority | Pass | RGD-U6's independent renamed-root package uses public reservation/control/drive/close APIs and rejects raw `App`, World, hidden-port, and Runner-trait shortcuts. |
| Exact step | Pass | Paused exact stepping completes one fixed/gameplay transaction, preserves time debt, and returns to `Paused`. |
| Fault closure | Pass | System, condition, command, observer, gameplay lifecycle, and explicitly required task/service integration failures become sticky runtime faults. |
| Runtime isolation | Pass | World, queue, time, task, service session, identity, WGPU instance, and cache namespace reconstruct independently. |
| Finite close | Pass | Deadline-bound participants, incomplete close, process quarantine, and truthful terminal state are covered by focused tests. |
| Stop-first workspace | Pass | A running, faulted, or incomplete owner remains retained until retirement truthfully permits replacement. |
| API authority | Pass | Managed public surfaces expose neither raw mutable World access nor a second scheduler/time model; raw runner and managed admission are mutually exclusive. |
| Early ownership value | Pass | The U26/U24 counterfactual preserves the named fault and ownership cuts without introducing a general framework. |

Required task/service failure means a domain integration has explicitly classified its terminal
failure as required and reported it through the runtime fault reporter. `nara_tasks` remains a
domain-neutral task substrate; it does not automatically make every task failure a required
runtime service failure.

# ADR 0082 Host Metrics

| Metric | Result | Evidence |
|---|---|---|
| Pre-mutation project rejection | Pass | Project lineage and schema validation reject before candidate construction or service-session creation. |
| Recipe coherence | Pass | Content and composition share one lineage and exact frozen behavior snapshot, which Host safe points revalidate. |
| Fresh plugin preparation | Pass | Each start materializes new candidate-local plugin owners from repeatable definitions. |
| Early topology value | Pass | The U26/U24 comparison remains a bounded product path rather than a public Host abstraction. |
| Runtime delegation | Pass | The Host drives `RuntimeStartAttempt -> complete_startup -> publish_into`; no branch publishes a raw `App`. |
| Parent lifetime | Pass | `CloseIncomplete` moves into retained cleanup and blocks replacement until the owned child retires. |
| Cross-host parity | Pass | RGD-U5 verifies the same declared semantic command stream across all three concrete Hosts. |
| Least privilege | Pass | Server composition remains separate from window, render, audio-device, editor, and raw-input sessions. |
| Embedded path | Pass | RGD-U6 proves code-first managed runtime use without a project file or public universal Host object. |
| Admitted external authority parity | Pass | The external Runner fixture proves the only currently admitted external runtime-driving role; replacement Render Host and universal Host/Runner roles remain outside this decision. |

# Combined Runtime/Host Scenarios

| Required scenario | Result | Required product behavior |
|---|---|---|
| Sequential RuntimeInstances | Pass | A truthful terminal owner may reconstruct a non-reused generation with fresh mutable runtime state. |
| Overlapping RuntimeInstances | Pass | Independent runtimes can drive concurrently; one `ProjectHost` still rejects a second active start before transfer. |
| Process-global authority contention | Pass | Per-runtime fault routes replace the old process-global reporter/schedule exclusion without corrupting a healthy peer. |
| Fresh-runtime reconstruction | Pass | World, queue, time, task, service, backend/cache namespace, and runtime identity reconstruct; named immutable inputs may remain shared. |
| Plan/World registry divergence | Pass | One frozen behavior snapshot binds composition, candidate materialization, and managed runtime safe points. |

The parity result compares the declared semantic command stream and its canonical output. It does
not claim that all Hosts make identical internal frame calls or share platform event-loop behavior.
The external Runner result proves code-first embedding; it is not a second `ProjectHost` publication
path.

# Independent Reviews

The independent Runtime review found no P0/P1 and accepted ADR 0084. It recorded one non-blocking
clarification: direct `promote()` preserves a faulted owner for code-first observation and close,
while a concrete Host must use `publish_into` to avoid exposing a runnable session after a final
fault race.

The independent Host/compatibility review found no P0/P1 and accepted ADR 0082 plus the combined
pair. It confirmed that retained cleanup owns the parent/child boundary, that the U5 fixture is a
product-level semantic oracle, and that U6 does not introduce a universal Runner/Host SPI.

# Consequences

- ADR 0082 and ADR 0084 become Accepted authorities for their bounded scopes.
- U8-U12 retain their own delivery, hosted, package, provenance, and publication gates; this review
  does not complete or authorize any of them.
- A future replacement Platform/Runner or Render Host must prove selection, sole authority, close
  ordering, and clean-room conformance through its owning ADR before it becomes public product
  authority.
- A future package/build activation path remains governed by OQ-031 or an Accepted successor.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u7-re-review-runtime-then-host-authority`
- `docs/architecture/adr/0082-process-host-authority-and-runtime-construction-topology.md`
- `docs/architecture/adr/0084-executable-runtime-ownership-and-isolation.md`
- `docs/knowledge/engineering/decisions/2026-07/2026-07-21T112729Z-rgf-u23-runtime-and-host-independent-decision-matrix-a5b3266847924dfc93667c72c8929550.md`
