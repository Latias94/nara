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
