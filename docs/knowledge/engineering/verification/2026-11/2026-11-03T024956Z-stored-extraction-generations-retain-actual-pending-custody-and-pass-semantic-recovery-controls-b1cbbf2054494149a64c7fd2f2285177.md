---
type: "Verification Evidence"
title: "Stored extraction generations retain actual Pending custody and pass semantic recovery controls"
description: "Actual Pending inventory, one unchanged Schema basis and fixed stored declarations pass correspondence and independent saved-generation checks without regenerating Q or refreshing allowances."
timestamp: 2026-11-03T02:49:56Z
record_id: "b1cbbf2054494149a64c7fd2f2285177"
producer_id: "codex-foundation-main"
run_id: "extraction-generation-20261103"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "88fd7289c0d680481e0f964561650de05f90cbad"
---

# Verification

Qualifies the private Project recovery-generation owner under real Pending/package storage and
actual all-profile native inventory. This follows the
[transport/inventory prerequisite](2026-11-03T022315Z-code-free-recovery-transport-and-actual-pending-extraction-inventory-qualification-d9300e228ce4473ebe5cea64378e2756.md).
It does not qualify source publication, Ready, Session/Application extraction, a new operating-system
process or native Editor interaction. The complete foundation Goal remains active.

# Result

The [owner](../../../../../src/project_host/authoring/project/prefab_extraction/recovered_generation.rs)
moves actual Pending custody, the captured inventory/models and original Host/Source allowances
together. Its finite Decode/Verify/Saved/Complete stages decode original disk, reviewed working,
fixed participant outputs and Q. Scene limits come from the actual inventory. Q metadata is decoded
through the existing bounded small-document admission; complete canonical record storage is
reserved before copying its alias and configured prefix. Failed capture drops temporary owners
before refunding Host storage. Source Work stays cumulative.

The [capture bridge](../../../../../src/project_host/authoring/project/prefab_extraction/recovered_generation/capture.rs)
uses configuration-local catalogues and the same fresh nominal basis. It calls existing
`PrefabExtractionRequest::admit_outputs`, then `prepare_saved_generation`; it never calls extraction
construction, assigns new Q IDs/input names, consults a live Workspace or creates another interpreter.
Semantic refusal retains the exact decoded request/outputs or verified corpus for retry. After
saved-generation admission, the proof retains the required disk guards; obsolete decoded disk
references end without refunding cumulative Source Work.

Five actual temporary-project tests cover:

- Normal output/package/Pending publication, retirement of the live producer, fresh root loading,
  one newly admitted basis, fixed Q identities and two nonempty outward input names.
- Original/output participant mixtures with Q present and sidecar independently absent/present,
  unchanged model/Adapter/custody/source pointers and exact live resource counts on confirmation.
- Grammar-valid forged Q rejected by correspondence with the same inputs and spent Work on retry.
- A removed SaveCurrent disposition leaving an old disk Guard route: working correspondence passes,
  independent saved-generation admission specifically refuses and retains that corpus on retry.
- Equal-byte source replacement after semantic completion: current observation refuses while fixed
  semantic allocations and both cumulative allowances remain owned.

The first test run passed 1/5; four assertions incorrectly equated cumulative `decoded_bytes` with
live decoded storage. Physical confirmation legitimately parses transient manifest/configuration
models. All preceding retained-byte, handle and pointer checks passed. The correction measures one
direct inventory-only validation as an independent control, then requires exactly that additional
decoded cost for semantic retries. Exact live storage/handle and allocation assertions remain.
An independent testing review confirmed this distinction; no cap or source semantic assertion was
relaxed. The specific dirty `U.v` dependency scenario in the research is not a separate new fixture;
the executed saved-generation negative control is the stored Guard route above.

Independent correctness review found the cross-crate private `retained_bytes` call in the initial
record reservation. It is replaced with checked public lengths for the known Prefab kind before
copying; scoped rereview closed the finding. Both correctness and testing reviews have no remaining
assigned P1/P2. Research and implementation/test agents are closed.

# Evidence

- Focused final five tests: `699750cf-a92a-42b9-9c09-03639b81e7d1`, 5/5 passed in 2.645s,
  1572 outside-filter skips. Test build 1m28s. The subsequent final regression includes them after
  removing obsolete Complete-stage disk retention.
- Final related Root regression: `09fd6956-b11c-4854-9808-0c5781b39298`, **303/303 passed** in
  68.051s, 1274 outside-filter skips; test build 31.11s.
- Root/Editor all-feature library check passed in 11.69s; Root minimal library check passed in
  0.33s; Root all-feature correctness Clippy passed in 12.06s. Ordinary warnings remain.
- `cargo +1.95.0 fmt --all -- --check` passed. All Cargo commands were serial, jobs=1, using the
  root target. No source files or build artifacts were deleted and no remote operation ran.
- Diff whitespace check passed with the repository's CRLF policy; 60 local links across the
  current plan, source study and both new qualification records resolve.

Final regression command:

```text
cargo +1.95.0 nextest run -p nara --all-features --locked --lib -E 'test(prefab_extraction::) | test(recovery_storage::) | test(schema_models::) | test(source_decode::) | test(source_inventory::) | test(source_upgrade) | test(authored_upgrade::)' --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
```

Machine-local logs are in TEMP as `nara-recovered-extraction-generation-tests.log`,
`nara-recovered-extraction-generation-regression-tests.log`, and
`nara-recovered-generation-{product-check,minimal-check,correctness-clippy}.log`.

# Follow-up

Implement one concrete extraction file-publication owner for live confirmed Storage and fresh
verified recovery. Keep both origins concrete; retain live Review/checkpoints and both actual
package/Pending receipts. Reuse `FilePublicationHost`, not the Schema-upgrade publication policy
which assumes Existing sources and a changed bundle. Q and Q.meta remain independent Missing or
already-output roles; original sources cannot be missing. Preserve Published evidence separately
from fresh output observations. A published checkpoint awaiting acceptance must retry acceptance
without writing its file again.

The operation's own replacements end the original unchanged-inventory observation. Do not rerun
the bridge, re-admit the basis or use the old Ready/absent-target validator after publication.
Per-file steps check actual Pending/package and retained target/parent/name; complete future native
inventory then admits Q and sidecar normally and checks namespace/Guard/metadata/root facts.
Keep native custody charged through its last shared owner. FilesComplete remains Pending until
source, saved, persistence-leaf, lane and future-project guards all bind before Ready.

Complete Session/Application, public independent project, actual fresh-process interruptions and
matching native qualification remain. Main and refactor/foundation remain at88fd728; all current
extraction work is authorized uncommitted WIP. No agents, Cargo or cleanup helpers remain from
this verification. Continue the full plan; do not reduce completion to this prerequisite.

# Citations

- [Complete extraction plan](../../../../plans/2026-11-02-prefab-extraction-plan.md).
- [Actual Pending inventory](../../../../../src/project_host/authoring/project/prefab_extraction/recovered_inventory.rs).
- [Integration controls](../../../../../src/project_host/authoring/project/prefab_extraction/recovered_generation/tests.rs).
- [Actual fixture and measured validation control](../../../../../src/project_host/authoring/project/prefab_extraction/recovered_generation/tests/fixture.rs).
- [Existing Core stored-pair and saved-generation owner](../../../../../crates/nara_scene/src/authoring/prefab_extraction.rs).
- [Pinned source comparison and publication responsibilities](../../../../research/2026-11-03-profile-resolution-and-saved-generations.md).
