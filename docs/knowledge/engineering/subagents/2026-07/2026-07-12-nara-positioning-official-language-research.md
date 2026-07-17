---
type: "Subagent Finding"
title: "Nara positioning: official language used by Godot, Bevy, and Unity"
description: "Primary-source comparison of how Godot, Bevy, and Unity separate product identity, audience value, features, and vision language."
timestamp: 2026-07-12T00:54:19Z
record_id: "8296b15f-1051-4ce5-993a-deaad670274a"
tags: ["nara", "strategy", "positioning", "godot", "bevy", "unity"]
status: "complete"
producer_id: "positioning-language-research"
run_id: "session-019f4ede-b40a-77c3-8336-c6f713f3fa86"
verified_by: "official-first-party-web-sources"
---

# Finding

## Executive conclusion

Godot, Bevy, and Unity do not use a list of architecture decisions as their product positioning.
Their public language follows a consistent hierarchy:

1. A short identity sentence names the product category and, sometimes, one durable qualifier.
2. A supporting sentence names the creation scope, audience, or outcome.
3. Technical mechanisms and workflow capabilities appear later as features.
4. Ambition and principles are kept in vision or marketing language rather than treated as a
   technical definition of the product.

Bevy is the most technical of the three. Its hero calls the engine "data-driven" and says it is
"built in Rust," but even Bevy puts ECS and hot reloading in separate feature blocks. Godot and
Unity keep language choices, scripting, and other implementation mechanisms out of their core
identity sentences. None of the cited core identity statements mentions reflection.

The direct implication for Nara is to remove both "Schema-driven" and "reloadable Behavior" from
the positioning sentence. They can be documented as architecture and product capabilities. The
identity can remain a plain description of what Nara is; its current scope and maturity should be
stated separately.

## Classification used in this note

- **Identity positioning** answers "what is this product?"
- **Audience/value** answers "for whom, for what work, or with what outcome?"
- **Feature language** names a concrete capability, workflow, technology, or mechanism.
- **Vision/marketing language** expresses ambition, principles, category leadership, or emotional
  promise. It should not be mistaken for an implemented feature contract.

All pages below are first-party sources and were accessed on **2026-07-12**. Web page wording is
mutable; the quotations record the page state on that date.

## Godot

### Homepage

Source: [Godot Engine homepage](https://godotengine.org/) (hero and feature grid; accessed
2026-07-12).

**Identity positioning, hero heading:**

> "Your free, open-source game engine."

This is category-first. The only qualifiers are ownership/licensing properties that are durable
and immediately meaningful to a prospective user. It does not name Nodes, Scenes, GDScript,
reflection, reload, or another engine mechanism.

**Audience/scope, hero supporting line:**

> "Develop your 2D & 3D games, cross-platform projects, or even XR ideas!"

This describes what users can make and where the scope extends. It still does not explain how the
engine is implemented.

**Feature language, below the hero:**

> "Use the right language for the job"

The accompanying feature text names "Godot's own GDScript, C#, C++, or bring your own using
GDExtension." The scripting languages are therefore presented as choices within the feature
inventory, not as Godot's identity.

The same feature grid separately presents "Innovative design," "Dedicated 2D engine," "Simple and
powerful 3D," and "Release on all platforms." Godot lets the complete feature set communicate its
technical direction instead of compressing those decisions into the hero sentence.

### Official documentation

Source: [Introduction to Godot, stable documentation](https://docs.godotengine.org/en/stable/about/introduction.html)
("What is Godot?" synopsis; accessed 2026-07-12).

**Expanded identity plus user value:**

> "Godot Engine is a feature-packed, cross-platform game engine to create 2D and 3D games from a
> unified interface. It provides a comprehensive set of common tools, so that users can focus on
> making games without having to reinvent the wheel."

The documentation is more descriptive than the homepage, but it still leads with category,
creation scope, and workflow value. "Unified interface" is an experiential property, not an
internal architecture label. The following documentation text describes one-click export to
desktop, mobile, web, and consoles; it does not make a scripting language or reflection system part
of the definition.

### Vision statement

Source: [Vision for the Godot Engine](https://godot.foundation/policies-and-procedures/project-vision-statement)
(official Godot Foundation project vision, last edited 2026-06-23; accessed 2026-07-12).

**Audience and outcome ambition:**

> "Godot wants to be the best tool available for small to medium-size teams to ship
> professional-quality games."

**Experience principle:**

> "Godot is fast and intuitive for anyone on a development team, from beginners to veterans, not
> just programmers. Fast iteration keeps creativity flowing and making games fun."

The page explicitly says it is a vision statement guiding long-term direction and resource
allocation. These are useful aspirations, but Godot keeps them separate from both its concise
homepage identity and its concrete feature descriptions.

## Bevy

### Homepage

Source: [Bevy Engine homepage](https://bevy.org/) (hero and feature sections; accessed 2026-07-12).

**Identity positioning, hero heading:**

> "A refreshingly simple data-driven game engine built in Rust"

**Value/ownership line immediately below:**

> "Free and Open Source Forever!"

Bevy is the technical outlier in this comparison. It includes one architectural orientation
("data-driven") and its implementation/ecosystem language ("built in Rust") in the core identity.
Both qualifiers are broad, durable, and directly relevant to Bevy's programmer audience. The
sentence still avoids naming individual subsystems or workflow features.

**Feature language, separate "Data Driven" block:**

> "All engine and game logic uses Bevy ECS, a custom Entity Component System"

**Feature language, separate "Hot Reloading" block:**

> "Get instant feedback on your changes without app restarts or recompiles"

The feature grid then limits the current claim to scenes, textures, meshes, and extensible asset
types. This is important: even for an engine whose public identity is explicitly technical, ECS and
hot reload are explained as capabilities with scope, not stacked into the hero definition.

Other major systems, including the 2D renderer, 3D renderer, render graph, UI, scenes, sound, and
compile times, are peers in the same feature inventory. No single one is presented as the complete
meaning of Bevy.

### Official quick-start documentation

Source: [Bevy Quick Start: Getting Started](https://bevy.org/learn/quick-start/getting-started/)
(accessed 2026-07-12).

The guide opens with "Welcome to Bevy!" and immediately helps readers create a project or examine
examples. It later states that Bevy is "built in pure Rust" while explaining installation. In this
documentation context, Rust is practical setup information; the guide does not repeat ECS or hot
reload as a product identity formula.

## Unity

### Homepage

Source: [Unity homepage](https://unity.com/) (hero and product overview; accessed 2026-07-12).

**Marketing headline:**

> "Build great"

**Category-leadership claim supporting the headline:**

> "Unity is the world's leading game engine, supported by the most successful game development
> community in history and powered by a system that ensures each decision is informed by what
> players love."

This is marketing and social proof, not an architecture definition.

**Product/value framing later on the homepage:**

> "Game development, unified. Develop, deploy, and grow your game in one place, on your terms."

Unity frames the product around lifecycle outcomes. It names C# only much later in the page under
the iteration-oriented copy: "Iterate quickly in C#, create 2D and 3D games..." C# is supporting
feature/workflow evidence, not the core identity.

### Unity Engine product page

Source: [Unity Engine product page](https://unity.com/products/unity-engine) (product hero,
features, and FAQ; accessed 2026-07-12).

**Product identity and scope, hero:**

> "Unity Engine"
>
> "Build 2D and 3D experiences in any style, for any platform. The Unity engine gives you the
> power and flexibility to realize your creative vision."

**Plain category definition, FAQ:**

> "Unity Engine is a game and app development software that allows game developers to create video
> games across 20+ platforms and billions of devices."

**Feature language, separate scripting section:**

> "Unity Engine relies on industry standard programming concepts such as .NET and C#, allowing you
> to use familiar tools such as Visual Studio and Jetbrains Rider."

The page treats scripting alongside graphics, performance, multiplayer, collaboration, and LiveOps.
It does not use C#, ECS, hot reload, serialization, or reflection to define what Unity Engine is.

### Company About page

Source: [About Unity](https://unity.com/our-company) (company hero; accessed 2026-07-12).

**Company/platform identity:**

> "We are the leading platform to create and grow games and interactive experiences across all
> major platforms from mobile, PC, and console, to extended reality (XR)."

This is broader than the engine product page because it describes Unity the company and platform.
It still uses user activities and output scope rather than implementation mechanisms.

## Mechanism placement comparison

| Mechanism or qualifier | Godot core identity | Bevy core identity | Unity core identity | Where it actually appears |
| --- | --- | --- | --- | --- |
| ECS | No | No; "data-driven" is used, but "Bevy ECS" is not in the hero | No | Bevy's separate Data Driven feature block |
| Hot reload | No | No | No | Bevy's separate Hot Reloading feature block |
| Reflection/schema | No | No | No | Not used in the cited identity language |
| Scripting language | No | Rust is named as the language the engine is built in, not as a scripting subsystem | No | Godot and Unity feature copy; Bevy setup documentation |
| Renderer/scene system | No | No | No | Feature inventories |
| Platform scope | Supporting hero/docs copy | Feature inventory | Supporting hero/product copy | Commonly used as scope/value, not mechanism |
| Open source/free | Core durable qualifier | Immediate value line | Not part of cited identity | Identity/value rather than architecture |

The careful distinction for Bevy is that "built in Rust" is a factual ecosystem qualifier, while
"Bevy ECS" and hot reload are subsystem claims. Bevy demonstrates that one durable technical
orientation can be appropriate for a programmer-first product; it does not support turning a
positioning sentence into a feature manifest.

## Recommendation for Nara

### Keep the public identity deliberately plain

Recommended durable identity:

> **Nara is an open-source game engine for building and shipping games.**

Recommended current status/scope line, kept separate:

> **Nara is currently in active development, with an initial focus on complete 2D PC game
> production.**

This answers what Nara is and honestly bounds the present proof target. It does not make an
unverified productivity promise or freeze an implementation choice into the product definition.

If Rust discoverability is useful in a technical venue such as the repository description, use a
plain factual sentence rather than a branded architecture term:

> **Nara is an open-source game engine written in Rust.**

That optional sentence follows Bevy's precedent, but it is not required in the main positioning.
"Rust-native" can remain technical shorthand in architecture documents when its exact contract is
defined.

### Separate the other layers

- **About/vision:** describe the kind of complete game-production experience Nara is trying to
  enable, and the teams or projects it intends to serve.
- **Current status:** state Trial/experimental maturity and the 2D PC reference-game proof target.
- **Features:** describe editor workflows, Behavior, data/schema evolution, headless testing,
  debugging, Rust extension points, rendering, packaging, and supported platforms individually.
- **Architecture:** use precise terms such as versioned schema, ECS, service boundaries, and reload
  semantics, including their limitations and ADR ownership.

This structure lets the feature set reveal Nara's direction, as requested, without asking one
sentence to carry the engine's entire technical design.

## Final answer to the framing question

"Schema-driven" and "reloadable Behavior" are too technical and too narrow for Nara's core public
identity. The issue is not that these ideas are unimportant; it is that they belong to different
layers. Schema is an architecture contract. Reloadable Behavior is a product capability. Neither
answers the basic identity question better than "game engine."

The official language of all three reference engines supports a simpler approach: state the
category plainly, state current scope and maturity separately, and let accurate feature pages and
documentation explain the direction.
