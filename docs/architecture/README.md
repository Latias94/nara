# Architecture Document Map

This page identifies the authority and maintenance role of Nara's architecture documents. It is
an index, not another source of architecture decisions.

## Authority Order

1. [Accepted ADRs](adr/README.md) own durable decisions.
2. [ADR implementation status](adr/implementation-status.md) owns repository evidence and remaining
   gaps.
3. The completed [2D foundation contracts](../plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md)
   plan closes its finite U1-U13 implementation through
   [registration `88b6faf455454957bc7e0f79e76b9850`](../knowledge/engineering/registry/2026-10/2026-10-15T202335Z-nara-2d-foundation-contracts-codex-root-88b6faf455454957bc7e0f79e76b9850.md)
   and [acceptance evidence `b09676d3fa354aa899a795a5dfe9d2ba`](../knowledge/engineering/verification/2026-10/2026-10-15T202228Z-foundation-u1-u13-acceptance-closes-with-actual-consumers-and-completed-full-review-b09676d3fa354aa899a795a5dfe9d2ba.md).
   [ADR 0117](adr/0117-2d-foundation-ownership-and-authoring-contracts.md) owns its focused contract
   refinements. All authorized implementation changes are committed locally through `ca3d803`; the separate
   research archive is `652b2eb`. Its completed successor is the
   [First 2D Author Workflow](../plans/2026-10-16-0940-feat-first-2d-author-workflow-plan.md),
   closed by [registration `fe7312236c5d4906b8e950d366111171`](../knowledge/engineering/registry/2026-10/2026-10-16T025148Z-nara-first-2d-author-workflow-codex-root-fe7312236c5d4906b8e950d366111171.md)
   and [acceptance `7a2024c6678b43a48a069406d6eb729d`](../knowledge/engineering/verification/2026-10/2026-10-16T025031Z-first-author-workflow-u1-u6-closes-with-clean-committed-external-products-and-measured-iteration-7a2024c6678b43a48a069406d6eb729d.md).
   U1-U6 proves independent creation, explicit schema refresh, a public-API game,
   ordinary editing/Play and development export from committed source `78faa7c` on Windows.
   It closes the prior excluded-source review scope and records actual iteration times.
   The explicitly authorized [Custom Component Authoring](../plans/2026-10-16-1101-feat-custom-component-authoring-plan.md)
   successor is active under registration `522f9c6d8d7b436abae267b098528179`.
   It covers finite scalar derives and ordinary schema-driven editing, with native
   preview and strict Runtime admission kept separate. Remaining native platform limitations are
   recorded in the implementation ledger.
   The previous [Standalone Supervised Play v1](../plans/2026-08-21-001-feat-standalone-supervised-play-v1-plan.md)
   lane was transferred by [record `d7cca2f42c6d4e07b762475aced7eb44`](../knowledge/engineering/registry/2026-10/2026-10-15T091614Z-standalone-supervised-play-v1-codex-root-d7cca2f42c6d4e07b762475aced7eb44.md).
   Its last predecessor boundary remains registered by
   [record `079f447a577b42929a258328ab95b1fe`](../knowledge/engineering/registry/2026-08/2026-08-21T101853Z-standalone-supervised-play-v1-codex-root-079f447a577b42929a258328ab95b1fe.md).
   SPL-U0 through SPL-U2 remain verified through `ab211bc` under
   [ADR 0116](adr/0116-standalone-supervised-editor-play.md). Foundation U12 now verifies child
   READY, ordinary lifecycle effects, saved-generation recovery and physical cleanup on Windows;
   complete native Linux/macOS release trajectories remain unverified. This preserves the
   historical SPL checkpoint instead of retroactively changing its evidence.
   Completed predecessor plans remain immutable evidence.
   The completed [Editor Image Import and Reimport v1](../plans/2026-08-20-001-feat-editor-image-import-and-reimport-v1-plan.md)
   plan closes at `ec64e4c` with
   [verification `ae97ce2c7330420d8b6851967fd7c8b4`](../knowledge/engineering/verification/2026-08/2026-08-21T025159Z-eir-u6-standalone-image-import-product-closure-ae97ce2c7330420d8b6851967fd7c8b4.md)
   and completed
   [registration `6a48e230cc9c4e2eb1faf8121a85d9ea`](../knowledge/engineering/registry/2026-08/2026-08-21T025159Z-editor-image-import-reimport-v1-codex-root-6a48e230cc9c4e2eb1faf8121a85d9ea.md).
   EIR-U0 accepted [ADR 0115](adr/0115-editor-image-import-metadata-and-authoring-publication.md).
   EIR-U1/EIR-U2 completed the typed recipe, strict-consumer, reload, and fixture migration at
   `334f946`, with the reviewed metadata-reload correction at `88f4a7a` and
   [verification `48b2b28ab1404017b7fe14ab2c335071`](../knowledge/engineering/verification/2026-08/2026-08-20T053914Z-eir-u1-u2-canonical-image-recipe-and-strict-consumer-migration-48b2b28ab1404017b7fe14ab2c335071.md).
   EIR-U3 completed the bounded Host image catalog and Import/Reimport transaction at `a0141e6`,
   with exact retained-pixel accounting and retryable physical task retirement closed at `cb2e6c1`,
   and no-presentation Runtime retirement plus replacement handoff closed across `c53b189` and
   `2376a3a`,
   under
   [verification `e6cb2cf25cdf4321a15368421154678d`](../knowledge/engineering/verification/2026-08/2026-08-20T094524Z-eir-u3-bounded-editor-image-import-and-reimport-transaction-e6cb2cf25cdf4321a15368421154678d.md).
   EIR-U4 completed the UI-neutral Inspector, private egui adapter, exact frame-bound action, honest
   last-good projection, and bounded native pending-work wake across `1620786` and correction
   `6797956`, under
   [verification `d109ae1760a2463a862ffd4c0148caaf`](../knowledge/engineering/verification/2026-08/2026-08-20T121501Z-eir-u4-image-import-inspector-and-native-wake-d109ae1760a2463a862ffd4c0148caaf.md).
   EIR-U5 completed same-generation UI authoring publication and referenced-only hydration across
   `a404a41` and source-proof correction `94157a1`, under
   [verification `b2dfa722baf349b9a75cc324086024ac`](../knowledge/engineering/verification/2026-08/2026-08-21T005752Z-eir-u5-same-generation-authoring-image-publication-and-referenced-hydration-b2dfa722baf349b9a75cc324086024ac.md).
   EIR-U6 proves the one-window standalone product trajectory, safe last-good failure, and exact
   Runtime closure exclusion at `ec64e4c`. EIR-U0 through EIR-U6 are complete; no unrelated Editor
   successor is automatically active; current foundation closure is recorded above.
   The completed
   [Editor Project Browser and Source Index v1](../plans/2026-08-19-001-feat-editor-project-browser-and-source-index-v1-plan.md)
   plan closes at `1a832df` with
   [verification `2acde0398a8b467b93fe714241a5e216`](../knowledge/engineering/verification/2026-08/2026-08-19T213500Z-epb-u5-standalone-project-browser-product-closure-2acde0398a8b467b93fe714241a5e216.md)
   and completed
   [registration `6c28391e9cd8451f92b821dba9d0c9e3`](../knowledge/engineering/registry/2026-08/2026-08-19T213600Z-editor-project-browser-source-index-v1-codex-root-6c28391e9cd8451f92b821dba9d0c9e3.md).
   EPB-U0 established the durable boundary at `4275d93`; EPB-U1 implemented and verified bounded
   handle-relative enumeration at `04914e1` with
   [evidence `e4b844f6b2764741a2bff0a731925a80`](../knowledge/engineering/verification/2026-08/2026-08-19T180000Z-epb-u1-bounded-handle-directory-enumeration.md).
   EPB-U2 implemented the Host-owned source-index transaction at `8bdc4de` with
   [evidence `9ac12073205d4064a4bc76729225caf8`](../knowledge/engineering/verification/2026-08/2026-08-19T182322Z-epb-u2-bounded-host-project-source-index-9ac12073205d4064a4bc76729225caf8.md).
   EPB-U3 implemented closure-external Authoring Scene/Prefab admission and independent path-bound
   persistence lanes at `fe0fa55` with
   [evidence `684770f357d14a0184e3cdbec4751355`](../knowledge/engineering/verification/2026-08/2026-08-19T192016Z-epb-u3-indexed-authoring-document-admission-684770f357d14a0184e3cdbec4751355.md).
   EPB-U4 implemented the UI-neutral browser projection, exact frame-bound Refresh/Open ingress,
   bounded egui Project Browser, and unified pending-work composition at `cdd5892`, followed by the
   bounded-allocation correction at `6fb84d0`, with
   [evidence `495c3c9cb92b4f94aa0895dfbce04ebf`](../knowledge/engineering/verification/2026-08/2026-08-19T204830Z-epb-u4-bounded-editor-project-browser-495c3c9cb92b4f94aa0895dfbce04ebf.md).
   EPB-U5 proves the one-window browse/filter/Open/Refresh trajectory, equivalent snapshot facts,
   and last-good retention at `1a832df`. EPB-U0 through EPB-U5 are complete and remain immutable
   predecessor evidence for the explicitly activated EIR lane.
   The completed [Standalone Generic Editor Project Open and Recovery](../plans/2026-08-18-2158-feat-standalone-generic-editor-project-open-and-recovery-plan.md)
   plan closes at `a86aee1` with
   [verification `25fbee67d7ea41a9bc8406abe6e8b221`](../knowledge/engineering/verification/2026-08/2026-08-19T110847Z-sge-u5-standalone-generic-editor-product-closure-25fbee67d7ea41a9bc8406abe6e8b221.md)
   and completed
   [registration `931591fb6d2b4bcbad3151922814d8e6`](../knowledge/engineering/registry/2026-08/2026-08-19T111039Z-standalone-generic-editor-open-recovery-codex-root-931591fb6d2b4bcbad3151922814d8e6.md).
   SGE-U1 through SGE-U5 establish the bounded data-only schema basis, lossless degraded Scene and
   Prefab authority, shell-first lifecycle, strict selected-native Runtime agreement, modular
   generic authoring workspace, and one-window Project Manager/Open/Recovery product proof. No
   successor Editor lane is active automatically. The completed EPB lane was the explicit
   project-browsing successor; ADR 0115 now admits only the EIR image workflow. External Play,
   watchers, unrelated source mutation, and extension hosting remain separately trigger-gated.
   The completed
   [Editor UI Prefab Structural Composition v1](../plans/2026-08-18-0848-feat-editor-ui-prefab-structural-composition-v1-plan.md)
   closes at `993a793` with conformance record `436c589870ba4f3c83548d349caa6d4c` and
   completed registration `5edbf5a02be34aa8bdf7f88e45981dab`. Its prior units remain verified at
   `fb66b84`, `4a6cd14`, `278b986`, and `9540deb` with evidence
   `8386ed22067743af8cfb1f88b3ffd314`, `7c4c0f3ba8924f7f84668341aa0011ad`,
   `14822e4a808f412b8216eb02401eaca8`, and `af32c829b365491e84fe0af17c46d6eb`.
   The completed
   [Editor UI Prefab Authoring and Canvas v1](../plans/2026-08-17-1930-feat-editor-ui-prefab-authoring-and-canvas-v1-plan.md)
   closes at `ed9f1cd` with conformance record `cf11b0c7d23a44e7b127b41199ee4ee7`
   and completed registration `fcbd471b11774b599cd190ead6827654`. Its prior units remain verified at
   `dc7ea07`, `16b0d30`, `bca3e62`, and `155d147` with evidence
   `e6b43a9f24ed42deb3c8d19a03a55cdf`, `19920c861d394a4fb43db6f60e4cf2a5`,
   `45077ece034c42569c6a460a562ab0b1`, and `984fe0030b4843c2a1165a9e812955f0`.
   The completed
   [Editor Game View v1](../plans/2026-08-16-2157-feat-editor-game-view-v1-plan.md) remains immutable
   lineage evidence at `b5697f3` with conformance record `9d0b0a17beef4c39bea70a8d6f025455` and
   completed registration `84ab40a2fd9b425a9d9827fd9a62e142`.
   The completed Editor Scene Composition and Scene View, Editor Authoring Loop, Fresh Project,
   Capability Directory Publication, Runtime UI, and focused correction plans remain immutable
   lineage evidence; the [UI program roadmap](../plans/ui-program-roadmap.md) is navigation only.
4. [Open questions](open-questions.md) own unresolved, trigger-gated decisions.
5. Design harnesses, appendices, and guides are non-normative. They must yield to Accepted ADRs and
   must not authorize implementation by themselves.

## Document Roles

| Document | Role | Current state and activation rule |
|---|---|---|
| [Nara Foundation Architecture](nara-foundation.md) | Accepted architecture summary | Summarizes current ADR authority; it is neither an implementation ledger nor a roadmap. |
| [Runtime Composition Interface Design](runtime-composition-interface-design.md) | Canonical harness for runtime-composition scenarios | Valid during the active RGF plan; rebaseline against landed owners and Accepted ADRs at RGF closure. |
| [Source Extension Package Interface Design](source-extension-package-interface-design.md) | Canonical harness for source-package scenarios | Tracer-gated; illustrative interfaces do not authorize a package kernel or public API. |
| [Multi-Role Extension Package Tracer](multi-role-extension-package-tracer-design.md) | Scenario appendix | Owns only the multi-role tracer; shared package decisions remain in the source-package harness. |
| [Extension Contract Kernel Interface Design](extension-contract-kernel-interface-design.md) | Candidate-interface appendix | No production consumer exists; it must not create a crate or public API before a named tracer admits it. |
| [Asset Import Host Interface Design](asset-import-host-interface-design.md) | Import specialization appendix | ADR 0115 admits only the concrete Editor Image workflow; this broader appendix remains non-normative and cannot authorize a registry, watcher, durable cache, project plugin, or multi-product graph. |
| [Extension Package Concept Guide](extension-package-concept-guide.md) | Explanatory guide | Explains vocabulary only and never adds decisions. |
| [Render Capability Demand and Pressure Matrix](render-capability-pressure-matrix.md) | Render evidence/admission appendix | Evaluates whether a capability deserves a tracer; it is not a renderer roadmap. |
| [Render Extension Capability Interface Design](render-extension-capability-interface-design.md) | Future render canonical harness | Inactive and needs rebaseline; the RGF closure architecture handoff owns activation against the landed render owners and evidence. |
| [UI Product Boundaries, Editor Dogfooding, and Porting Strategy](ui-product-boundaries-editor-dogfood-and-porting-strategy.md) | UI boundary canonical draft | Evidence-gated; it does not select a final editor toolkit or a shared game/editor widget core. |
| [UI Foundation Design](ui-foundation-design.md) | UI deep-module design blueprint | Non-normative module map, reference-pressure matrix, component admission map, and scale vocabulary. ADR 0102 and ADR 0107-0109 own durable decisions. |

`Canonical harness` means the primary non-normative scenario workbench for one subject. `Appendix`
means a focused specialization that cannot redefine its canonical harness. `Guide` explains the
current model. `Needs rebaseline` means its scenarios may remain useful, but file maps, type names,
implementation order, and authority claims must be reconciled before execution.

## Rebaseline Rules

- RGF closure updates this index, the foundation summary, runtime-composition status, and a durable
  architecture handoff bound to the reviewed release commit.
- Future render work starts only after its activation record binds the landed owners and symbols,
  current ADR statuses, relevant open questions, and differences from package/runtime harnesses.
- A design draft may propose names and test scenarios. Only an Accepted ADR selects a durable
  invariant, and only an active plan may order implementation.
