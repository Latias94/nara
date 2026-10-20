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
