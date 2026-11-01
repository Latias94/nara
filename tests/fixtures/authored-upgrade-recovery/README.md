# Authored Upgrade Recovery Consumer

This independent executable depends on Nara's `project-workflow-cli` feature without Tooling.
It admits a Pending project's authenticated recovery package through the public recovery owner.
It receives a project location, profile and finite operation limits; it contains no project
conversion code, old Session, Workspace or restored nominal tokens.

From the repository root, build serially into the shared target:

```text
cargo +1.95.0 build --manifest-path tests/fixtures/authored-upgrade-recovery/Cargo.toml --locked --target-dir target
```

Set `NARA_AUTHORED_UPGRADE_RECOVERY_EXE` to the resulting executable's absolute path, then run:

```text
cargo +1.95.0 nextest run --manifest-path tests/fixtures/behavior-languages/Cargo.toml --locked --target-dir target --test authored_upgrade_command --run-ignored only --test-threads 1 -E "test(authored_upgrade_session::process_recovery::subprocess_)"
```

The producer uses the public Session operation and exits after actual Pending publication, before
driving a source step. Separate processes inspect or resume all-old, mixed and all-new sets,
refuse source/metadata/recovery conflicts, and open the actual Ready result. Inspection-time
successor bytes are distinct from replacements performed in the current process. The tests also
hold already-new leaves across recovery to check that they were observed without rewriting.

`--stop-after-sources <count>` interrupts between source steps; `--stop-after-sources all` stops
after sources and bundle, while the project remains Pending. `--stop-after-ready` exits after
actual Ready publication without consuming its receipt. Ordinary Open in a subsequent process
must establish its own fresh generation. These process-interruption checks do not establish
power-loss durability or supply v2 Runtime/Play qualification.
