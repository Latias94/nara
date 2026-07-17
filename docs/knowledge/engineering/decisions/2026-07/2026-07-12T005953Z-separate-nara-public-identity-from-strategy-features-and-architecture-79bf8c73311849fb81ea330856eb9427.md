---
type: "Decision"
title: "Separate Nara public identity from strategy features and architecture"
description: "Nara's public identity will state only its product category and implementation language; maturity, strategy, features, validation scope, and architecture will remain separate claims."
timestamp: 2026-07-12T00:59:53Z
record_id: "79bf8c73311849fb81ea330856eb9427"
producer_id: "codex-root"
run_id: "session-019f4ede-b40a-77c3-8336-c6f713f3fa86"
---

# Decision

Nara's durable public identity is deliberately plain:

> Nara is an open-source game engine written in Rust.

Current maturity is a separate statement:

> Nara is in early development. APIs and project formats are expected to change.

The identity will not encode `Schema-driven`, `data-driven`, `code-first`, ECS, a
hot-reload mechanism, a scripting-language choice, a reference genre, a market
wedge, or a claim of Unity-level productivity. Rust-first production, modularity,
and iteration behavior belong in strategy and concrete feature documentation.

# Context

Positioning drafts attempted to compress Nara's architecture and future product
experience into one sentence. Schema metadata, ECS, modular crates, editor tooling,
and reload paths are important implementation or product capabilities, but none
answers the basic identity question better than `game engine`.

Official first-party language from Godot, Bevy, and Unity shows a useful separation. Godot and Unity lead with category, creation scope, licensing, or outcome. Bevy is more technical but keeps ECS and hot reload in feature sections even though its identity includes the broader qualifiers `data-driven` and `built in Rust`.

# Alternatives

- **Use architecture as positioning.** Rejected because it is implementation-led, difficult for ordinary creators to interpret, and unstable while the architecture is still being validated.
- **Use one flagship capability as the identity.** Rejected because a reload, editor, scripting, or Schema feature cannot represent the whole engine and may change after evidence.
- **Make an unverified productivity or market-leadership claim.** Rejected until the reference game and external author evidence support it.
- **State only the product category and verified facts.** Chosen because it remains accurate while the engine, target users, and feature set develop.

# Consequences

- A future root README opens with the plain identity and an adjacent early-development warning.
- Product strategy explains the target problem, current primary user, validation approach, metrics, and non-goals without becoming a tagline.
- Feature documentation lists concrete capabilities and labels them as implemented, experimental, or planned.
- Architecture documents may use precise terms such as versioned schema, ECS,
  service boundary, Rust plugin, and optional scripting adapter, with their exact
  contracts and limits.
- The Brotato-like 2D PC game remains the current proof scope, not Nara's permanent public category.
- `Cargo.toml`, `AGENTS.md`, and future README wording use the same plain identity;
  strategy and feature docs carry the detailed direction.

# Citations

- [Official positioning language research](../../subagents/2026-07/2026-07-12-nara-positioning-official-language-research.md)
- [Product strategy grilling checkpoint](../../sessions/2026-07/2026-07-11T143713Z-nara-product-strategy-grilling-checkpoint-0b40da43ccfd4e1eaf1efb0ea0406b3b.md)
- [Reference game production-proof decision](2026-07-12T003130Z-use-the-brotato-like-reference-game-as-a-production-proof-1e9cf73b7f144e05bed780c0f84bf7a1.md)
- [Godot Engine homepage](https://godotengine.org/)
- [Bevy Engine homepage](https://bevy.org/)
- [Unity Engine product page](https://unity.com/products/unity-engine)
