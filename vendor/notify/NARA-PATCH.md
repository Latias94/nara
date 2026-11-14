# Nara notify maintenance patch

This directory imports the published `notify` 8.2.0 package from the local Cargo
registry, not a checkout of the upstream monorepo. The original
`.cargo_vcs_info.json` records upstream revision
`a1d7c2d8f80786679d58ec6d5986a1d4278bc8cf`, with `path_in_vcs` set to `notify`.
The registry package checksum recorded by Nara's existing lockfile is
`4d3d07927151ff8575b7087f245456e549fea62edf0ec4e565a5ee50c8402bc3`.

The published normalized `Cargo.toml`, `Cargo.toml.orig`, `README.md`,
`LICENSE-CC0`, `.cargo_vcs_info.json`, and all other unmodified package files are
preserved byte for byte. Cargo cache markers and checksum metadata, including
`.cargo-ok` and `.cargo-checksum.json`, are excluded. This patch does not select
itself in Nara's manifests or lockfiles.

The published license and README contain trailing spaces, including Markdown hard line breaks.
They remain byte-identical imports and are excluded from the maintained-file whitespace check.

## Changed sources and API

Only `src/windows.rs` and `src/inotify.rs` differ from the published sources.
Each concrete watcher gains these inherent methods; the `Watcher` trait is
unchanged:

```rust
pub fn try_close(&mut self) -> notify::Result<bool>;
pub fn is_closed(&self) -> bool;
```

The methods are available on `ReadDirectoryChangesWatcher` on Windows and
`INotifyWatcher` on Linux. The inotify module is also compiled on Android;
Android execution has not been qualified by this patch.

- The watcher retains `Option<JoinHandle<()>>` for the original native worker.
  No wrapper thread, executor, alternate watcher, or callback lifetime proxy is
  introduced.
- The first `try_close` requests the existing native `Stop` or `Shutdown` and
  attempts to wake its worker. Later calls do not send another shutdown request.
  A disconnected command channel does not cause an unwrap panic.
- Inotify records shutdown queuing separately from a successful wake. A failed
  wake returns an error, and later `try_close` calls retry only that wake until it
  succeeds or the worker disconnects or finishes. The worker owns the waker;
  the watcher holds a weak reference and stops waking once disconnection or worker
  completion is observed, including after join. Losing the waker does not imply
  `is_closed()`; that still requires joining the original worker. A successful
  wake is not repeated.
- `Ok(false)` means the original worker has not reported completion. The method
  calls `join` only after that worker's `is_finished()` returns true.
- `Ok(true)` means the original worker was joined successfully. A worker panic
  produces an error that remains observable on every later `try_close` call.
- `is_closed()` means the original worker has been joined, including when it
  panicked. It stays false for an exited worker that has not yet been joined.
  Callers can distinguish physical worker retirement from successful shutdown
  by inspecting both methods.
- `watch`, `unwatch`, and `configure` reject changes once close is requested or
  the worker is observed to have finished. Existing inotify channel/waker
  failures return errors rather than panicking on a disconnected worker.
- Ordinary unawaited `Drop` requests shutdown once and drops the join handle
  without joining. Inotify Drop makes a best-effort retry of a previously failed
  wake, ignoring its error. Repeated close or subsequent Drop does not duplicate
  the stop request or repeat a successful wake.
- Worker spawn failures are returned during construction. Windows also closes
  its newly allocated wakeup semaphore on that failure path.

Normal native event processing, watch registration, cancellation, and cleanup
algorithms are otherwise unchanged. This is a bounded prerequisite for the
original Nara Runtime plugin close owner to retain and poll the real watcher;
it does not implement that integration or change publication routing.

## Retirement scope and limits

A successful join proves the original worker has returned, including its
callback destruction. It is stronger than observing a captured callback object
being released. It does not certify every operating system cleanup operation:
upstream Windows cleanup still ignores some `CancelIo`/`CloseHandle` failures,
and inotify shutdown still ignores watch-removal and close errors. Panic paths
are joined and reported as failures, not treated as successful native cleanup.

`try_close` never joins a worker that reports itself unfinished. There is no
strict wall-clock bound: signaling, scheduling, platform cleanup, and a join
after `is_finished` can still take time. Deadline, retry, and retained ownership
remain the caller's responsibility. A process abort cannot be recovered through
this API.

FSEvents, kqueue, PollWatcher, and NullWatcher are unchanged and do not gain these
methods. This import is source-owned maintenance for the named backends, not a
claim of full notify lifecycle parity across platforms.

Retire this path dependency when an upstream source exposes equivalent original-worker close,
retry and persistent-failure facts and passes the affected native and Runtime custody controls.
Do not substitute callback destruction or an unawaited Drop for that qualification.

## Scoped controls and verification

Both changed backends contain these controls:

- `close_waits_for_original_worker_and_rejects_mutations` installs a real native
  directory watch, holds handler destruction on the original worker, verifies
  pending close and mutation rejection, then permits retirement and checks
  successful repeated close.
- `close_joins_disconnected_worker_and_retains_panic` causes handler destruction
  to panic on the original worker after native shutdown, observes its completed
  but unjoined state, then checks joined retirement and persistent failure.

Inotify additionally contains deterministic wake-failure controls using a private
closure parameter, without replacing the native event loop or adding a mocking
framework:

- `close_retries_failed_wake_without_duplicate_shutdown` holds native event-loop
  startup, injects two failed wakes, then permits a successful wake. The worker
  checks that exactly one shutdown message was queued before running the native
  loop; subsequent polls do not repeat the successful wake, including after join.
- `close_skips_failed_wake_retry_after_worker_disconnects` reports an injected
  wake error after allowing native shutdown, holds handler destruction on the
  original worker, and rejects any attempted retry wake after its command receiver
  and waker have been released but before the worker finishes.

The import and patch were checked against the local published package, including
byte equality of unchanged files and diff/whitespace inspection. Cargo, Rust
formatting, compilation, and tests were not run during this bounded import or
the inotify wake-retry correction. The controls therefore describe intended
coverage, not passing native Windows, Linux, or Android execution evidence.
Consumer integration and platform runs remain with the owning workflow.

The owning Nara integration subsequently executes the two Windows worker controls in
`86a73815-97aa-4c7e-9be0-d8bc04759773`: 2/2 passing, seven unrelated upstream cases outside
the selection. The Nara watcher library passes all 47 controls in
`b58b122d-9cd6-41a4-b688-49726383a6d3` against its original locked production dependency
closure. Root stable formatting changes only the two already maintained source files; all
other imported package files remain byte-identical to the published package. Actual integrated
Scene results and platform limits belong in the
[owning plan](../../docs/plans/2026-11-14-durable-material-and-shader-plan.md).

The maintained library and tests also pass a Rust 1.95.0 Linux cross-compilation check using
`--target x86_64-unknown-linux-gnu`. The original Nara locked watcher-library closure passes
its Linux all-target check separately. Neither is Linux/Android execution evidence.
