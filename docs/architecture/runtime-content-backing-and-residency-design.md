# Runtime Content Backing And Residency

Date: 2026-10-31. Status: implemented and qualified in the
[bounded production scope](../knowledge/engineering/verification/2026-10/2026-10-30T184847Z-retained-runtime-pack-and-independent-encoded-residency-qualification-386d2e5e9c124b66bc4390f69b796053.md).
Owner: [ADR 0007](adr/0007-asset-identity-and-import-pipeline.md).
Parent: [foundation R6/R7](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r7-target-build-and-base-source-free-delivery).
Finding: CP4 in the [research reconciliation](../research/2026-10-30-engine-direction-reconciliation.md).

## Problem And Outcome

The captured directory base owns every encoded member. Every decoded acquisition retains its
original base and executable plan, so one escaped Image also retains all those encoded bytes.
This is a useful complete in-memory capture policy, but it must not become the only possible
relationship between immutable content identity and resource lifetime.

Keep the captured backend and add one concrete retained local pack. Both admit the same catalogue,
original executable selection and decoded/native workflows. The pack retains one exact opened
file and bounded member extents. It reads and authenticates only a requested encoded member,
releases that temporary allocation after decoding, and keeps no encoded cache. Decoded Images,
Input definitions and CPU products retain their original base without retaining unrelated encoded
payloads. Neither backend changes Immediate/Deferred demand or native publication authority.

## Authority And Availability

An immutable content context fixes expected catalogue/member bytes and its original compiled
plan. It does not imply that every possible storage medium keeps those bytes available forever.

| Backend | Retained encoding | Later availability |
| --- | --- | --- |
| Captured directory | All verified members in memory | Acquisitions survive replacement/removal of every input file and directory |
| Retained local pack | Original catalogue bytes, admitted extents and one exact file capability | A read succeeds only if that original object's requested range still yields its admitted digest; I/O failure or mutation rejects that acquisition |

The pack never reopens a filename, re-resolves a directory, consults the current global base or
selects another codec. Replacing a pathname cannot redirect it. An open handle proves continuing
object authority, not portable byte immutability. Windows file sharing/locks and Unix advisory
locks have different guarantees; this design does not claim a portable sealed file. Each read
copies a bounded range into privately owned bytes and verifies its original length/digest before
the decoder sees it. Mutation during or after a read cannot change an admitted decoded value.
Already decoded pixels remain valid if later reads of the backing fail.

`open` retains its complete capture guarantee. `open_pack` explicitly selects the second policy:
it verifies bounded metadata, executable agreement and physical extent before publication, while
member payload integrity is checked on acquisition. `validate_deep` uses those actual reads and
the selected production decoders for both backends. A successful open is not a deep report.

## Concrete Format And API

The local transport is uncompressed canonical version 1:

1. Eight bytes `NARAPACK`, little-endian `u32` version `1`, then little-endian `u64` catalogue length.
2. The original catalogue bytes, whose exact digest comes independently from the Host.
3. Every complete member in admitted catalogue order, with lengths/digests from that catalogue.

There is no second product map, filename table, offset authority, codec selector or padding.
Checked prefix sums derive member extents; the final extent must equal the opened file length.
The declared total/member encoding limits apply independently of whether bytes are resident.
Header, wrong version, truncation, trailing data and overflowing extents reject admission.

`RuntimeContentBase::open_pack(FileCapability, expectation, recipe, limits)` consumes the exact
read authority. `RuntimeContentBase::write_pack(&mut (impl Write + ?Sized))` emits the base's original
catalogue and authenticated members without domain decoding. The caller owns destination
publication and durability; failure may leave partial output and grants no publication receipt.
Keeping original catalogue bytes preserves the independent digest, including accepted whitespace.
Writing a pack does not require a new source resolver or build/package executor.

The private backing enum is sufficient. Existing typed Scene/asset acquisitions, prepared
collections, task cancellation and native residency keep one implementation. There is no public
storage-provider trait, configurable cache, virtual filesystem or new loader service.

## Ownership And Admission

Catalogue/compiled-plan authority, encoded backing and decoded/native residency have different
costs and lifetimes. A decoded owner retains its original base as today. In pack mode that keeps
the small metadata/file owner, rather than the full encoded content set. Two separately opened
bases remain distinct owners even when catalogue bytes match.

Before allocation, the shared budget admits the catalogue and backing metadata. Captured mode
also admits all retained encoded payloads; pack mode admits its extent index and file owner.
The original raw catalogue is an intentional retained allocation in both modes, rather than an
unused conservative charge.

For a pack acquisition, the exact selected encoded length has a separate WorkBytes reservation.
That reservation overlaps the decoder's complete output/work promise and dependency/parser
scratch. It remains live until the encoded vector is dropped, including failure and panic paths.
It is released before successful publication. Acquisitions never add encoded bytes to decoded
retention or borrow unreserved memory after allocating it. Concurrent acquisitions share the
same aggregate ledger; independent bases have independent ledgers as before.

No encoded cache is the eviction control: every completed/failed acquisition releases all its
selected encoded scratch. A later reacquisition performs a new authenticated read. A cache may
be justified later by measured repeated-read cost; its membership, eviction and outstanding
borrow costs must remain separate from catalogue identity and decoded owners.

## Alternatives And Required Evidence

Raising the eager limits preserves the whole-pack retention problem. Retaining one file per
member removes byte retention but consumes an unbounded set of handles and does not provide
portable byte freezing. Reopening paths permits retargeting. A memory map still needs an honest
mutation/truncation contract and risks exposing mutable bytes directly to codecs. A general
provider/cache hierarchy adds policy without a current consumer. The finite pack is the bounded
comparison; it is not a universal content store.

Qualification must compare real captured and pack bases using existing public independent
consumers: startup and later Scene travel, shared Image/TileSet/CPU data, old/new overlapping
generations, held physical cancellation, escaped prepared pixels after Scene retirement, exact
work/retained boundaries, failed/panicking decode, changed/truncated original backing and pathname
replacement. Prove no unrelated encoded residency and no fallback to a replacement. Record
portable contract tests separately from native platform results. Throughput, RSS, remote storage,
compression, mmap, patch overlays and whole-game streaming remain outside this unit.
