---
type: "Verification Evidence"
title: "Neutral source inventory preserves strict authority and both real consumers"
description: "Physical scanning and custody are split from Schema-upgrade recovery proofs; final Root209, feature checks and correctness Clippy pass."
timestamp: 2026-11-02T23:48:52Z
record_id: "fdd43928e9ab444bac78ed2c16d92bf9"
producer_id: "codex-foundation-main"
run_id: "extraction-source-inventory-20261103"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "88fd7289c0d680481e0f964561650de05f90cbad"
---

# Verification

The physical source inventory now has two actual consumers: authored Schema upgrade and Prefab
extraction. Its scanner, source/file/metadata facts, per-configuration visibility, namespace
revalidation and continuous native custody belong to `source_inventory` / `SourceInventory`.
The neutral code has no upgrade-recovery AST or generation dependency and retains no displaced
`SourceUpgrade*` aliases. Ordinary capture still strictly admits Ready.

`source_upgrade_inventory` retains concrete recovered/verified generation proofs and disposal.
It verifies Pending, recovery digest, actual bundle bytes, source universe and complete modeled
configuration before supplying the already parsed manifest plus finite borrowed physical claims
to `capture_admitted`. That private physical boundary neither grants Ready nor approves old/new
meaning. The parser reservation and claims' temporary storage stay on the original allowance;
the real native handles and continuing custody retain their original scopes.

# Result

All affected commands were serial, Rust1.95/jobs1, reusing the root target. No default caps were
increased, artifacts removed, remote publication performed or code committed. The full foundation
Goal remains active and complete Prefab extraction publication/recovery remains open.

- Final Root extraction/Review, physical inventory/catalogue/native source, actual extraction
  leaf persistence and existing Schema-upgrade/recovery cohort: **209 passed**, run
  `56249350-1973-445f-92ea-a431f72b2113`, 30.472s after2m15s build. There were1195 outside-filter
  skips. This includes the new explicit claim without recovery/open facts and refusal of a
  mismatched recovery configuration before a missing claimed member is read. Session14664
  terminated0. This result replaces the pre-split207-test evidence for the changed owner.
- Final saved-generation/precise Guard diagnostic and shared raw accounting cohort:
  **24 passed**, run `5d7ec4fe-7a96-49ba-9490-e93dab16217b`, 0.872s,919 outside-filter skips.
  Session64387 terminated0. An obsolete stored Guard route now has the specific
  `scene.prefab-extraction-saved-guard-relocation` diagnostic; malformed/mismatched captures
  retain their different invalid-source diagnostic. Root209 checks the actual Host error too.
- Root and Editor all-features libraries passed22.56s. Minimal Root passed0.28s. Independent
  `project-tooling` without Editor/tooling features passed11.73s, covering the separate CLI
  recovery build path. Root correctness Clippy on the production union passed21.10s.
- Reflect/Scene/Tooling all-features/all-targets correctness Clippy passed1m17s, with general
  warnings. It preceded only the private Guard diagnostic variant and neutral Root split;
  Root209 and the24-test cohort cover those changes. Earlier Reflect196 and raw12 evidence
  remain scoped in the prior progress record.
- Independent adversarial review of the frozen split found no confirmed P1/P2. The separate
  Review ownership P2 is fixed and independently rereviewed. All participating agents are closed.
- fmt and whitespace checks passed before Root209. The only later Rust edits replace planning
  labels in three comments with Schema-upgrade terminology; no behavior changed. Five owning
  documents have923 checked local Markdown targets without missing files. Memory validation
  succeeded with retained historical advisory warnings.

# Evidence

The final Root command was:

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo +1.95.0 nextest run -p nara --no-default-features --features 'project-workflow-cli,tooling,presentation-2d' --locked --lib -E 'test(project::prefab_extraction::) | test(catalogue::) | test(source_inventory::) | test(source_upgrade_inventory::) | test(source_upgrade::baseline::) | test(native_source::) | test(authored_upgrade::) | test(project::source_upgrade::) | test(source_upgrade_adoption::) | test(source_upgrade_resume::) | test(persistence::tests::prefab_extraction::)' --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
```

TEMP logs: `nara-neutral-source-inventory-root-tests.log`,
`nara-extraction-saved-guard-diagnostics-tests.log`, `nara-neutral-source-inventory-product-check.log`,
`nara-neutral-source-inventory-minimal-check.log`, `nara-neutral-source-inventory-cli-check.log`,
`nara-neutral-source-inventory-root-clippy.log`,
`nara-extraction-saved-generation-domain-clippy.log` and
`nara-extraction-saved-generation-memory-check.log`. The first product-check invocation used
an incorrect package spelling and stopped before compilation; it was corrected to `nara-editor`
for the actual successful command. No build result is inferred from that refused invocation.

# Follow-up

Continue the extraction-specific fixed output/recovery member table and actual publication
owner. Retain complete original disk Guards, exact SaveCurrent/ReplaceSource checkpoints,
fixed Q/sidecar identity and one publisher per physical leaf. Reuse file transport and leaf
binding, while keeping unchanged nominal Schema and concrete extraction generation proof.
Actual Session/Application Confirm, physical execution retirement, Pending/Ready, successor
project binding, fresh-process recovery and native/public workflows remain unfinished. The
large authorized WIP stays uncommitted until the complete extraction unit is qualified.

# Citations

- [Prior saved-generation and Review ownership checkpoint](../../progress/2026-11/2026-11-02T233154Z-saved-generation-proof-and-complete-review-refusal-ownership-pass-actual-consumers-8ca79f0732a34e799c8e58a4e4ba061f.md).
- [Complete extraction plan](../../../../plans/2026-11-02-prefab-extraction-plan.md).
- [Mature-engine and publication ownership comparison](../../../../research/2026-11-03-profile-resolution-and-saved-generations.md).
- [Workspace contract](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md).
