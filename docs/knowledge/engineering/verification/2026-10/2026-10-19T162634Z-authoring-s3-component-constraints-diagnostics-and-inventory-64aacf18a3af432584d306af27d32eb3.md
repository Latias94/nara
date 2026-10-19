---
type: "Verification Evidence"
title: "Authoring S3 component constraints diagnostics and inventory"
description: "Shared interval contracts, safe domain facts, explicit UI evolution and independently compiled component authoring evidence."
timestamp: 2026-10-19T16:26:34Z
record_id: "64aacf18a3af432584d306af27d32eb3"
producer_id: "codex-root"
run_id: "authoring-s3-20261020"
related_plan: "docs/architecture/authoring-interface-ergonomics-design.md"
git_branch: "refactor/foundation-contract-hardening"
git_commit: "2ca6771"
---

# Verification

S3 / E6 then E5 of the [authoring design](../../../../architecture/authoring-interface-ergonomics-design.md)
completes component declaration through explicit evolution. The baseline was `dff3490`; the final
behavior source is `64e8484` (shared component contracts/inventory) followed by `2ca6771` (UiText
successor, shipped fixtures and native interval qualification). `33a9a8b` then refreshes the remaining
sample/fixture dependency graphs without changing behavior or external versions. Subsequent closure
is documentation-only.
This work does not reopen U1-U15 or start another Goal, tile tools, push, PR or release.

The implementation uses separate Reflect modules for numeric intervals, safe domain facts and
inventory callbacks. Scene and tooling retain their diagnostic adapters; the existing provider,
ProductRecipe and SchemaContribution owners retain installation authority. Three implementation
lanes and independent range/adversarial, diagnostic/security, API and final correctness reviews
were integrated by the root verifier. Final reviews reported no actionable findings.

# Result

Windows 11, Rust 1.95.0, shared `target`, `CARGO_BUILD_JOBS=1`; all Cargo commands were serialized.
The final graphs are locked. Lockfile changes add the existing diagnostic workspace edge to
Reflect and the root facade to the independent component fixture; no external package versions
changed. Existing feature-dependent unused/dead-code warnings remain outside this scope.

| Command / cohort | Actual result | Nextest run |
| --- | --- | --- |
| `cargo +1.95.0 nextest run -p nara_reflect -p nara_scene -p nara_ui -p nara_tooling --features nara_reflect/serde,nara_scene/serde --locked -E 'not binary(derive_ui)' --test-threads=1 --no-fail-fast` | 537 passed; trybuild run separately | `eb86ae8b-0b9e-452c-8efe-c7a1552ed041` |
| Reflect, `--features serde --test numeric_intervals --test derive_ranges --test derive_ui --locked --test-threads=1 --no-fail-fast` | Final 13 passed, including strict compile-fail fixtures without overwrite, RON/JSON extremes, reference traversal and macro scope/shadowing | `50fa7e41-6710-4f79-9b55-0a3d6284ccf1` |
| Root, `--no-default-features --features project-workflow,tooling --test component_domain_diagnostics --locked --test-threads=1 --no-fail-fast` | 1 complete scenario passed; two native domain codes through actual Check and trusted compiled Editor | `2bfe9ac9-d08d-4241-b0c0-b86d0da1fd7a` |
| Independent module workspace, `-p nara-scene-module-consumer --test component_inventory --locked --test-threads=1 --no-fail-fast` | Final 3 passed: atomic admission, ordinary data-only authoring/persistence, explicit native migration and strict Check | `e611e061-3aa3-459b-aa4b-9adc2a129828` |
| `cargo +1.95.0 nextest run -p nara_scene --features serde --locked --test-threads=1 --no-fail-fast` | Final 210 passed after removing the duplicate early value guard | `dc490118-7250-488b-be00-0b40192c1760` |
| UI lib, `--locked -E 'test(component_evolution_tests::)' --test-threads=1` | Final 3 passed; 82 unrelated tests excluded | `2151efea-742a-4b9a-ac13-1206363157c0` |
| Root-only renamed dependency workspace, `cargo +1.95.0 nextest run --manifest-path tests/fixtures/derive-dependencies/renamed-root/Cargo.toml --lib --locked --test-threads=1` | Final 1 passed; only dependency is renamed `nara`, without direct Reflect | `bba1544c-3c77-4912-8509-b55b1daf2984` |
| `cargo +1.95.0 nextest run -p nara-editor --lib --test application_contract --locked --test-threads=1 --no-fail-fast` | 172 passed; 18 ignored native tests excluded | `2ccfe91d-c3f0-4288-839f-70f2268ee2be` |
| Editor lib, ignored selection of field draft/IME and Canvas persistence trajectories below | Both executed and passed on actual DX12 adapter; 2 tests, 6.275s | `99c40b7b-a600-41dc-ab78-a81a0511a7b9` |
| Reference-game `authoring_bundle --write`, then `--check --locked` through its existing Cargo binary | Passed; exact previous current UI catalog equals the new predecessor; all other owners unchanged | Not a nextest run |
| Full `cargo +1.95.0 metadata --format-version 1 --filter-platform x86_64-pc-windows-msvc`, then `--locked`, for the six remaining independent Reflect graphs | Passed; exact external package/version sets unchanged; all tracked Reflect lockfiles contain the new edge | Not a nextest run |
| Root `cargo +1.95.0 fmt --all -- --check`, focused independent-package fmt checks, `git diff --check`, affected Markdown/HTML link checks | Passed; no runtime changes during documentation closure | Not a nextest run |

Focused rows overlap earlier cohorts; these counts must not be added as unique test coverage.
Independent workspaces reuse the root target through `CARGO_TARGET_DIR`. The external component
command uses `--manifest-path tests/fixtures/module-consumer/Cargo.toml`.
Engineering-memory validation and derived-view checks pass, with existing historical metadata
and absolute-path warnings retained rather than rewriting unrelated records.
The final graph-only refresh covers collect-game, the two presentation slices, public-prelude,
runtime-runner and schedule-extension. Runtime-runner also records its resolved existing Scene/RON
edge; these metadata checks do not claim new execution coverage of those consumers.

The native command uses `cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only
--test-threads=1 --no-fail-fast`, with `WGPU_BACKEND=dx12`, `DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1` and
this exact selection:

```text
test(native_field_draft_focus_and_ime_survive_unrelated_document_revision)
| test(native_canvas_widgets_pick_move_resize_and_save_reopen_real_source)
```

# Evidence

Finite numeric intervals retain signed/unsigned/F64 endpoint kinds, inclusive/exclusive bounds
and exact large integers. Admission rejects wrong kinds, out-of-domain endpoints and intervals
with no representable value. F32 checks materialize native F32 then widen for comparison without
rewriting saved bytes. Defaults, templates, data-only validation and native preflight share this
rule. Absence preserves old fingerprints; present metadata is closed to unsupported readers;
addition/change/removal requires component and owner succession. Derive delegates literal typing,
aliases, suffixes and overflow to Rust rather than implementing a type resolver.

Domain errors carry checked static code/summary and an optional stable root field. Scene and
tooling publish the field only with admitted schema membership; unknown or missing schema omits
the claim. Tests cover two extension codes, component/owner provenance and canary redaction for
dynamic values, paths and map keys. Free-form errors retain their private fallback.

The independent component fixture uses one current inventory in a version-two owner, preserves
the generation-one membership and adds Armor only to the current inventory. The same contribution
works through direct App and the compiled product plan. A callback failing after candidate mutation
publishes no aggregate changes and can be retried. Ordinary Open executes no project code; public
actions prove Create/Edit/Undo/Redo, rejected bounds with unchanged revision/history, actual Save,
strict Check, Reopen of saved data and explicit close retirement. Valid historical bytes remain
unchanged during native migration; invalid values reject without clamping.

UiText advances to component version 3, owner generation 3 and native binding/codec/migration
version 3. Its shared native size constraint is F32 `(0, 256]`. The exact generation-two fingerprint
is frozen as `d7468da88c3ad3ca5e92d1a02e00e2a3d091037773abcec0bb552f19da626238`.
The reference game's generated bundle retains that complete predecessor, and its eight UiText
records change only version 2 to 3. The image-import fixture changes only the UI owner; its
intentional compile-error project files were not executed. Ordinary reference-game Open and the
real Canvas Save/Reopen trajectory pass with the updated shipped content.

The native field trajectory enters 31, proves rejection retains input/error and history, then
corrects to 30 and proves one exact accepted result before the existing focus/IME sequence.
It uses hidden native windows and owned Context input through real production presentation paths.

## Corrected Probes

Initial focused run `d99a1577-17de-4198-8df0-ac1a5f5ea4e2`, 45/48 passed, exposed noncanonical
inventory schema order and a RON representation problem; one assertion also incorrectly expected
a non-reference visitor to validate unrelated leaves. Inventory output now canonicalizes at its
own validation boundary. The visitor regression now contains an actual entity-reference record.
RON's buffered nested tag path needed private struct-variant wire enums for numeric bounds while
the public API remains `Included(T)` / `Excluded(T)`; JSON keeps the same explicit bound/value
shape. The isolated RON probe `b014f543-7378-40a8-9f56-ea3dcd7252cb` was a failing diagnosis,
not passing evidence. Final strict round trips cover signed and unsigned extreme endpoints.
Compile-fail snapshots were initially generated with TRYBUILD overwrite, then verified strictly
in the final 13-test run above.

Initial external run `0236e40b-ebf6-4d40-98d2-ca10cd068de5` passed 2/3 but the ordinary Editor
test could not find the promised range diagnostic: the old preservation-safe field guard rejected
the value before shared component validation. The owner now checks mutation eligibility there and
validates the complete local candidate during classification before returning Prepared state.
Scene/Prefab format, schema, reference and unavailable-manifest checks still precede history or
document publication. Independent review traced both paths; the final 210 Scene tests and external
3-test workflow pass. The aborted assertion in that probe is not a product memory-corruption claim.

API review also found child-module-relative type resolution and caller-owned `Ok` shadowing in
the first inventory macro. Its intentionally narrow grammar now requires imported/aliased type
identifiers, uses explicit `super` references and fully qualified Result constructors. Nested
import-alias/shadowing and root-only renamed-dependency tests execute successfully.

# Follow-up

S1-S3 are complete within their evidence scope. App plugin free-form error identity is not covered
by the component bridge repair, and arbitrary cross-field/native invariants remain codec-owned.
Data-only acceptance does not assert those native invariants. The macro is mechanical convenience;
its raw callback is not a new transaction, historical catalog generator or registration authority.

D6 and G1-G9 remain tied to named consumers or measurements. No universal async service, job
scheduler replacement, inline property framework, dynamic plugin ABI or tile tool was introduced.
Native Linux/macOS, OS IME delivery/candidate-window placement, scanout, accessibility, full export
and full release qualification were not run or newly claimed. Existing earlier evidence keeps its
original source and platform scope. Unrelated `myeasylog.log` and `vendor/` remain untouched.

# Citations

- [Persistent component guide](../../../../guides/persistent-components.md)
- [ADR 0045](../../../../architecture/adr/0045-component-schema-capability-metadata.md)
- [ADR 0081](../../../../architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
- [Authoring HTML report](D:/Frankorz/Downloads/nara-authoring-interface-design-2026-10-19.html)
- [Foundation HTML review](D:/Frankorz/Downloads/nara-foundation-next-stage-review-2026-10-19.html)
