# Prefab Extraction Recovery Consumer

This independent executable consumes an actual Pending Prefab extraction through Nara's public
`project-workflow-cli` facade with default features disabled and without Tooling. It receives an
explicit project root and finite limits. It has no producer, conversion callback, old Session,
Workspace, custom process runner, filesystem mutation helper, or reconstructed publication receipt.

The root-owned process tests create the Pending operation and own the process matrix. They can
select this executable through `NARA_PREFAB_EXTRACTION_RECOVERY_EXE`, set to its absolute path.
This fixture contains only the consumer. Its lockfile is generated serially by the integrating
root workflow; source preparation alone does not generate it or establish execution evidence.

After lockfile generation, build from the repository root into the existing shared target, with
no concurrent Cargo command in the checkout:

```text
cargo +1.95.0 build --manifest-path tests/fixtures/prefab-extraction-recovery/Cargo.toml --locked --target-dir target -j 1
```

The executable is `target/debug/nara-prefab-extraction-recovery.exe` on Windows and
`target/debug/nara-prefab-extraction-recovery` on Linux. The command interface is:

```text
nara-prefab-extraction-recovery inspect <root> --host-work N --open-handles N --max-steps N
nara-prefab-extraction-recovery resume <root> --host-work N --open-handles N --max-steps N [--all | --stop-after-files N | --stop-before-ready | --stop-after-ready]
```

All three limits are required nonzero `usize` values. `--max-steps` bounds the total number of
`advance()` and `confirm()` calls together, including the read-only inspection stages. Cleanup
has a separate ceiling of the same number of `close()` calls and retains the operation's original
Host and source allowances. It never advances publication or renews those allowances. Zero is
valid only for `--stop-after-files`. Options cannot be repeated. Inspect rejects every stop
selection. Resume defaults to `--all` when the optional selection is omitted. There is no profile
override: the authenticated operation's stored selected profile remains authoritative.

For a producer-created project, examples from the repository root are:

```text
target/debug/nara-prefab-extraction-recovery.exe inspect <root> --host-work 200000000 --open-handles 128 --max-steps 256
target/debug/nara-prefab-extraction-recovery.exe resume <root> --host-work 200000000 --open-handles 128 --max-steps 256 --stop-after-files 2
target/debug/nara-prefab-extraction-recovery.exe resume <root> --host-work 200000000 --open-handles 128 --max-steps 256 --stop-before-ready
target/debug/nara-prefab-extraction-recovery.exe resume <root> --host-work 200000000 --open-handles 128 --max-steps 256 --stop-after-ready
target/debug/nara-prefab-extraction-recovery.exe resume <root> --host-work 200000000 --open-handles 128 --max-steps 256 --all
```

Use separate producer roots when comparing stop scenarios. A successful Ready publication ends
that root's Pending operation; a subsequent recovery invocation cannot inspect it as Pending.

The consumer opens the root through `project_tooling::host_fs::writable_directory`, authenticates
one `PendingAuthoringRecoveryInspection`, and dispatches its actual kind without rereading it.
An authored schema upgrade is reported as a wrong-kind refusal. The extraction owner begins at
`Inspecting`; only read-only `advance()` calls run until the complete stored-generation Review is
available. Inspect borrows that Review and closes without confirmation or publication. Resume
borrows the same Review, explicitly confirms it, and drives only finite native owner steps.

Stop selections describe actual owner boundaries:

- `--stop-after-files N` stops after at least N file steps are complete, before Ready. These steps
  include Q, its metadata sidecar, and sparse original-source outputs. Already-present outputs
  count as observed progress; they are not claimed as replacements by this process. Zero stops
  immediately after read-only confirmation. A count above `file_count` is refused before confirm.
- `--stop-before-ready` stops at `PublishingReady`, after future Project preparation and binding
  but before the Ready publication call. The project remains Pending.
- `--stop-after-ready` stops at `Complete`, after actual Ready publication and before
  `into_outcome()`. It closes required native custody without consuming the outcome. A later
  ordinary Open must establish its own fresh generation.
- `--all` closes required native custody and consumes the actual `into_outcome()`. The JSON records
  that outcome's Ready receipt evidence, actual schema summary, retained Review-basis comparison,
  and actual source keys. No private Session is exported.

Every invocation writes one compact JSON line to stdout and flushes it before exit. Object fields
and the owner's canonical member order are deterministic; the report includes no timing or
pointer values. The schema is:

- `action`, `status`, `operation: {path, digest: {length, hash}}`, `max_steps`, and `steps` describe
  the invocation and authenticated recovery package. Status is `inspected`, `stopped`, `complete`,
  `rejected`, or `unresolved`.
- After owner admission, `phase`, `source_count`, `file_count`, `completed_files`, `limits`,
  `compilation_limits`, `usage`, `uncertain`, `cleanup_pending`, `refusal`, and `ready_published`
  report the exact boundary observed before close. Host work and peak handles include transport
  inspection. Compilation limits are the default public limits with available numeric accessors.
- `review` contains the stored selected profile/source/root, original-source keys and paths,
  encodings, explicit native participation, observed states, dispositions, dirty comparisons and
  byte lengths. `review.q` includes Q and its metadata's observed states and output lengths.
  The unchanged bundle observation and admitted schema summary are included. Source and recovery
  payloads are never printed; even a 64 MiB package is represented by metadata and lengths.
- `failure` borrows the actual owner failure before cleanup, with its kind, message and at most
  sixteen diagnostic codes. `driver_error` separately describes a CLI boundary or finite-step
  refusal. `refusal` retains the actual returned refusal kind even when the call exposes no stored
  owner failure. Early failures report their stage and available facts without inventing an owner
  view.
- `close` records resolved status, attempts, final phase, pending cleanup, uncertainty, cumulative
  usage, last close error and the actual final failure. Top-level progress remains the stopping
  boundary; `close.usage` includes any additional required cleanup work.
- `outcome_consumed` is false for inspection and every intentional stop, including after Ready.
  Only `--all` can produce `outcome`, which preserves the real receipt's written/published/verified
  digests, identity comparison and publication/durability facts, plus actual schema/source keys.

Exit status is 0 for the requested inspected, stopped, or complete result; 2 for invalid input,
refusal, exhausted steps, output failure, or unresolved cleanup/uncertainty. Errors retain the
owner while observing its actual failure and driving a finite close loop. If close remains
unresolved, the owner stays in scope until whole-process exit; dropping it is never reported as
cleanup completion. Intentional stops finish the current native step and close only required
custody, preserving the fixed Pending package and outputs for the next process.

This executable supplies an independent process boundary for the root tests' all-old, mixed,
all-output and refusal controls. Its source and reports alone do not prove exact final source
semantics, absence of rewrites, strict Check/Play, native Editor behavior, power-loss durability,
or platform qualification. Those claims require the root-owned producer/oracles and actual runs;
inspection-time observations must remain distinct from receipts obtained by this process.
