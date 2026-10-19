---
type: "Verification Evidence"
title: "N05 safe plugin failure facts and invocation context"
description: "Checked App plugin facts, actual invocation identity and retained primary/shutdown diagnostics through public product and Editor failure paths."
timestamp: 2026-10-19T17:09:51Z
record_id: "d01f7391b84348a8827a0daa760dd1a4"
producer_id: "codex-root"
run_id: "plugin-error-20261020"
related_plan: "docs/architecture/authoring-interface-ergonomics-design.md"
git_branch: "refactor/foundation-contract-hardening"
git_commit: "40bcdc6"
---

# Verification

This is the separately authorized App plugin-error portion of N05 in the
[authoring design](../../../../architecture/authoring-interface-ergonomics-design.md). The baseline
is `cc69ea8`; the behavior commit is `40bcdc6`. S3's component-domain
qualification remains historical evidence with its original scope. This work does not reopen
U1-U15, start a Goal, add tile tools, push, open a PR or claim a release.

Two read-only probes examined the lifecycle owner and product adapters. An App implementation
lane and a public-consumer lane worked on disjoint files; root integrated the bridges and ran all
Cargo commands serially. Independent adversarial review returned no findings after the invocation
and delayed-close corrections below.

# Result

Windows 11, Rust 1.95.0, shared `target`, `CARGO_BUILD_JOBS=1`. No dependency or lockfile change is
needed: App owns static plugin facts without depending on the diagnostic runtime. Existing
feature-dependent unused/dead-code warnings are not part of this correction.

| Command / cohort | Actual result | Nextest run |
| --- | --- | --- |
| `cargo +1.95.0 nextest run -p nara_app --locked --test-threads=1 --no-fail-fast` | Final 121 passed across six binaries, including all eight domain/invocation scenarios | `b5685d14-5c93-4b3c-9790-62c35dc95138` |
| App, Image, Tasks, Asset Watch and Transform: `nextest run -p nara_app -p nara_image -p nara_tasks -p nara_asset_watch -p nara_transform --lib --test plugin_composition --test plugin_domain_errors --locked --test-threads=1 --no-fail-fast` | 254 passed; predates the direct-seal correction, which the final App run above supersedes | `b4da33c0-461f-46e4-b748-124c1c66106c` |
| Root: `nextest run --no-default-features --features project-workflow,tooling,runtime-ui,presentation-2d --lib --test project_runtime_boot --test workspace_play_runtime --test plugin_composition --locked -E 'test(project_host::runtime::tests::) \| binary(project_runtime_boot) \| binary(workspace_play_runtime) \| binary(plugin_composition)' --test-threads=1 --no-fail-fast` | Final 133 passed; 85 excluded by the selection | `4be5535e-73ac-452d-b40c-31138c564221` |
| `nextest run -p nara_reflect -p nara_gameplay --lib --locked --test-threads=1 --no-fail-fast` | Final 90 passed, including the direct-seal consumer migrations | `c3cebcc0-a793-4355-b790-4630f30ab6fb` |
| `cargo +1.95.0 check --workspace --all-features --all-targets --locked` | Passed after the desktop cfg import correction, 1m 56s | Not a nextest run |

The table's cohorts overlap and must not be summed as unique coverage. All commands use
`cargo +1.95.0`. The final desktop cfg import does not change the already executed App, Headless
or compiled Editor session paths. No product changes followed these final passing checks.
Only module-declaration placement and documentation were tidied during final integration.
`cargo +1.95.0 fmt --all -- --check`, staged/unstaged `git diff --check` and affected local
Markdown links pass. The HTML reports remain outside `docs` in the user's Downloads directory;
their native nesting, IDs and local fragments are checked separately.
The three updated design/index/ledger documents have 32, 31 and 524 valid local link targets;
the ADRs, guide and this evidence have ten. Both HTML reports pass nesting, unique-ID and
fragment checks. Engineering-memory validation and derived-view checks pass with pre-existing
historical metadata, absolute-path and legacy-plan warnings preserved. Registration has a completed
successor and no active work remains in this lane.

# Evidence

`PluginDomainError` validates static code/summary with App's existing runtime-fault policy, now in
one private module. It replaces `PluginPrepareFailure`; hooks use `PluginError::Domain`.
Preparation remains distinct from preflight/build/finish/shutdown. App records actual invocation
identity instead of trusting a legacy error's self-reported plugin/hook. Local Bevy's Plugin hooks
are infallible; copying that surface would not satisfy Nara's already accepted fallible lifecycle
and retained-close contract.

The final tests cover:

- Invalid, oversized and sensitive-shaped metadata; shared runtime-fault validation remains covered.
- A first typed preflight rejection through direct App, retained plan and runtime construction:
  correct invocation, configurable App, no installation or poison, and no rejected-plugin shutdown.
- A later preflight wrapper, build and finish failure: domain identity survives source wrapping;
  primary failure stays separate from shutdown failures; acquired owners close exactly once.
- Actual plugin/hook identity when legacy payloads falsely claim a different owner or phase.
- Consuming `App::seal` with the second plugin failing finish: returned `PluginCommitError` alone
  preserves the actual plugin/hook/domain, and both installed plugins shut down once.
- Ordinary runtime retirement: borrowed shutdown facts survive ownership transfer and repeated
  retirement drives without a second failure collection or raw strings in close evidence.
- Actual `ProductRecipe` Headless Run failures in every hook, primary plus secondary domain codes,
  and public Debug/JSON reports that omit dynamic error text and path canaries.
- Compiled Editor Play start rejection without document/runtime publication, retained cleanup and
  retry, failed restart after terminal shutdown, and immediate once-per-generation domain reporting
  while a separate close participant keeps physical retirement incomplete.

Product lowering revalidates App facts into diagnostic primitives, preserves generic context in
`project-context-code`, and attaches bounded actual invocation or preparation-phase fields. It
never parses error Display text. Check, Export and ordinary data-only Open do not execute extra
factories or hooks to obtain diagnostics. Existing lifecycle, publication and retirement authority
remain with their current owners. The product bridge was extracted into a private module rather
than growing the Host's main runtime file further.

Review and verification corrected three evidence-loss cases: a first direct preflight error lost
its envelope; consuming `App::seal` lost the failing finish invocation; and Editor originally
waited for physical close to finish before publishing terminal shutdown facts. The first two now
return the existing `PluginCommitError`; Editor lowers the existing report once per runtime
generation. Advanced `instantiate`/`into_error` convenience projections explicitly document their
loss of invocation and cleanup evidence.

Intermediate checks were not accepted as completion evidence:

- The initial minimal root test feature selection omitted presentation types used by the existing
  integration binary. The final root command includes `presentation-2d`.
- Run `46c1f6dd-064c-4db6-9dfb-229d85788c6f` passed 132 tests and failed the new delayed-close test:
  its helper submitted RetryClose again before driving the accepted request. A frame drive fixes
  the test protocol; the final 133-test run above passes.
- The first all-feature check exposed a missing desktop-only import for the new shutdown
  diagnostic adapter. The corresponding cfg import was added before rerunning the workspace gate.

# Follow-up

The focused qualification does not extend S3, native Editor control, GPU, Linux/macOS or full
release claims. The desktop-only adapter receives build coverage; these public failure scenarios
execute through Headless Run and compiled Editor session APIs. Broader async work, code-currentness,
multi-document ownership and G1-G9 remain triggered by their documented concrete consumers.

# Citations

- [ADR 0010](../../../../architecture/adr/0010-plugin-lifecycle-dependencies-and-failure.md)
- [ADR 0068](../../../../architecture/adr/0068-global-resource-budgets-metrics-and-diagnostic-privacy.md)
- [Plugin domain and invocation scenarios](../../../../../crates/nara_app/tests/plugin_domain_errors.rs)
- [Public Headless Run](../../../../../tests/project_runtime_boot.rs)
- [Compiled Editor Play](../../../../../tests/workspace_play_runtime.rs)
- [Product lowering](../../../../../src/project_host/runtime/plugin_diagnostics.rs)
- [Product recipe guide](../../../../guides/rust-product-recipes.md)
