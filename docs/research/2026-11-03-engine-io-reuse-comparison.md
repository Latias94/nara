# Pinned Engine IO Reuse Comparison

**Date:** 2026-11-03. **Status:** Research; no architecture change or implementation qualification.
This supplements [IO Substrate And Engine Contracts](2026-11-03-io-substrate-and-engine-contracts.md)
with a bounded comparison of asset/file IO, not another crate or executor survey.

## Evidence Boundary

- Bevy: local `repo-ref/bevy`, revision `85503941e488152711a4695bbe248e9ed9f453fc`.
- Godot: local `repo-ref/godot`, revision `81ab961f91066a63bcbf3ec340962bb1a1ada558`.
- Both reference checkouts were clean. Links below use local source paths and line anchors at those
  revisions; these are observations of pinned source, not claims about the latest releases.
- Nara: `refactor/foundation`, HEAD `88fd7289c0d680481e0f964561650de05f90cbad`, with concurrent WIP.
  Nara links describe inspected working-tree content. No runtime, performance or adversarial tests ran.

## What The Engines Retain And Reuse

| Engine | Engine-owned abstraction | Reused implementation |
| --- | --- | --- |
| Bevy | [`AssetReader`](../../repo-ref/bevy/crates/bevy_asset/src/io/mod.rs#L187), [`AssetWriter`](../../repo-ref/bevy/crates/bevy_asset/src/io/mod.rs#L329) and [`AssetSource`](../../repo-ref/bevy/crates/bevy_asset/src/io/source.rs#L390) retain asset/metadata access and optional writers, processed readers and watchers. [`Reader`](../../repo-ref/bevy/crates/bevy_asset/src/io/mod.rs#L102) extends ecosystem `AsyncRead` with optional seeking. | The [multithreaded file backend](../../repo-ref/bevy/crates/bevy_asset/src/io/file/file_asset.rs#L74) joins the root path and calls `async_fs::File::open`; the [manifest](../../repo-ref/bevy/crates/bevy_asset/Cargo.toml#L42) declares `async-fs`, `async-lock`, `futures-io` and `futures-lite`. The [feature selection](../../repo-ref/bevy/crates/bevy_asset/src/io/file/mod.rs#L10) instead uses a [synchronous backend](../../repo-ref/bevy/crates/bevy_asset/src/io/file/sync_file_asset.rs#L19) without `multi_threaded`: `poll_read` calls `std::io::Read` on `std::fs::File` and returns `Poll::Ready`. |
| Godot | [`FileAccess`](../../repo-ref/godot/core/io/file_access.h#L45) retains file operations and access categories; [path dispatch](../../repo-ref/godot/core/io/file_access.cpp#L68) selects resource/user/filesystem/pipe access. [`open`](../../repo-ref/godot/core/io/file_access.cpp#L160) tries packed data before the native backend. [`PackSource`](../../repo-ref/godot/core/io/file_access_pack.h#L161) owns pack lookup, while [`ResourceLoader`](../../repo-ref/godot/core/io/resource_loader.cpp#L275) selects format loaders. | Reviewed desktop drivers use the C runtime: Unix [`fopen`](../../repo-ref/godot/drivers/unix/file_access_unix.cpp#L170) / [`fread`](../../repo-ref/godot/drivers/unix/file_access_unix.cpp#L298), Windows [`_wfsopen`](../../repo-ref/godot/drivers/windows/file_access_windows.cpp#L218) / [`fread`](../../repo-ref/godot/drivers/windows/file_access_windows.cpp#L336). [Threaded resource loading](../../repo-ref/godot/core/io/resource_loader.cpp#L831) dispatches through Godot's `WorkerThreadPool`; the reviewed file methods remain synchronous. |

The common pattern is ownership of resource meaning and backend selection over reused byte-access
mechanisms. It does not prescribe one interface shape: Bevy exposes Future-based asset IO, including
a blocking implementation behind that interface; Godot uses synchronous file access with optional
threaded resource loading. Neither observation selects an execution model for Nara.

## Strict Semantics Are Separate Questions

Bevy's reviewed file open is path-based. Its AssetServer also has an
[unapproved-path policy](../../repo-ref/bevy/crates/bevy_asset/src/server/mod.rs#L2074), backed by
[lexical component checks](../../repo-ref/bevy/crates/bevy_asset/src/path.rs#L569). Those checks are
not evidence of Nara-equivalent handle-relative traversal, live identity or publication guarantees.
Their presence also rules out describing Bevy as having no path policy. No exploit or caller defect
is established here.

Godot's Unix tools explicitly [allow shared-addon symlinks and warn on case mismatches](../../repo-ref/godot/drivers/unix/file_access_unix.cpp#L102).
Its Windows [case check is also tools-only and warning-based](../../repo-ref/godot/drivers/windows/file_access_windows.cpp#L138).
Optional [backup save](../../repo-ref/godot/drivers/unix/file_access_unix.cpp#L118) creates a temporary
file and [renames it on close](../../repo-ref/godot/drivers/unix/file_access_unix.cpp#L198).
These are distinct portability/save policies; they do not prove Nara's exact-spelling rejection,
recovery conflict protection or crash-durability requirements unnecessary or satisfied.

Nara itself already separates several concerns:

1. **Traversal policy and exact spelling:** [`TrustMode`](../../crates/nara_fs/src/capability.rs#L27)
   makes `Untrusted` and `Recovery` strict, while the
   [README](../../crates/nara_fs/README.md#L18) makes exact stored spelling a separate opt-in.
   Accepted [ADR 0070](../architecture/adr/0070-capability-oriented-filesystem-substrate.md#L146)
   owns strict untrusted/recovery policy; the name "IO" does not make all its axes mandatory.
2. **Import input:** [`AssetImportSource`](../../crates/nara_asset/src/import.rs#L110) exposes
   immutable bytes and a scoped dependency reader; the caller owns scope, observations and budgets.
   Importers do not need a second filesystem abstraction to acquire directory authority.
3. **Captured Runtime content:** [`RuntimeContentBase::open`](../../src/runtime_content.rs#L388)
   captures bounded members, verifies their digests and publishes captured bytes. Subsequent
   [captured-member access](../../src/runtime_content/backing.rs#L55) performs no filesystem read.
4. **Retained Runtime pack:** The same [member-access code](../../src/runtime_content/backing.rs#L61)
   reads a bounded range through the retained `FileCapability`, then verifies its expected digest.
   The [reader](../../crates/nara_fs/src/file.rs#L169) borrows that same handle; its
   [positional implementation](../../crates/nara_fs/src/file.rs#L443) reuses standard `Read`/`Seek`
   and platform `FileExt`. It does not repeat path traversal or publication. A changed backing
   object can reject later reads; retaining a handle does not freeze bytes.

There is actual Runtime strict-traversal usage: the executable-relative
[pack entry](../../src/project_host/runtime_pack.rs#L34) opens its parent through the product-root
helper, which selects [`Untrusted` on Windows/Linux](../../src/project_host/product_launch/root.rs#L56).
Thus "strict semantics belong only to project/recovery" would also overstate the evidence.
Pack admission and later positional reads are different phases. Digest integrity alone does not
authorize an initial path or justify dropping its existing traversal contract.

## Recommendation And Limits

Reuse established byte/OS mechanisms beneath Nara's owning contracts. For each proposed reduction,
name the consumer, phase and required facts: initial authority/traversal, bounded transport,
integrity, retained-object access, publication conflict protection and durability are independent.
The reviewed Runtime acquisitions demonstrate narrower needs than a universal strict filesystem
contract; they do not demonstrate that current initial pack opening is unnecessarily strict.

Keep project/recovery guarantees where their owners require them, preserve the current Runtime
admission contract, and evaluate a concrete mechanism replacement only against the actual consumer.
These sources justify neither a universal IO trait nor an executor migration. They establish no
Nara defect, crate equivalence, platform qualification or measured performance advantage. No Cargo,
formatter, dependency change, commit or edit outside this note was performed for this comparison.
