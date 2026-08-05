---
type: "Decision"
title: "NUI post-U8 identity receipt and reference UI checkpoint"
description: "Fixes the minimum source/projection identity rule, exact backend-submission receipt semantics and retention, AccessKit trial boundary, and the reference HUD/terminal/Retry product matrix before UI core implementation."
timestamp: 2026-08-05T10:15:28Z
record_id: "cafb82e406d24834a129c782e07b65bb"
tags: ["nui", "ui-identity", "presentation-receipt", "accessibility", "reference-game", "decision"]
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "6f2f370321b1c86b5b715eebfb70f8f1b3c75098"
---

# Decision

- New UI identity projects an opaque source `SceneEntityId` together with a separate structured
  `PrefabProjectionPath`. Existing `anchor/source` strings remain transitional Scene locators but
  cannot become UI, session, semantics, selection, or persistence keys.
- `UiPresentationReceipt` means that the concrete backend accepted and submitted one exact
  immutable packet for a matching surface/environment revision. It does not prove that a human saw
  the pixels. The adapter retains the current receipt/frame pair plus two prior pairs, and routes
  outside that journal fail closed.
- The immutable semantic tree and typed accessibility actions are formal kernel facts. AccessKit is
  a bounded desktop-adapter trial, not a kernel, widget, packet, or headless dependency contract.
- The reference UI product input is the authored HUD, terminal overlay, and Retry modal described
  in `docs/architecture/reference-game-ui-product-slice.md`, across 1280 x 720, 1920 x 1080, and
  2560 x 1440 with the documented state matrix.

# Context

NUI-U8 closed runtime structural ordering, which exposed three decisions that could otherwise
become accidental compatibility contracts in NUI-U10 through NUI-U13. ADR 0038's transitional
expanded string mixed a prefab source entity with its projected location. ADR 0102 and the active
plan used “presented” language that could overstate what a backend receipt proves. The reference
game still constructed most HUD geometry in Rust, so the UI core lacked a concrete data ownership
target.

# Alternatives

- Accept all of Proposed ADR 0083 now: rejected because its asset/product/package identity tracer
  has not run and the UI slice needs only the source-versus-projection separation.
- Keep parsing `anchor/source` strings in the UI adapter: rejected because nested prefabs and future
  identity migration would permanently leak a transitional encoding into logical keys.
- Treat compilation, queueing, or acquire as presentation evidence: rejected because none proves
  submission of the exact packet and all can make stale geometry interactive.
- Make AccessKit a required cross-platform boundary now: rejected because the semantic snapshot is
  the durable contract and only the desktop adapter currently supplies a concrete consumer.
- Continue designing the reference HUD in Rust while building generic crates: rejected because it
  would let infrastructure lead the product proof again.

# Consequences

- ADRs 0038 and 0102 and the active plan now use one compatible minimum identity/receipt contract.
- NUI-U9 can proceed independently. NUI-U10 must introduce the minimum structured projection type
  before stable logical keys, and NUI-U11/U12 must implement the exact receipt journal and route
  semantics without claiming human visibility.
- NUI-U13 must replace the current Rust-built HUD structure with Scene/Prefab-authored structure,
  theme, labels, and layout. Rust remains responsible only for typed facts, intents, and domain
  rules.
- The complete durable identity model, generalized editor provenance, and cross-platform
  accessibility backend policy remain evidence-gated rather than implicitly accepted here.

# Citations

- `docs/architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md`
- `docs/architecture/adr/0083-durable-project-asset-and-document-entity-identity.md`
- `docs/architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md`
- `docs/architecture/reference-game-ui-product-slice.md`
- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md`
