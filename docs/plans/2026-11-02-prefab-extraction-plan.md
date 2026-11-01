# Reviewed Prefab Extraction And Occurrence Continuity

**Status:** Root/placement/inline-reference direction selected after independent comparison.
The [pure identity ownership prerequisite is qualified](../knowledge/engineering/verification/2026-11/2026-11-01T215838Z-canonical-source-identity-ownership-prerequisite-25574e28909847799bbcd2c2b1f1d469.md).
No extraction implementation or qualification is claimed; the remaining replacement and complete
consumer stay in this unit.
**Parent:** [Foundation R5](2026-10-23-foundation-architecture-rebaseline-plan.md#r5-durable-addressing-prefab-overlays-and-projection-lifetime).
**Evidence:** [Pinned extraction study](../research/2026-11-02-prefab-extraction-and-occurrence-continuity.md).

## Complete Consumer

Extract an explicitly selected, source-owned branch of Prefab P into a new Prefab Q. Move the
selected root itself, its understood components and its source-owned descendants. Replace that
branch in P with an instance of Q. Preserve effective transforms, component values, hierarchy,
ordering and reference targets. Do not add an observable wrapper entity to make the cut fit an
incumbent representation. A later source edit in Q must propagate through both repeated and nested
instances while explicit pins remain pins and Revert resumes inheritance.

Use two Scenes, repeated/nested P instances, an inactive dirty Scene and Prefab, unopened
dependants and dormant declarations. Include a local entity named identically to a projected
entity in another occurrence, references entering and leaving the cut, a reference to the selected
root, and a nested instance whose declaration is moved. Visual descendants added by an outer
overlay remain owned by that overlay; selection is not permission to move their declarations.

The public Session/Application operation captures, prepares, exposes the complete Review,
confirms, publishes and installs all affected sources. Then perform ordinary edit/Undo,
JSON/RON Save/Reopen and strict Check/Play. The actual Schema basis is unchanged. Successful
publication establishes explicit saved/history checkpoints; cancellation and refusal before the
first possible replacement preserve the old working inputs and history.

## Owning Representation Problems

The current [compiler layout](../../crates/nara_scene/src/prefab/overlay/compile/staged.rs)
mounts a Prefab's roots as children of a containing entity. Merely moving the selected record to
Q retains the old anchor and adds Q's root below it. This changes entity count, hierarchy and
transform composition. Renaming or ignoring the wrapper in an Editor panel cannot repair that
meaning.

The current [neutral reference](../../crates/nara_identity/src/types.rs) contains a document-local
ID or a persistent Runtime reference. [Overlay bindings](../../crates/nara_scene/src/prefab/overlay/references.rs)
have exact payload coordinates and typed occurrence targets, but direct source records have no
equivalent binding. Moving a local reference across the cut can make its target unrepresentable.
A global source-key substitution or preserving terminal ID spelling does not establish its target.

The staged compiler retains one declared node scope/origin and one final entity provenance.
Any root binding must therefore select canonical entity identity and contributor provenance
explicitly, including reference aliases and local-versus-inherited child order. Aliasing two arena
nodes to one materialized entity without those rules is not a complete root design.

There is also an owning-source ordering counterexample: P's root k binds Q, Q owns children d1
and d2, and P owns local child b under k. Ordinary editing of P must express d1, b, d2 without
moving b's declaration into Q or an instance-addition namespace. P's local-ID `children` list
cannot name d1/d2; k's overlay is interpreted in Q and cannot name P:b. An outer overlay over P
in another Scene cannot replace P's own authoring Interface. Resolve this together with root
binding, including whether source-owned typed placements replace the local-only source hierarchy
and lower into the existing flat materialized hierarchy.

## Root Comparison And Selected Direction

| Representation | Leverage | Obligation |
| --- | --- | --- |
| Explicit root-bound instance alongside an explicit forest mount | Represents ordinary branch extraction and retains genuine multi-root composition. | Define root/anchor identity, component ownership, child order, root deletion and invalid source-root changes. Never infer the mode from current root count. |
| Universally single-root Prefab with instance root substitution | Gives every reusable instance one root surface. | Replace current real forest consumers, define local additions and inherited component intent, and prove that the smaller data model improves the common Interface. |
| Separate source entity and instance declarations | Makes declaration ownership independent of materialized entity shape. | Account for the extra author-facing concepts and transform the existing source/history/query operations together. |

The first candidate must preserve the selected-root workflow above. A children-only extraction
experiment is useful as a control, not as the production outcome. Decide root semantics against
actual hierarchy, ordering, provenance, references and subsequent editing, rather than matching
one serialized example.

Select explicit root-bound instances alongside explicit forest mounts. The actual Reference UI
Prefab has two roots; forest composition is a demonstrated use, not a compatibility requirement.
Select source-owned desired placements as the authored hierarchy authority, replacing authored
roots/children. The compiled hierarchy remains a separate flat result. Source and overlay layout
share target grammar and one implementation; each is interpreted in its real declaration scope.

The bound source root contributes to the containing anchor's canonical occurrence. Preserve
their distinct declaration facets and instance slots while materializing one entity, one canonical
provenance and one Runtime identity. Repeated and addition-origin anchors remain distinct. An
instance's selected overlay must yield its specified single root; missing/replaced roots and
extra roots conflict instead of falling back to mounting. Root-binding chains normalize to the
outer containing anchor without losing canonical-base checks.

Compose root components inside-out: source programs, instance intent, containing-anchor complete
records, then outer intent. Every payload retains its declaration scope. Transform follows this
component rule and is not additionally multiplied by a wrapper. Source placements can interleave
local and inherited ordinary children under the same normalized parent. Normalize aliases before
duplicate, self-relative, parent and cycle checks. Persisted declaration order settles same-slot
cohorts; runtime IDs or map iteration never supply order.
Forest mounts keep roots-before-local as their initial composition order, not an immutable
partition: a valid containing-source or outer placement may interleave actual same-parent siblings.

## Reference Comparison And Selected Direction

Compare neutral occurrence references with Scene-owned direct-record bindings and explicit Prefab
inputs. A neutral reference must not pull Scene, Asset or source-file authority into Identity.
A Scene-owned binding must prove exact slot consumption and retain versioned payload/default
meaning. If a reusable Q refers outward, make that dependency explicit rather than hiding a lookup
in a compiler-generated entity name or allowing it to reconnect to another occurrence.

The selected design must cover internal, entering, leaving, nested and repeated references;
root self-references; optional/default values; dormant claims; detach/Revert and conversion-to-local.
Source preservation may retain an unresolved declared input, while activation must resolve every
required surviving reference or refuse specifically. Runtime publication receives fully resolved
references; authoring bindings cannot silently pass through a Native-only spawn path.

If bindings remain separate metadata, their versioned payload must own them atomically. A binding
map on an entity beside its component map allows a component replacement to leave stale reference
intent behind. Compare full-record binding ownership in `SceneComponentRecord` with the current
addition/Set-specific carriers, and define ordinary field-edit/default/migration behavior before
selecting it. Fewer declarations at the Interface matter more than retaining incumbent struct
fields. Separate source meaning from flattened Runtime references where that simplifies the owner.

Select inline authored reference values over persistent payload-coordinate sidecars. Two equal
`SceneLocal("socket")` placeholders can carry different targets in sidecars; swapping the bare
list values cannot express swapping those targets. A field's explicit Null can also hide a
reference-bearing default, for which a strictly consumed current-slot table has no entry. These
are value-language problems, not only metadata placement problems.

Use one component-value language for providers, Schema defaults, ordinary field/history operations
and source records. Lower the pure canonical source-key/ID/path values to Identity, keeping it
independent of Asset/App. Asset keeps actual metadata/path association; Scene keeps typed scope,
occurrence and input interpretation. Extend inline references with explicit authored targets and
declared input use, then remove the displaced addition/Set/property binding tables together.

The alternative generic reference-parameter value tree offers stronger leaf-type separation, but
also requires default/initial-value lifting, migration-domain conversion and source/opaque
representation bridges. A Rust1.95 metadata probe confirms that a default generic parameter alone
breaks unconstrained enum construction; concrete role aliases fix that inference issue but do not
remove the semantic conversions. Prefer one value language plus an owner-created flat compiled
boundary. This choice does not claim that extending an enum alone is Native-safe.

Flat compiled entities contain neither Prefab declarations nor unresolved authored targets/inputs.
All Native admission paths must reject surviving authored-only references after defaults and
before codecs or World mutation, including hand-written codecs and processed admission. Opaque
records stay complete in source preservation; neither their representation nor a pure key grants
interpretation, extraction or execution authority. Retain one shared Reflect walker and one Scene
compiler, with no second catalog or evaluator.

Source-dependent reference defaults need explicit version/field-bound contextual declarations in
the authored record. They remain part of the complete tree even when an explicit value hides them.
Reset/Absent exposes that declared default; Revert resumes inheritance. Extraction rewrites their
outward targets to declared inputs without covertly changing omission into a value pin. Scalar,
Null and context-free Persistent defaults can continue using the canonical Schema template.
Schema succession must account for contextual defaults explicitly. Default editing retains field
capability and preservation constraints; this is not an alternate path around readonly fields.

## Scene Module Interface

The Scene module owns finite construction from a concrete cut. The ordinary caller supplies the
actual source input, selected root and destination declaration/identity; it does not produce or
certify an arbitrary old/new graph. The prepared result owns:

- The exact retained input and fresh output documents.
- Complete declaration ownership correspondence, including dormant nested declarations.
- An injective old/new occurrence correspondence with exact canonical source predicates.
- Every affected structural target, binding, reference and explicit instance-root correspondence.
- The bounded work, retained storage and exact expected source/Schema facts needed for commit.

Borrowed Review exposes facts and outputs without allowing candidate mutation while retaining
proof authority. Preparation failure returns the same captured inputs. A source-local edit or
an effective projection is insufficient authority for a complete-project topology rewrite.
Do not add a general semantic-equivalence engine or a second overlay interpreter.

## Complete Project Owner

Capture one bounded all-profile corpus and the complete open Workspace. Reuse the existing
inventory, file observations, temporary publication and final guarded installation mechanisms
where their contracts are neutral. The corpus now has modified-existing and create-new roles;
Q and its identity sidecar require actual observed-absent destinations. Distinguish an uncreated
new member from deletion of an original member during recovery.

The concrete extraction proof admits the transition from old membership to new membership. It
does not fabricate a Schema successor, field upgrade or CP1 receipt. Final adoption binds all
source/history/saved state, persistence Hosts, lane observations and the unpublished future
project before final Ready. The unchanged nominal authoring basis remains a separate fact.

Publish bounded reviewed recovery inputs before possible destructive replacement. Fix new IDs,
bindings and output bytes once; retries do not allocate new identities or transform values again.
Already-new leaves are observed without replacement witnesses. Source, metadata, missing-root,
Q/sidecar and recovery conflicts must remain repairable and distinguishable.

## Implementation Order

1. Resolve root and reference semantics using current consumers and pinned Godot/Bevy evidence.
   Reconcile the affected Accepted ADRs before changing their production contracts. Record the
   selected alternatives and concrete counterexamples that reject weaker representations.
2. Replace the owning source format/compiler/authoring semantics. Exercise the actual shared
   compiler for Native and source-authoring interpretations; do not create a test-only evaluator.
   Update bounded decode, defaults, references, provenance, construction and exact history together.
3. Implement Scene-owned extraction and complete declaration/occurrence correspondence. Qualify
   root, non-root, nested/repeated, cross-cut reference, dormant and unknown-semantics controls.
4. Integrate complete inventory, borrowed Review, execution retirement, new-member publication,
   extraction recovery and bind-all-before-Ready installation in Session/Application.
5. Qualify an independent public project and actual fresh-process interruption; run affected
   native trajectories. Remove displaced readers/helpers, update guides/ledger and commit the
   complete unit locally. The full foundation Goal remains active.

## Acceptance

The controls must check actual values and reference targets, not only successful expansion:

- No extra materialized root, changed transform, reordered sibling or merged repeated occurrence.
- Unrelated declarations are equal; all affected bindings/hops/defaults and dormant declarations
  have a complete mapping or a specific semantics refusal.
- Local edits and instance intent follow their real owner after extraction; Revert and later Q
  edits preserve the intended inheritance. Complete conversion can localize the result.
- Stale input/basis, late source, collision, unknown affected meaning and exact/one-short capacity
  controls refuse before publication with old working state/history intact.
- Process interruption before/after Q or sidecar creation, between dependant replacements, after
  bundle/project publication and after Ready before acknowledgment recovers from stored outputs.
- Ordinary Open remains strict for Pending; successful fresh Open and Check/Play consume the
  complete new generation. State the actual filesystem tier rather than claiming multi-file
  atomic rename or power-loss durability.

These are the complete CP2 requirements. A root-only or reference-only prerequisite cannot be
reported as completed extraction, R5 or foundation qualification.
