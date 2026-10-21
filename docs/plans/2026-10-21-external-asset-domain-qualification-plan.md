# External Asset Domain Qualification

**Status:** Complete for the source-backed qualification scope below.

## Outcome

Qualify one independently locked Rust package that contributes a text resource through public
product composition, strict Check and headless Play. Verify its exact source-delivery contract
through the Export owner's publication tests. The
[design draft](../architecture/asset-extension-host-interface-design.md) supplies the broader
direction; the [ADR 0103 refinement](../architecture/adr/0103-concrete-domain-owned-asset-import-execution.md)
adopts the narrower implemented source-domain contract.

## Implementation

1. **Complete.** Add an explicit source-content contribution to `ProductRecipe`, separate from runtime plugin
   hooks and Editor Import/Reimport. The concrete domain owns typed settings and parsing. The
   project Host owns scoped reads, aggregate accounting and exact source receipts.
2. **Complete.** Retain immutable typed domain collections with their admitted content generation. Expose typed
   read access, with private heterogeneous storage and no domain callback receiving a mutable
   `World`. Publish the complete collection with startup or a successful Scene replacement.
3. **Complete.** Add a separately compiled text-domain member to the independent consumer workspace. Reference
   it from persistent data; include a real auxiliary source and domain validation diagnostic.
4. **Complete.** Verify Check source union, useful runtime values, missing contribution, invalid settings,
   aggregate limits, failed Scene replacement and source changes before Export publication.
5. **Complete.** Record implementation evidence, update the owning contract and public guide, and review the
   final diff. Keep unqualified Editor import, cooking, cross-domain product dependencies and
   native presentation claims explicit.

## Verification

Run Cargo commands serially with `CARGO_BUILD_JOBS=1` and the existing `target`. Prefer focused
Nextest suites for the external package, project content, residency and source Export. Check the
runtime-core-only feature combination because recipe composition is available without 2D/Serde.
Use `cargo fmt --all`, fixture formatting, document link checks and `git diff --check`.

## Observed Results

Windows 11, 2026-10-21. Cargo commands ran serially with one build job and the shared repository
`target`. The independently locked consumer did not inherit root workspace dependencies or features.

| Check | Result |
|---|---|
| External `nara-asset-domain-consumer`, locked Nextest | 7 passed; Check/source union, headless typed values, missing domain/dependency, duplicate ownership, settings, repeated reads, pre-execution reservations, panic diagnostics, retained owners and Scene transitions |
| Root `project-tooling` library, locked Nextest | 157 passed, 1 skipped; includes 5 source Export tests |
| Root content boot/boundary/limits/delivery, runtime boot, workflow Check and plugin composition integration tests | 108 passed |
| `cargo check -p nara --no-default-features --features project-workflow --lib --locked` | Passed; existing feature-specific warnings |
| `cargo check -p nara --no-default-features --features runtime-core --lib --locked` | Passed; existing feature-specific warnings |
| `cargo clippy -p nara --no-default-features --features project-tooling --lib --locked` | Passed with existing warnings; no warning in the new source-domain module |
| Independent correctness and adversarial code review | No confirmed defects; static review included the untracked domain and consumer files, excluded `vendor`, and did not rerun Cargo |

After final implementation cleanup, a combined locked Nextest run of the root library and the
seven integration suites passed 265 tests, with 1 skipped. Formatting, local-link checks and final
diff checks are recorded at closure below.

## Closure

- Final independent consumer rerun: 7 passed (`71832278-6d6f-4aef-9fc2-fb636ffdf196`).
- Final combined root run: 265 passed, 1 skipped (`f42269ad-6e94-40db-83c8-ab15b5a9ee7c`).
- Root and independent workspace `cargo fmt --all -- --check`: passed.
- Local-link check: 12 affected Markdown documents, 683 local links, no missing targets.
- Final `git diff --check`: passed. `vendor/` and the pre-existing commit were excluded from this work.
- Independent correctness and adversarial review: no findings. Code review was static; execution
  evidence above comes from the implementation session.

The external package uses public interfaces only. The Export owner uses its existing synthetic
build observations to verify exact source publication and changed/missing dependency rejection.
This does not qualify an actual Cargo-built exported executable containing the external domain,
native desktop presentation, Editor Import/Reimport, hot reload, cooking or platform support.

## Follow-up

The subsequent [compiled executable qualification](../verification/2026-10-21-compiled-asset-domain-export.md)
closes the real Cargo-built exported-executable gap for its recorded Windows trajectory. It does
not change the source-only scope of the observations above.
