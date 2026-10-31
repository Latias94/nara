# Explicit Cargo Build Selection And Captured Artifacts

Date: 2026-10-31. Status: finite explicit-build outcome qualified; cooked package delivery remains separate.
Parent: [foundation R7](2026-10-23-foundation-architecture-rebaseline-plan.md#r7-target-build-and-base-source-free-delivery).

## Complete Outcome

One independent ordinary Cargo workspace selects its Host tool and Player separately, using
custom package/bin/profile/features, an actual compiler selection and explicit target triples.
The build owner returns exact captured target bytes and bounded provenance independently of
execution. A real Host/target-different build succeeds as a build-only artifact without being
executed on the build Host. The existing source Export and supervised desktop adapters consume
the same general Cargo selection/JSON facts, keeping their deliberate starter presets and
their separate product/Runtime qualification.

Build output is not a content package, active executable Host, Ready Runtime or last-good
activation. Preserve the existing original content base and supervised activation owners.
The complete packaged Player and target-content plan remain the next R7 outcomes.

## Selected Boundaries

- Cargo owns package, dependency, feature and target resolution. Trusted Host selection names
  package, binary, profile, default-feature policy, requested features, target triple and actual
  Cargo/rustc tool paths/identities. A project profile label cannot inject flags or code.
- Metadata supplies the exact manifest owner, workspace membership, target source and a
  workspace-level feature observation. The selected compiler artifact supplies the actual
  compiled features; they can differ from metadata. Cargo decides target eligibility, including
  dependency-qualified `required-features`. Do not implement feature closure or eligibility
  from metadata declarations.
- Reuse a bounded incremental Cargo message decoder for ordinary explicit selections. Move
  desktop/headless conventions into their owning adapters and remove the old fixed decoder
  types rather than retain compatibility aliases.
- The native process adapter owns invocation, bounded local output and physical child return.
  It always uses locked builds. Tools are explicitly selected; no automatic download, new
  resolver or runtime Cargo path is introduced.
- The supervised Editor separates bounded complete machine stdout from lossy diagnostic tails.
  Full metadata and Cargo JSON cannot be evicted by stderr, late polling or small OS read chunks.
  Protocol overflow/cancellation refuses the build; native reader/process retirement remains
  with the existing owner. A larger diagnostic ring is not this protocol boundary.
- Capture exact selected inputs and revalidate before/after build and return. State the selected
  input inventory, per-file consistency and untracked Cargo/native/environment inputs honestly;
  do not claim an instantaneous complete workspace snapshot or hermetic compilation.
- Captured output bytes and exact digest/length retain their own allowance, independently of
  Cargo's mutable output pathname. Artifact integrity and target qualification remain separate.
  A foreign build has no automatic execution or readiness transition.

## Ordered Work

1. Compare pinned Bevy/Godot, inspect current real consumers, reconcile the owning Accepted
   workflow contract and this finite extraction from Proposed0086.
2. Replace fixed pure metadata/invocation/stream decoding with explicit selection and
   Cargo-authoritative resolved facts. Migrate supervised desktop, tool bootstrap and source
   Export without obsolete aliases.
3. Implement the finite native build owner and exact captured artifact/provenance result using
   existing capabilities/bounds. Keep activation and content publication with their owners.
4. Qualify a public independent custom workspace, native tool and foreign build-only target,
   stale selected inputs, wrong/missing/duplicate artifact messages, source/cache-independent
   captured bytes, and original source Export/supervised consumers.
5. Independently review, run appropriate serial owner/public/native checks, update actual
   implementation evidence and guides, commit precise units and locally fast-forward main.
   Keep the complete foundation Goal active.

## Evidence Limits

Only actual foreign compilation qualifies that cell; synthetic JSON establishes decoder
behavior only. Native activation still needs the original product/Runtime path. Selected
source observations do not intercept arbitrary trusted build-script/proc-macro inputs.
No universal exporter, new VFS, remote runner framework, compatibility reader, migration guide,
ABI loader or compiler-like verification script is selected.

## Qualified Outcome And Successor

The [qualification record](../knowledge/engineering/verification/2026-10/2026-10-31T035034Z-explicit-cargo-builds-captured-executables-and-supervised-protocol-qualification-4cfff5128fa64423b6f234ea7b312c63.md)
records final owner151, deadline6, actual independent native/foreign2, original processed process,
real source CLI Export/relocation and final Windows/DX12 supervised2. It retains the reproduced
protocol queue failure, independent corrections and stale cadence-oracle diagnosis. Minimal,
workspace/affected correctness Clippy, strict consumer and formatting checks retain their recorded
source scope. The [build guide](../guides/cargo-builds.md) documents the finite public contract.

The next [cooked Player delivery unit](2026-10-31-cooked-player-delivery-plan.md) supplies the
production catalog/package join. This qualification does not close that workflow or full R7,
adopt Proposed0086/0088 wholesale, or complete the active foundation Goal.
