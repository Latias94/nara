# Nara Engine Architecture Language

**Status**: Vocabulary only; terms may describe proposed architecture and do not assert
implementation or ADR acceptance.

**Authority**: Accepted ADRs take precedence. Implementation evidence for the extension terms is
summarized in [Extension Package Concept Guide](docs/architecture/extension-package-concept-guide.md#what-exists-today).

Nara is a Rust-native game engine product whose runtime, authoring, import, tooling, and delivery
workflows share explicit declarations while retaining separate authorities and lifecycles.

## Architecture Basics

**Module**:
A logical ownership unit with one Interface and hidden implementation; it may be a function, Rust
module, crate, or larger subsystem.
_Avoid_: Assuming every Module is a Rust `mod`, crate, process, or service

**Declaration**:
Bounded, inspectable data stating what a package or domain intends to contribute.
_Avoid_: Executable implementation, active registration

**Implementation**:
Compiled code capable of performing a declared role after verification and a later authority grant.
_Avoid_: Declaration, semantic plan, proof of support

**Authority**:
The scoped ability to observe or mutate protected Host or domain state.
_Avoid_: Metadata claim, receipt, stable identifier

**Generation**:
A non-reused identity for one compiled, candidate, or active state used to reject stale evidence and
results.
_Avoid_: Semantic version, frame number, reusable counter

**Execution Affinity**:
A constraint on where an implementation may execute, such as a main-thread lane, worker pool, or
future isolated process.
_Avoid_: Permission, Host role, product target

## Extension Packages

**Source Extension Package**:
A product-facing distribution unit that declares one or more Nara contributions and is anchored to
resolved source and build provenance.
_Avoid_: Runtime plugin, Cargo crate, cooked content package

**Contribution**:
One stable, inspectable capability declared by a source extension package for a specific contract,
such as runtime behavior, schemas, asset import, or tooling.
_Avoid_: Callback, permission, plugin instance

**Contribution Contract**:
A versioned semantic agreement owned by one engine domain that defines what a kind of contribution
means without granting execution authority.
_Avoid_: Cargo feature, Rust ABI, Host interface

**Atomic Package Authoring**:
The authoring rule that all compiled contribution claims for one package are submitted as one
package draft or rejected together.
_Avoid_: Atomic artifact publication, runtime activation transaction

**Package Draft**:
An inactive package-authoring result containing declared contribution claims before final catalog
verification and Host selection.
_Avoid_: Resolved package, active package, verified catalog

**Contribution Locator**:
A stable generated reference to one canonical package declaration. It identifies what must be
verified but is not itself a verified key, executable lookup handle, or authority.
_Avoid_: Runtime registry key, verified contribution key, capability

**Binding Claim**:
A typed claim that associates one declared contribution locator with a compiled domain
implementation while conferring no authority to execute it.
_Avoid_: Verified contribution key, active provider

**Verified Contribution Key**:
A private typed identity minted only after final catalog admission joins the canonical declaration
to the matching compiled binding evidence.
_Avoid_: Package-authored key, capability, executable lookup handle

**Provider**:
A domain implementation selected to perform one specialized role through an owner-issued narrow
Interface.
_Avoid_: Domain owner, universal plugin, package resolver

## Contract Resolution

**Final Catalog Admission**:
The private Leaf operation that joins a root-selected canonical declaration, typed binding claim,
and compiled contract/Adapter evidence into one sealed admission bundle without invoking a
provider/factory or acquiring Host authority.
_Avoid_: Package installation, dynamic loading, Host activation

**Leaf Contract Kernel**:
The domain-independent Module that validates common declaration/binding structure, including the
private final catalog admission operation, and delegates domain meaning to catalog-verified pure
decoders and resolvers. It invokes no package provider or factory, grants no Host authority, and
depends on no runtime, import, schema, tooling, or native Host implementation.
This is internal architecture vocabulary, not a public authoring Interface.
_Avoid_: Operating-system kernel, package manager, plugin registry

**Semantic Plan**:
An immutable, typed description of selected domain behavior produced without executable callbacks
or Host authority.
_Avoid_: Active registry, mutable builder, executable plan

**Resolution Receipt**:
A non-authoritative proof that a specific set of declarations was validated into a specific
semantic plan.
_Avoid_: Capability, activation token, serialized authority

**Opaque Inactive Transfer**:
A move-only carrier that retains verified compiled implementation values across pure resolution.
Only an admitted domain binder may consume it, and no public operation can inspect or invoke it.
_Avoid_: Provider registry, callback bag, execution capability

## Product Composition

**Concrete Root Composition**:
The executable-specific assembly step that selects supported contracts and builds concrete typed
projections for one product such as the Editor, a server, or a cook tool.
_Avoid_: Repository root, universal Host, generic plan registry

**Concrete Projection**:
A typed, inactive product result whose fields correspond to the domain plans supported by one
specific executable.
_Avoid_: Type-erased plan bag, active engine state

**Domain-Specific Host Binding**:
The verified association of a semantic plan with one compiled domain implementation and one
concrete target or affinity, while the implementation remains inactive; "Host" here names a typed
owner role, not necessarily a running process.
_Avoid_: Factory invocation, placement, activation

**Bound Plan**:
A typed, inactive domain result containing the implementation selected for a semantic plan but no
native Host authority.
_Avoid_: Semantic plan, running provider, active generation

**Binding Receipt**:
A non-authoritative proof that a semantic plan was joined to an exact compiled Adapter and target.
_Avoid_: Placement receipt, activation receipt, capability

## Domain Ownership

**Host**:
A concrete engine or executable role that owns lifecycle and scoped authority for one supported
domain operation or activation intent; it may coordinate domain owners but is never one universal
engine abstraction. The term does not imply a separate operating-system process.
_Avoid_: Universal `EngineHost`, package provider, process synonym

**Domain Owner**:
The Module that owns one domain's lifecycle, policy, candidate preparation, retirement, and
publication semantics. It does not necessarily own the visibility switch when its candidate is a
required member of a concrete Host activation cohort.
_Avoid_: Universal engine context, global service locator

**Concrete Editor / Project Host**:
The executable-specific owner that selects one activation intent, retains the selected domain
candidate owners, waits for every required member to become ready, and linearizes their visibility
through one private cohort record.
_Avoid_: Universal public Host trait, domain implementation, global service locator

**Runtime Host**:
The domain owner that constructs, drives, and retires isolated runtime candidates and active runtime
generations.
_Avoid_: Package resolver, import worker, repository root

**Import Host**:
The asset-domain owner that selects import providers, tracks attempts and observations, and
publishes verified artifact groups while preserving last-good data.
_Avoid_: Import provider, task executor, filesystem

**Schema Catalog Owner**:
The reflection-domain owner that builds, validates, freezes, and retires schema catalog candidates.
When a candidate belongs to an Editor catalog activation, the concrete Host publishes the complete
cohort rather than exposing the schema candidate independently.
_Avoid_: Schema provider, Editor process, universal Host

**Tooling Workspace Owner**:
The tooling-domain owner that coordinates UI-neutral models, validated commands, selection, and
workspace revisions without making package activation the authority for document state.
_Avoid_: Tooling provider catalog, UI toolkit, mutable World access, universal Host

**Tooling Provider Catalog Owner**:
The tooling-domain owner that builds, validates, and retires candidate topologies of tooling
providers available to workspaces. When selected by an Editor catalog activation, the topology
becomes visible only through the concrete Host's complete cohort.
_Avoid_: Workspace state owner, saved-document publisher, UI toolkit

**Candidate**:
A domain-owned unpublished prospective state that may be preparing, ready, or failed; only a ready
candidate may activate or publish.
_Avoid_: Package draft, readiness receipt, active generation

**Activation Intent**:
A concrete Host request that names exactly which required candidates must become visible together,
such as an Editor provider-catalog activation or a Play runtime activation.
_Avoid_: Package draft, semantic plan, global package transaction

**Activation Cohort**:
The required ready-but-unpublished candidates selected by one activation intent and plan
fingerprint. One Host-private cohort record linearizes their visibility; ordinary compatible asset
reimport and document saves remain separate publication axes.
_Avoid_: Every role in a package, global rollback transaction, public service locator

**Activation**:
The lifecycle transition that makes fully prepared runtime or tool state active. When several
required candidates share one activation intent, the concrete Host performs the transition for the
complete cohort rather than allowing independent active-pointer swaps.
_Avoid_: Binding, construction, publication

**Publication**:
The visibility and authority transition that makes an immutable verified record authoritative for
future readers on one declared publication axis.
_Avoid_: Binding, staging, activation

Activation is a domain-semantic state change; publication is a visibility and authority
linearization mechanism. A domain commit may implement activation by publishing an activation
record, but the two terms are not interchangeable.

**Adapter**:
A concrete implementation that occupies one declared seam and translates a domain plan into the
form required by a specific Host or backend.
_Avoid_: Universal extension object, source package, semantic contract
