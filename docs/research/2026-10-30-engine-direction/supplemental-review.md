# Supplemental Review: Consumer And Acceptance Closure

Date: 2026-10-30. Baseline: `75e1284ed28bc5a96686609f7dae8805d1924788`.

This bounded supplement adds five acceptance refinements to the existing 26 observations, not
five new defects. It uses local source, Accepted ADRs and the current
[rebaseline plan](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md).
The existing research lanes were read for overlap. Proposed contracts remain proposals;
unimplemented future workflows are not failures of today's supported product. No Cargo, native
run, benchmark, network access, reference update or agent creation was performed. Tests cited
below were inspected, not executed. Only this file was written.

## S1. Prove Authoring-To-Runtime Expansion Without Weakening Persistent Bindings

**Implemented and owning boundaries.** Accepted
[ADR 0006](../../architecture/adr/0006-scene-and-prefab-data-model.md) permits runtime-only derived
state and owns processed Scene cooking. [Scene cook](../../../crates/nara_scene/src/runtime_scene/cook.rs)
normalizes a complete expansion, rewrites references and retains provenance;
[identity admission](../../../crates/nara_scene/src/runtime_scene/identity.rs) checks every entity's
logical origin. This is not a general baker that splits one author component into arbitrary
runtime objects. [ADR 0122](../../architecture/adr/0122-scoped-imported-product-identity-and-authored-continuity.md)
already supports multiple imported products. That asset identity mechanism does not itself
define generated runtime entity identity or source-property diagnostics.

**Already planned.** R4c/R10 supplies the concrete physics/animation consumer; R6 supplies typed
import/cook ownership. CP5 already covers recipe identity and reproducibility. The additional
question is how derived outputs remain explainable and removable, not another cache design.

**Minimum complete consumer and new acceptance.** Extend the selected motion actor: one authored
collision definition produces solver body/collider state and debug geometry. Follow editing,
Play, Save/Reopen and the selected export path. Compare domain-owned derivation after spawn with
a domain-specific cooked product only if startup or delivery needs it. Record each generated
role's originating source, occurrence, component/property and input revision. A failed generation
must identify that author input; runtime handles must never become source addresses. Rebuild
after changing and removing the definition: current generated membership must match current
intent, while retained old outputs retire under their original owners. Duplicate the actor and
verify distinct occurrences without guessing from names or ECS indices.

**Preserve.** Keep Schema-first truth, existing exact native persistent bindings, explicit provenance
and [plugin-owned specialized domains](../../architecture/adr/0095-plugin-owned-specialized-domains-and-project-configuration.md).
Derived outputs can exist beside the primary binding without turning a native codec into an
arbitrary World-mutating callback. A concrete derivation can remain private; this does not select
a generic baking framework or require one authored object to produce only one runtime object.

## S2. Specify Interrupted Project Migration As An Observable Recovery Workflow

**Implemented and owning boundaries.** Accepted
[ADR 0043](../../architecture/adr/0043-scene-prefab-and-patch-document-migration-policy.md)
separates value, selector and document migration;
[ADR 0047](../../architecture/adr/0047-editor-workspace-and-scene-document-state.md) requires
explicit cross-document transactions. Existing
[Schema adoption](../../../src/project_host/runtime/editor/schema_owner.rs) distinguishes
`PublishedWithoutAdoption`, `Uncertain` and reconciliation. Its
[tests](../../../src/project_host/runtime/editor/schema_owner/tests.rs) include interrupted
publication and retained dirty-state reconciliation.
[File publication](../../../src/project_host/file_publication.rs) separately records replacement
evidence and directory synchronization. These are useful implemented mechanisms, not a
crash-recoverable project migration protocol.

**Already planned.** R5 leaves Schema evolution open. CP1/CP2 already request coordinated value,
claim and occurrence remapping, history policy and failed multi-file publication. Add the
following recovery oracle when implementing that workflow; do not reopen additive adoption.

**Minimum complete consumer and new acceptance.** Upgrade one nested Prefab and two dependent
Scenes, with a dirty working copy, dormant override and unavailable owner. Preview the complete
participating source/Schema bases and chosen history checkpoint or retained-history policy.
Interrupt before publication, between file replacements, and after disk publication but before
Session acknowledgment; then reopen in a fresh process.

| Interruption | Required observation |
| --- | --- |
| Before the first replacement | No source replacements; retain live history or recover the operation's reviewed input. |
| Between replacements | Identify each verified old/new member and unresolved member; do not report global success or automatic rollback. |
| After publication, before acknowledgment | Reconcile actual disk facts; retry cannot apply the transformation twice or invent a missing receipt. |

Publish recovery information before the first destructive replacement through a bounded owning
operation, including reviewed dirty inputs needed to reconstruct it. This need not recover
arbitrary session history after process loss. A concurrent external edit must cause a specific
conflict, never unconditional restoration over somebody else's work. Unknown records remain
preserved or cause explicit refusal.
Recovered Check/Play must consume a coherent admitted generation or refuse clearly.

**Preserve.** Keep per-document history, exact source observations, pure transformations and
truthful partial outcomes. Cross-file atomicity is not inferred from per-file rename; no global
Undo engine or migration DSL is needed.

## S3. Turn Existing Color And Coordinate Decisions Into Numerical Cross-Path Oracles

**Implemented and owning boundaries.**
[Image upload](../../../crates/nara_render_wgpu/src/texture.rs) distinguishes sRGB and linear
formats; [quad blending](../../../crates/nara_render_wgpu/src/quad.rs) selects
`ALPHA_BLENDING`; [Color](../../../crates/nara_core/src/lib.rs) carries four untagged floats.
These facts do not prove a current visual defect. Accepted
[ADR 0033](../../architecture/adr/0033-asset-import-and-render-resource-preparation-seam.md)
separates image interpretation from material alpha policy.
[ADR 0108](../../architecture/adr/0108-ui-surface-presented-session-and-atomic-interaction.md)
and [UI geometry](../../../crates/nara_ui_kernel/src/geometry.rs) already provide forward/inverse
mapping and reject letterbox input.

**Already planned.** R9a explicitly selects linear premultiplied reusable outputs, numeric
readback and captured time; R11 retains color, clipping and input authority. Proposed ADR 0121
does not make these implemented. The supplement is a concrete oracle for that selected direction,
not a missing-color-contract finding.

**Minimum complete consumer and new acceptance.** Display the same translucent sprite, glyph
edge and UI rectangle directly and through an offscreen image inside clipped Editor UI. For the
simple linear control, half-opacity red over transparent yields premultiplied
`(0.5, 0, 0, 0.5)`; compositing that over opaque blue yields `(0.5, 0, 0.5, 1)`.
Compare numerical readback against those values before presentation encoding. Add alpha-zero
colored texels, overlapping edges and sRGB texture/tint controls to expose double premultiplication
or conversion. Specify format-dependent tolerances before execution and retain actual errors.

Use nonzero letterbox origin, fractional DPI, clipping and mirrored placement. The painted point,
pick result and accessibility rectangle must use the same admitted display mapping; outside
content must never clamp onto a control. Resize must invalidate the old mapping. A geometry-changing
effect must supply its mapping or explicitly disable precise picking. Run this through the real
GPU/Host path; screenshots alone cannot establish the numeric contract.

**Preserve.** Keep image/material ownership separate, linear composition distinct from authored
color encoding, and display placement distinct from producer identity. Carry these checks into
R8/R9a/R11 without redesigning the render graph or UI kernel.

## S4. Distinguish Independent Compilation From Independent Author Success

**Implemented and owning boundaries.** The
[module-consumer workspace](../../../tests/fixtures/module-consumer/README.md) has its own lockfile,
direct/renamed dependencies and public semantic tools. These establish real dependency/API
boundaries. They do not by themselves establish completion by an author unfamiliar with the
implementation. [Strategy](../../../STRATEGY.md) explicitly measures integration completion time,
private engine changes and programmable authoring; R12 already owns consumer integration.

**Minimum complete consumer and new acceptance.** Give one non-implementing developer a fresh
external project, current guides and a bounded task: add a persistent gameplay property and a
semantic editing tool, then edit, Play, Undo, Save/Reopen and use the currently supported Export
route. Record elapsed time, assistance, undocumented steps, engine patches and missing diagnostics.
The pass condition is completion through documented public contracts with zero private engine
edits and a reproducible project artifact. Assistance must remain visible; maintainer repairs
followed by a rerun are valuable evidence, not an unaided first-pass success.

**Preserve.** Retain automated independent consumers and source-built Rust workflows. This adds
human workflow evidence to E1/E2 and feedback F4; it does not require a new SDK, second language
or larger sample game before bounded API experiments.

## S5. Bind Platform And Performance Claims To Explicit Pass Conditions

**Implemented and owning boundaries.**
[Native qualification](../../verification/native-editor-qualification.md) carefully limits its
Windows/DX12 evidence; [Unix filesystem capabilities](../../../crates/nara_fs/src/platform/unix.rs)
expose unsupported operations. The [Rust/Play measurements](../../benchmarks/2026-10-20-rust-play.md)
retain small historical cohorts and distinct endpoints. Feedback F1/F3 already identify these
limits; R11/R12 and Strategy own subsequent qualification.

**Minimum complete consumer and new acceptance.** Reuse S4's project. For every claimed OS,
record backend, filesystem guarantee tier and actual Open/new-Save/rename/Export/Play/close
results. Include failed publication and physical cleanup. Custom UI additionally needs actual
IME candidate placement and assistive-technology behavior. Missing hardware or a skipped native
trajectory leaves that cell unqualified. A macOS spike is a separate capability decision, not
evidence of a broken supported release.

Before comparing implementations, bind numeric frame-tail, owner-occupancy, memory and
edit-to-result budgets to a named workload/machine. Pass requires both semantic completion and
those limits; faster compilation ending before correct presentation does not count. Separate
cold/warm builds, unchanged Restart, body edits and structural edits. Select sample counts and
independent repetitions for the chosen endpoint and percentile before execution; retain raw
distributions, counts and run-to-run variation. Report short edit cohorts as median/range unless
their tail estimate is supported. There is no universal sample quota in this handoff. These are
new acceptance recommendations, not measured results or adopted SLAs.

**Preserve.** Reuse existing harnesses and platform owners. Add the evidence to each selected
implementation unit; introduce no generic kernel, new release-validation script or broad audit.
