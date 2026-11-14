# Native asset watch lifecycle

The optional [hosted Scene adapter](../../docs/guides/materials.md#native-source-scene-watching)
uses this same lifecycle owner without the stock generic asset publisher. Its actual workflow
and platform evidence belong in the
[owning qualification](../../docs/plans/2026-11-14-durable-material-and-shader-plan.md#native-watching-qualification).

`AssetWatchRuntime` is the concrete owner of native watcher sessions. Register it inside the
declaring plugin's build hook, before acquiring a native watcher:

```rust,ignore
let runtime = AssetWatchRuntime::register(app, WATCH_OBLIGATION, WATCH_PARTICIPANT)?;
app.insert_resource(runtime.clone())?;
```

Registration installs the existing App `RuntimeCloseParticipant` and registers the supplied
shutdown obligation exactly once. Do not also call `register_plugin_shutdown_obligation` for
that obligation. Registration does not install a watcher, insert a resource, or schedule work;
the consumer selects when its admitted source context is ready for native acquisition.

The returned owner is cloneable. Every clone shares a factory fence, a budget of four native
instances, and a strong session registry. Current watchers, pending replacements, and unfinished
retirements all consume that budget. Capacity exhaustion returns
`AssetWatchError::NativeCapacityExceeded { limit }` before constructing another native instance.

## Selected paths and translation

`watch_recursive(&root, sender)` is the convenience form for one recursive path.
`watch_paths(&root, &paths, sender)` attaches a concrete set of paths to **one**
`RecommendedWatcher` and its original worker:

```rust,ignore
let paths = [
    AssetWatchPath::new(project_root.clone(), false),
    AssetWatchPath::new(project_root.join("assets"), true),
    AssetWatchPath::new(project_root.join("scenes"), true),
    AssetWatchPath::new(project_root.join("prefabs"), true),
];
let watcher = runtime.watch_paths(&source_root, &paths, queue.sender())?;
```

`AssetWatchPath::new(path: PathBuf, recursive: bool)` exposes `path() -> &Path` and
`recursive() -> bool`. Relative paths resolve against the source root. Paths outside the lexical
or canonical source root are rejected before native construction; a missing path is checked
against its existing ancestors and left to native `watch()` to reject. Empty selections are
rejected. The consumer owns deduplication and recursive-parent coverage.

The source root remains the canonical translation namespace for every selected path. Selected
directories do not create separate logical roots, queues, workers, or callback filters. This
adapter preserves the existing generic asset routing, bounded queue cutoff, sticky rescan
requirement, and exact World/root/session checks on rescan receipts.

## Retirement and close

Every constructed native session enters the registry before its first `watch()` call. Partial
path installation failures request close and leave unfinished sessions in that registry. Facade
Drop, rescan stop, replacement, supersession, and receipt abandonment request the same close;
none removes an unfinished managed worker from custody.

`AssetWatcher::try_close(&mut self) -> Result<bool, AssetWatchError>` requests close and polls
without waiting for the worker to return. `Ok(true)` certifies successful original-worker join.
`is_closed()` reports actual join, including failed joins. A `NativeClose { joined: true, .. }`
error reports a joined failure; unfinished errors leave the session retained for another poll.

`runtime.poll_retired() -> Result<(), AssetWatchError>` polls only closing sessions. It leaves
active watchers running. Ordinary consumers should poll retirements during their existing update
path; new creation also reaps completed retirements before checking capacity. The stock
`AssetWatchPlugin` performs this polling and registers its participant under the original
`ASSET_WATCH_SHUTDOWN_OBLIGATION`. Stock rescans require the registered owner in the World;
raw Worlds without it return `ManagedRuntimeMissing` before native acquisition.

The Runtime participant fences all factory clones, begins close on every retained session, and
returns `Pending` while any original worker remains unfinished. Unfinished close errors are
retryable through the existing Runtime deadline/retry protocol. Actual joined failures remain
sticky and eventually return `CompleteWithError`, preserving failure evidence rather than
reporting a clean Stop. No helper worker, async Drop executor, or separate shutdown scheduler
is involved.

## Observation

`runtime.stats() -> AssetWatchLifecycleStats` reads:

| Getter | Type | Meaning |
| --- | --- | --- |
| `active()` | `usize` | Constructed sessions not yet asked to close, including pending replacements |
| `retiring()` | `usize` | Close requested, original worker not yet joined |
| `started()` | `u64` | Constructed native instances, including failed watch setup |
| `joined()` | `u64` | Original workers actually joined, including failed joins |

`runtime.observer() -> AssetWatchRuntimeObserver` returns a cloneable read-only live observer.
Its sole observation method is `stats() -> AssetWatchLifecycleStats`; it exposes no factories
or close controls. A retained observer, facade, receipt, or stopped World cannot keep a backend
alive after Runtime close has joined and removed its session. Joined counts establish physical
completion; Runtime close evidence separately establishes whether completion was clean.

## Standalone and platform limits

The ordinary `AssetWatcher::watch_recursive` signature is preserved. Standalone Drop requests
legacy unawaited stop. Callers needing completion on Windows/Linux must poll `try_close`.
If setup fails after native construction and cleanup is unfinished, the returned
`AssetWatchError::StartFailure(AssetWatchStartFailure)` owns that cleanup and exposes
`try_close()`, `is_closed()`, and `stats()`. Dropping its last clone remains an explicitly
unawaited stop. Managed startup instead leaves cleanup in its registered Runtime owner.

Strict managed original-worker join currently requires the repository's patched notify 8.2
Windows or Linux backend. Other managed platforms fail with an explicit unsupported error
before native acquisition. Standalone foreign backends retain legacy Drop behavior and cannot
certify join through `try_close` or `is_closed`. Callback lifetime is only a test observation,
never the proof of original-worker completion. Native platform qualification and application
source-binding/full-rescan policy belong to the consumer's qualification evidence.
