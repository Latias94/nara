---
type: "Work Progress"
title: "Native composition stack correction and physical pump ownership integration"
description: "Records actual native stack reproduction, heap slot correction, external drawing qualification and the selected reusable physical event-loop successor."
timestamp: 2026-10-31T22:18:31Z
record_id: "33af692382f5443998b6c1bd125f7550"
producer_id: "codex-foundation-main"
run_id: "window-composition-20261101"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/foundation"
git_commit: "b34741c"
---

# Summary

Complete foundation Goal remains active on `refactor/foundation`, based on `b34741c`; main is
still that commit. Public native-window/read-use, the reader, heap-slot correction and independent
consumers remain uncommitted. Source/Processed interleaving needs the selected physical-loop
ownership correction; it is not qualified by the ordinary content test.

# Details

- Rechecked the supplied `nara-engine-research-2026-10-30.zip`: archive SHA-256 remains
  `99589eef3287cb9e353501bf1d27d1f4287ec825dbaa45c97abee38033ed2c36`, all91 manifest entries
  match size/digest and all10 imported study documents remain byte-identical. No new imported
  study or roadmap reset is needed. Existing R5, joint execution/motion, material and editable
  Nara-drawn Editor work remains open.
- Actual two-window execution aborted with Windows stack overflow `0xc00000fd`
  (`c1fd90c3-d9e8-43d7-b8e1-801935240a78`); a256KiB CPU thread regression also reproduced it
  (`441ea5e2-5865-4786-9101-331c338c9bf9`). `PolicyInstances` embedded large64/1024 tables in
  producer BTreeMap values. Fallibly allocated heap tables remove that inline amplification,
  preserve stable indices and charge actual retained capacities. Do not increase the example
  stack or discount pruned capacity. CPU owner29 passed (`18d61441-cccf-4cfe-b489-bb64e2a27121`);
  actual two-window/image execution passed (`6aa7d282-7d96-4567-8f2a-7c190feb448c`).
- Independent clipped drawing executes declared typed imports and a readback Feature, with no
  raw-texture/manual submission-custody bypass. Native drawing and two CPU controls passed in
  `e9678aeb-7887-467e-94c9-2b7ae3e61a2a`; the fourth content test initially failed due hand-built
  Runtime wire omitting defaults. Fixture now uses actual source expansion and production cook;
  ordinary content passed (`95d233bb-b7ea-4364-a371-190a74c323ce`). This test does not run Runtime.
- Root/Editor checks from the predecessor do not cover this new physical-loop interface. Renderer
  all-features ordinary367 passed (`36caa9bf-7769-42e0-aa95-6d70b9ea3504`), before the final two
  review corrections. Their focused native/ordinary window8 passed
  (`babbfd45-c880-435f-8cd7-f20d654271a5`): maintain original custody before new metadata admission,
  and isolate an unpublished result's destructor while preserving the original native error.
- Broader native Host cohort35/36 passed (`64731eba-d9b5-46a2-ac4e-b783f104a364`). The failing
  old custody fixture assumed `usize::MAX` could bypass the now-fixed64MiB program metadata
  ceiling. Actual error is `MetadataBytes`, before native allocation. Corrected the fixture to
  test that actual hard envelope, without changing product caps or priority; focused native1
  passed (`0e186eba-b925-45b5-9f7b-35b8c5d69cd6`). Ordinary custody-count tests still separately
  check100,000 required entries against65,536. Renderer all-target/all-feature correctness
  Clippy passed,35.61s, with advisory warnings.
- Read-only audit found no analogous large inline-table issue in the selected App/Tasks/Asset/
  Reflect/UI owners. This is bounded source review, not all-platform stack qualification.
- Source/Processed capture cannot use two ordinary DesktopRuns unchanged: native Winit0.30.13
  rejects another EventLoop per process. A single direct-App substitute also conflicts with actual
  hosted source residency and separately admitted native Schema witnesses. Preserve those checks.
  Accepted ADRs0078/0084/0110 now select explicit physical-loop ownership, returned only after
  complete native-session retirement. Root options extract supplied ownership before source
  preparation, so a pre-native startup failure can return an unused loop. This is a production
  ownership correction, not a private capture hook or global cache.

# Next Action

Franklin (`01a193ff-f936-7701-a44b-77dfb2845d5f`) owns nara_winit production/tests. Bernoulli
(`01a19400-789e-7b70-8a2f-308c5511d27c`) owns independent content fixture/example/subprocess test.
Heisenberg (`01a19407-8874-7551-8fb7-4b8caa54b779`) reviews physical-loop/Root custody read-only;
Jason (`01a193d3-acf0-7901-8fe2-454f36ca7c8b`) reviews final native-window corrections read-only.
No agents run Cargo/fmt. Parent owns Root and verification. Renderer Clippy exec74588 has a
terminal successful log but should be polled before another Cargo. Wait for writer source-ready,
inspect/fmt, then run Winit/Root/independent/native qualifications before commit and local ff main.
Keep full R8 open: same external pre-UI effect in Runtime/Scene View/offscreen with crisp HUD and
exact one-shot UI completion still needs its complete public stock-composition seam consumer.

# Citations

- `docs/plans/2026-10-31-render-composition-and-host-execution-plan.md`
- `docs/architecture/render-program-production-design.md`
- `docs/architecture/render-host-ownership-design.md`
- `docs/research/2026-10-30-engine-direction-reconciliation.md`
- `tests/fixtures/render-composition-consumer/`
- Previous progress record `d3f6a6a45a4a4f5c9797bbd00deac863`.
