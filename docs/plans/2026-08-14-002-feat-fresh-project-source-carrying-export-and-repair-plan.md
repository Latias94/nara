---
title: Fresh Project, Source-Carrying Export, and Explicit Repair - Plan
type: feat
date: 2026-08-14
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
execution: code
execution_state: active
product_contract_source: docs/architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md
origin: STRATEGY.md
depends_on: docs/plans/2026-08-14-001-refactor-capability-directory-publication-foundation-plan.md
plan_id: fresh-project-source-carrying-export-and-repair-2026-08
unit_namespace: FPER
operator_execution_authority: external-control-plane
---

# Fresh Project, Source-Carrying Export, and Explicit Repair - Plan

## Goal Capsule

- **Objective:** Deliver one complete bounded Rust-author startup-closure workflow from an authorized
  parent with an absent destination child to a versioned desktop-2D project, normal desktop and
  presentation-free headless execution, a checkout-free source-carrying startup-closure export, an
  actionable missing-startup-Scene failure, and an explicit repair followed by successful rerun and
  re-export. This is not a complete Editor or release-content workflow.
- **Product outcome:** A Rust author chooses one starter and one supported Nara dependency source;
  the tool owns agreement among Cargo, `nara.toml`, ProductRecipe, schema and initial content.
- **Architecture outcome:** One root project-workflow Module owns creation, check, export and repair
  transactions. `nara_project` remains the pure manifest-semantics owner, `nara_fs` remains the
  native filesystem substrate, and Runtime `ProjectHost` remains a consumer of authorized content.
- **Activation gate:** Directory Publication conformance record
  `1c01523e7462406dabf4a177cf64285c` is pinned. FPER-U0 closes at `0b99ad4` with conformance
  record `6717111091f5451fa4708a04b79e1c35`; reviewed registration
  `3227ab58386f457496c02fb0cfc04771` activates FPER-U1 as the sole active unit. FPER-U2 and
  FPER-U3 require their own successor registrations.
- **Scope guard:** Reference Game and its Python package tools remain fixed historical product
  fixtures. They are not mandatory consumers and are not modified or generalized by this plan.

## Product Contract

### Primary author flow

```mermaid
flowchart LR
  P[Authorized parent; destination child absent] --> N[Create desktop-2d/v1]
  N --> G[Follow generated author guide]
  G --> E[Edit ordinary authored Scene data]
  E --> C[Check project startup closure]
  C --> H[Run presentation-free headless]
  C --> D[Run bounded real desktop verification]
  H --> X[Publish source-carrying startup-closure candidate]
  D --> X
  X --> A[Run from arbitrary CWD without checkout or toolchain]
  A --> B[Move the selected startup Scene]
  B --> F[Typed missing-startup-Scene issue]
  F --> R[Select a scenes-root-relative replacement]
  R --> C
```

### Project creation and identity

- **R1 — One closed starter request.** `new desktop-2d/v1` has no per-layer override surface. It
  selects matching generated entry points, Cargo features, manifest capabilities, ProductRecipe,
  built-in schema closure and initial content. Its generated Cargo manifest declares an independent
  `[workspace]` with resolver `3`, so a local-source project does not accidentally join an ancestor
  checkout workspace. Existing direct-App and advanced composition Interfaces remain outside this
  plan and are neither removed nor narrowed.
- **R2 — Dependency distribution is explicit and separate.** Creation accepts one typed
  `NaraDependencySource`: an authorized local checkout whose destination parent is proven as a
  descendant of that checkout capability, or an immutable Git URL plus exact revision. The workflow
  Module derives the Cargo path from the descendant relation and future child name; callers cannot
  submit a native or Cargo path as proof. The starter selects compatibility; the request selects
  distribution. Starter version, manifest schema, Scene envelope and dependency identity remain
  distinct receipt facts. Absolute checkout paths, branch heads and implicit network fallback are
  forbidden. A local-path project is location-bound by contract, and generated guidance states that
  upgrades are explicit future migrations rather than automatic starter updates.
- **R3 — Directory publication uses the predecessor baseline.** Creation publishes a known finite
  tree through the exact Directory Publication Foundation receipt. The destination must not exist;
  no check-then-rename or overwrite fallback is allowed. Ordinary admission occurs after publication.
  A reopen/admission failure retains the directory and publication receipt as
  `PublishedButUnadmitted`; only successful ordinary admission produces a creation success receipt,
  and no rollback is claimed.
- **R4 — Normal project authority.** The created project is admitted by the ordinary
  `ingest_project_manifest -> product closure -> ProjectContentLoader` path. There is no
  starter-only loader, hidden provider list or Reference Game plugin.
- **R5 — Complete-schema-only v1.** Check, run, repair and export fail closed when required schema is
  unavailable. Unknown fields or unavailable schema are not preserved through repair.

### Runtime products

- **R6 — Two normal products.** The generated project owns explicit `desktop` and `headless`
  binaries over one manifest byte digest, ordinary ProductRecipe, schema composition and
  startup-source content digest. Desktop and headless retain distinct profile-bound
  `ProjectSettingsLineage` values and neither is used as the other's identity. The desktop binary
  provides a documented bounded verification mode that uses its real Winit/WGPU path and exits
  through normal App/Host shutdown only after an exact first-party presented receipt matches the
  admitted startup content and live backend generation within a product-owned timeout. Initialization
  without matching presentation cannot pass, and test-private plugins cannot satisfy this proof.

### Source-carrying startup-closure export

- **R7 — Existing content owners produce the member inventory.** The project-content owner produces
  one bounded `StartupContentClosureReceipt` with ordered logical paths, member kinds, byte lengths,
  content digests, source/content revision and exact manifest/schema facts. Export consumes that
  receipt and reopens bytes through `nara_fs`; it does not parse Scene, Prefab, image metadata or Rust
  source to rediscover dependencies.
- **R8 — The claim is deliberately narrower than a complete project export.** Version one publishes
  selected executables plus an executable-relative `project/` tree containing the exact startup
  runtime closure. Unreferenced or dynamically requested content is not implicitly included. The
  artifact is neither a complete source-project backup nor a cooked/source-free catalog.
- **R9 — Runtime source remains source.** The candidate may contain `nara.toml`, Scene/Prefab
  documents, images, `.meta` files and schema facts. ADR 0088 remains Proposed.
- **R10 — Cargo artifacts are selected structurally.** A private Cargo JSON Adapter accepts one
  Cargo JSON stream and matches exact invocation identity, manifest/workspace, package ID, binary
  target kind/name, target triple, named profile and normalized features. Each selected binary target
  must produce exactly one executable entry with its byte digest; the build receipt carries the
  desktop and headless entries together. Before `cargo build --locked`, the Adapter captures a
  bounded build-input revision over the application Cargo manifest, `Cargo.lock` and generated Rust
  sources; export revalidates those inputs before publication. Stale source or lock inputs reject the
  attempt. Target-directory scans and timestamp guesses are forbidden.
- **R11 — Publication and acceptance are distinct.** All member, digest, source-change and layout
  validation completes before directory publication. Publication then yields a non-forgeable
  receipt. Post-publication process acceptance may return `PublishedButUnaccepted`; the candidate is
  retained and no rollback is claimed. Version one has no in-place acceptance retry: the author fixes
  the cause and publishes a new destination.

### Diagnostics and repair

- **R12 — Typed issue, no policy callback.** Check reports a privacy-safe missing-startup-Scene
  issue bound to project capability identity, manifest digest and validated settings lineage.
  Diagnostics are observational and cannot mutate the project. That issue is session-scoped evidence,
  not a portable repair authorization: a separate repair command reruns the authoritative check under
  its current capability session before it prepares any mutation. The cross-process trajectory links
  the persistent manifest/content digests and predecessor identity, never a serialized capability or
  settings-lineage value.
- **R13 — Explicit selection, not rename inference.** The author supplies a logical Scene path
  relative to the effective `paths.scenes` authority. Repair does not infer a rename, preserve asset
  stable identity, search candidates, rewrite arbitrary references or perform Fix All.
- **R14 — Honest cooperative file transaction.** All first-party manifest writers take the existing
  exclusive cooperative lock. Repair validates the expected manifest identity/digest, replacement
  Scene digest and full closure, canonically rewrites all modeled manifest fields through
  `nara_project`, and publishes through `nara_fs`. Unknown fields reject; comments, field order and
  original formatting are not preserved. The receipt reports the actual replacement proof tier.
  A post-publication authoritative check is required: mismatch returns `PublishedButNotRepaired`
  rather than a success receipt. This plan does not claim strong CAS against non-cooperating writers.

### Evidence discipline

- **R15 — One linked trajectory.** One parent-owned machine-readable record links exact create,
  manifest, content-closure, desktop, headless, build, export, issue, repair and re-export receipts.
  The parent assigns each child process a predecessor-bound attempt identity and accepts its
  observation only when executable digest, manifest digest, product profile and startup-content
  digest match the prepared facts. Session-scoped filesystem identity is local evidence, not
  cross-process lineage. A step must consume its precise predecessor; colocated but unrelated fixture
  results cannot be spliced into the record.
- **R16 — Moderate proof.** Owner-local tests cover state machines and failure algebra; one bounded
  real process trajectory proves the product. No platform-by-state Cartesian matrix, opaque image
  hash or duplicate smoke implementation is admitted.
- **R17 — Author usability is observed.** The starter includes a minimal generated guide with the
  real create/check/run/edit/export/repair commands and one observable data edit. The guide identifies
  the artifact as a startup-closure development/verification candidate, records the exact starter and
  Nara dependency identities, and states that full content inventory, Editor workflow, formal release
  delivery and automatic starter upgrades remain outside version one. A clean-terminal rehearsal
  follows only public instructions and records command count plus first-run, first-export and repair
  completion time as observations, not frozen SLAs.

### Key Technical Decisions

- **KTD1 — Root deep Module, thin Adapters.** Reusable transactions live in one root
  project-workflow Module. The CLI and the private Cargo/process integration are separate thin
  Adapters. Across U0 through U3, one compile-only consumer with the smallest product features calls
  creation planning, check, export and repair without depending on either Adapter or desktop
  presentation; failure moves the seam before it becomes public.
- **KTD2 — Dependency source is not starter policy.** `NaraDependencySource` is a closed v1 value,
  not a registry/provider. A local source is derived only from a capability-proven checkout-to-parent
  descendant relation; an immutable-Git source carries an exact revision. They share compatibility
  validation and privacy-safe receipts without creating a package manager or accepting a naked path.
- **KTD3 — One immutable starter.** `starters/desktop-2d/v1/` is source-controlled input. Generated
  member bytes are compiled into the workflow Module and are never discovered through a runtime
  checkout-relative path. Generated files become project-owned copies. There is no template registry,
  download, update, inheritance, merge, placeholder language or marketplace lifecycle. The generated
  project remains pinned to its recorded starter and dependency identities; later compatibility or
  migration guidance does not turn version one into an automatic updater.
- **KTD4 — Export reuses a validated startup closure.** The first export serves the runtime shape
  Nara actually owns today. Complete project inventory, dynamic asset requests, cook catalogs and
  archive transport require their own concrete successors.
- **KTD5 — Repair one manifest selection.** Asset move/rebind, `.meta` transactions, unknown-schema
  preservation, recovery journals and project-wide reference rewrites remain behind their Proposed
  ADRs/open questions.
- **KTD6 — Semantic canonical rewrite.** `nara_project` owns pure typed mutation and a deterministic
  writer for its modeled manifest schema. It does not promise lossless TOML syntax editing.
- **KTD7 — Two explicit binaries and one bounded desktop mode.** Desktop and headless are separate
  thin product Adapters. The desktop verification mode is product-owned, requires the R6 exact
  presentation receipt before a fixed timeout, exercises the real window and WGPU path, and exits
  normally; it is not a hidden test presenter.
- **KTD8 — Predecessor before activation.** Product code cannot begin until the exact directory
  publication baseline is pinned. Finishing that Foundation plan does not activate this plan
  automatically.

### Non-goals

- Full project backup/export, dynamic content inventory, cooked catalogs, artifact stores, mounts,
  DLC, archives, signing, store upload or supply-chain provenance.
- Template/provider registries, starter upgrades, project overwrite/merge or creation into an
  existing destination.
- Generic importer/exporter SPI, build trait, process framework or diagnostic action bus.
- Automatic asset rename correlation, stable-ID rebind, `.meta` move, unknown-schema save,
  degraded Editor open or Fix All.
- Editor Shell, docking, Viewport, multi-window, scripting language or UI Foundation expansion.

## Planning Contract

```mermaid
flowchart TB
  CLI[Thin nara CLI Adapter] --> WF[Root Project Workflow Module]
  WF --> PM[nara_project typed manifest semantics]
  WF --> FS[nara_fs file and directory transactions]
  WF --> PC[Product capability and recipe closure]
  WF --> CL[ProjectContentLoader startup closure]
  WF --> PH[ProjectHost run products]
  CLI --> CG[Private Cargo JSON Adapter]
  CG --> WF
  ED[Future Editor Adapter] -. later consumer .-> WF
```

The root Module exposes concrete request/outcome families for one starter creation, one project
check, one startup-closure export and one startup-selection repair. It does not expose a mutable
service, native path authority, Cargo message types, toolkit IDs or Reference Game identities.

### Unit outputs and mandatory reuse

| Unit | Owned output | Existing owners that must be reused | New abstraction budget |
|---|---|---|---|
| FPER-U0 | `src/project_workflow/` creation/dependency/starter Modules, `starters/desktop-2d/v1/`, thin `src/bin/nara/` command Adapter and one `module-consumer` crate | `nara_project` parse/effective settings, `nara_fs` capabilities and directory publication, `ProductRecipe`/profile lowering, ordinary project ingest/content load | One public project-workflow Module and one closed dependency-source family; no new crate in the engine workspace |
| FPER-U1 | project check Module, project-content-owned `StartupContentClosureReceipt`, generated desktop/headless binary sources and bounded desktop mode | `ProjectContentLoader`, `HeadlessRun`, `DesktopRun`, `ProjectHost`, existing startup activation and run reports | One concrete owner-local startup-closure receipt; no second Host, runner lifecycle, schema owner or content loader; generated binaries are thin Adapters |
| FPER-U2 | private Cargo JSON Adapter and startup-closure export transaction inside `src/project_workflow/` | `StartupContentClosureReceipt`, `DirectoryPublicationTransaction`, existing digest and process outcomes | One private Cargo-artifact selector/build-input revision and one public export request/outcome family; no build trait, exporter SPI or process framework |
| FPER-U3 | `nara_project` typed mutation/canonical writer plus project-workflow repair and evidence Modules | cooperative manifest lock, `nara_fs` file replacement, ordinary check, existing desktop/headless run reports | One public typed issue/repair family; trajectory/child-observation machinery remains private evidence infrastructure |

### Receipt inventory

| Stage | Reused authority | New workflow fact | Visibility and consumers |
|---|---|---|---|
| Creation | directory-publication receipt, ordinary manifest candidate and project-content digest | starter/dependency plan plus either admitted creation receipt or `PublishedButUnadmitted` outcome referencing those owners rather than copying their fields | public; CLI and compile-only consumer |
| Check | profile-bound `ProjectSettingsLineage`, `SchemaCompositionFingerprint`, `ProjectContentRevision` and `ContentDigest` | one project-content-owned `StartupContentClosureReceipt`; successful check returns this receipt directly rather than wrapping or copying its manifest, lineage, schema and content facts | public; compile-only consumer, run and export preparation |
| Runtime | existing `HeadlessRunReport` and `DesktopRunReport` | private child observation bound to attempt, executable, manifest, profile and content identities | private Adapter/evidence only |
| Cargo build | Cargo JSON artifact messages, application Cargo manifest, `Cargo.lock`, generated Rust sources and executable byte digests | one private build-input revision and build receipt containing exactly one desktop and one headless artifact entry | private Cargo Adapter and export transaction |
| Export | `StartupContentClosureReceipt` and directory-publication receipt | export receipt referencing member-plan and publication identities | public; CLI, compile-only consumer and trajectory owner |
| Issue/repair | manifest identity/digest, Scene/content digests, file replacement receipt and fresh check | typed missing-startup-Scene issue plus truthful repair outcome | public; CLI and compile-only consumer |
| Trajectory | identities and digests from the preceding rows | parent-owned predecessor chain; no duplicate domain fields | private evidence record consumed by V5 |

### Failure algebra

| Owner | Representative outcomes | Publication rule |
|---|---|---|
| Starter planning | invalid name/source, unsupported starter, inconsistent closure | no staging directory |
| Directory publication | unsupported proof, collision, write/sync failure, orphaned staging | final destination absent; orphan disclosed |
| Creation admission | published tree cannot reopen or ordinary manifest/content admission fails | directory retained as `PublishedButUnadmitted`; no creation success receipt or rollback claim |
| Project check | invalid manifest, unavailable schema, missing dependency, source changed | no runtime/export publication |
| Build Adapter | Cargo failure, unlocked/stale build input, incomplete artifact key, missing/ambiguous executable | no export staging |
| Export | member changed/missing, limit/collision/special-file rejection | final destination absent |
| Acceptance | process startup or arbitrary-CWD failure | candidate retained as `PublishedButUnaccepted` |
| Repair prepare | stale manifest, invalid Scene, closure invalid, lock unavailable | manifest unchanged |
| Repair acceptance | publication succeeded but fresh check disagrees | `PublishedButNotRepaired`; no false success |

## Implementation Units

### FPER-U0 — Versioned starter and deep project-workflow Module

**Goal:** Create a normal Nara project from one closed first-party starter.

**Approach:** Implement the typed dependency sources, immutable starter plan, generated author guide,
pure project plan/receipt types, thin `nara` CLI Adapter and compile-only non-CLI consumer. Validate
all bytes and product-layer agreement before staging, publish through the pinned Directory
Publication baseline, then reopen through ordinary capabilities.

**Test scenarios:** exact inventory; unsafe name; destination collision; invalid dependency source;
local parent outside the authorized checkout relation; immutable Git formatting;
feature/manifest/recipe mismatch; unrelated CWD; ordinary manifest/content load; two binary targets;
ancestor virtual Cargo workspace isolation; publish succeeds but ordinary reopen/admission fails; no
absolute checkout path, caller-authored Cargo path, runtime checkout-relative starter lookup,
placeholder or fixture identity.

**Exit:** One public command creates and admits the project, and the root Module's creation planning
surface is independently consumable without CLI/Cargo/desktop coupling.

### FPER-U1 — Shared check and two normal run products

**Depends on:** FPER-U0.

**Goal:** Make one project source authoritative for check, headless and desktop.

**Approach:** Extend the project-content owner to return the bounded R7
`StartupContentClosureReceipt`; export will reuse it without rediscovering dependencies. Generated
products share the manifest byte digest, recipe, schema composition and startup-source content digest
while retaining distinct desktop and headless profile-bound settings lineages. Add the bounded real
desktop verification mode and normal shutdown contract. Extend the compile-only consumer from FPER-U0
to call the ordinary check surface, still without the CLI, Cargo process Adapter or desktop
presentation.

**Test scenarios:** ordinary Scene edit changes both products; both profile-bound runs report the
same manifest/content digests and schema composition without claiming equal settings lineage; missing
startup Scene and unavailable schema fail before runtime publication; unrelated process CWD cannot
change source authority; closure receipt ordering/member facts/limits; desktop initialization without
the exact presented receipt times out; matching presentation exits through App/Host cleanup; headless
remains presentation-free.

**Exit:** Both normal products consume one source authority, the independent consumer proves the
check seam, and export can reuse a validated closure.

### FPER-U2 — Rust-owned startup-closure standalone export

**Depends on:** FPER-U1 and the Directory Publication baseline.

**Goal:** Publish a checkout-free, toolchain-free directory for the exact admitted startup closure.

**Approach:** Capture the bounded build-input revision, run one `cargo build --locked`, bind exact
Cargo artifacts using the full R10 key, consume the `StartupContentClosureReceipt`, assemble a finite
member plan, revalidate build and content inputs before publication, publish through `nara_fs`, and
run the two real product processes from an unrelated CWD. Extend the compile-only consumer to call the
public export surface using typed closure/build inputs without importing Cargo JSON, process, CLI or
presentation types. Keep archive transport and Reference Game scripts outside the Interface.

**Test scenarios:** missing/ambiguous artifact; wrong feature/profile/target; unlocked/missing/stale
`Cargo.lock`; Rust/Cargo source mutation after build planning; source mutation after export planning;
missing closure member; special file/link/reparse/case collision; limits; output collision; receipt
privacy; independent export consumer; arbitrary-CWD headless and bounded desktop acceptance.

**Exit:** One fresh starter candidate runs without checkout, Cargo or toolchain, the independent
consumer proves the public export seam, and the artifact carries no claim about unreferenced/dynamic
content.

### FPER-U3 — Explicit startup Scene repair and complete trajectory

**Depends on:** FPER-U1 and FPER-U2.

**Goal:** Close the first actionable recovery loop without inventing asset identity repair.

**Approach:** Move the selected startup Scene, return a typed issue, accept one scenes-root-relative
replacement, take the cooperative manifest lock, validate manifest/Scene/closure identities,
canonically publish modeled manifest fields, then run a fresh authoritative check. Rerun both
products and publish a new destination. Extend the same compile-only consumer to call the repair
surface without importing the CLI, Cargo process Adapter or desktop presentation. Emit the linked
trajectory record and author rehearsal.

**Test scenarios:** missing Scene; valid replacement; traversal/outside authority; malformed or
wrong-kind Scene; stale issue; lock conflict; publication failure; unknown manifest field; semantic
preservation of every modeled field; post-publication mismatch; prior export unchanged; new export
succeeds; every trajectory link matches its predecessor.

**Exit:** R1-R17 have clean, linked evidence; the independent consumer proves creation, check, export
and repair; the ADR ledger and engineering memory agree; this plan becomes inactive without
activating Editor work.

## Verification Contract

- Cargo commands run serially with `CARGO_BUILD_JOBS=1`; focused `cargo nextest run` is preferred.
- **V1:** starter/root Module tests prove closed product planning, typed dependency sources, ordinary
  project admission and the independent compile consumer.
- **V2:** check/run tests prove one source authority, a presentation-free headless product and a
  bounded real desktop product with normal shutdown; the independent consumer compiles against the
  ordinary check surface without the CLI, Cargo process Adapter or desktop presentation.
- **V3:** export tests prove the locked build-input revision, full Cargo artifact key, exact
  `StartupContentClosureReceipt`, pre-publication revalidation, no-overwrite publication and receipt
  privacy; the independent consumer compiles against the ordinary export surface without CLI, Cargo
  JSON/process or presentation types.
- **V4:** manifest tests prove semantic canonical rewrite, scenes-root-relative path semantics,
  cooperative-lock/stale rejection and truthful post-publication outcomes; the independent
  consumer compiles against the ordinary repair surface without the CLI, Cargo process Adapter or
  desktop presentation.
- **V5:** one parent-owned linked trajectory creates, edits, runs, exports, breaks, repairs, reruns
  and re-exports from public instructions and unrelated process CWDs; each child observation consumes
  one predecessor-bound attempt and matches the prepared executable, manifest, profile and content
  facts.
- **V6:** public/dependency inventory rejects a template registry, exporter SPI, generic importer,
  build framework, diagnostic callback bus or Reference Game identity.
- **V7:** `cargo fmt --all -- --check`, scoped strict Clippy, `git diff --check`, engineering-memory
  validate/render and final Standards/Spec review pass on the proving commit.

## Success Metrics

| Metric | Target |
|---|---:|
| Manual product-layer edits after creation | 0 |
| Partial or overwritten final destinations after pre-publication failures | 0 |
| Source parsers duplicated in project/export code | 0 |
| Checkout/toolchain requirements in exported runtime | 0 |
| Automatic asset rename/rebind decisions | 0 |
| Linked create-to-repair product trajectories | 1 |
| Generic registries/traits added for one consumer | 0 |

## Risks and Mitigations

| Risk | Severity | Mitigation |
|---|---|---|
| Local dependency source is mistaken for portable distribution | High | Explicit source kind, location-bound contract and immutable-Git alternative |
| Startup closure is mistaken for complete project export | High | Name the artifact and reject unproven dynamic-content claims |
| Root Module absorbs CLI/Cargo/platform details | High | Independent compile consumer and private Adapters |
| Ancestor Cargo workspace captures a local-source project | High | Generated independent workspace boundary plus build regression |
| Build output no longer matches the prepared lock/source inputs | Critical | `--locked` build, input receipt and pre-publication revalidation |
| Desktop evidence hangs or uses a test-only shortcut | High | Product-owned bounded mode with real presentation and normal shutdown |
| Manifest rewrite loses unknown or user-formatted content silently | Critical | Complete-schema-only rejection and explicit semantic-only preservation |
| Repair publishes but content changes before acceptance | Critical | Exact Scene/closure facts plus post-publication authoritative check |

## Definition of Done

- R1-R17 and KTD1-KTD8 have executable evidence bound to reviewable commits.
- `desktop-2d/v1` is created through one deep Module and ordinary Nara project authority.
- The independent compile consumer uses creation, check, export and repair without Adapter or
  presentation coupling.
- Desktop and presentation-free headless consume one manifest/content source revision through their
  own profile-bound settings lineages without Reference Game glue.
- A source-carrying startup-closure candidate runs from arbitrary CWD without checkout or toolchain.
- A moved startup Scene produces a typed issue and an explicit, stale-safe, truthfully qualified
  repair restores run/export without claiming automatic asset rebind or strong non-cooperative CAS.
- Final review has no unresolved P0/P1; plan, Accepted ADR, ledger and engineering memory agree.
