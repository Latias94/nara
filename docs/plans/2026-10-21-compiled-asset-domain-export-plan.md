# Compiled Asset Domain Export Qualification

**Status:** Complete for the Windows executable-delivery scope below.

## Outcome

Close the executable-delivery gap left by the
[source-domain qualification](2026-10-21-external-asset-domain-qualification-plan.md). A generated
independent product must select the external text-domain package through its real Cargo
dependency and use the same recipe for Check, headless Play and source-carrying Export.

## Work

1. Strengthen the existing compiled custom-project CLI trajectory with a persistent text reference,
   an external domain dependency and a runtime observation of its parsed primary/auxiliary data.
   Keep the existing schema migration, declared second Scene and startup-repair coverage.
2. Run the actual CLI trajectory serially. Export must build and observe the headless and desktop
   products, preserve the exact domain source members, and run the exported headless executable
   from an unrelated working directory after the development project coordinate is detached.
   A missing published auxiliary file must reject before gameplay.
3. Review the change, record actual results and limits, update public guidance and the
   implementation ledger, then create a scoped Conventional Commit.

## Evidence Strategy

This is qualification work, with no expected production behavior change. Extend
`tests/project_workflow_cli.rs` instead of introducing another build/export adapter. The external
package is compiled by the generated product; the test process may include its source to construct
the matching public schema/content inspection recipe without adding a root development dependency.
All product IO, build receipts and exported executable observations use existing public paths.

Run Cargo commands serially with `CARGO_BUILD_JOBS=1` and the shared `target`. The ignored CLI
test deliberately invokes nested Cargo builds and therefore needs explicit selection. Use the
ordinary CLI tests and the seven external source-domain tests for focused regressions, followed
by formatting and document/diff checks.

Native desktop build/startup agreement is scoped to the Export acceptance observations actually
executed. This work does not claim full native presentation/input qualification, another platform,
Editor Import/Reimport, cooking, scripting or hot reload.

## Closure

The [execution record](../verification/2026-10-21-compiled-asset-domain-export.md) records the real
release-product observations, final corrected trajectory, focused regressions, lint/format/link
checks and completed independent review. The public guide and ADR implementation ledger now link
that evidence. No production runtime change was needed.
