---
type: "Verification Evidence"
title: "U5 asset association desired recipe and runtime-owned watch rescan"
description: "Managed store pairing precedes baseline capture; retained image intent and runtime-owned rescan are independently qualified."
timestamp: 2026-10-22T19:39:28Z
record_id: "e081ec487db94262b392fc8b7d7ff3ef"
producer_id: "codex-root"
run_id: "low-level-contract-refactor-20261023"
related_plan: "docs/plans/2026-10-23-0135-refactor-low-level-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "cfde813"
---

# Verification

Windows 11 / Rust 1.99.0, serial Cargo with one job and root target reuse. Final owner tests include
the desired/publication/revocation repairs and same-frame event ordering. Final independent Image
and Watcher rereviews have no remaining concrete blocker.

# Result

`AssetMutationBaseline<T>` validates the actual value/state store pairing before capturing slot
and state revisions. Both sides publish their initial pairing under the same ordered lock interval;
a rejected pairing changes neither store. Success, failure and removal consume the same baseline.
Resource replacement, empty-slot ABA and late predecessor outcomes reject before their effects.

Metadata observations survive reload-request rejection. Image retains validated desired settings
separately from last-good pixels and in-flight task attempts. Review required additional binding
to publication/slot and metadata observation, plus retained removal revocation. The final owner
run includes those fixes and their public reimport/removal/successor regressions.

Watcher directory/unknown/ambiguous rename and overflow observations retain a sticky rescan.
The replacement native watcher and incremental tail have a private runtime owner; the Host holds
only a weak confirmation/cancellation receipt. Abandonment, supersession and Stop retire the exact
native owner without allowing an older receipt to cancel a successor.

# Evidence

- Initial Asset/Image/Watch/Tilemap cohort: 250/250 passes in
  `3e20db3f-5ed4-4b94-8019-9e05976e362f`. This predates the independent review findings and
  does not close the final desired-settings/removal changes by itself.
- Final `cargo nextest run --locked -p nara_asset -p nara_image -p nara_asset_watch -p
  nara_tilemap --all-features --lib --test-threads=1 --no-fail-fast`: 257/257 passes in
  `bd648f81-6e6e-4798-803a-61ab093a8d02`. Includes cross-frame desired retention, successful
  own-publication successor, later public reimport winning over retained settings, rejected Remove
  revocation before request admission, public metadata observation invalidating old tasks, common
  baseline failure/removal atomicity, resource replacement and empty-slot ABA.
- Subsequent final combined Asset/Image/Watch/Tilemap/Hierarchy/Transform all-feature library
  cohort: 329/329 passes in `3bb6abef-0e57-4bf0-b7dd-160cf4d191b6`. Includes post-removal
  metadata observations before/after removal, cross-frame and same-frame push/batch rejection,
  full metadata identity rejection and corrected-notification recovery, and all 27 three-event
  coalescing sequences through batch splits and drain/requeue roundtrips.
- Initial combined run `8134f4bd-886c-4290-afe9-a326e56b2728` passed 328/329; the new held-rescan
  test incorrectly required pollable Task shutdown to finish in exactly two drives. Its assertion
  saw Stopping, then panic during abnormal owner disposal produced Windows fail-fast. The test
  now polls the existing owner with a five-second deadline and one-millisecond yielding, without
  changing production shutdown or watcher semantics. The complete 329-test rerun passes.
- Watcher final owner cohort: 29/29 passes in `68fd4c49-4063-4f68-bd8f-517a5d54be61`.
  The held receipt across Stop, supersession, abandonment and exact native callback lifetime
  tests exercise actual destructor reachability as well as sticky coverage and incremental tails.
- Independent watcher rereview reports no blocker after checking strong/weak custody, wrong World,
  overflow, superseded receipts and mutex/drop ordering. It did not execute a second Cargo run.
  Windows notify 8.2.0 performs native cleanup asynchronously: callback release is eventual,
  not a claim that every native cleanup has completed synchronously when Stop returns.

# Follow-up

Independent rereview found and closed the cross-frame and same-frame retry combinations after
removal and metadata requests were rejected. Removal-time metadata identity plus retained semantic
coalescing order supplies the missing fact; a new observation permits full revalidation, never
identity bypass. The final rereview checked a stable source snapshot and found no remaining
concrete blocker. Fresh public content refresh and final-source native consumers remain U11.
No generic importer or additional runtime is added.

# Citations

- [Plan](../../../../plans/2026-10-23-0135-refactor-low-level-contracts-plan.md)
- [Managed baseline](../../../../../crates/nara_asset/src/storage.rs)
- [Image intent](../../../../../crates/nara_image/src/reload.rs)
- [Rescan custody](../../../../../crates/nara_asset_watch/src/lib.rs)
- [Watcher tests](../../../../../crates/nara_asset_watch/src/tests.rs)
- [ADR 0037](../../../../architecture/adr/0037-asset-load-request-cache-and-lifetime-policy.md)
