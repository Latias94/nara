---
title: Retained Pack And Resource Lifetimes
date: 2026-10-31
subject: Pinned Bevy/Godot encoded backing and decoded ownership constraints
---

# Retained Pack And Resource Lifetimes

This bounded read-only source comparison supplies constraints to the
[backing design](../architecture/runtime-content-backing-and-residency-design.md).
It does not qualify Nara runtime behavior, measure memory/speed or adopt an upstream API.
The inspected local pins are Bevy `85503941e488152711a4695bbe248e9ed9f453fc`
and Godot `81ab961f91066a63bcbf3ec340962bb1a1ada558`.

## Backing Object And Authenticated Bytes

Godot registers a pack pathname, offset and size. `FileAccessPack` reopens that pack pathname
for each member. Nara's original-context guarantee needs a different choice: retain the exact
opened file capability and never resolve a later acquisition through the current pathname.
That preserves object authority; it does not freeze the object's bytes.
Sources: [pack metadata](../../repo-ref/godot/core/io/file_access_pack.h#L70),
[member open](../../repo-ref/godot/core/io/file_access_pack.cpp#L526).

Bevy's processor opens input to calculate its hash and then opens it again for processing or
copying. The prior read cannot prove the later read's byte identity. Nara's finite pack comparison
must authenticate the exact private member buffer passed to the codec, rather than hash a range
and reread it for decoding.
Source: [processor input reads](../../repo-ref/bevy/crates/bevy_asset/src/processor/mod.rs#L1106),
[processing](../../repo-ref/bevy/crates/bevy_asset/src/processor/mod.rs#L1168),
[copy path](../../repo-ref/bevy/crates/bevy_asset/src/processor/mod.rs#L1224).

Godot clips member reads to a declared range, but its ordinary `get_buffer` path ignores the
actual returned underlying read length and reports the planned count. Nara must account for
checked range sums, the original opened extent, actual EOF/short reads and independent positional
cursors when sharing one file. A directory digest field is not evidence of a verified read.
Source: [PCK read/seek implementation](../../repo-ref/godot/core/io/file_access_pack.cpp#L443).

Bevy's processed reader locks coordinate participants in its processor protocol; they do not
exclude arbitrary external writes. Successful Nara reads prove agreement with the original
member digest, not a simultaneous physical snapshot of every pack member. An earlier deep report
does not guarantee that a later file read will still be available.
Source: [processor-gated reader](../../repo-ref/bevy/crates/bevy_asset/src/io/processor_gated.rs#L48).

## Decoded Lifetime

Bevy processes final strong-handle drops to remove decoded assets. Godot's resource cache stores
resource pointers while actual `Ref<Resource>` and loader tasks retain values. Both provide
useful separation of indexing and decoded ownership, but their live handle/path policies do not
establish Nara's exact immutable base/plan agreement.
Sources: [Bevy asset events](../../repo-ref/bevy/crates/bevy_asset/src/assets.rs#L568),
[strong handle](../../repo-ref/bevy/crates/bevy_asset/src/handle.rs#L98),
[Godot cache](../../repo-ref/godot/core/io/resource.h#L197),
[loader ownership](../../repo-ref/godot/core/io/resource_loader.cpp#L582).

Nara already passes an actual acquisition and metadata reservation through published Image
retention and into prepared pixels. Replacing its encoded backing is sufficient to avoid keeping
unrelated member vectors alive; replacing that acquisition with a bare file handle would lose
the original plan, decoded reservation and base identity.
Sources: [Image residency](../../src/project_host/runtime/content_residency/images.rs#L195),
[prepared retention](../../crates/nara_image/src/prepare.rs#L31).

## Selected Comparison

Keep complete captured-directory memory and add one retained local file with authenticated
temporary member reads and no encoded cache. This directly compares encoded residency while
reusing actual current-demand traversal, typed acquisition, physical task cancellation and native
publication. A cache is a later measured policy, not required to fix the ownership boundary.

The decisive cases are original-object versus replacement-path reads, simultaneous members with
independent cursors, truncated/changed ranges, exact overlapping decode budgets, overlapping
bases, held cancellation and escaped prepared pixels after Scene retirement. Eager input removal
continues to succeed. Pack mutation must reject a later affected read without fallback, while
already decoded values remain valid. Throughput, OS freezing, remote storage and general streaming
are separate claims.
