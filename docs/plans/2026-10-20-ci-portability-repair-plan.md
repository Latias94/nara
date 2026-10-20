# CI And Portable Wasm Repair

Status: In progress alongside the selected locality follow-up. Requested after repeated hosted
failures; this does not add browser execution to the first production proof.

## Confirmed Failures

Recent hosted runs `39398286157`, `39385253214` and `39375892490` fail independently on Linux
handle-bound directory enumeration, stale reference-game retirement/lease expectations and
standalone module-consumer formatting. The `8afbe42` run repeats the formatting failure.
The first local portable wasm check fails in `getrandom 0.3.4` before engine library checking.

## Ordered Corrections

- Implement Linux enumeration behind the existing capability owner using `rustix::fs::Dir` and
  an independent identity-checked handle-relative cursor. Preserve entry/name/work limits,
  no-follow child admission, rejected-entry isolation and explicit unsupported tiers elsewhere.
- Validate content versus independently retained startup-closure leases and their actual release;
  observe terminal Stop instead of counting drive calls. Keep the existing gameplay assertions.
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
