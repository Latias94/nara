# Source-Tool Artifact And Project-Source Ownership

**Date:** 2026-10-28. **Status:** Owning ADRs amended; managed artifact publication/inventory,
project-owned text import and reviewed registered-artifact reclamation implemented.
Incomplete-publication recovery has separate public/native qualification; native template import
remains open.
**Research baseline:** `874bfc0`.

The [implementation plan](../plans/2026-10-28-managed-source-tool-artifacts-plan.md) continues
the complete finite-tool lifecycle. This design does not qualify a production reclaimer or
replace the full foundation Goal with a file-management task.

## Problem And Evidence

The installed release currently treats every declared member, including `Content`, as part of
its verified artifact. `PreparedEditorSourceToolRelease::capture` captures those bytes;
`release::installation` inspects them again before admitting installed execution. Editing a
sample inside that installation therefore also damages its release observation.

The protection implementation compensates by recording modification/adoption and intercepting
five document acceptance seams. When the optional content record is unavailable,
`source_tool_content_persistence_rejection` and `source_tool_content_ready_for_close` can gate
an ordinary edited document whose installation membership is still unknown. The previous
qualification establishes that implementation's behavior; it does not make this ownership
coupling desirable.

The original reclamation premise also requires more than a default-stream digest. Native NTFS
probes in `crates/nara_fs/src/platform/windows/file_removal_tests.rs` demonstrate alternate-stream
writes while a restrictive default-stream handle is held, and distinguish writable mappings.
[Microsoft's stream contract](https://learn.microsoft.com/en-us/windows/win32/fileio/file-streams)
states that sharing and oplocks are maintained per stream. An unchanged default stream cannot
stand for every byte disposed of by whole-file deletion.

These findings challenge the model in which the same installed object is both disposable tool
storage and an editable project source. They do not challenge the need for `unlink`.

## Ownership Decision

Use three explicit ownership domains:

| Domain | Owner and admission | Update and removal |
| --- | --- | --- |
| Installed executable artifacts, required tool data and unimported templates | Managed artifact owner in `.nara/source-tools/artifacts/`; ordinary source-document admission refuses this namespace and actual protected-object aliases | Publish a new installation; explicitly reviewed disposal may remove selected whole objects after their users return |
| Imported templates and ordinary project sources | Existing source, document, metadata and history owners outside that namespace | Package update does not overwrite them; package withdrawal or reclamation does not delete them |
| Local, Git and Cargo source packages | Existing source workspace and toolchain | Nara does not resolve or garbage-collect those source checkouts |

Only the fixed artifact subtree has disposable-store semantics. Other `.nara` files include
durable project facts and are not thereby a cache. A conventional store path is not deletion
authority: reviews must retain the actual project association, selected directory/file objects
and bounded member inventory. Arbitrary files found under the subtree are not implicitly owned.

Store membership is an application ownership contract, not OS-enforced immutability. External
changes may make execution admission fail. A fresh explicit disposal review can still authorize
deleting a selected damaged artifact, including its alternate streams; it must not describe
that authorization as evidence that the artifact never changed.

`Content` must not simply become an alias for sample. Runtime-required data stays in the
artifact closure. An editable template gains project ownership through explicit import.
An imported origin remains provenance and never restores installation deletion authority.

## Preserve Actual Unlink

Keep `DirectoryCapability::unlink_file(path, &observed)` and its existing exact-object contract.
The read-write parent supplies mutation authority; the original live object is compared against
the deleting handle; disposition uses that same handle. The adapter refuses unsupported
platforms instead of emulating exact removal with a path check followed by an unrelated delete.

The source-tool owner supplies the domain permission: the selected file is a disposable artifact,
its installation ID and actual directory are absent from the current graph, its actual users have returned, and the
original explicit review remains valid. The generic filesystem primitive must not absorb
installation graphs, history, digests or adoption rules.

The removal receipt establishes namespace removal. Retained readers and unsupported parent
directory synchronization remain separate facts. Do not promise immediate storage release or
power-failure durability.

## Complete Workflow

1. Capture a finite release without writes or execution. Confirm installation into a new
   store-owned directory; publish the ready receipt last. Preserve incomplete publication and
   cleanup custody rather than recursively deleting an unready directory.
2. Register exact installation and requirement bindings in the existing installed graph.
   Disabled and nonselected dependents still participate. A newer installation does not
   silently rebind a dependent to equal or newer release bytes.
3. Inspect and approve captured artifacts through the current finite tool owner. Desired
   enablement is not executable authority. Preserve the existing trusted runtime prerequisite,
   probe, source proposal, review, history and actual process/pipe return boundaries.
4. Preview and explicitly import a supported template into an absent project destination.
   Its published copy is project-owned immediately. Subsequent editing and persistence use
   existing source owners without an artifact protection ledger prerequisite.
5. Withdraw installations through the existing original-basis graph review, including explicit
   dependent cascade. Withdrawal can succeed while physical work is still returning.
6. Prepare a move-only disposal review without scheduling deletion. Capture actual objects,
   original graph/source observations, affected installations, unknown retained siblings and
   all current users. Retained document objects remain protected even if their original names
   have changed; current namespace owners must also remain protected.
7. Confirm that original review, fence conflicting tool admission, and publish a bounded
   removal intent before the first unlink. Wait for accepted publication and actual IO return.
   Failure or uncertainty at this boundary starts no deletion.
8. Unlink selected members individually, retaining each actual result before starting another.
   Cancellation stops subsequent work; it cannot erase earlier receipts. Publish the finite
   mixed results. A failed result publication retains real progress and publication debt.
9. Fresh Open only observes recovery data. Recorded Pending is not Removed; a missing file is
   not proof that this attempt removed it. Resuming requires another explicit original review.

No step is replaced by a no-op review, quarantine, graph withdrawal alone, unknown-file recursive
deletion, a second executor or a universal transaction framework.

## Import Is A Source Boundary

The first independent consumer can import the actual finite text sample. That establishes a
bounded copy workflow, not arbitrary Scene/Prefab template correctness.

Native source formats need their existing owner to prepare identity and references:

- `ProjectLocation` identity includes source universe, path and document kind; the destination
  receives its own location identity.
- Stable sidecars require fresh identity allocation and collision checks. Copying metadata
  verbatim does not satisfy this contract.
- Supported template closures remap internal source and asset references through their actual
  format owners; external references require explicit bindings or refusal. Do not implement
  textual replacement or a second Scene interpreter.
- Schema-owner identity is independent of installation and copied source identity. Existing
  metadata adoption/evolution remains its owner, with its own admission and qualification.

Repeated import defaults to another absent destination. Updating an existing imported document
requires its ordinary explicit source-edit transaction, identity and history contract.

## Implementation Boundary And Deletions

The retained-source alias and physical-pending corrections precede this replacement.
Managed publication/inventory and explicit text import replace the mixed installation/project
content model. The superseded sticky installation-content interception, classification debt in
ordinary Save/Reopen/Close, old management/adoption APIs and old record reader are removed.
There is no parallel compatibility model. The predecessor implementation evidence qualifies the
store/import replacement. The registered-artifact owner now publishes reviewed intent, performs
real per-member unlink and retains partial recovery facts through Session/Application Close.
Its qualification is recorded separately; the predecessor evidence does not establish it.

An incomplete publication retains its original plan, published-directory facts, created readers
and cleanup-only handles in a concrete publisher-created owner. The common inventory distinguishes
Ready and Incomplete without a fictitious ready receipt. Actual receipt temporary paths remain
distinct from the ready receipt. Exact cleanup-to-reader handover and source-owner checks precede
registration; refusal returns original custody, and failed confirmation returns the prepared owner.
Failed asynchronous registration retains actual custody for explicit reconcile/registration retry.
Both kinds reuse the same publisher/review/reclaimer and one canonical format-1 reader.
The [implementation continuity](../knowledge/engineering/progress/2026-10/2026-10-28T182559Z-reviewed-real-artifact-unlink-integration-and-incomplete-publication-follow-up-1de5c56631a146318b4b1098eb0fdfd8.md)
records concrete source gaps and the proposed next unit; it is not qualification of that successor.

Canonical formats remain version 1 because the product has not shipped. No old reader,
conversion helper, compatibility alias or migration guide is required. Existing historical
verification records retain their original scope; they are not current release authority.

ADRs 0020/0035, 0047/0118 and 0070 reconcile the production ownership boundary. The
[replacement evidence](../knowledge/engineering/verification/2026-10/2026-10-28T163225Z-managed-source-tool-artifact-store-and-project-owned-text-template-replacement-7c31339a78064eb8bc385a9bd82b372a.md)
records actual workflow results and limits. The filesystem exact-object guarantee remains unchanged.

## Reference Scope

- [Unity 6.0 package samples](https://docs.unity.com/en-us/engine/6000.0/manual/packages-list/cus-pkg-development/cus-samples)
  explicitly copies samples into project `Assets`; this supports the ownership distinction,
  not Nara's identity-remapping or deletion qualification.
- Godot at `81ab961f91066a63bcbf3ec340962bb1a1ada558`,
  `editor/asset_library/editor_asset_installer.cpp`, imports selected archive members into the
  project. Its ordinary import path supplies no proof of Nara's proposed reviewed disposal.
- Bevy at `8550394`, `crates/bevy_app/src/plugin.rs`, defines `Plugin::cleanup` before schedule
  execution. That construction hook is not runtime package unloading or file reclamation.
- The [qualified protection predecessor](../knowledge/engineering/verification/2026-10/2026-10-28T132217Z-accepted-document-protection-and-repairable-installed-content-close-qualification-8a03235372c74fcbb4a02f7ee23eaca2.md)
  records actual prior behavior and platform limits. It does not qualify this replacement.
