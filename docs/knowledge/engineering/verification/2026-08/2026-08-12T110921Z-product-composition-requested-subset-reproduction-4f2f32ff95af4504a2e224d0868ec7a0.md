---
type: "Verification Evidence"
title: "Product composition requested-subset reproduction"
description: "Reproduces the all-features compile-ceiling leak that changes a render-wgpu-only requested plugin closure."
timestamp: 2026-08-12T11:09:21Z
record_id: "4f2f32ff95af4504a2e224d0868ec7a0"
tags: ["product-composition", "wgpu", "cargo-features", "rup-u3", "red"]
status: "failed"
producer_id: "codex-root"
run_id: "product-composition-contribution-correction"
related_plan: "docs/plans/2026-08-12-002-refactor-product-composition-contribution-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "459bdf2"
verified_by: "cargo-nextest:b2613f0f-0f19-4cff-b931-fc6cfc92a48e"
---

# Verification

Run the existing requested-subset canary under the complete compiled feature ceiling:

```powershell
$env:CARGO_BUILD_JOBS='1'; cargo nextest run --locked --all-features --test plugin_composition `
  -E 'test(render_wgpu_profile_satisfies_the_shared_render_plugin)' `
  --test-threads=1 --no-tests fail
```

# Result

Failed as expected on `459bdf2`. Nextest run `b2613f0f-0f19-4cff-b931-fc6cfc92a48e`
reported `MissingPlugin { plugin: nara.render-wgpu, required: nara.ui-render }` while the manifest
requested only `render-wgpu`.

# Evidence

- `Cargo.toml#presentation-ui` weakly compiles `nara_render_wgpu/ui-submitter`.
- `crates/nara_render_wgpu/src/lib.rs#WGPU_RENDER_PLUGIN_REQUIREMENTS` changes the base plugin
  declaration when that compile feature is present.
- `tests/plugin_composition.rs#render_wgpu_profile_satisfies_the_shared_render_plugin` is a genuine
  requested-subset test: it does not request Runtime UI or UI presentation.
- The same compile/runtime authority leak exists in the Winit platform-accessibility path because
  runner and presentation behavior are guarded only by `cfg(feature = "platform-accessibility")`.

# Follow-up

Pause RUP-U3 and execute the focused Product Composition Contribution Correction. The green proof
must preserve the manifest request and remove the compile-feature dependency from runtime plugin and
platform behavior; broadening the requested product is not a valid fix.

# Citations

- `docs/architecture/adr/0079-root-product-capabilities-and-placeholder-domain-retirement.md`
- `docs/architecture/adr/0106-content-and-presentation-product-composition.md`
- `docs/plans/2026-08-12-002-refactor-product-composition-contribution-correction-plan.md`
