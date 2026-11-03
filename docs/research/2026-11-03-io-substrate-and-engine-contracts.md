# IO Substrate And Engine Contracts

**Date:** 2026-11-03

**Scope:** Direction research for the question of using existing crates versus implementing Nara's
IO abstractions. This is a read-only implementation review plus this note, not an Accepted ADR,
an executor selection, or qualification of a complete IO replacement. The active foundation and
Prefab extraction units retain their existing acceptance scopes.

## Architecture Overview

The recommended boundary is **crate-backed mechanisms with Nara-owned engine contracts**.
These responsibilities already have different owners:

- `nara_fs` operates on Host-issued handles and validated relative names. It owns the association
  between authority, live objects and reported filesystem guarantees. Asset and Editor owners
  retain their transaction policies.
- `nara_tasks` executes owned closures on bounded worker queues. Its `Io` kind is a blocking-work
  queue; `AsyncCompute` is explicitly not a Future executor. Admission, first-terminal results,
  cancellation, physical disposal and worker retirement are separate observations.
- Asset importers receive bounded bytes and a caller-authorized dependency reader. Project Hosts
  own filesystem authority, source observations, budgets and publication. Importing a format does
  not grant filesystem or Runtime authority.

These boundaries follow Accepted [ADR0070](../architecture/adr/0070-capability-oriented-filesystem-substrate.md),
[ADR0008](../architecture/adr/0008-runtime-concurrency-and-task-pools.md),
[ADR0052](../architecture/adr/0052-task-backpressure-cancellation-and-long-running-diagnostics.md)
and [ADR0103](../architecture/adr/0103-concrete-domain-owned-asset-import-execution.md).

The focused implementation review has five anchors:

| Source | Observed responsibility |
|---|---|
| [Unix adapter](../../crates/nara_fs/src/platform/unix.rs#L562) | Strict Linux opens call `rustix::fs::openat2` with `BENEATH`, `NO_MAGICLINKS`, `NO_SYMLINKS` and `NO_XDEV`. Other strict Unix opens fail explicitly. The same adapter uses crate syscalls for rename, sync and locking. |
| [Windows adapter](../../crates/nara_fs/src/platform/windows.rs#L1060) | `NtCreateFile` receives the parent handle through `RootDirectory`, a validated component and reparse-point options. This adapter also renames the opened source handle and verifies exact stored spelling on the opened object. |
| [Task submission](../../crates/nara_tasks/src/runtime.rs#L1718) | The public submission accepts `FnOnce(TaskCancellationToken) -> T`; the implementation owns queue state, controls, result custody, physical-close owners and retained disposal. It does not expose asynchronous file operations. |
| [Import contract](../../crates/nara_asset/src/import.rs#L90) | `SourceAssetImporter` prepares effect-free CPU data. `AssetImportSource` exposes immutable primary bytes and a typed auxiliary reader without a directory handle, World or publication authority. |
| [Actual project reader](../../src/project_content/domains.rs#L590) | `DomainSourceReader` implements the import seam using a `DirectoryCapability`, cached bounded reads, work accounting, dependency-edge accounting and recorded content members. Those are project-content responsibilities. |

## Change Assessment: What Existing Crates Supply

The local workspace manifest declares `rustix = "1.1.4"` and `windows-sys = "0.61.2"`.
`Cargo.lock` resolves those versions for the current adapters; it also contains an older transitive
`rustix`. `nara_fs/Cargo.toml` selects `rustix` filesystem APIs on Unix and `windows-sys` APIs on
Windows. It also reuses `blake3` and `thiserror`. The adapters therefore already delegate system
calls, hashing and error support to crates. No dependency upgrade is proposed.
Dependency evidence: [workspace manifest](../../Cargo.toml), [filesystem manifest](../../crates/nara_fs/Cargo.toml)
and [lockfile](../../Cargo.lock).

Official documentation was fetched by the main session on 2026-11-03. Versioned links below pin
those observations; `latest` links are navigation aids, not Nara's dependency selections.

| Candidate | Documented mechanism | Consequence for Nara |
|---|---|---|
| [rustix 1.1.6](https://docs.rs/rustix/1.1.6/rustix/) ([latest](https://docs.rs/rustix/latest/rustix/)) | Safe wrappers for OS interfaces and system calls. | Continue using the existing substrate. A safe syscall wrapper does not itself provide Nara's sandbox policy, live-object guarantees or publication receipts. Local selection remains 1.1.4. |
| [cap-std 4.0.3](https://docs.rs/cap-std/4.0.3/cap_std/fs/index.html) ([latest](https://docs.rs/cap-std/latest/cap_std/fs/index.html)) | Capability-oriented filesystem operations relative to directory objects. | Directory-relative open/create/rename is a concrete area of overlap. The reviewed API description does not establish equivalence to Nara's complete strict-resolution, exact-spelling, lock-ownership or publication contracts. Compare the actual operations before replacing an adapter. |
| [Tokio 1.54.1](https://docs.rs/tokio/1.54.1/tokio/fs/index.html) ([latest](https://docs.rs/tokio/latest/tokio/fs/index.html)) | The default filesystem path delegates blocking file work through `spawn_blocking`. Cancellation can stop waiting without stopping the underlying blocking IO. Optional unstable io_uring support is a distinct configuration. | An async return type alone does not remove blocking work or establish physical retirement. Tokio filesystem APIs do not replace filesystem authorization or publication semantics. Nara's lockfile contains Tokio 1.53.3, which does not establish a configured filesystem runtime. |

Pinned Bevy delegates its path-based file reader to `async_fs`; this does not establish Nara's
handle-relative authorization. Its task pool joins threads in `Drop`, so direct substitution would
change Nara's physical-close contract. Sources:
[file reader](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_asset/src/io/file/file_asset.rs#L74-L90),
[pool destruction](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_tasks/src/task_pool.rs#L616-L627).

The [pinned engine comparison](2026-11-03-engine-io-reuse-comparison.md) adds Godot's file and
resource-loading boundaries and distinguishes initial Runtime pack authorization from later
retained-object reads. Both engines retain resource meaning over reused file mechanisms; their
interfaces do not establish equivalence with Nara's required publication or traversal guarantees.

## Compliance Check

Keep reusable implementation below the existing contracts:

| Responsibility | Appropriate owner |
|---|---|
| Syscalls, OS bindings, byte hashing and ordinary byte-stream traits | Standard library and established crates, used inside adapters. |
| Host authority, relative-name validation, required traversal policy, live identity and honest unsupported/unproven results | `nara_fs`, even if a different crate implements some underlying operations. |
| Source binding, atomic name publication, conflict strength and durability evidence | Filesystem primitives report facts; domain owners decide which facts permit their publication. Atomic rename must not become a strong compare-and-swap claim. |
| Pending limits, coalescing, cancellation/result separation, physical disposal and Runtime close | Nara's task and lifecycle contracts; a candidate execution substrate must preserve them. |
| Asset identity, declared dependency reads, import limits, immutable content and saved-state acknowledgement | The existing asset, project-content and Editor owners. |

The import seam already inverts the dependency correctly: importers consume a caller's reader.
No new universal VFS, transaction framework, executor trait or IO graph is justified.

## Risk Analysis

No confirmed defect in this bounded review requires an immediate blanket migration. Two existing
maintenance concerns deserve separate treatment:

1. **Task mechanism and lifecycle implementation are substantial.** The reviewed task runtime
   contains generic worker/queue machinery together with Nara-specific admission, result and
   retirement policy. Worker dispatch is a concrete candidate for a later substrate comparison;
   no reviewed alternative has yet demonstrated equivalent disposal and retryable-close behavior.
   Replacing it merely because another executor is available could preserve most custom policy
   while adding a second lifecycle. A private implementation split may help evaluation without
   introducing another public abstraction.
2. **Filesystem portability remains narrower than the API vocabulary.** The Unix adapter refuses
   strict operations without its required Linux mechanism; the Windows adapter explicitly refuses
   unproven parent-directory sync. These are truthful existing limits, not evidence that every
   platform is qualified. A portable crate can reduce implementation work, but does not by itself
   qualify the stronger guarantee on each target filesystem. See the reviewed adapters and the
   [filesystem capability guide](../../crates/nara_fs/README.md).

## Recommendations And Next Experiment

**Now:** Retain the crate-backed platform mechanisms and the current capability, publication and
physical-custody contracts. Finish the active foundation consumers under their owning plans.
This study selects neither a Tokio migration nor a complete IO rewrite. It also does not close
R4b deployment or R4c execution work in the
[foundation plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md).

**Next, if reducing filesystem adapter maintenance is selected:** Compare one private operation,
strict descendant file opening, against a `cap-std` implementation. Keep the public
`DirectoryCapability` interface and one actual project reader unchanged. Check directory-relative
authority, symlink/reparse rejection, cross-mount refusal, retained-handle identity, hard-link
policy and exact spelling wherever the consumer requests it. Report which requirements the
candidate supplies, which still require native extensions, and whether meaningful implementation
is actually removed. Stop at that operation if equivalence is absent; do not hide a weaker
guarantee behind the old receipt types.

Retain the actual filesystem invariant tests in
`crates/nara_fs/tests/filesystem_contract.rs`, including
`derived_directory_handle_survives_name_replacement_without_escaping`,
`strict_mode_rejects_multi_link_files`,
`exact_spelling_rejects_aliases_in_every_directory_and_filename_component` and
`bounded_read_accepts_exact_length_and_rejects_one_sentinel_byte`.
If a later candidate changes publication or lock primitives, also retain
`same_directory_replace_is_atomic_but_does_not_claim_strong_cas`,
`replacement_receipt_detects_candidate_bytes_changed_outside_its_capability` and
`exported_native_file_lease_prevents_later_lock_authority`.
Existing tests are a baseline, not proof of an untested cross-mount or platform case; add only the
missing control required by the compared operation.

**Separately, if execution maintenance is selected:** Compare the private blocking-work substrate
with the existing bounded admission and return-observation contracts held fixed. Preserve the
actual task tests `bounded_queue_rejects_without_running_on_the_caller`,
`task_return_waits_for_cancelled_late_result_disposal`,
`running_stats_track_physical_work_after_terminal_cancellation`,
`concurrent_admission_and_close_never_lose_accepted_work` and
`shutdown_poll_returns_while_work_is_held_and_joins_once_after_release` in the existing execution,
scope and close test modules. Use the same real consumer and compare worker counts, queue wait,
owner occupancy and retirement behavior before making a performance claim. Keep this independent
of an eventual Future-based IO consumer.

**Verification limits:** Only source and documentation were reviewed. No Cargo command, formatter,
dependency update, runtime experiment or platform qualification was performed for this note.
