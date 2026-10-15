---
title: Foundation Correctness and Refactoring Audit
date: 2026-10-15
type: audit
status: evidence-only
baseline: b91223a
scope: engine-foundation-and-editor-infrastructure
---

# Foundation Correctness and Refactoring Audit

## Judgment

Nara has a substantial, coherent runtime foundation, but it is not yet justified to call that
foundation correct across its admitted failure paths. Three executed probes expose two hierarchy
contract defects and one asset-to-render defect in code unchanged from HEAD. Additional static
findings concern persistence, render configuration, and the uncommitted supervised Play integration.

A focused foundation correction program is warranted. The evidence does not justify replacing
`bevy_ecs`, rewriting App and TaskPools, merging every domain into the root crate, or introducing a
universal transaction/service framework. Refactor the owners of the broken invariants; preserve the
working architecture around them.

The user's clarified priority is technical correctness and sound infrastructure before game-making
workflows. The proposed sequence below follows that priority. It is a review recommendation, not an
active implementation plan or authorization to supersede the current SPL registration.

## Baseline, Method, and Limits

- Reviewed branch: `refactor/engine-foundation-contracts`; HEAD `b91223a`.
- The checkout contains substantial staged, unstaged, and untracked SPL work. No pre-existing work
  was modified, formatted, staged, discarded, or committed by this audit.
- Three Codex subagents reviewed runtime/assets, Editor/Host/filesystem, and documents/validation.
  A second pass examined hierarchy/Scene/Reflect/Identity and filesystem publication. The primary
  reviewer checked retained findings and executed focused tests and independent probes.
- `codebase-audit` supplied the review structure; its requested Opus model was unavailable. All
  delegated reviews used Codex. `codebase-design` supplied the interface/ownership lens, and
  `rust-best-practices` governed the temporary Rust probes.
- HEAD findings below were checked against the current files. `git diff --quiet HEAD` confirmed
  no changes in the affected hierarchy, asset, image, WGPU, file-publication, and policy-test paths.
  Filesystem line numbers refer to the working tree where its launch-handle WIP shifted lines.
- Tests ran serially with `CARGO_BUILD_JOBS=1`, the existing `target`, and one nextest test thread.
  The host was Windows x86_64 with Rust/Cargo 1.99.0; CI declares Rust 1.95. This audit does not
  certify MSRV, Linux runtime behavior, GPU readback, native Editor E2E, or full workspace green.
- No external API, dependency-version, or comparative engine recommendation was needed. Conclusions
  are grounded in repository contracts, source, local dependency source, tests, and probes.

## Actual Engineering Progress

| Area | Evidence-backed state | Judgment |
|---|---|---|
| ECS, App, fixed time, runtime admission and close | Real startup/frame/retirement chains and extensive fault tests | Retain; no wholesale replacement supported |
| Tasks | Logical cancellation, physical running, destructor reaping, worker join, and retryable close are distinct | Retain; focused close/execution tests pass |
| Hierarchy and transform completion | Ordered transaction and completion architecture implemented | Correctness blockers F01/F02 invalidate a blanket completion claim |
| Scene/Prefab/Reflect/Identity | Scratch patch publication, native/schema agreement, separate identity claims and tombstones | No additional high-confidence defect in the traced paths; preserve these owners |
| Assets/Image/render preparation | Versioned CPU publication, immutable render snapshots, stale-result checks | F03 breaks last-good derivation across owners |
| File-backed persistence | Checkpoints, identity/digest checks, uncertainty and reconcile states exist | F04 leaves resources behind on an already-known unsupported path |
| WGPU/UI | Captured frame packets, surface ownership, presentation receipts and input generations exist | Preserve; F05 and known SDR/background-policy gaps need bounded correction |
| Generic Editor authoring | Open/Recovery, browsing, Scene/Prefab/UI editing, image import have recorded closure | Useful infrastructure, with continuous native coverage gaps |
| Standalone supervised Play | Committed SPL-U0 through U2 verified at `ab211bc`; HEAD records evidence | U3+ has extensive WIP, but F06-F08 and missing closure prevent completion claims |
| Validation and architecture documents | Broad compile/behavior coverage and explicit ADR authority | F09/F10 show maintenance and coverage defects; simplify rather than add governance |

No percentage of total engine completion is assigned: the repository has no finite general-purpose
engine specification against which such a percentage would be meaningful.

## Findings

### F01 — P1 — Hierarchy proof becomes stale inside its own preparation

**Baseline:** HEAD and working tree. **Confidence:** executed reproduction.

`crates/nara_hierarchy/src/writer.rs:119` resolves topology and insertion indices before lifecycle
validation. That validation calls `world.flush()` at `:639`, executing previously queued external
World commands. The resolved proof is not rebuilt afterward. Root placement has no component target
to recheck (`:705`), and commit inserts the old Entity into the root resource (`:755`).

A public caller can spawn an entity, queue its despawn through `world.commands()`, and then call
`HierarchyWriter::place`. The probe returned:

```text
hierarchy: result=Ok(()), entity_alive=false, registered_as_root=true
```

The method has published invalid topology while reporting success. Pending changes to sibling/root
collections can also invalidate precomputed placements; panic variants were not separately executed.
This is not misuse of the hidden prepared-proof API: ordinary public methods suffice.

**Correction:** establish and materialize the deferred baseline before semantic resolution. Keep
external command execution out of the proof-to-commit interval. Apply one ordering contract to
standalone writers, deferred writers, and Scene construction/retirement integration. Inspect the
post-commit flush at `writer.rs:257` under the same contract. Do not patch callers with extra flushes.

**Required regression:** pending despawn, pending sibling mutation, valid deferred batch, rejected
batch with no hierarchy publication, and one Scene/Prefab replacement round trip.

### F02 — P2 — Despawning a leaf root leaves a valid-looking completion receipt

**Baseline:** HEAD and working tree. **Confidence:** executed reproduction.

Root membership exists only in `HierarchyRoots` (`crates/nara_hierarchy/src/relation.rs:10`).
`Parent` and `Children` have discard hooks, but a leaf root has neither component. Removing that
entity therefore does not dirty `HierarchyGenerationState`. The completion fast path
(`lib.rs:74`) compares generations and two equally stale root lists; the validator skips unchanged
generations (`lib.rs:213`).

The probe installed `HierarchyPlugin`, completed a leaf root, despawned it, and drove another frame:

```text
before=Some(1), after_despawn=Some(1), next_frame=Ok(...), after_frame=Some(1)
```

Downstream modules continue to receive completion evidence and structural order containing a dead
Entity. ADR 0100:71-75 requires raw despawn to leave hierarchy fail-closed. Existing
`missing_registered_root_is_reported_exactly` (`tests.rs:658`) calls the full validator directly and
therefore does not cover the broken runtime fast path.

**Correction:** cover every participating entity, including roots without children, with a private
hierarchy lifecycle invalidation mechanism. A private membership marker is one candidate. Preserve
the separation between structural ancestry and destruction ownership. Avoid scanning all roots on
every static frame to compensate for missing invalidation.

### F03 — P2 — Failed reload prevents derivation from a valid published image

**Baseline:** HEAD and working tree. **Confidence:** executed reproduction plus an existing reachable fixture.

`Assets::record_reload_failure` retains the CPU value (`crates/nara_asset/src/storage.rs:169`),
while `AssetStates::set_failed` preserves its version/hash and changes the attempt state
(`state.rs:244`). `prepare_images` accepts only `LoadState::Loaded`
(`crates/nara_image/src/prepare.rs:195-202`). If failure occurs before the first Prepare, the valid
CPU last-good value never receives a prepared resource. A later Prepare still skips it.

The independent probe successfully imported the project's PNG, recorded a reload failure, and ran
the public prepare system twice:

```text
image frame 1: cpu_last_good=true, render_ready=false
image frame 2: cpu_last_good=true, render_ready=false
```

The existing metadata-reload test (`crates/nara_image/src/tests.rs:1773`) reaches the same state but
asserts only CPU retention. Its normal test passes. Missing prepared images are omitted by the
sprite queue (`crates/nara_sprite_render/src/queue.rs:261`). A related sequence can leave prepared A
visible after CPU B was published and the next reload failed before Prepare; that variant was not
executed separately.

**Correction:** distinguish the published usable value/version from the latest load attempt and
diagnostic. Make preparation derive from published availability, including retained last-good.
Keep invalidation on actual removal or replacement. Do not add sprite/UI fallback readers.

### F04 — P2 — Unsupported strict save writes and loses temporary-file ownership

**Baseline:** HEAD; filesystem WIP does not introduce the defect. **Confidence:** cross-file static proof;
Linux execution not performed in this Windows audit.

`FilePublicationHost::publish` checks only the static platform matrix
(`src/project_host/persistence/file_publication.rs:211`). Linux NameBound publication passes this
check. It creates, writes, and syncs a full temporary document (`:259-270`). Normal Editor roots use
`TrustMode::Untrusted` (`editor/src/app.rs:2100`), so `DirectoryCapability::replace_temp` then rejects
the NameBound operation (`crates/nara_fs/src/capability.rs:448`; HEAD `:429`).

The strict rejection is correct. The defect is effect ordering and cleanup: replace consumes
`TemporaryFile`, Unix exact discard is unsupported (`platform/unix.rs:327`; HEAD `:312`), and Drop
ignores that error (`replace.rs:126`). The Host returns only UnsupportedGuarantee (`file_publication.rs:295`),
retaining no orphan owner or cleanup evidence. Repeated Save can leave an additional full document
copy each time. The low-level Linux test explicitly asserts the rejected temporary remains
(`crates/nara_fs/tests/filesystem_contract.rs:1045-1066`).

**Correction:** perform capability-instance publication admission before creating a temporary file.
For failures only knowable after creation, retain bounded orphan ownership/evidence. Preserve exact
handle-based safety; do not substitute a racy name-based unlink. Original-file and dirty-checkpoint
retention already work and should remain intact.

### F05 — P2 — Same-size present-mode changes do not reach surface configuration

**Baseline:** HEAD and working tree. **Confidence:** static control flow; no native GPU toggle executed.

`Window.present_mode` is public (`crates/nara_window/src/lib.rs:154`) and is copied into every
immutable render packet (`crates/nara_render/src/lib.rs:531-534`). However,
`configure_surface_if_needed` considers only extent changes and `surface.dirty`
(`crates/nara_render_wgpu/src/backend.rs:1354-1366`). Mode selection is applied only inside
configuration (`surface.rs:315`). Changing VSync/present mode at unchanged size leaves the old mode
active until another condition causes reconfiguration.

**Correction:** make the configuration identity include requested present mode as well as extent
and surface/device identity. Record the request separately from the selected fallback mode, so an
unsupported request does not itself cause reconfiguration every frame. Test same-size transitions,
unchanged requests, and unsupported-mode fallback.

### F06 — P1 — External Play close failure has no reachable retry action

**Baseline:** uncommitted SPL work only. **Confidence:** complete static failure/recovery trace.

During close/replacement, `SupervisedPlayOwner` retains `close_cleanup_incomplete`; only RetryClose
clears it (`editor/src/supervised_play/owner.rs:154`, `:338`, `:1301`). Repeated close and continued
polling cannot clear it. The application closing filter forwards only WorkspaceIntent.Resolve
(`editor/src/app.rs:1915-1923`), while the external toolbar emits RetryRetirement
(`crates/nara_tooling_egui/src/editor/external_play.rs:77`). The Host projects all cleanup failure as
RetirementIncomplete (`src/project_host/runtime/editor/play.rs:39`).

One cleanup failure can therefore leave normal close or project replacement permanently pending.
Existing direct-Owner retry tests do not exercise this application/Host/UI action path.

**Correction:** preserve CloseIncomplete as a distinct lifecycle fact, expose the existing RetryClose
action, and admit it during closing. Refactor the private Owner's close/retirement state contract;
do not add a second cleanup manager.

### F07 — P2 — Identical external observations cause self-sustaining Editor redraw

**Baseline:** uncommitted SPL work only. **Confidence:** static event-loop trace.

The Owner returns observations on every building/running advance, even without a change
(`owner.rs:682`, `:1011`). Host application returns true for every matching attempt
(`src/project_host/runtime/editor/external_play.rs:467-482`). That becomes actions_observed
(`editor/src/app.rs:1017-1025`), which requests another redraw (`editor/src/platform.rs:2201-2217`).
The next frame repeats the chain while an external child is quiet.

**Correction:** publish/deduplicate actual changes in lifecycle, result, and output sequence. Add a
no-change/no-redraw regression at the application seam; keep OS wake and deadlines responsible for
future work. Existing wake-coalescing tests do not cover this feedback loop.

### F08 — P2 — Generation publication and launch checks block the UI thread

**Baseline:** the publication mechanism exists in HEAD; its ordinary UI-thread integration is WIP.
**Confidence:** static execution trace; latency not benchmarked.

`app.rs:1017` drives Owner advance, which publishes a generation synchronously (`owner.rs:793`,
`:1167`). Publication hashes the source (`generation/store/publication.rs:91`), copies and syncs it,
and verifies it again (`:270`). Launch revalidation performs further synchronous checks
(`supervisor.rs:160`). Executables may be up to 1 GiB. Slow storage or antivirus can block processing
Cancel, Stop, or Close during these operations; the later spawn worker does not move this I/O off
the UI thread.

**Correction:** put publication and launch validation behind a private cancellable work phase with
explicit resource ownership and completion. Retain executable digest, exact handle, and immutable
generation guarantees. Do not introduce a new async runtime or universal filesystem-job framework.

### F09 — P2 — Architecture tests simulate Rust semantics and freeze implementation spelling

**Baseline:** HEAD. **Confidence:** direct inspection of test predicates.

`tests/schedule_extension_contract.rs:118` scans the entire reference-game source tree. It collects
cross-file aliases (`:426`) and guesses member use from macro token co-occurrence (`:441`); macro
strings are split into identifier-like words (`:606`, `:635`). Thus harmless `let include = true;`
violates its identifier prohibition, and diagnostic text such as
`println!("FixedUpdateSet::Prepare")` can be treated as a forbidden member use.

`tests/project_host_boundary.rs:331` freezes a constructor argument's local name as `commands`;
`:339-350` constrain equivalent type spelling. These checks reject valid refactors without proving
runtime invariants. Similar import-resolution machinery exists in `tests/module_consumer_boundary.rs`.

**Correction:** replace semantic/source-spelling inference with minimal independent consumers,
Cargo dependency checks, meaningful compile-fail privacy checks, and behavioral transactions.
Keep a small explicit source check only where its literal policy is itself intentional. Do not
extend the existing scanners into a compiler frontend or recreate them in Python/xtask.

### F10 — P2/P3 — Coverage and progress claims have concrete drift

**Baseline:** HEAD. **Confidence:** repository-wide inventory and document comparison.

- **P2:** `ui-module-consumer` is excluded from the root workspace (`Cargo.toml:11`) and has no CI
  execution. `tests/ci_policy.rs:58` additionally requires exactly the three older workspace jobs.
  Its direct Text/UI consumer independence can regress while all configured jobs pass. Merge the
  consumer into the existing module-consumer workspace or explicitly execute it; verify coverage
  semantics rather than a frozen job count.
- **P2:** native Generic Open and Image Import canaries exist under `editor/src/platform/`, but CI
  never activates their environment-variable entries. The real build/export authoring trajectory
  is ignored (`editor/tests/authoring_loop.rs:352`). Preserve local historical evidence, but add one
  bounded native representative regression lane rather than claiming nextest covers that loop.
- **P3:** ADR 0116's ledger row (`docs/architecture/adr/implementation-status.md:230`) still says U0
  active/U1-U5 pending, while the ledger introduction, map, registry, and verification correctly
  record U2. The map also refers to an active RGF harness after RGF closure (`README.md:107`). The
  plan activation convention asks for frontmatter active state, but the SPL plan has no such field.
  Reduce duplicate progress prose and reconcile the owning ledger/registration. Do not build another
  prose validator.

## Designs to Preserve and Limited Cleanup Candidates

The following mechanisms earn their complexity through real consumers and failure contracts:

- `bevy_ecs` as the execution substrate; fallible App composition; explicit runtime admission,
  publication, retirement and quarantine; fixed-time/debt/step semantics.
- Task cancellation distinct from physical completion; actual worker join and destructor-reaper
  receipts. Rust unwind remains an explicit boundary, not a guarantee of universal panic isolation.
- Scene scratch publication and prepared commit tails; strict native/schema agreement; identity
  lifetime claims separated from bounded tombstone detail; immutable derived render resources.
- Data-only Authoring separated from strict Runtime; neutral tooling separated from filesystem,
  native process, and presentation ownership; generation/receipt-stamped stale-result rejection.
- Exact launch handles, one-shot first-present READY, explicit last-good, truthful cleanup receipts,
  and distinct Windows/Unix guarantees.
- Decision-local measurements and the fixed real archive consumers. ADR 0099 already removed a
  generic evidence/approval framework; recreating it would repeat an acknowledged mistake.

One small public-surface cleanup deserves assessment: `PluginCapability` and `PluginServiceId`
(`crates/nara_app/src/plugin.rs:60-61`) have declaration/resolver machinery but no production
declaration consumers found outside tests. The ledger already acknowledges this (`:317`). Remove
or narrow the unused promises rather than inventing a service registry to justify them. Preserve
actual plugin dependency IDs, product capabilities, schema contributions, and shutdown obligations.

## Known Gaps Kept Separate from Proven Bugs

- ADR 0092's SDR contract is partial: public Color semantics, explicit sRGB conversion, finite
  persistent colors, and output-format validation need a bounded conformance pass. The use of
  `add_srgb_suffix` alone is not proof of a concrete adapter failure.
- Winit Wait/background redraw is a known incomplete policy: `nara_winit/src/lib.rs:1053` drives a
  frame and `:421` unconditionally requests redraw. Verify its behavior with a static/no-VSync
  native probe before broad scheduling changes; do not fold it into F07's distinct Editor loop.
- Source carrying/export and reference-game completion are separate product questions. Their
  unfinished scope is not evidence that the ECS, identity, or task foundation should be rewritten.

## Recommended Foundation Correction Sequence

This is a candidate scope for a new plan, deliberately limited to admitted contracts. No game
feature, 3D, physics backend, hot-reload ABI, extension kernel, or general RenderGraph is required.

| Unit | Owning contract and work | Exit evidence |
|---|---|---|
| B0: Establish authority and baseline | Record exact SPL WIP boundary; choose explicit completion or supersession of that lane; reconcile current ledger only | One unambiguous active registration and a retained inventory of user changes |
| B1: Hierarchy correctness | Fix deferred baseline/proof/commit ordering and complete member-lifetime invalidation (F01/F02) | Both executed counterexamples become passing behavioral regressions; unchanged frames retain the scan fast path; Scene/Prefab replace/export still agrees |
| B2: Asset publication semantics | Separate usable published value from load attempt/failure; update Image prepare consumers (F03) | Initial failure, success/failure before first Prepare, published replacement/failure, and explicit removal give correct CPU and prepared snapshots |
| B3: Persistence admission and cleanup | Capability-instance preflight, resource-preserving failure result, bounded residue (F04) | Windows and native Linux reject unsupported paths before effects; injected write/sync/replace failures retain ownership and dirty state |
| B4: Render/UI conformance | Correct surface configuration identity (F05); close admitted SDR semantics and measure background wake | Same-size mode change/fallback tests; finite and color-space round trips; one native idle/wake and rendered SDR check |
| B5: Editor lifecycle integration | Correct close retry reachability, change observations, and cancellable publication phases (F06-F08) | One injected close failure recovers through normal actions; quiet child does not self-redraw; slow publication remains cancellable; no conflicting child survives |
| B6: Verification simplification | Remove source-semantic imitation, retain real consumers/privacy tests, cover UI consumer and one native lane (F09/F10) | Legal renaming/reexports do not fail; public contract regressions do fail; no new generalized proof tool |

B1-B5 are the correctness work; B6 should accompany each owning correction where existing tests
obstruct it. Stop expanding a unit once its concrete invariants and consumers are covered. If a
fix changes an Accepted ADR's decision, update or refine that decision explicitly; ordinary bug
corrections should implement the existing contract rather than create another architecture layer.

Foundation acceptance should mean these finite counterexamples and admitted contracts are covered,
not that every Proposed ADR or future engine domain is implemented. Only after this baseline should
the next game-production plan be selected, as requested by the user.

## Executed Verification

| Check | Result | What it establishes |
|---|---|---|
| `cargo nextest run -p nara-editor --lib --locked --test-threads=1` | 88 passed | Current Windows Editor library tests; does not include native E2E or close-failure application recovery |
| `cargo nextest run -p nara_hierarchy --lib --locked --test-threads=1` | 20 passed | Existing hierarchy suite passes despite F01/F02; confirms the missing cases matter |
| `cargo nextest run -p nara_tasks --lib --locked -E 'test(tests::close::) \| test(tests::execution::)' --test-threads=1` | 43 passed | Actual cancellation, physical retirement, destructor reaping, panic containment, and close/retry behavior in the selected suites |
| `cargo nextest run -p nara_image --lib --features serde,render-prepare --locked -E 'test(meta_reload_admits_changed_recipe)' --test-threads=1` | 1 passed, 64 filtered | Existing CPU last-good test passes while omitting prepared availability |
| Independent public-API probes linked to freshly built Nara libraries | Three defects reproduced | F01, F02, and F03, without modifying engine/test sources |

The temporary probe is retained locally at
`C:/Users/Frankorz/AppData/Local/Temp/nara-foundation-audit-20261015/repro.rs`.
It uses public App/World/hierarchy/Image APIs; F02 also reads the existing hidden completion fact
used by production consumers. Initial direct linking needed the Cargo-installed Windows import-lib
directory; after supplying it, compilation and execution succeeded. This setup retry was unrelated
to the engine defects. Assertions in this diagnostic probe confirm the observed defective states;
implementation regressions must instead assert the corrected contract.

Deprecation warnings appeared because the local toolchain is newer than the CI toolchain. They are
not classified as architecture defects. No full-workspace, Linux runtime, GPU, packaging, or native
Editor success claim is made by this audit.
