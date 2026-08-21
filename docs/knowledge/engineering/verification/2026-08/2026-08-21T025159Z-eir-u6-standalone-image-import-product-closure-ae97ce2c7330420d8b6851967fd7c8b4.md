---
type: "Verification Evidence"
title: "EIR-U6 standalone image import product closure"
description: "Verifies the one-window Editor image import trajectory, safe last-good failure, Runtime closure exclusion, and final EIR lane closure."
timestamp: 2026-08-21T02:51:59Z
record_id: "ae97ce2c7330420d8b6851967fd7c8b4"
status: "verified"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-20-001-feat-editor-image-import-and-reimport-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "ec64e4c"
verified_by: "codex-root"
---

# Verification

Verified EIR-U6 at `ec64e4c`. The installed `nara-editor` now proves one representative native
Windows workflow from a missing PNG sidecar through canonical sRGB Import, UI Canvas use, Prefab
Save, same-root session replacement, referenced-only hydration, Linear Reimport, and a corrupt
source failure that preserves the exact prior presented Canvas resource.

This record closes the Editor Image Import and Reimport v1 plan. It does not activate watchers,
batch import, other formats, a durable import cache, project plugins, or project Rust execution.

# Result

The native canary runs through the ordinary Editor application and one real winit window. It only
captures UI image-resource facts when this private canary is active; normal Editor frames do not
copy the Canvas image table. The state machine admits exact frame-bound actions, checks canonical
metadata and stable identity through the published Host projection, observes real presented Canvas
facts, and completes only after the prior Linear resource remains byte-identity-equivalent at the
Editor presentation boundary following a safe `ImageProcessing` failure.

The same-root replacement publishes a fresh generic Authoring session. Session-local Browser and
Prefab state is not inherited, the saved Prefab is reopened from source, and the referenced image
is hydrated from the accepted authoring closure before Linear Reimport. Runtime remains
`ExternalExecutionHostNotImplemented`, ProjectContent remains `NotInstalled`, Play remains empty,
the guarded compile-error sentinels remain intact, and the temporary project does not acquire a
`target` directory. These facts prove that this product path did not cross Nara's project-execution
boundary; they are not presented as an operating-system-wide process trace.

# Evidence

- The final Windows PowerShell native trajectory completed successfully in one window, observed 35
  presented workspace frames, and finished within its 55-second absolute deadline:

  ```text
  $env:CARGO_BUILD_JOBS='1'
  New-Item -ItemType Directory -Force 'target\codex-temp'
  $env:TEMP=(Resolve-Path 'target\codex-temp').Path
  $env:TMP=$env:TEMP
  $env:NARA_EDITOR_IMAGE_IMPORT_CANARY='1'
  cargo run -p nara-editor -- .\tests\fixtures\generic-editor\image-import --profile desktop
  ```

  Exit code: `0`.

  ```json
  {"kind":"nara_editor_image_import_canary","version":1,"window":"single","presentation":{"frames_observed":35,"deadline_ms":55000,"completed_within_deadline":true},"workspace":{"predecessor":"authoring_ready","successor":"authoring_ready","same_root_replacement":true},"image":{"path":"textures/missing.png","metadata":"published_canonical","stable_id_consistent":true,"initial_import":"srgb","reimport":"linear"},"canvas":{"initial":"presented","reopened":"presented","linear_successor":"presented","corrupt_failure_retained_exact_last_good":true},"persistence":{"prefab_saved":true,"closure_hydrated":true},"failure":{"category":"image_processing","public_diagnostic_leakage":false},"runtime":"external_host_unavailable","project_content":"not_installed","play":"empty","project_rust":"not_executed","result":"ok"}
  ```

- Check and real Headless Run each compared baseline and successor projects around one canonical but
  unreferenced imported PNG. Both retained identical content digests, closure-plan digests, and
  ordered members, and neither closure contained the PNG or its metadata. The batch passed 2/2:

  ```text
  CARGO_BUILD_JOBS=1 cargo nextest run --features project-workflow --test project_workflow_check --test project_runtime_boot -E 'test(unreferenced_import_does_not_enter_the_checked_startup_closure) | test(unreferenced_import_preserves_the_headless_startup_closure)' --test-threads=1
  ```

- Export's existing acceptance proof passed 1/1 and binds a product observation to the exact
  startup-closure plan digest:

  ```text
  CARGO_BUILD_JOBS=1 cargo nextest run --features project-workflow-cli --bin nara -E 'test(product_observation_binds_the_exact_startup_closure_plan)' --test-threads=1
  ```

  `ProjectExportPlanReceipt` derives every startup-content member and its closure-plan digest from
  that receipt. Because the focused Check and Headless Run comparisons prove identical ordered
  closure facts before and after the unreferenced import, and the executable inputs are unchanged,
  the deterministic Export member plan is unchanged as well. This composes the existing Export
  proof instead of introducing a second build/export fixture.

- `CARGO_BUILD_JOBS=1 cargo check -p nara-editor --tests` passed after the final module
  simplification. `cargo fmt --all` and `git diff --check` passed before the implementation commit.
- AE7 boundedness and wake behavior remain anchored to verified owner evidence rather than being
  multiplied into the native product case: EIR-U3 record
  `e6cb2cf25cdf4321a15368421154678d` proves bounded catalog/decode/retention and one task owner;
  EIR-U4 record `d109ae1760a2463a862ffd4c0148caaf` proves the 16 ms pending-work deadline, terminal successor
  frame, and earliest-wake composition 3/3. The final native trajectory additionally observed 35
  real presentations before its deadline.
- Recipe, strict-load/reload, malformed metadata, duplicate identity, stale lineage, failed
  Reimport, same-generation choices/resources, and referenced reopen remain covered by verified
  EIR-U1 through EIR-U5 records `48b2b28ab1404017b7fe14ab2c335071`,
  `e6cb2cf25cdf4321a15368421154678d`, `d109ae1760a2463a862ffd4c0148caaf`, and
  `b2dfa722baf349b9a75cc324086024ac`.
- Boundary searches found no caller-authored settings hash, global color-space resource, ambient
  filesystem access in the Tooling/Host image-import boundary, generic importer/executor registry,
  or production `.nara/import-cache`. Exactly one `TaskPools::try_new` remains in
  `authoring_runtime.rs`. `FileIdentity` remains private Host proof and is not projected as public
  diagnostic text. The only import-cache match is the existing `nara_image` artifact-key owner test.
- Simplification removed the duplicate 500-line non-GPU product trajectory and its test-only public
  API, unified both native canaries on one private advance protocol, moved terminal evidence out of
  stage payloads, and reused the Canvas presentation fact directly. The canary remains split into
  orchestration, contract, and guarded-fixture modules. Final independent correctness review
  reported no findings or residual testing gaps.

# Follow-up

No Editor image-import successor is active automatically. Watchers, batch workflows, other image
formats, durable artifact publication, thumbnails, unrelated source mutation, external supervised
Play, animation/shader authoring, and extension hosting require separate evidence and explicit
registration.

# Citations

- `ec64e4c` (`feat(editor): prove standalone image import workflow`)
- `docs/plans/2026-08-20-001-feat-editor-image-import-and-reimport-v1-plan.md`
- `docs/architecture/adr/0115-editor-image-import-metadata-and-authoring-publication.md`
- `editor/src/platform/image_import_canary.rs`
- `editor/src/platform/image_import_canary/contract.rs`
- `editor/src/platform/image_import_canary/fixture.rs`
- `tests/project_workflow_check.rs`
- `tests/project_runtime_boot.rs`
