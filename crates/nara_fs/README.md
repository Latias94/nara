# nara_fs

`nara_fs` provides bounded filesystem operations over Host-issued handles. Its public Interface is
limited to explicit handle authority, validated relative names, bounded file operations and finite
publication transactions.

Direct consumers depend on this crate rather than the root `nara` facade:

```toml
[dependencies]
nara_fs = { path = "../nara/crates/nara_fs" }
```

The Host opens a directory handle and imports it with explicit rights, trust and generation. Domain
code then uses validated `RelativeComponent` and `RelativePath` values. No public operation accepts
or returns a native path as authorization.

## Exact Stored Spelling

`DirectoryCapability::open_file_exact` and `open_directory_exact` opt into exact stored spelling
for **every** requested descendant component, not only the leaf. The ordinary open methods keep
their existing native lookup behavior. Canonical source admission can use the exact methods
without lowercasing names, normalizing Unicode or recovering path authority:

```rust,ignore
let path = nara_fs::RelativePath::new("Assets/Scenes/startup.scene.json")?;
let source = project_directory.open_file_exact(&path)?;
let bytes = source.read_to_end_bounded(max_encoded_bytes)?;
```

On local Windows NTFS/ReFS, each component is opened relative to the preceding capability and
passes the existing object-kind, reparse, volume and trust-specific identity/link checks. Before
using that handle, the adapter queries `FileNormalizedNameInformation` on the **same** handle and
compares its final long-name component as raw UTF-16 units. This rejects case, Unicode-spelling
and DOS short-name aliases without reopening the returned name. The query uses a fixed 64 KiB
buffer, once per component, with no growing retry or directory enumeration. `RelativePath` already
bounds traversal to 256 components and 32,767 native units. The 64 KiB temporary name allocation is
released before the next component; kernel I/O latency and internal allocations are not bounded
by that output-buffer limit.

A mismatch returns `FsError::ExactSpellingMismatch`. Unsupported volume semantics, a failed query,
overflow (including a long Host-root prefix), or incomplete name evidence returns `Unproven`.

Linux does not assume that `openat`/`openat2` imply case-sensitive stored names. It uses an
independent handle-bound directory cursor to compare native entry bytes exactly, then joins the
matching entry's inode and no-follow `statat` device/inode to the already opened handle. The scan stops
at the match and retains no sibling list; it rejects after 65,536 entries per component. This is
bounded linear sibling work, unlike the Windows constant-query path, and can reject very large
directories. Windows and WSL Linux filesystem cohorts execute separately; they do not qualify
casefold-volume behavior or native product loading. Other targets currently return `Unsupported`
before opening descendants. There is no permissive fallback.

Verification starts below the Host-issued root; it does not certify that root's ambient spelling.
Like ordinary capability opens, it retains the opened object, not an atomic namespace/content
snapshot: an external rename can invalidate an earlier component's observed name. Strict trust
still rejects multi-link regular files. Exact spelling alone is not persistent source identity or
protection from subsequent content changes.

Native design references: Microsoft's [global case-sensitivity caveat](https://devblogs.microsoft.com/commandline/per-directory-case-sensitivity-and-wsl/)
rules out relying only on clearing `OBJ_CASE_INSENSITIVE`;
[`FileNormalizedNameInformation`](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-fsa/3d5c68f8-fbc5-4f94-a51d-3600a319fd45)
uses [`BuildRelativeName`](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-fsa/280d599a-e920-4710-9e7c-611b64d5a15e)
and the opened link's stored name rather than its separate short-name alias. Linux's
[ext4 documentation](https://www.kernel.org/doc/html/v6.16/admin-guide/ext4.html) documents
per-directory case-insensitive Unicode lookup; a Unix compile target is not exact-name evidence.
These references explain the mechanism, not a claim of executed platform qualification.

## Explicit File Unlink

`DirectoryCapability::unlink_file(&path, &observed)` deletes one current regular-file link
through a read-write parent. Keep the observed `FileCapability` alive; a bare copied file ID is
insufficient. The adapter opens a deleting handle relative to the verified parent, compares the
actual live objects and sets disposition on that same handle. A different same-name object
rejects before mutation. Read-only authority, reparse points, multi-link files and unavailable
identity also reject.

This explicitly authorizes deletion of the whole object, including its alternate data streams.
It does not require unchanged bytes and does not decide installation ownership, sample adoption
or document protection. The caller supplies those domain policies. In particular, default-stream
digest equality cannot establish whole-file unchanged-content reclamation.

The first qualified adapter is Windows x86-64, Windows 10 1809+ and local NTFS. It uses DELETE and
attribute access, shares read/write access, excludes competing rename/delete opens and consumes
the exact deleting handle after successful POSIX disposition. Read-only file attributes are
honored. `FileUnlinkReceipt` reports the namespace cut; independent read handles may still access
the old object, and a new object may later occupy its name. Parent-directory crash durability
and physical disk-space release are not claimed. Unix and other unqualified adapters return
Unsupported without a name-based fallback.

## Directory Publication

Directory publication is a move-only transaction:

1. The caller supplies a validated same-parent staging name and
   `DirectoryCapability::begin_directory_publication` creates it exclusively. A production Host
   must choose a bounded, high-entropy name that is unpredictable within the authorized parent;
   fixed names are suitable only for fixtures that exclusively own their temporary parent.
2. The caller writes and validates a known finite member plan through the staging capability.
3. `DirectoryPublicationTransaction::publish` performs an exact no-replace name switch, or returns
   a truthful clean rejection or recoverable orphan.
4. The caller reopens the destination through the original parent capability and validates the
   published contents.

If member construction or validation fails after begin, the caller consumes the transaction with
`abort` and preserves its clean-rejection or orphan outcome. Dropping an active transaction does
not perform hidden cleanup.

The receipt keeps source binding, parent authorization, topology, conflict protection, atomicity,
identity and durability progress as separate facts. A compile target alone never proves a runtime
filesystem tier. Unsupported or unproven combinations fail closed; there is no check-then-rename,
overwrite, ambient-path or recursive-delete fallback.

See `tests/fixtures/module-consumer/fs-consumer` for a direct public consumer that stages, publishes
and reopens a bounded nested tree without importing the root facade or any private Nara crate.
