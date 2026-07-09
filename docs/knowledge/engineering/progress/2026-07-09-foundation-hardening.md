---
type: Work Progress
title: foundation hardening
tags: nara,foundation,reflection,plugins,render,diagnostics
timestamp: 2026-07-09T12:37:27+08:00
status: verified
---

# Summary

The foundation hardening plan in
`docs/plans/2026-07-09-002-refactor-foundation-hardening-plan.md` has landed the
core structural changes through implementation commits and final verification.

# Implemented Contracts

- Reflection persistence is stricter: serializable component registrations need
  explicit schemas, duplicate runtime type registrations are rejected, defaults
  are kind-checked, and scene/prefab spawn remains preflight-first.
- `nara_reflect` is split into focused `value`, `schema`, `path`, `codec`,
  `migration`, and `registry` modules.
- Plugins are fallible: `Plugin::build` returns `PluginError`, duplicate unique
  plugins are rejected, and plugin groups use `add_plugin_if_missing` when
  idempotent composition is intended.
- Diagnostics are collection-first: `DiagnosticReport::push` no longer logs
  implicitly, and tracing output goes through explicit `emit_to_tracing` bridge
  methods.
- `nara_render` exposes backend-neutral render backend status resources, while
  `nara_render_wgpu` updates those resources for skipped frames, rendering, and
  backend errors.
- The unused public `RenderBackend` trait was removed. Backend extension remains
  resource/system/plugin based until a second backend or test adapter creates
  real abstraction pressure.

# Commit Trail

- `e8d3252 docs(plan): add foundation hardening plan`
- `3d27c4e refactor(reflect)!: require explicit component schemas`
- `d4f449d refactor(app)!: make plugin installation fallible`
- `27480c2 refactor(render)!: expose backend status resources`
- `c6bdec7 refactor(reflect): split reflection modules`

# Citations

- [Foundation plan](../../../plans/2026-07-09-002-refactor-foundation-hardening-plan.md)
- [Foundation architecture](../../../architecture/nara-foundation.md)
- [Open questions](../../../architecture/open-questions.md)
- [Verification evidence](../verification/2026-07-09-foundation-hardening.md)
