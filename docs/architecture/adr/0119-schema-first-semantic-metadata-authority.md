# ADR 0119: Schema-First Semantic Metadata Authority

**Status**: Accepted
**Date**: 2026-10-23
**Owner**: `nara_reflect`, persistent document domains and native reflection adapters
**Supersedes**: [ADR 0004](0004-use-bevy-reflect-backed-component-metadata.md)
**Related**: ADR 0002, ADR 0011, ADR 0045, ADR 0051, ADR 0081, ADR 0090, ADR 0098,
[Proposed ADR 0120](0120-schema-publications-and-execution-representations.md)

## Context

ADR 0004 selected Bevy reflection as the substrate of all persistent metadata. The implementation
has since developed an explicit Nara Schema/value/codec/migration model. Native
`PersistentComponentProvider` requires `Component`, not `Reflect`; independent consumers already
declare recursive semantic Schemas without Bevy reflection. Requiring reflection as a second
authority would describe neither the current native path nor future runtime-defined data.

This decision corrects metadata authority. It does not implement a scripting language, dynamic
components, owner-local reload, a general object model or a second native binding mechanism.

## Decision

1. Nara's admitted semantic Schema, stable type/field/owner identity, canonical value grammar,
   codec and migration contracts are authoritative for persistent data and schema-driven tools.
   Type names, Rust `TypeId`, Bevy `ComponentId` and VM identities are not persistent identity.
2. Native Rust execution remains typed. The current ADR 0081 native binding retains its complete
   Schema association, exact-`T` preparation and World-bound one-to-one mapping. This decision
   does not admit incomplete runtime records by bypassing native freeze checks.
3. Bevy reflection may support native inspection/access where a concrete caller uses it. It is
   an adapter, not a prerequisite for every persistent provider or a parallel mutable source of
   persistent Schema truth. Removal of unused reflection storage is an implementation change to
   verify separately, not a consequence inferred from this documentation commit.
4. Keep metadata planes distinct: persistence/eligibility, semantic constraints, presentation
   preferences and domain-specific interaction/binding descriptors. Base capabilities may stay
   closed. New domain metadata needs a typed/versioned owner and explicit unknown-required-data
   behavior, not additions to an unbounded global attributes bag.
5. A Schema catalog is not itself an executable binding, native-code trust, valid file contents,
   Runtime readiness or a rendering/presentation receipt. Operation-specific owners prove those
   facts. Unavailable records remain losslessly authorable under ADR 0090 without fake native
   placeholder components.

Runtime-defined Behavior representations and narrower publication generations are considered in
Proposed ADR 0120. They do not become supported through this accepted metadata correction.

## Alternatives

- **Bevy TypeRegistry as the authority:** reuses mature native reflection but cannot honestly
  represent the existing explicit persistent model as merely derived reflection. It also ties
  runtime-defined data to a native type universe unnecessarily.
- **Serde alone:** remains useful for envelopes but does not define stable property identity,
  reference traversal, editor operations, migrations or runtime binding proof.
- **A universal reflected object runtime:** would duplicate ECS identity and create a large
  speculative callable/type system. Not selected.
- **Schema-first semantics with optional native adapters:** selected; it states the actual
  durable boundary while preserving useful ecosystem implementation.

## Consequences And Verification

- Ordinary native gameplay keeps `Query`, `Res`, typed components and the full advanced Bevy path.
- Persistent providers need one complete semantic declaration, not parallel Bevy/Serde/Nara
  declarations manually synchronized by authors.
- Current recursive non-Reflect consumers and native binding/format tests are the implementation
  evidence to retain when code changes. This documentation correction did not rerun them.
- New storage families require their own prepare/encode/retire and missing-data qualification;
  they do not weaken the current native mapping.
- The implementation ledger owns test revisions and remaining coverage. The
  [reassessment D02](../foundation-architecture-reassessment.md#d02-schema-authority-and-executable-representations)
  records the full alternatives and follow-up workflow.
