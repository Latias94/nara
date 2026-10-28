---
title: Native Prefab template identity and reference preparation
date: 2026-10-29
status: qualified for the closed native Prefab collection workflow
related_plan: 2026-10-28-managed-source-tool-artifacts-plan.md
---

# Native Prefab Template Import

Continue the full foundation rebaseline after `main@ba6dc1e`. The managed-artifact plan still
requires native source identity, sidecars and reference remapping. Its text-copy and real unlink
qualification does not establish native template correctness.

## Complete Workflow

Import a declared finite Prefab closure from an installed tool into an absent directory under
the configured Prefabs root. Prepare without writes; review exact destination documents and
identity mappings; confirm through the existing Session Tasks lane; observe the actual directory
publication and physical return. Open the entry through the ordinary Browser/source owner,
edit, Undo/Redo, Save and reopen. Withdraw/reclaim the installation and prove that the project
copy survives. A second import receives independent identities and internal references.

Use one strict format-1 `prefab_template` descriptor declared as Content. It states the source
universe, native identity policy, original Prefabs root, entry and complete document/member coordinates. It is data,
not a script, package resolver or second Scene format. Input document bytes and sidecars remain
admitted by their existing format owners. Location keys use the descriptor's full original
project paths; stable keys require matching captured sidecars. Destinations use actual project
settings. No identity is inferred from the installed directory name.

The initial closed subset includes Prefabs, nested Prefabs and schema-declared references to
that same copied set. Unbound external assets, persistent entities and unavailable/mismatched
Schema records refuse preparation. Scene/Prefab/image closures across separate roots and explicit
external bindings remain subsequent work; do not weaken the complete architecture goal to this
subset or describe it as arbitrary native template support.

## Owners And Contract

- Reflect collects and rewrites only declared asset-reference leaves, including payload-relative
  scalar leaves. It validates original observations before callbacks and charges replacement
  allocation before construction. Ordinary strings are never reference candidates.
- Scene prepares copied document data and remaps instance references, overlay bases, every
  occurrence hop and all declared payload references. Local entity IDs remain in their document
  namespace. Schema owner identity stays unchanged. Reference-bearing defaults become explicit
  copied values where required; unsupported absence semantics refuse rather than retain an old
  hidden default. Preparation has aggregate raw/value/reference-work/depth bounds.
- Project Host authenticates descriptor/member/sidecar bytes against the captured installation.
  Stable destinations allocate fresh IDs against the original indexed metadata set and copied
  closure. Location destinations use independent complete project coordinates without
  interpreting unrelated asset metadata. Host encodes via existing format owners and retains
  original project/selection/Schema observations. The review fixes all identities and bytes;
  confirmation never silently rebuilds the candidate.
- The existing template-import Tasks lane owns scheduling, cancellation and physical return.
  A concrete directory-publication result remains separate from source admission and tool
  execution. Browser/source-index refresh and ordinary source opening supply later admission.

Directory publication must obey the actual Windows adapter. Retaining child handles prevented
populated-directory rename in the tool-installation predecessor. The existing project-creation
owner instead quiesces/closes staged member handles, publishes the exact directory, and freshly
admits current members. Reuse that distinction if qualified here: do not compare closed historical
file IDs or claim continuous member provenance. Retain exact directory/orphan outcomes, never
recursively remove project copies, and never call a published-but-unadmitted copy unpublished.
An import operation may leave an explicitly observed staging orphan; no automatic cleanup claim
follows. Directory namespace atomicity is separate from file-content immutability and durability.

## Implementation And Verification

1. Add the bounded Reflect leaf rewrite and source-owned Prefab copy through production helpers.
   Challenge omitted defaults, selected scalar payloads, nested declaration/target locations,
   unknown records, external references, duplicate identities and cumulative budgets.
2. Capture the native descriptor/closure and generate exact destination files/sidecars through
   existing owners. Join it to the existing import lane and Application forwarding, without a
   parallel executor, source store or generic transaction framework.
3. Qualify actual multi-file publication, destination/parent competition, cancel/Close while
   return is held, partial staging failure, fresh Open, ordinary history/persistence and installed
   artifact reclamation through an independently compiled consumer. Add matching native scope
   only where the actual visual path changes; the future Nara-owned UI remains independent.
4. Independently review, run serial focused nextest and affected feature/static checks, update
   owning ADRs/guide/ledger and immutable evidence, then commit and ff local main. No push/PR.

The parent owns Cargo and root formatting with `CARGO_BUILD_JOBS=1` and shared `target`.
Reflect worker owns only asset-reference helpers/exports/tests; Scene worker owns only the new
Prefab-copy module/exports/tests; parent owns Host/Tooling/Application integration and documents.
Workers do not format, run Cargo or commit. Preserve concurrent work and untouched `vendor/`.

## Qualified Result

Filesystem retained creation is `0bd3d62`, Reflect reference rewriting is `a2a06f0`, Scene copying
is `c533ad1` and Host/Tooling/Application plus the independent workflow are `d98f39a`. The
[verification record](../knowledge/engineering/verification/2026-10/2026-10-28T211402Z-native-prefab-template-identity-remapping-and-retained-publication-qualification-288aa15af1244e52b81c3c364689d633.md)
records 592 source-library passes, 1,258 final Host/Tooling/Editor/filesystem passes with 38 explicit
skips, 33 independent consumers and separately selected actual managed/Application/Windows-DX12
passes. Correctness Clippy, minimal feature and root/fixture formatting checks pass. That native
selection retains its tool/facet/selection scope; Prefab import/reclamation uses the independent
window-free source-format consumer. No newly drawn import control is qualified.

The closed Prefab unit is complete within those limits. The parent lifecycle and complete
foundation Goal remain active. Scene/image closures, external bindings, general asset templates
and complete management/custom-drawn UI remain separate. The
[creation-metadata evolution successor](2026-10-29-installed-owner-creation-metadata-evolution-plan.md)
separately qualifies bounded existing-owner creation changes. General stored-value migration
remains outside both workflows.

## References

Pinned source comparison: Godot `81ab961f91066a63bcbf3ec340962bb1a1ada558`,
`scene/resources/resource_format_text.cpp`, records external resource UID, path and local resource
ID separately; `editor/asset_library/editor_asset_installer.cpp::_install_asset` writes selected
archive members to their chosen project destinations. Those facts support separating import
coordinates from native resource identity; that ordinary installer is not proof of Nara's
retained publication or complete source-reference remapping.

Bevy `85503941e488152711a4695bbe248e9ed9f453fc`,
`crates/bevy_world_serialization/src/dynamic_world.rs::write_to_world_with`, first creates its
entity map and then applies mapped reflected components. This is Runtime World instantiation,
not a durable source-document copy contract. Nara similarly keeps reference interpretation with
its admitted type metadata, while an independent source namespace lets whole-document copies
retain local IDs. These are source facts and design inferences, not new Bevy/Godot runtime tests.

- [Managed artifact plan](2026-10-28-managed-source-tool-artifacts-plan.md)
- [Artifact ownership](../architecture/source-tool-artifact-ownership-design.md)
- [ADR 0038](../architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md)
- [ADR 0070](../architecture/adr/0070-capability-oriented-filesystem-substrate.md)
- [ADR 0118](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Full foundation plan](2026-10-23-foundation-architecture-rebaseline-plan.md)
