---
type: "Decision"
title: "Use language-neutral packages and isolated extension hosts by default"
description: "User-directed product decision separating Package identity from Contribution execution and making replaceable isolated Extension Hosts the ordinary editor-extension baseline."
timestamp: 2026-07-20T12:10:51Z
record_id: "b133546ad9c84682bd570e388532bf18"
tags: ["package", "extension-host", "editor", "native", "csharp", "rust", "lifecycle"]
producer_id: "codex-root"
run_id: "package-extension-product-contract-20260720"
related_plan: "docs/plans/2026-07-20-001-feat-package-extension-product-contract-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "5bc321d41aba59072a1f97ccc0473f91e0b2c161"
---

# Decision

Nara's product-facing Package is a language-neutral distribution, installation, version, update,
ownership, and removal unit. It may carry content, source/static Rust plugins, managed modules,
native target artifacts, Editor tools, importers, build/export support, samples, documentation, and
migrations as distinct typed Contributions.

Ordinary executable Editor Contributions use a replaceable isolated Extension Host by default.
Trusted same-process managed or native execution remains an explicit privileged tier for a proven
latency, affinity, or authority need and may truthfully require Host or Editor restart.

(session-settled: user-directed - chosen over loading all executable extensions into the Editor by
default: isolated Host replacement preserves the no-Editor-exit workflow and a reliable reclamation
baseline without eliminating deep privileged integration.)

Package installation and Contribution activation remain separate. The product exposes immediate,
Extension Host replacement, Play/Runtime replacement, and Editor restart as distinct user-visible
effects rather than promising universal hot unloading.

This is a product requirement and research disposition only. It does not accept an ADR, authorize a
Package Manager, select an IPC or Widget protocol, add CoreCLR, define a Native Extension ABI, or
activate work outside the registered reference-game plan.

# Context

The discussion began with the expectation that a Package should behave like a Unity Package: it can
be installed, updated, removed, distributed through an eventual Asset Store, and may include both
runtime functionality and Editor UI. Treating Package as the narrow OQ-045 Rust plugin-plus-Schema
helper would not satisfy that product meaning.

The Godot audit showed that a C++ Editor can dynamically add docks, inspectors, gizmos, importers,
and other UI because script and GDExtension code register objects through a language-neutral Editor
model. Godot does not hot-replace its Editor core, and GDExtension reload is conditional and can
return `NEEDS_RESTART`. The .NET audit likewise showed that collectible assembly unload is
cooperative and can fail when roots, threads, callbacks, or native dependencies remain.

The Unity audit showed stronger package governance: project/package manifests, dependency graph,
lock state, multiple sources, update/removal awareness, and editable embedded packages. Godot's
Asset Store path is primarily archive extraction and addon discovery, so its distribution model is
not sufficient for Nara's stated update/removal promise.

# Alternatives

- **Make Package a Rust `Plugin` or source crate.** Rejected because it cannot represent content,
  managed and native artifacts, Editor-only roles, install ownership, or platform/export closure.
- **Make C# the Package extension model.** Rejected because Package must not depend on an optional
  second author language; C# remains one first-class managed Contribution family.
- **Load all trusted extensions in the Editor process.** Rejected as the default because unload and
  failure containment are not reliable enough for the promised install/update workflow.
- **Isolate all extensions without exception.** Rejected because low-latency physics, rendering,
  window, native SDK, and custom-surface integrations may require a privileged Runtime or
  same-process boundary.
- **Use a graduated Package model.** Selected: one Package identity coordinates Contributions whose
  placement and restart behavior remain honest and role-specific.

# Consequences

- OQ-031 owns the Product Package and Host/trust topology. Its Cargo-backed source package is one
  distribution form, not the product umbrella.
- OQ-045 remains a narrow Rust root-contribution ergonomics tracer and must not acquire Package
  Manager, managed/native loading, or multi-role activation semantics.
- OQ-007 continues to own the optional C# gameplay Adapter. Managed Editor Contributions require a
  separately admitted Editor contract even if both roles ship in one Package.
- The Editor Shell retains workspace, document, undo, selection, window/event-loop, Nara UI, and
  Package transaction authority. Ordinary extensions operate through versioned models, commands,
  registrations, and generation-owned leases.
- Rust runtime plugins retain complete typed ECS and schedule freedom through fresh executable or
  Runtime generations. They are not forced through an IPC abstraction.
- Native DLL/SO/dylib support remains technically reachable through a separately admitted
  C-compatible ABI or disposable Native Host; raw Rust ABI is not the ecosystem contract.
- Package Manager UX must distinguish installed release, enabled Contributions, built artifacts,
  active generations, trust placement, and restart impact.
- Unity-like dependency/lock/ownership governance is the target shape, while Cargo and
  NuGet/MSBuild remain authoritative for their own source graphs.

# Citations

- `docs/plans/2026-07-20-001-feat-package-extension-product-contract-plan.md`.
- `docs/knowledge/engineering/godot-unity-package-extension-lifecycle-research.md`.
- `docs/knowledge/engineering/godot-csharp-integration-research.md`.
- `docs/knowledge/engineering/extension-ecosystem-engine-research.md`.
- `docs/architecture/open-questions.md`, OQ-007, OQ-031, and OQ-045.
- `docs/architecture/extension-package-concept-guide.md`.
- `docs/architecture/source-extension-package-interface-design.md`.
- `docs/knowledge/engineering/decisions/2026-07/2026-07-20T035959Z-adopt-graduated-extension-freedom-as-a-product-requirement-8afea5ae39a74890a531cb7c3d839f26.md`.
- `docs/knowledge/engineering/decisions/2026-07/2026-07-11T163345Z-defer-extension-technology-selection-behind-a-unified-package-experience-7f435154e74e45359c661b98d145d693.md`.
