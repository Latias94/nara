---
type: "Work Progress"
title: "Shared TileSet content ownership and concrete resource baseline integration"
description: "Records in-progress processed TileSet/native integration, actual dependency inventory and the resource-baseline correction; not qualification."
timestamp: 2026-10-29T10:12:27Z
record_id: "839822b154f24bf6b658c0c9954fa2b1"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "refactor/foundation"
git_commit: "c9cb764"
---

# Summary

The active foundation goal continues on `refactor/foundation`, based on `c9cb764` already
fast-forwarded to local `main`. No remote publication is authorized. The current TileSet production
replacement is uncommitted and still under integration, not qualified by predecessor Image tests.

# Details

- Descriptor/codec and shared native owner writers have frozen their files. `TileSetDescriptor`
  carries own/Image complete Products and actual atlas/material policy without native handles or
  source metadata. The strict version-one codec visits its actual Image dependency. Source and
  Product declarations share one content owner, exact extent helper, claim/baseline/atlas barrier
  and retained Image custody. Root source-residency callers use the neutral receipt/input names
  and explicitly supply `Some(database)`; processed callers require no database.
- Runtime codecs now explicitly enumerate actual required references. The same captured base
  checks membership plus Required declarations before returning decoded assets. This neither
  selects dependency codecs nor proves their native residency. Existing Image/TextBundle codecs
  explicitly enumerate none; wrapper codecs delegate and panic recovery has a new counterexample.
- Read-only integration review found that Scene product-resource overlays checked only type
  presence. A worker is adding mandatory concrete `SceneProductResource` baseline capture and
  comparison in the existing transaction, including incarnation/content drift, with precise
  missing/changed refusal. Independent consumer implementations are migrated together.
- A separate worker is creating the source-free TileSet/Image/Tilemap consumer, including actual
  dependency/cells admission, native publication, cancellation/stale refusal and escaped owners.
  This aggregate is not stock renderer residency; first-party Host work remains next.
- An Astra max read-only rendering review identified five still-missing decisions: preserving
  writes, exact cross-capture output leases, predecessor-fenced history publication, one versioned
  material declaration joining semantic fields/GPU ABI/graph access, and exact reconstruction
  input custody independent of GPU completion. Proposed ADR 0121 and R8/R9/D14/D15 are amended
  with these contracts and bounded executable tracers; rendering remains unimplemented.
- Fresh Asset/Image all-feature nextest: 290 passed, one default skip across five binaries,
  run `343ef9d9-42a1-43d6-a783-2a82291e35d2`. The earlier all-feature check also passed.
  These results cover the mandatory visitor migration, not TileSet or Scene/resource changes.
  TileSet all-feature nextest is the sole active Cargo command at capture time (exec `19391`).

# Next Action

Integrate the two remaining writers without overlapping edits, inspect and format after freeze,
run focused owner/Scene/root/independent consumers serially in the shared target. Resolve actual
failures and the independent TileSet owner review, then qualify affected native/GPU paths. Update
real evidence/ledger and commit coherent verified units, fast-forward local main and return to
`refactor/foundation`. Do not claim full processed Player, completed rendering or foundation goal.

# Citations

- [Ordered delivery plan](../../../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Proposed render contract](../../../../architecture/adr/0121-composable-render-programs-and-host-execution.md)
- [Predecessor actual-output qualification](../../verification/2026-10/2026-10-29T094316Z-actual-runtime-decoded-output-charging-and-shared-pixel-retention-qualification-14f023431b4c462dbeb1051f1739662e.md)
