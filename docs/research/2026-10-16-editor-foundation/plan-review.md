# Editor foundation plan review

Date: 2026-10-16.
Scope: [Editor Foundation and Frontends](../../plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md) and [ADR 0118](../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md).
Source baseline: `072a0da`.
This is planning evidence, not runtime, frontend or native-platform acceptance.

## Coverage

Three independent research lanes examined the pinned Dear ImGui tag, mature editors and Nara's actual module boundaries.
The dependency lane subsequently checked current official releases and migration information after the user explicitly allowed breaking upgrades.
Six fresh independent Codex document reviewers then read the written plan and ADR without peer findings.
No external-model review was used.

| Reviewer | Outcome | Disposition |
|---|---|---|
| `editor_plan_coherence` | One confirmed dependency-diagram mismatch | Applied C1 below |
| `editor_plan_feasibility` | No actionable finding | Confirmed template coverage and private GPU integration requirements |
| `editor_plan_scope` | No finding | Existing semantic owners preserved; no generic framework required |
| `editor_plan_product` | No finding | Refactor serves the integrated Rust-first 2D workflow |
| `editor_plan_design` | No finding | Lifecycle/flow coverage adequate for this architecture plan; native proof remains future work |
| `editor_plan_adversarial` | No finding | Candidate failure preserves useful boundaries; adoption and full migration remain separate |

No selected reviewer failed.
Security-specific review was not separately activated: this plan retains data-only project Open and admits no arbitrary project-code loading, dynamic ABI or new external access policy.
Implementation review must still examine actual resource/input/FFI changes when they exist.

## Finding and correction

C1, P2, confidence 100: the implementation diagram allowed U7 to follow the U6 gate without showing U3, while the U7 dependency declaration explicitly required U3.
The root checked both passages and added `Structure --> Port`, preserving the text's authoritative dependency and avoiding a second port of obsolete component projections.
The corrected graph now agrees with every declared prerequisite.

## Targeted strengthening

Feasibility research confirmed that the ordinary value validator permits some missing fields under existing optional/default semantics.
The plan now explicitly requires complete creation-template coverage inside that same owner without weakening ordinary record decoding.
This clarifies the existing R2/U2 completeness requirement rather than adding a second validator system.

The plan now explicitly lists `nara_render_wgpu/src/editor_execution.rs` and the existing viewport/input modules in U5.
The private GPU seam must permit the backend's same-lineage retained handles, including Instance/Adapter, while panel/tooling APIs remain opaque.
The synchronous backend path, device-generation recovery obligation and compatible feature-profile checks are explicit.
These edits make the source study's concrete requirements visible where execution starts.

## Dependency scope refinement

The user explicitly permitted necessary cross-version dependency upgrades.
R11/KTD10 and U9 now make version selection/caller migration a prerequisite for U1/U2; existing versions are reproducible research baselines rather than design ceilings.
The same feasibility reviewer independently checked this delta against the current manifests/lock and official package sources and returned no blocking finding.
The selected families must remain coherent, with actual MSRV/feature evidence and migrated consumers.
The [dependency study](dependency-cohort.md) distinguishes independent Bevy ECS/reflection MSRV from the complete engine, WGPU platform fixes, stable Winit from beta, and the existing glam type boundary.

## Confidence and final state

The external-source-shaped plan received a risk-weighted confidence check.
Initial KTD and implementation-unit gaps were template completeness, retained GPU handles and the affected WGPU execution owner; the corrections above close them.
Final requirements/research/technical-design sections have no checklist gap; KTDs, units, impact and risks have only their critical-section bonus, below the two-point strengthening threshold.

All R1-R11 requirements map to implementation and verification; U9 then U1-U8 have stable dependency IDs and concrete affected tests.
No launch-blocking planning question, unapplied correction or reader decision remains for U9.
The U6 native qualification remains an execution gate for U7, not a planning result.
The completed predecessor plans were not modified.

Applied changes: one independent review correction and the targeted source-grounded strengthening above.
Proposed fixes: zero. Decisions remaining: zero. FYI findings: zero.
Documentation verification consists of diff/whitespace, local link and authority-map checks; Cargo and native probes were not run for this documentation-only delivery.
