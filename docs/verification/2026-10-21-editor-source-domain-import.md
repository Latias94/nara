# Editor Source Domain Import Qualification

## Source and Environment

This record covers the source-backed metadata facet implemented under the
[Editor import plan](../plans/2026-10-21-editor-source-domain-import-plan.md), on top of
`fe37b1536fa9386d4650b3de6777b88e8e4dde55`. The implementation commit containing this record
binds the tested change. No unrelated checkout changes are included.

The development qualification used Windows 11 (`10.0.26300.0`), PowerShell 7 and
`rustc 1.99.0 (b940084d7 2026-09-28)`. Cargo commands ran serially with
`CARGO_BUILD_JOBS=1`, the existing repository `target`, and each workspace's locked dependency
graph. The external consumer workspace retains its own lockfile; its dependency additions
do not upgrade existing package versions.
Work began on October 21, 2026; final verification continued after midnight on October 22
(UTC+08:00). The record and plan retain their original start-date filenames.

## Qualified Workflow

The independently compiled text package supplies the same `consumer.text` parser used for
Check, source-backed Play and the earlier [Export qualification](2026-10-21-compiled-asset-domain-export.md).
A separate Editor adapter adds typed settings, explicit compiled registration and a real Dear
ImGui Import/Reimport panel. Ordinary project Open remains data-only.

The subsequent [ergonomics plan](../plans/2026-10-22-public-extension-ergonomics-plan.md) uses the
same independent packages. The source domain names its settings type; the Editor facet names
that domain without repeating its parser. A typed action helper derives the domain kind and
settings envelope. Complete consuming composition transfers both Host selection and panel
factories; the advanced trusted path retains the same selection for its replacement Open Host.
Its initial compiled lane has no generic source index and does not admit metadata import.
Panel-only attachment to an existing application rejects selected Host facets without changing
the workspace. Domain settings admission and diagnostics remain with the concrete parser.

The Host validates the primary and auxiliary sources through the domain's bounded reader,
revalidates their observations, and publishes canonical metadata through existing filesystem
receipts and cleanup custody. A later frame reports `MetadataPublished`; the temporary CPU
product is discarded. This result does not claim an installed Editor or runtime resource.

The external consumer and shared-owner tests exercise these observable outcomes:

- Import creates real metadata with typed settings and a stable ID.
- Reimport changes settings while preserving the ID.
- Invalid UTF-8 fails preparation and preserves the exact previous metadata bytes.
- A stale frame cannot start import or mutate metadata.
- A duplicate stable ID in another sidecar prevents publication.
- Closing during a blocked parser waits for physical retirement and publishes no metadata.
- A current-frame action during the successor metadata scan is rejected before starting work;
  after that scan, the same source is reimportable with its preserved identity.
- A failed successor scan cannot authorize import from its retained catalog, including while a
  manually requested retry holds the old Browser snapshot.
- An auxiliary source edited after the parser reads it is rejected before publication, preserving
  the accepted metadata byte-for-byte.
- Project replacement keeps the compiled contributor selection and starts with fresh operation
  state in the new session.
- Explicitly trusted compiled construction reports source indexing unavailable; after replacing
  that session with an ordinary authoring project, its retained facet performs real Import.
- Late panel attachment rejects a complete Host contribution instead of displaying its panel
  with no importer; the complete workspace remains unchanged.
- A terminal source-import view requests presentation through the public Shell update; subsequent
  unchanged, idle frames remain quiet.
- The resulting primary/auxiliary source closure passes strict Check, and actual headless
  gameplay reads `Hello world` through the game's compiled domain collection.

## Execution Evidence

| Check | Result | Evidence |
|---|---|---|
| Public external API characterization | Expected failure before implementation | `E0432`: `EditorSourceAssetDomain` unavailable |
| External asset-domain and Editor-panel regressions, including eight source-import/composition cases | 17 passed; 2 native cases skipped by the ordinary selection | nextest `47589488-769c-4935-816b-7670fe5bbd29` |
| Explicit Windows/DX12 source import after complete composition and presentation corrections | 1 passed; 0 skipped | nextest `64959f4e-8f69-4277-ab58-2fd2e023eae6` |
| Shared Host, Image/catalog and source-export regressions after final interface changes | 124 passed; 187 outside the selected cohort | nextest `39432f58-b995-4f53-ae11-eb407095097a` |
| Ordinary Editor library, composition and stateful panel regressions | 181 passed; 20 ignored native cases skipped | nextest `0a4649e6-a56f-4154-a1a6-86c65c5b316e` |
| Root facade with only `runtime-2d,serde,tooling` | Passed locked library check | No Editor/window feature union required |
| Root facade with only `runtime-core,tooling` | Passed locked library check | New source facet remains feature-gated |
| Root and Editor all-features library Clippy | Passed; warnings retained | `cargo clippy -p nara -p nara-editor --all-features --lib --locked` |
| Independent consumer workspace Clippy | Passed with warnings denied for workspace targets | `cargo clippy --manifest-path tests/fixtures/module-consumer/Cargo.toml --locked --workspace --all-targets -- -D warnings` |
| Both workspaces' formatting | Passed | Root and independent workspace `cargo fmt --all -- --check` |
| Root and Editor test-target compatibility | Passed compile-only check | `cargo check -p nara -p nara-editor --all-features --tests --locked` |
| Changed documentation links | 707 local links checked; none missing | Includes both new plans, research and this record |

The main-workspace Clippy result is not a warning-free claim. Existing lint debt remains across
the workspace; the existing large Editor error type also produces `result_large_err` at the
additional composition/trusted-construction facades. No lint suppression or unrelated error-type
redesign is included. The new source-import implementation paths reported no other Clippy
diagnostics in this run.

The explicit native command was:

```powershell
$env:CARGO_BUILD_JOBS='1'
$env:CARGO_TARGET_DIR=Join-Path (Get-Location) 'target'
cargo nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-editor-panel-module-consumer --test native_text_import --locked --run-ignored only --test-threads=1
```

The shared owner selection was `cargo nextest run -p nara --all-features --lib --locked -E
'test(project_host::runtime::tests::) | test(image_import::catalog::) | test(project_workflow::export::source_tests::)' --test-threads=1`.
It includes existing metadata conflict, last-good Image, failed-cleanup custody and physical-close
tests and five source-export checks. Its 187 filtered tests are not claimed as executed.

The gated source-refresh regression first failed against the initial implementation: nextest
`14864808-936b-4b11-b842-237ceabced5e` observed `Started` where the current-frame resubmission
required `Rejected(Unavailable)`. Routing source publication through the existing metadata-refresh
owner invalidates the old catalog immediately. A second deterministic regression held a manual
retry after a failed scan: `f630c6a2-05d9-4239-88fa-f116c72fd0c4` reproduced the same incorrect
admission through the retained catalog. The shared snapshot now withholds authority while refresh
or reconciliation remains outstanding. Both tests release their gates and prove subsequent
Reimport succeeds with the original identity. The final 124-case owner/source-export cohort passes.

The close regression keeps preparation gated until the public workspace Exit intent has actually
canceled source work, then releases preparation and waits for physical retirement. Requesting
Exit alone does not claim synchronous cancellation.

### Panic Teardown Diagnosis

The failing owner regression subsequently terminated with Windows status `0xc0000409`. ProcDump
and CDB captured the event before rebuilding its matching executable and PDB. The exception's
first parameter was `7` (`FAST_FAIL_FATAL_APP_EXIT`), and the symbolized stack led through
`RuntimeOwnerQuarantine::drop`, `std::process::abort`, and the Windows abort intrinsic during
thread-local destruction. This captured event is an intentional abort, not evidence of a buffer
overwrite. The underlying test assertion remained the incorrect source-refresh admission.

During panic unwinding, a Runtime owner enters the owning thread's quarantine. Its existing
contract requires driving retirement before that thread exits; dropping a nonempty quarantine
aborts rather than abandoning resources with owner-thread cleanup obligations. The new regression
wrapper captures a test panic, drives that public cleanup contract, and resumes the original panic
after retirement. The runtime-wide quarantine safety policy is unchanged. See
[`drive_runtime_quarantine`](../../crates/nara_app/src/runtime.rs) and the quarantine's `Drop`.

### Native Presentation Correction

The test starts a separately compiled executable on its main thread through
`EditorNativeComposition::run`, with DX12, isolated configuration and an unrelated working
directory. A floating contributed canary panel submits Import and Reimport through the public
frame/action interface, waits for later publication and metadata views, checks stable identity,
and requests normal Exit. After process completion, the test reads the actual sidecar settings
and runs Check/Play. The actual typed text-import panel is installed in the same composition.

An earlier native attempt (`999b873b-3eb3-4aee-9210-a269de00d02a`) timed out before establishing
the workflow. Its canary used a docked Bottom placement. Moving the canary to Floating, as in the
existing external native-panel fixture, made its painting independent of the active dock tab;
the subsequent explicit run passed. The failed run is not acceptance evidence.

After the metadata-refresh corrections, native run `ff99eb53-3530-4e9e-97b0-46ebc94528ea` reached
Import but timed out before the next panel result. Public-consumer run
`c1f2d383-0dc5-49ac-af3e-f186c3ef0b15` independently reproduced the cause: a terminal source view
changed without `EditorApplicationUpdate::actions_observed`, so native maintenance could settle
after painting without requesting another presentation. This regression closed its application
before raising the assertion and reported a normal test failure, without the quarantine abort.
The Shell now observes changes to the source-import view and its pending flag alongside persistence.
It also observes the shared metadata-work pending flag when the source view stays unavailable.
The external suite checks the terminal notification and three quiet subsequent polls; the final
native run completes Import, Reimport, normal Exit, and Check/Play.

The expanded public observation check additionally found a replacement handoff notification gap:
run `90abfc21-1acf-4b94-bb13-a6f324535c50` completed 14 cases and failed the replacement case
normally when the Shell changed its published project without reporting a change. The handoff
now observes actual candidate publication and restoration of a retained project. Focused run
`dcc119a6-4a8f-4e12-816f-6182db8f8af6` passes that workflow. Pending unchanged owners still do not
generate a handoff notification.

### Trusted-Lane Capability Correction

The first ergonomics qualification (`6a7999d5-6429-46a3-ad59-a2b811a796bf`) passed 16 cases and
timed out in its new trusted case. That test incorrectly waited for a Browser index in the
advanced compiled lane, which explicitly reports `project.source-index.authoring-project-required`.
It was not evidence that passing importer selection creates a generic authoring capability.
The test now asserts that initial unavailability, replaces the project through the ordinary
authoring flow, and then performs Import with the retained selection. The final 17-case external
cohort passes. Timeout reporting now closes the application before raising its assertion.

### Review

The initial full Codex source review covered correctness, project standards, tests,
maintainability, security, public contracts, reliability and adversarial sequences. Its two P2
findings were stale metadata admission and missing post-preparation auxiliary-change coverage;
the corresponding deterministic regressions and publication correction close both. Follow-up
review also checked the presentation edges, replacement behavior, typed settings/facet interface
and consuming composition. Its trusted-test capability mismatch was corrected as described above.
The review uses independent Codex agents within one model family, not cross-model corroboration.
Execution evidence belongs to the serial runs recorded in this report; reviewers did not run
Cargo or applications.

## Limits

This is focused development qualification on the recorded compiler and Windows/DX12. It does
not qualify the full native release group pinned to Rust 1.95.0, Linux/macOS, OS mouse/keyboard
delivery to the text panel, or monitor scanout. The canary proves native semantic-action routing
and later-frame observation; it does not click the panel's buttons through synthetic OS input.

Native/GPU import, automatic reimport, watchers, prepared cross-domain dependencies, cooked
products, dynamic code loading and hot reload remain outside this metadata-only contract.
The earlier Export record retains its own source revision and limits; this change does not
claim a new Export execution.
