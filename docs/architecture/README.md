# Architecture Document Map

This page identifies the authority and maintenance role of Nara's architecture documents. It is
an index, not another source of architecture decisions.

## Authority Order

1. [Accepted ADRs](adr/README.md) own durable decisions.
2. [ADR implementation status](adr/implementation-status.md) owns repository evidence and remaining
   gaps.
3. The completed [Editor Project Browser and Source Index v1](../plans/2026-08-19-001-feat-editor-project-browser-and-source-index-v1-plan.md)
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
   and last-good retention at `1a832df`. EPB-U0 through EPB-U5 are complete; no unrelated Editor
   successor is active.
   The completed [Standalone Generic Editor Project Open and Recovery](../plans/2026-08-18-2158-feat-standalone-generic-editor-project-open-and-recovery-plan.md)
   plan closes at `a86aee1` with
   [verification `25fbee67d7ea41a9bc8406abe6e8b221`](../knowledge/engineering/verification/2026-08/2026-08-19T110847Z-sge-u5-standalone-generic-editor-product-closure-25fbee67d7ea41a9bc8406abe6e8b221.md)
   and completed
   [registration `931591fb6d2b4bcbad3151922814d8e6`](../knowledge/engineering/registry/2026-08/2026-08-19T111039Z-standalone-generic-editor-open-recovery-codex-root-931591fb6d2b4bcbad3151922814d8e6.md).
   SGE-U1 through SGE-U5 establish the bounded data-only schema basis, lossless degraded Scene and
   Prefab authority, shell-first lifecycle, strict selected-native Runtime agreement, modular
   generic authoring workspace, and one-window Project Manager/Open/Recovery product proof. No
   successor Editor lane is active automatically. The now-completed EPB lane was the explicit
   project-browsing successor; external Play, import/reimport, watchers, source mutation, and
   extension hosting remain separately trigger-gated.
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
| [Asset Import Host Interface Design](asset-import-host-interface-design.md) | Import specialization appendix | Dormant until ADR 0087 or an Accepted successor admits a concrete importer workflow. |
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
