# Native asset watch lifecycle

`AssetWatchPlugin` forwards filesystem changes to the asset source. The hosted Scene adapter
uses the same lifecycle owner with its own queue consumer.

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

The returned owner is cloneable. Every clone shares a factory fence and a registry limited to four
watcher handles. Current watchers and pending replacements both occupy slots. Capacity exhaustion
returns `AssetWatchError::NativeCapacityExceeded { limit }` before constructing another instance.

## Selected paths and translation

`watch_recursive(&root, sender)` is the convenience form for one recursive path.
`watch_paths(&root, &paths, sender)` attaches a concrete set of paths to **one**
`RecommendedWatcher`:

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

The source root remains the canonical translation namespace for every selected path. All paths
publish through the same bounded queue. For an untyped create or modify notification, the adapter
can send a file change when every reported absolute path still names a regular file. Ambiguous
changes such as directory removals and renames require a full rescan. A rescan receipt is tied to
its World, source root, and replacement session.

## Retirement and close

Partial path installation failures release the watcher handle before returning the setup error.
Facade Drop, rescan stop, replacement, supersession, and receipt abandonment stop publication and
release the same session's handle. Already queued events remain available to the queue owner.

`AssetWatcher::close()` releases the handle. `is_running()` reports whether the session can still
publish. Both standalone and managed watchers use the upstream `notify` destructor. On Windows
and Linux, native teardown continues on the backend worker. On macOS, the default FSEvents
destructor stops its own run loop synchronously.

The Runtime close participant fences all factory clones and releases every retained watcher
handle, including those associated with live facades or receipts. `AssetWatchPlugin` registers
this participant under `ASSET_WATCH_SHUTDOWN_OBLIGATION`. Stock rescans require the registered
owner in the World.

## Observation

`runtime.stats() -> AssetWatchLifecycleStats` reads:

| Getter | Type | Meaning |
| --- | --- | --- |
| `active()` | `usize` | Watcher handles currently owned, including pending replacements |
| `started()` | `u64` | Constructed native instances, including failed watch setup |

`runtime.observer() -> AssetWatchRuntimeObserver` returns a cloneable read-only live observer.
Its counts describe Nara's ownership of watcher handles. Native backend shutdown follows the
platform behavior described above.
