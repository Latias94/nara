# CI And Portable Wasm Repair

Status: In progress alongside the selected locality follow-up. Requested after repeated hosted
failures; this does not add browser execution to the first production proof.

## Confirmed Failures

Recent hosted runs `39398286157`, `39385253214` and `39375892490` fail independently on Linux
handle-bound directory enumeration, stale reference-game retirement/lease expectations and
standalone module-consumer formatting. The `8afbe42` run repeats the formatting failure.
The first local portable wasm check fails in `getrandom 0.3.4` before engine library checking.

After `fce2e70`, hosted run `39409199063` passes the portable wasm job and Linux module consumer.
Linux reference-game advances through the original enumeration failure and exposes the next
failure: its schema-bundle CLI requests strict untrusted replacement for an explicit local
`--write`, although Linux correctly reports name-bound publication.

The Linux root cohort also reaches seven failed-cleanup recovery tests whose terminal assertions
require Windows exact-handle temporary discard. Unix deliberately preserves recoverable orphans;
it cannot satisfy those assertions after the injected failure releases. Keep these seven cases
Windows-only, with the portable close/admission case and Unix fail-closed orphan contract retained.
The focused Windows cohort passes all eight tests (`318c24ec-4f50-4f3e-a475-1de0fcb2a1f9`); the
Linux test target cross-check also passes. These local results do not substitute for hosted CI.

Run `39414102233` at `da76a21` passes the three portable wasm checks and the Linux independent
consumer job. Linux reference-game passes 83 default tests, then exposes a production issue in
all-feature external Play/Stop: probing every `/proc` PID through `rustix::getpgid` encounters a
valid zero process-group result and aborts on its nonzero-ID assertion. The same upstream API
can construct an invalid nonzero value in optimized builds; upgrading or selecting its libc
backend does not resolve [rustix issue 1696](https://github.com/bytecodealliance/rustix/issues/1696).
Use a narrow signed-result OS query that distinguishes failure, zero and positive group IDs.
Keep the owned positive group, unreaped leader reservation, descendant observation and physical
reaping contract. This production defect must be repaired rather than excluding its test.

The same run's Linux root suite passes the previously repaired custody admission case, then
exposes a nondeterministic identity-replacement fixture. Removing the old file before recreating
identical bytes permits the filesystem to reuse its inode, as ADR 0070 explicitly allows. Retain
the old object under another name before creating the replacement, so the test actually supplies
a different live native identity. Keep the original identity-rejection and unchanged-workspace
assertions. This fixture correction does not upgrade source-index values to persistent identity
or silently change the admission contract.

Local qualification of the signed query builds its exact private source for Linux musl in debug
and optimized modes: three tests pass in each actual WSL execution. A separate temporary caller
of the same source scans the real `/proc` and safely observes two zero-group records. Six existing
Windows supervised-process capture/flood/force-kill/exit/descendant tests pass. These bounded
results and source review do not replace hosted Linux external Play/Stop and descendant coverage.

The successor `49423ae` run executes all 151 Linux reference-game all-feature tests: 150 pass,
one external Play test reaches Faulted/Start. The old helper waits 600 seconds without printing
its existing bounded output. Make terminal Faulted fail immediately with that public diagnostic
projection, retaining the exact Running/Restart/Stop/Exit success requirements.

Independent source review also confirms a desktop-session input omission: exact product launches
clear ambient environment, retain DISPLAY but discard XAUTHORITY and HOME. `xvfb-run` creates a
temporary authentication file, so the existing desktop-session grant needs its explicit XAUTHORITY
locator. Add only that fixed key with existing size/NUL limits; do not read or log cookie contents,
inherit arbitrary credentials or weaken Sealed/injection exclusions. This is a confirmed projection
gap; the earlier generic Start result alone does not establish it as the only failure cause.

With full failure collection, the same run executes all 2,641 Linux root tests: 2,618 pass and
23 fail, with 12 platform-scoped skips. Nineteen failures are Editor consumers: seventeen expose
the native Host's OS-derived strict write policy, while two generic composition fixtures still
assume Browser Open completes synchronously. The remaining four failures are two stale manifest
boundary fixtures, one unstable compiler-help snapshot and a font-license byte mismatch.

The user selects cooperative local writes for explicitly selected native workspaces. Issue
`ReadWrite + TrustedLocal` at the existing native root-capability entry, independently from
data-only Open and explicit Rust execution. Preserve conflict checks, atomic name publication,
receipt acknowledgement and honest platform guarantees. Lower caller-issued strict capabilities
must still refuse unsupported writes; do not gate the nineteen consumer tests away on Linux.
ADR 0118 owns this product policy.

Update dependency fixtures to the actual direct-domain and dev-only workflow-facade boundary,
including negative cases for default/extra features, wrong paths and facade placement. Replace
only the unstable Resource compiler-help snapshot with a compile-time negative trait assertion;
retain all other authority/privacy compile-fail cases. Generic composition fixtures must await
and acknowledge the exact new Open request with bounded elapsed time, preserving all domain
assertions. Preserve the pinned upstream Fira Sans license's original CRLF bytes through Git
attributes; do not change the expected provenance hash to match transformed text. Fira Mono's
existing canonical LF fixture remains LF on every checkout.

The completed Windows root job runs 2,659 tests: 2,650 pass and nine fail, with 42 skips.
Two stale boundary fixtures and six asynchronous Open assumptions overlap the repairs above.
The ninth failure is an actual GPU oracle mismatch: in a 5-by-3 physical window with scale 1.25,
the canvas content spans physical y=0.25..2.75, but pixel (0, 2) is black rather than the expected
green. Investigate composite viewport/scissor and raster sampling against the shared content
mapping; preserve exact visible-pixel and input agreement rather than relaxing the oracle.

`0d71c4d` rasterizes the integer pixel-center coverage already selected by the scissor, while
the original fractional rectangle remains the source-sampling uniform. All five canvas GPU tests
pass with both automatic adapter selection and forced fallback on Windows. The undersized-window
oracle, nearest sampling, bars and native UI assertions remain unchanged; hosted GPU qualification
is still pending. `ecb8410` applies the selected local-workspace policy; `a9b110f` repairs exact
asynchronous Open observation. The remaining local Browser test now confirms hard-linked local
assets remain inspect-only; both recovery/Browser consumers pass, while the independent lower
strict multi-link rejection still passes.

The cold Windows job spends about 71 minutes in checks, feature-matrix builds and the default
suite before its existing all-feature suite and Dear adapter gate. Its 75-minute job budget
cannot cover that complete sequence. Set the root job envelope to 180 minutes, retaining serial
Cargo, individual test deadlines, complete suites and failure semantics. This admits the existing
workload; it is not a performance target or evidence that the next run will finish in that budget.

## Feedback Cost Follow-up

The user challenges this duration and explicitly selects `repo-ref/bevy` as a CI design reference.
The 180-minute envelope is a temporary qualification guard, not the desired feedback loop. At
Bevy revision `85503941e488152711a4695bbe248e9ed9f453fc`, separate test, lint, compile-test and wasm
jobs, CI debug settings and trusted cache writers provide comparison evidence. Preserve Nara's
actual platform and feature obligations rather than copying Bevy's feature selection.

For Windows run `39418300517`, checks take 4m34s, the feature matrix 15m26s, nextest preparation
about 15m50s and default test execution 35m16s. Eight selected compiler/consumer tests consume 1,614.084 seconds
(76.29% of that test cohort). Nested Cargo and compile-contract construction dominate. Attribute
cache and feature-profile reuse first; move compile-only positive fixtures out of mixed trybuild
suites where their only effect is to force full dependency code generation. Keep executable
positive behavior and every negative assertion. Review a small job/cache correction and actual
before/after hosted evidence before claiming an improved feedback time. No new compiler-analysis
framework, broad test exclusion or unmeasured speed claim is authorized by this comparison.
The [source comparison and measured attribution](../research/2026-10-20-ci-feedback-and-bevy-comparison.md)
records exact Bevy evidence, distinct feature obligations and the ordered small corrections.

## Ordered Corrections

- Implement Linux enumeration behind the existing capability owner using `rustix::fs::Dir` and
  an independent identity-checked handle-relative cursor. Preserve entry/name/work limits,
  no-follow child admission, rejected-entry isolation and explicit unsupported tiers elsewhere.
- Validate content versus independently retained startup-closure leases and their actual release;
  observe terminal Stop instead of counting drive calls. Keep the existing gameplay assertions.
- Make the schema-bundle CLI's explicit local `--write` select trusted local workspace authority;
  retain strict Check reads and the existing atomic preflight, identity, digest and cleanup path.
  Do not upgrade Linux publication guarantees or silently suppress its failure.
- Scope exact-discard recovery tests to their required Windows capability. Preserve portable
  admission and Unix recoverable-orphan coverage rather than pretending cleanup completed.
- Admit signed Linux process-group query results before converting to a nonzero ID; zero means
  no membership in the owned positive group. Preserve query errors and the existing process-group
  signaling/leader reservation contract. Qualify the exact query source and actual Play/Stop.
- Make identity-replacement test inputs deterministic by retaining the old object before creating
  the replacement; preserve complete rejection and workspace-state assertions across platforms.
- Let each root/reference nextest suite report all independent test failures with `--no-fail-fast`.
  Keep serial execution, existing timeouts and failure exit status; an early defect should not
  leave thousands of later tests unobserved on every cold hosted build.
- Preserve the selected X11 desktop session's XAUTHORITY input and make terminal external-start
  failures print existing bounded public diagnostics immediately. Qualify the actual Linux
  Play/Restart/Stop/Exit journey without expanding ambient environment inheritance.
- Apply the selected native local-workspace policy at its owning entry and qualify Save, Reopen,
  replacement and lifecycle consumers. Preserve lower strict-capability rejection and execution
  admission. Repair the independently observed manifest, async fixture, negative assertion and
  canonical font-byte contracts without excluding their consumers.
- Format independent workspaces and refresh only required lockfile dependency edges.
- Reconcile the stale exact fixture dependency policy with its existing dev-only component
  integration consumer. Keep the library's direct-domain dependencies and explicit facade features.
  Replace two short fixed-poll Close/Exit loops with bounded elapsed-time waits; these are robustness
  corrections to the same asynchronous lifecycle, not separately observed hosted failures.
- Select the wasm unknown entropy backend only for the asset owner on that target. Add locked
  compile-only checks for `nara_fs`, core runtime and the backend-neutral 2D/UI/tooling library
  combination. Do not enable unsafe browser Send/Sync or hide unsupported native operations.
- Run relevant local source, consumer, format and policy checks; push precise commits and inspect
  actual hosted results. Record compile evidence separately from Linux runtime and browser support.

Root owns serial Cargo, formatting, locks and integration. Parallel lanes own filesystem/consumer
repairs and wasm manifest/workflow changes; each receives independent review. Preserve concurrent
changes, including the unrelated untracked `vendor/` directory.
