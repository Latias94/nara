# Retained Runtime Pack And Encoded Residency Plan

Date: 2026-10-31. Status: qualified in the
[recorded scope](../knowledge/engineering/verification/2026-10/2026-10-30T184847Z-retained-runtime-pack-and-independent-encoded-residency-qualification-386d2e5e9c124b66bc4390f69b796053.md).
Baseline: `c0a8325`. Parent: [foundation R6/R7](2026-10-23-foundation-architecture-rebaseline-plan.md#r7-target-build-and-base-source-free-delivery).
Design: [backing and residency](../architecture/runtime-content-backing-and-residency-design.md).
Research finding: CP4; qualified reference/demand CP3 remains a separate completed unit.

## Complete Outcome

An independent game writes a real local pack from an admitted captured base, opens it through
one Host-issued file capability and executes the same processed startup/later Scene workflows.
Preparation decodes only current demand. Encoded member buffers are temporary, while escaped
Image pixels and held task outcomes retain their original data, plan, budget and file owner.
An overlapping successor cannot redirect old work. File mutation rejects future affected reads;
already decoded results remain usable. The captured backend still survives complete input removal.

## Ordered Work

1. Reconcile ADR0007, document the concrete format, authority/availability distinction and pinned
   Bevy/Godot constraints. Keep transport, current demand and native publication separate.
2. Replace the private all-members field with captured/retained-pack backing. Share catalogue and
   executable admission; retain exact raw catalogue bytes for faithful writing. Admit derived
   extents and all raw allocations before use. Add actual bounded file reads and per-read digest
   checks without changing domain codecs or introducing another resolver.
3. Route existing Scene/asset/deep paths through the same encoded access seam. Overlap selected
   encoded scratch with decoder and dependency work. Release bytes before refund on every exit.
   Keep no encoded cache. Preserve caught codec panic and cancellation custody semantics.
4. Extend complete independent consumers for real pack production, startup/travel, shared
   Image/TileSet/CPU data, overlapping generations, physically held cancellation and an escaped
   Image after its Scene retires. Exercise old-object mutation, range/truncation and no retargeting.
   Compare captured versus pack exact/one-short costs and repeated acquisition without encoded
   retention. Use existing actual owners rather than a fake loader.
5. Independently review ownership, I/O/format admission and budget overlap; fix owning contracts.
   Run focused serial nextest, affected public/minimal checks and native qualification when actual
   native behavior changes. Update current guides, implementation ledger and immutable evidence;
   commit qualified units and locally fast-forward main. Overall foundation Goal stays active.

## Scope And Verification

No compatibility reader, transport compression, remote source, general provider/VFS, cache policy,
mount precedence or target-build executor is added. New pack metadata is canonical version 1;
the current catalogue/member formats remain unchanged. Standard `Write` is a byte sink, not
destination publication authority. Test partial sink failure and never claim atomic pack export.

Use CARGO_BUILD_JOBS=1, root target and one Cargo at a time. Qualify actual member reads and
production decoding, not only parsed metadata. Numerical acceptance is zero retained selected
member encoding in pack mode after acquisition, exact expected digests and membership, one
member decode per prepared generation, exact budget admission/refund and no redirected old
acquisition. Report logical modeled bytes separately from allocator/RSS or speed claims.
