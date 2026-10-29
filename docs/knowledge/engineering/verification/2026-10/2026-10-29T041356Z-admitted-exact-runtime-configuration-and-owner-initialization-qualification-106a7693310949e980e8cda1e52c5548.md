---
type: "Verification Evidence"
title: "Admitted exact Runtime configuration and owner initialization qualification"
description: "Qualifies source-independent admitted Runtime values, exact duration records, final profile invariants and actual clock diagnostics ownership through public and native consumers."
timestamp: 2026-10-29T04:13:56Z
record_id: "106a7693310949e980e8cda1e52c5548"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "refactor/runtime-configuration-ownership"
git_commit: "a5563ca"
---

# Verification

Continue the full active foundation Goal and ordered Runtime delivery plan from `main@3daf069`.
This unit qualifies the configuration prerequisite, not processed Scene/catalog/source-free
execution. Rust 1.95.0, Windows 11, PowerShell 7, serial Cargo, `CARGO_BUILD_JOBS=1` and the existing
root `target` are used, including the independent Reference Game workspace. No remote publication
or vendor/reference mutation. Implementation: `552fbb0` (diagnostics), `a5563ca` (configuration).

# Result

`RuntimeConfiguration::resolve` validates raw values before constructing immutable
`EffectiveRuntimeConfiguration` fields through existing section validation/domain lowering.
Source candidates retain real manifest/profile/source lineage and full source settings separately.
`RuntimePlan::settings()` exposes only admitted runtime values.

`RuntimeTimeConfiguration` exposes `Duration` and persists strict integer `{secs,nanos}` maps.
Unknown, duplicate, missing, noninteger, overflowing and positional inputs refuse. Nanoseconds at
or above one billion refuse before normalization. Zero durations fail resolve at their exact
field. Export copies exact durations, including values outside floating-point seconds' image and
`Duration::MAX`. TOML's integer range does not become a Runtime admission limit. Source manifests
retain readable floating-point seconds and share timing validation and the domain constructor chain.

Profile overlays finish before preset invariants. A base server preset no longer leaks its forced
window/catch-up values into a final non-server profile. Named headless/server and final server
rules remain. Source-relative input maps have no processed representation yet: projection refuses
a remaining path; a profile can explicitly clear it. Extra public task/diagnostic policies outside
their current raw codecs refuse rather than truncate or disappear.

A private canonical initial-time plugin installs clocks before ordinary product build/finish.
Configuration participates in plugin identity without elapsed/tick/debt counters. Materialization
no longer overwrites clocks afterward. Six Reference Game construction paths in five files remove
redundant late injection and exercise the plan directly. Configured Diagnostic definitions encode
all runtime/pressure limits and retention and configure actual profile-owned storage across
duplicated default-group occurrences.

# Evidence

| Scope | Actual result |
| --- | --- |
| Configured Diagnostic all-feature library | 55 passed; build 6.05s, run 2.086s |
| Final exact-duration Project library | 77 passed, no skips; build 7.82s, run 2.653s |
| Final configuration/composition/runtime/Host boundaries | 99 passed, one default ignored case skipped; build 1m42s, run 64.131s |
| Earlier root library and Runtime/content/Workspace/Play cohort | 956 passed, five default ignored cases skipped; build 2m33s, run 331.635s; before exact-duration wire correction |
| Final native Editor Windows/DX12 critical group | All three executed/passed; 283 outside-selection skips; build 2m58s, run 6.026s |
| Independent Reference Game all-feature eight-binary cohort | 57 passed, no skips; build 9m22s, run 29.229s |
| Diagnostic all-feature/all-target correctness Clippy | Passed; 6.61s; existing warnings |
| Project/root/Editor all-feature/all-target correctness Clippy | Passed; 2m38s; existing warnings |
| Root no-default `runtime-2d,serde,tooling` library check | Passed; 17.53s; existing feature warnings |
| Root no-default `runtime-core` library check | Passed; 8.68s; existing feature warnings |
| Project compile-fail doctests | Both passed; build 7.12s, run 0.49s |
| Root/Reference Game formatting and intended diff whitespace | Passed |

Nextest: Diagnostic `43ac7331-c260-4e8a-a041-4d299bac8e71`; final Project
`28ddce31-a215-4d06-84fb-a0f948435d0b`; final root
`1b445d69-cdff-4497-b0f5-6d274330627f`; earlier broad root
`e28ee8db-ce16-42fa-bc2e-fb78dce65aaf`; native
`b3e30845-3413-45eb-bf72-6803cdcd2020`; Reference Game
`fcc6cfaf-57c2-4750-9452-7272aa514b20`.

Final root selects `runtime_configuration`, `project_composition`, `plugin_composition`,
`project_runtime_boot` and `project_host_boundary`, all features, locked, one test thread.
It reruns the behavior affected by exact duration/shared validation. The earlier 956-case run is
not a full final-source rerun. Earlier 72-case Project and 78-case composition cohorts also passed.

Native selects all three critical names in the qualification procedure:
`native_resume_redraw_rebases_paused_idle_before_the_owner_drive`,
`native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime` and
`native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts`.
This is actual hidden-window Windows/DX12 execution, not monitor scanout or another-platform proof.

Reference Game selects `first_wave`, `desktop_flow`, `desktop_parity`, `desktop_render`,
`ui_product`, `ui_authoring_roundtrip`, `project_manifest_ingest` and `plugin_composition`, all
features. It exercises the cleaned clock consumers, rendering probe, UI, profile admission and
source editing/reload; it does not qualify a source-free catalog.

## Review And Rejected Approach

Worker `01a18592-b428-7831-a356-cfd22862660a` implemented Project. Independent reviewer
`01a18593-49d2-7a61-ac6b-3547f1f0c2b7` identified late time initialization, missing clock
fingerprint and premature base-server invariants; all are corrected. Its final frozen-source
review covers strict duration admission, shared rules, root/source ownership and all six
Reference Game paths, with no remaining actionable P1/P2. Worker
`01a18596-d0de-7733-8366-6c3fa24b8228` implemented Diagnostic configuration. Writers froze before
format/build. Reviews are source evidence, not test execution.

The initial Runtime record reused source float seconds. Export tried standard conversion and two
adjacent floats with exact lower-back comparison. Success was lossless, but candidate coverage
for all valid source values was unproved. Exact Runtime data should not inherit that inverse-search
gate. Integer records remove the search, refusal code and export `expect`; tests include
`8_388_608s + 1ns` and maximum durations. No compatibility reader or migration guide.

The first locked Diagnostic invocation required a lock update for Project's added dev
`serde_json` dependency. Offline resolution changed only that edge, with no dependency upgrade.
Subsequent recorded checks use `--locked`.

# Follow-up

Keep the complete foundation Goal active. Next implement Scene-owned cook/wire/admission and
shared preparation, then root verified-base catalog admission and codec-identity checks, then
source/cache-inaccessible startup and later Scene acquisition through the same base. Do not
disguise flattened output as a plain source candidate, fabricate manifests/receipts, or bypass
content residency with direct sources. Preserve containing identity, complete typed Prefab
provenance and strict native/data binding through existing owners.

The retained catalog study recommends a private source/runtime materialization view, deterministic
mapping of full logical occurrences and Reflect-owned reference rewrite. Root needs distinct
Source/RuntimeBase admission proofs. Expected digest, target and recipe/Schema come from the
executable/Host, never the catalog's self-supplied expectation. Missing/corrupt later members
refuse while preserving last-good; no source/cache fallback. Do not redo that study or report its
design as implemented.

Build-selection separation, processed input maps, CAS/group publication, remote delivery, patch/DLC
and Nara-owned Editor UI remain open. Actual filesystem unlink and reviewed per-member tool
reclamation remain implemented; imported project sources remain outside installation disposal
authority. This unit changes none of their platform claims.

# Citations

- [Ordered delivery plan](../../../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md)
- [Full foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Settings authority](../../../../architecture/adr/0035-project-manifest-and-runtime-settings-authority.md)
- [Public configuration guide](../../../../guides/rust-product-recipes.md)
- [Runtime configuration owner](../../../../../crates/nara_project/src/runtime_configuration.rs)
- [Public initialization consumer](../../../../../tests/runtime_configuration.rs)
- [Native qualification](../../../../verification/native-editor-qualification.md)
- [Artifact/project ownership and retained unlink](../../../../architecture/source-tool-artifact-ownership-design.md)
