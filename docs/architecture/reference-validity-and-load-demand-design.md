# Reference Validity And Current Load Demand

Date: 2026-10-30. Status: implemented and
[qualified in bounded scope](../knowledge/engineering/verification/2026-10/2026-10-30T172312Z-reference-validity-and-current-demand-production-replacement-qualification-b16511fa33a74c2b808270394b28bf62.md).
Owner: [foundation R6/R7](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r7-target-build-and-base-source-free-delivery).
Execution: [complete consumer plan](../plans/2026-10-30-reference-validity-and-load-demand-plan.md).
Baseline: `8e175db`. This design does not implement CP4 backing/residency policy.

## Problem And Terms

The current source and processed loaders interpret every declared AssetRef as current preparation
demand. A Door from A to B, then B to C, prepares resources for all three even when only A is
materialized. Cycles are finite and transactional publication is correct; the selected resource
set is too broad for the named future-level workflow. Moving that work onto another thread or
retaining a non-owning handle does not change which resources were requested.

A **reference** names a logical target. **Current demand** says that the current consumer requires
that target's decoded value/dependencies. A **resolved target** has an actual source/catalogue
binding, not necessarily a semantically valid decoded payload. **Strict content validation**
executes the selected production parsers/codecs over the delivery closure. **Native residency**
and accepted render presentation remain separate downstream facts.

Optional/null describes whether a reference value is present. It does not describe load demand
or permit a present reference to name missing content. Travel-root authorization is independent
from all these facts.

## Selected Contract

1. Declare `AssetReferenceDemand::{Immediate, Deferred}` in the asset vocabulary shared by Schema
   and concrete Runtime codecs. Schema stores it on each recursive AssetRef leaf, including nested
   Records, collection items and defaults. The canonical shape is `AssetRef { demand }`; there is
   no retired unit-variant reader. Fingerprints include demand. An ordinary Rust `AssetRef` field
   codec constructs Immediate; explicit derive metadata can mark a field's asset leaves Deferred.
   Hand-authored nested shapes can mix both. Reject an annotation with no asset leaf instead of
   silently accepting meaningless metadata.

   The derive delegates shape authority to its existing sealed field-codec family, including
   aliases and recursive supported containers. It does not infer Rust types from token spellings.
   Any compile-time asset-leaf predicate must be supplied by that same closed codec family and
   agree with its shape; the annotation overrides all asset leaves of the field by definition.
   Do not make every Schema provider fallible or add a public field-codec protocol solely for
   this annotation. Its final diagnostic mechanism remains subject to focused implementation review.
2. Preserve actual leaf demand through the one bounded reference walker, selected property
   inventory, Prefab composition and declared-reference rewriting. Rewriting identity cannot
   change demand. Do not add a second traversal or encode Deferred as an opaque string.
3. Replace the Runtime codec's required-only visitor with an actual Product-and-demand visitor.
   Keep pure borrowing, bounded original work, explicit empty inventories, panic containment and
   unchanged propagation of the caller's refusal. This does not add a codec-side loader.
4. The concrete Runtime catalogue retains Required for Immediate, adds Reference for Deferred,
   and retains Soft's existing missing-allowed meaning. Required and Reference both require
   catalogue membership and executable format agreement. Soft grants neither target validity nor
   current demand and cannot satisfy a present Schema/codec reference.
5. Normalize actual dependencies per complete Product: any Immediate occurrence wins; otherwise
   the aggregate is Deferred. Compare this complete mandatory set with Required/Reference
   declarations before following a member's outgoing edges. Reject missing, extra mandatory or
   demand-mismatched edges. Optional Soft metadata is excluded from this exact mandatory set.
   This closes the current one-way check that allows an extra catalogue edge to trigger work.
   Soft metadata remains budgeted, cannot duplicate a mandatory Product and never enters either
   preparation or deep mandatory validation. Payload visitors cannot emit Soft.
6. Ordinary preparation walks only verified Immediate edges. Each visited member is decoded once
   per current preparation under the selected original definition; its actual dependency result
   drives further demand. Do not traverse unverified catalogue edges, re-decode for discovery and
   publication, or charge only the final collection after intermediate allocation. Retain only
   actually demanded CPU data/native preparation in the candidate. Cancellation and publication
   use the existing owners and exact receiving baselines.
   Use a private ordered pending set containing only demand revealed by already admitted parents.
   Choose the next currently pending Scene by canonical member order, then Asset by canonical
   codec-definition/member order. A decoded X may reveal an earlier-priority Y; Y cannot precede
   X's discovery/admission. This replaces the old global family-first decode order. Final typed
   collections sort independently by canonical identity. Equivalent recipe insertion order must
   produce the same discovery/admission order and budget outcome; this is no public scheduler.
7. Strict validation traverses both mandatory roles, including dependency-only members. It invokes
   the same selected production compiler/codec and dependency admission, verifies actual compound
   membership, and drops nonresident outputs between finite validation units. It does not publish
   native stores. Opening an immutable base still proves captured bytes/agreement; that operation
   alone does not prove every member's payload semantics. Expose the deep result distinctly.

   Processed deep validation decodes each member once. Before dropping the output, retain only
   bounded CPU facts needed by the existing Image/TileSet/Tilemap contracts: exact Product and
   Image extent, the Copy TileSet descriptor, and a compact Tilemap atlas requirement. The Tilemap
   owner still parses every cell and checks duplicate coordinates through its one parser. Because
   atlas membership is a continuous prefix of tile indices, its greatest actual tile is a
   sufficient witness; ties use the smallest coordinate for deterministic diagnostics. Empty
   cells retain their existing no-atlas-obligation behavior. An obligation is not a successful
   cross-member validation until the actual descriptor is checked.

   These facts retain independent shared Work reservations through final verification, overlapping
   the current decoder's promises and parser scratch. Vector growth admits old and new allocations
   before transfer. Deep Check may still refuse its finite summary budget; it claims neither
   constant memory, native device creation nor arbitrary gameplay success. It does not keep Scene
   candidates, Image pixels or acquisition owners alive merely to finish the deep traversal.

The catalogue is a compiled dependency declaration, not a second semantic owner. The executable
Schema/codec determines actual demand. One typed Product may have both leaf uses without two
resource identities. No new generic provider, VFS, broker, residency cache or prefetch scheduler
is selected by this design.

## Source And Editor Semantics

`ProjectContentLoader` distinguishes current preparation from strict saved-source delivery.
For a Deferred reference, ordinary preparation resolves the actual canonical reader, source kind,
source/Product selection and identity metadata, checks the source is present/readable, and retains
the narrow binding/receipt facts needed by that result. It does not expand the target Scene or
prepare its native resource closure. These facts are called resolved targets, never validated
decoded Products. A future request revalidates and decodes its actual target under the original
admitted plan before Ready/publication.

For a fresh read-only Authoring binding, identity means the admitted canonical source/key and
metadata association, not the continued lifetime of a particular native file ID. Deferred
revalidation checks that reader/key/kind/metadata association and actual presence/readability.
It does not authenticate the target body, whether bytes change in place or the leaf is replaced.
Do not add a full-body digest merely to distinguish those equivalent changes. Immediate fresh
disk dependencies separately compare their actually consumed content. Open working copies and
expected-object writes keep their exact file-owner custody; neither rule is inferred from a
missing token or from a changed copied ID. The owning ADR0047 records this distinction; older
qualification that rejected every Deferred native replacement retains its historical scope.

Generic metadata cannot prove an arbitrary compound auxiliary Product exists. Strict Check/Export
must execute its actual selected importer and membership query. Ordinary Deferred resolution
must not claim that proof from an assignment name or Host-parsed private importer settings. A
future domain-specific bounded target-validation facet requires its own real consumer; there is
no default-success facet here. Invalid payload/missing auxiliary selection refuses deep validation
and the actual acquisition, while A's ordinary preparation does not masquerade as either result.

Source Check/Export follows both reference roles over the saved-source delivery closure, even
when an encountered Scene is not a public travel root. Reuse actual readers, compilers, importers,
per-root snapshot disposal and exact receipt merging. Validation-only files cannot silently enter
the current Runtime's decoded/native collection. Distinguish the startup immediate receipt from
the independently retained complete delivery validation result; Export consumes the latter.

Authoring Immediate Scene dependencies continue to use the exact admitted dirty working copy,
Schema basis and revision. Deferred B does not put B's document/body revision/resources into A's
current preview participants. Its source identity/metadata/resolution changes still invalidate the
target binding facts. Any separately advertised deep validation of current working copies must
capture and fence dirty B's Session/revision/Schema; disk Check cannot certify a dirty document.
Preserve missing/unavailable declarations and report unavailable target resolution without
inventing source data or falling back to another same-name source.

Immediate Scene data remains useful independently of travel. A can read an encounter/rule Scene
D as finite CPU data without instantiating it. Keep its flat ownership collection and provide an
actual borrowed public read in the complete consumer. D's own Immediate edges intentionally
propagate demand. Neither retention as data nor Deferred reference membership authorizes travel;
D must separately be declared as a requestable Scene root.

## Alternatives And Reference Lessons

- Keep every AssetRef eager: preserves current behavior but makes a valid future-level graph the
  current native preparation set. It fails the named workflow before package contract freeze.
- Reinterpret Soft: conflates missing-allowed membership with required future delivery and weakens
  Check/Export. Rejected.
- Put demand only in the catalogue: lets package bytes override the executable consumer's actual
  needs. Put it only on a top-level field: cannot express mixed nested reference use. Rejected.
- Defer native upload after still decoding everything: can be a separate backend optimization,
  but does not establish selected preparation or bounded target-request costs. Insufficient.
- Hide a destination path/UUID as opaque data: prevents the semantic reference visitor from
  checking delivery and rewrite operations. Rejected.

The [pinned reference comparison](../research/2026-10-30-reference-validity-and-current-demand.md)
shows Bevy separates load/dependency/render readiness, while actual LoadContext calls still start
loading. Its Uuid Handle does not prove membership. Godot normal external resources start loading;
path/placeholder mechanisms defer acquisition but do not automatically supply complete reference
enumeration/validation. Borrow these distinctions without importing either object/cache model.

## Qualification Boundary

Use actual A -> B -> C Door Products, a shared atlas, B-only heavy resources and Immediate CPU
Scene D. Strict Check/Export and processed deep validation must cover all targets/resources.
After resetting isolated decode observations, ordinary A startup/preparation must decode no B-only
resource. Real B/C requests then prepare their own closure, with Ready/cancel/commit, failure
preserving the current Scene, and no dependency-only D travel authority. Validate nested/default/
overlay demand, mixed duplicate roles, extra edges, missing targets and auxiliary products.
Include X revealing an earlier codec-definition Y, reversed recipe contribution insertion and
exact/one-short overlapping decode/output budgets. Require deterministic refusal/success and
one acquisition shared by typed/native consumers, not a global pre-sort of undiscovered work.

This does not change the eager base's encoded capture/removal guarantee, implement eviction,
qualify streaming backing or claim improved RSS/P99. Numerical bounded decode counts and actual
resident membership are the CP3 oracle; CP4 owns the encoded backing comparison separately.
