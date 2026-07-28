---
type: "Decision"
title: "RGD-U7 refreshed Runtime and Host independent decision matrix"
description: "Records the post-registry-authority refresh ADR 0084 Runtime verdict, dependent ADR 0082 Host verdict, compatible-pair scope, and next delivery gate."
timestamp: 2026-07-28T21:48:15Z
record_id: "cb08ecb6f5054f938f8a6d7de30941e4"
tags: ["rgd-u7", "runtime", "host", "compatibility", "adr", "refresh"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "27cbd1298c8cefd470efc25a4d99396305b5b4ea"
verified_by: "Independent read-only Runtime review, dependent Host and compatibility review, and serial verification"
supersedes: "e2e5ea1ed4cf4e28860cedb32f0e7e48"
---

# Decision

RGD-U7 re-reviewed ADR 0084 and ADR 0082 in dependency order after the executable component
registry authority was made private. The Runtime review ran first. Only after every Runtime metric
passed did the dependent Host and combined-topology review proceed.

| Scope | Verdict | Product consequence |
|---|---|---|
| ADR 0084 executable Runtime | **Accepted** | One thin lifecycle owner around one `App` remains the accepted runtime boundary for the reviewed scope. |
| ADR 0082 outer Host | **Accepted** | Concrete Headless, Desktop, Editor, and code-first embedding paths retain the accepted authority and lifetime graph. |
| Combined topology | **Compatible Accepted Pair** | The Host owns project/process authority and publication; the Runtime owns one generation's execution, fault, and close state. |
| U11 admission | **Still blocked** | The Runtime/Host decision gate is refreshed, but current-revision hosted CI, baseline, candidate, and approval evidence must be renewed in U8-U11 order. |

This refresh does not create a universal `EngineHost`, service hub, `RuntimeFactory`, Runner trait,
registry provider interface, or package activation system. It remains limited to already-compiled,
Host-trusted Rust code and the concrete product paths named below.

# Trust Scope

The reviewed scope covers direct managed embedding plus the concrete Headless, Winit-backed
Desktop, and Editor paths that Nara constructs today. Project data still cannot authorize Cargo
resolution, build scripts, proc macros, native packages/importers, or in-process package code.

A product Host publishes only through its owned `RuntimePublicationSlot`. Direct code-first use may
call `ReadyRuntimeCandidate::promote()` and receive a faulted but observable/closeable owner; that
does not create a Host-published runnable session. `CloseIncomplete` retains parent authority and
blocks replacement. Direct embedding owns no implicit `ProjectHost` and must drive its own bounded
close.

# Evidence Revisions

| Evidence | Exact revision | Role in this decision |
|---|---|---|
| RGF-U23 historical matrix | `f7e5ee283e06ff156224b0f11fcc1df0c31284a3` | Records the earlier Proposed/Proposed verdict and the metrics this refresh cannot weaken. |
| RGF-U24 | `5ddbf186712b6c829dc0134a9f41a0ac8250fa3e` | Concrete Host start/publication and reversal evidence. |
| RGF-U26 | `a2d695d6c58b8f8f21bb33aaf18fa27e18661a79` | Independently frozen manual raw-`App` counterfactual. |
| RGD-U2 original | `5c06cdeb015612bfbe0feb93f21d8fe3e7603116` | Original frozen executable component behavior authority. |
| RGD-U3 | `6c6813848ea6335ef0a3eb40c16a9e6bbfa9ce39` | Per-runtime Bevy fault routing and truthful route retirement. |
| RGD-U4 | `549d5c25a4091585c8cca3dc51e1f7748fd2cd9d` | Fresh mutable service/backend/identity reconstruction. |
| RGD-U5 | `0c2dadd849e08d5b3f22dcedac963bbfccf08595` | Public Headless/Desktop/Editor semantic command parity. |
| RGD-U6 | `12696d45b167cb4b8f0cb9ed61060f8b9cda7dae` | Renamed-root external managed-runtime Runner fixture. |
| Cleanup anchor correction | `78300531c23317665da5b042e5ba4963107c3122` | Makes frame-end `Cleanup` a validated public anchor rather than relying on a private fixed set. |
| Schema owner lineage | `9e3ae84dac22c805751f1223b2bee85699e9597a` | Separates optional owner absence from tombstoning and preserves atomic active composition. |
| Persistence authority | `9dcc8cf31915237db5441a33ae9af32b1c564901` | Makes saved-state advancement receipt-backed and Host verified. |
| Bounded reload terminality | `46d8c55fdedcab0006d67d9d8c655ed821a81368` | Closes unbounded/unconsumed reload retention without changing Runtime ownership. |
| Paused input retention | `5c9a622cb615b6327d4a2fed8ae72e1b2f520d6b` | Preserves bounded input transitions across paused frames. |
| RGD-U2 authority refresh | `b4d105cbf6312cb4006d8b06b0170f8cfdc1a8ec` | Removes public executable-registry replacement authority and validates direct plus managed boundaries. |
| Initial U7 review baseline | `088e233b4b80f1cafc6f56997751ebbfccc4d77c` | Historical review input: the independent Runtime review exposed a direct-App fault-bridge replacement-and-restore bypass, so this revision cannot close U7. |
| U7 repair and final review | `27cbd1298c8cefd470efc25a4d99396305b5b4ea` | Freezes direct and managed reporter/handler identity, detects structural replacement, and carries rolling Bevy-semantic change epochs across every reviewed execution and maintenance boundary. |

# ADR 0084 Runtime Metrics

| Metric | Result | Refreshed evidence |
|---|---|---|
| Startup publication | Pass | `RuntimePublicationSlot` locks the reporter across the final check, owner transfer, and publication marker. |
| Ownership handoff | Pass | The sealed `App` and obligation ledger move once through candidate, publication failure, or published retirement. |
| App admission | Pass | Unsealed, started, raw-runner, and unregistered obligation-bearing paths reject before ownership transfer. |
| Play execution | Pass | Headless, Desktop, and Editor execute through `RuntimeInstance`; none publishes a bare `World`. |
| Driver parity | Pass | The current reference-game suite retains one bounded semantic command oracle across all three concrete Hosts. |
| Driver authority | Pass | The renamed-root fixture uses public reservation/control/drive/close APIs and no raw App or hidden port. |
| Exact step | Pass | Authority is checked before clock writes; one paused request executes one complete fixed/gameplay transaction. |
| Fault closure | Pass | Named fallible execution and required integration failures become sticky. Registry and fault-bridge drift is canonical on direct and managed boundaries; structural revision plus rolling change epochs reject structural and normally change-detected temporary replacement-and-restore, including maintenance observers. |
| Runtime isolation | Pass | World, queues, time, tasks, service/backend sessions, identity, and cache namespaces reconstruct per generation. |
| Finite close | Pass | Incomplete close retains ownership and bounded retry state; abnormal unwind uses observable process quarantine. |
| Stop-first workspace | Pass | Running, faulted, or incomplete owners cannot be silently replaced by Editor or product Hosts. |
| API authority | Pass | Managed public surfaces expose no mutable World or second scheduler/time owner; executable Registry replacement is no longer public. |
| Early ownership value | Pass | The thin owner still closes the U26/U24 fault and ownership cuts without a universal framework. |

ADR 0084 deliberately does not promise recovery after an arbitrary Rust system panic. Unwind
semantics remain outside this decision; this refresh therefore does not convert panics into a new
runtime error model or claim rollback after partial mutation.

# ADR 0082 Host Metrics

| Metric | Result | Refreshed evidence |
|---|---|---|
| Pre-mutation project rejection | Pass | Project lineage, content, schema, and composition reject before candidate/service publication. |
| Recipe coherence | Pass | Content and plan share exact lineage, fingerprint, frozen snapshot identity, and private executable Registry authority. |
| Fresh plugin preparation | Pass | Repeated starts retain definition/configuration identity while constructing fresh candidate-local owners. |
| Early topology value | Pass | The concrete U24 path remains bounded by the independently frozen U26 counterfactual. |
| Runtime delegation | Pass | `RuntimeStartAttempt -> complete_startup -> publish_into` remains the only product publication path. |
| Parent lifetime | Pass | `CloseIncomplete` moves into retained cleanup and keeps its start claim/Host parent until truthful retirement. |
| Cross-host parity | Pass | The same declared semantic input and canonical output remain exercised across Headless, Desktop, and Editor. |
| Least privilege | Pass | Server composition remains separate from window, render, audio-device, editor, and raw-input sessions. |
| Embedded path | Pass | The locked renamed-root fixture runs a code-first managed runtime without project or universal Host APIs. |
| Admitted external authority parity | Pass | The one admitted external Runner role remains selectable without first-party identity checks; other replacement roles remain unaccepted. |

# Combined Runtime/Host Scenarios

| Required scenario | Result | Required product behavior |
|---|---|---|
| Sequential RuntimeInstances | Pass | A truthfully retired generation can be reconstructed with fresh mutable state and a non-reused generation. |
| Overlapping RuntimeInstances | Pass | Independent runtimes can overlap while one `ProjectHost` still rejects a second active start. |
| Process-global authority contention | Pass | Bounded per-runtime routes isolate reporters and retained/quarantined owners preserve their route until safe retirement. |
| Fresh-runtime reconstruction | Pass | Mutable World, queue, time, task, service, backend/cache, and identity state reconstruct; named immutable inputs may be shared. |
| Plan/World registry divergence | Pass | Composition, candidate materialization, published safe points, and direct App execution bind one immutable executable Registry instance. |

Cross-host parity compares declared semantic commands and canonical product output. It does not
claim identical platform-loop internals. The renamed-root fixture proves managed embedding, not a
second `ProjectHost` publication path or a universal Runner SPI.

# Independent Reviews

The Runtime review examined ADR 0084 first at `088e233` and found a P1: direct App code could
temporarily replace and restore the runtime fault reporter or selected fallback handler while
preserving final identity, allowing a fallible system fault to escape the canonical sticky route.
That revision is therefore review input rather than closure evidence. Commit `27cbd12` freezes both
identities, adds structural revision hooks, and validates rolling change epochs at built-in
schedules, custom schedules, custom runners, direct `run_once`, and managed candidate/driver safe
points. A final P1 challenge showed that `check_change_ticks()` itself can run observers; the repair
now captures and validates an additional epoch around that maintenance before refreshing the next
guard. Follow-up review also challenged exact raw tick comparison and the Bevy change-age horizon;
the final implementation uses `Tick::is_newer_than` semantics and rolls the App guard at each safe
point so one long-lived outer epoch cannot forget an old mutation.

The final Runtime review at `27cbd12` found no remaining fixed-revision authority bypass in the
reviewed contract and retained
ADR 0084 for the bounded scope. The wording remains precise: no typed Registry replacement
authority is exposed, while structural and normal Bevy change-detected mutation fail closed.
Explicit `bypass_change_detection`, manual tick rewriting, unsafe/raw ECS mutation, and equivalent
Host-trusted escape hatches are not a tamper-proof guarantee. The static API-authority oracle may
later expand across every `nara_app` source file, but that test-scope follow-up does not create a
current authority bypass. Custom-schedule unwind and exact fixed stepping retain their existing
rejection behavior.

The final correctness review reported no P0/P1 and one documentation P2: the prior wording could
be read as a tamper-proof claim over explicit Bevy change-detection bypasses. The bounded wording
above resolves that finding without weakening the fail-closed contract for ordinary supported
mutation paths.

The dependent Host and compatibility review then retained ADR 0082 and the compatible pair. It
found no path that publishes a raw `App`, loses a retained child owner, or turns the private Registry
owner into a public Host/provider abstraction. No universal Host, service hub, package activation,
replacement Render Host, or Runner SPI is admitted.

# Context

The previous U7 decision was correct at its reviewed revision but became historical after later
Rust, workflow, and product corrections changed executable evidence. The most important Runtime
change is the Registry authority refresh: `ComponentRegistry` remains a standalone build/freeze
value but no longer implements ECS `Resource`; the executable owner is private, immutable to normal
consumers, and checked at direct and managed schedule boundaries.

After the repair, the full serial workspace run passed 1,091 tests with 3 skipped, the focused
`nara_app` run passed 84 tests, and the focused root Runtime suites passed 81 tests. Targeted strict
Clippy and formatting passed. Per owner direction, the `architecture_docs` Cargo binary was not
used as evidence; immutable-memory validation, render freshness, exact source links, and manual
table reconciliation own this documentation decision instead.

# Alternatives

1. **Retain the bounded Accepted pair.** Selected because every original metric remains satisfied
   and the authority refresh removes a public bypass without adding a second runtime/Host owner.
2. **Return ADR 0084 to Proposed.** Rejected because the reviewed defects are closed and no metric
   lacks executable evidence at the current source revision.
3. **Accept ADR 0084 but return ADR 0082 to Proposed.** Rejected because the concrete Host retains
   coherent pre-mutation inputs, atomic publication, parent lifetime, parity, and least privilege.
4. **Expand acceptance into a universal Host/Runner/package model.** Rejected because no independent
   replacement producer or package-activation tracer justifies that compatibility surface.

# Consequences

- ADR 0084 and ADR 0082 remain Accepted only for the bounded already-compiled, Host-trusted scope.
- U7 closes at `27cbd12`; U8 is the next active unit and must refresh hosted evidence at the final
  integrated source revision before U9/U10/U11 can close.
- This decision does not authorize candidate dispatch, evidence ingest, approval, tag, draft,
  release, or publication mutation.
- Dependency correction remains deferred until the active plan's four gates prove production-edge
  classification, a real hierarchy/transform/UI consumer slice, the `nara_reflect -> nara_asset`
  deletion test, and direct plus renamed-root consumers.
- Future replacement Platform/Runner or Render Host roles require their own selection,
  sole-authority, close-order, and clean-room conformance evidence.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u7-re-review-runtime-then-host-authority`
- `docs/architecture/adr/0082-process-host-authority-and-runtime-construction-topology.md`
- `docs/architecture/adr/0084-executable-runtime-ownership-and-isolation.md`
- `docs/knowledge/engineering/verification/2026-07/2026-07-28T212146Z-rgd-u2-registry-authority-refresh-verification-054dab6712644124af153f058b763fff.md`
- `docs/knowledge/engineering/decisions/2026-07/2026-07-23T074018Z-rgd-u7-runtime-and-host-independent-decision-matrix-e2e5ea1ed4cf4e28860cedb32f0e7e48.md`
