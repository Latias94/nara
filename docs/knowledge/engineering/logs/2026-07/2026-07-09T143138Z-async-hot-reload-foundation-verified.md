---
type: Memory Event
title: async hot reload foundation verified
tags: nara,async,tasks,asset,hot-reload,verification
timestamp: 2026-07-09T14:31:38+08:00
status: passed
---

# Event

The async hot reload foundation plan is implemented and verified. The slice adds `nara_tasks`,
`CoreStage::TaskUpdate`, `AssetPlugin` reload scheduling, typed import job contracts, async image
reload through `ImagePlugin`, optional `nara_asset_watch`, and review hardening for stale result,
atomic-save, dependency, prepare-registration, and watcher boundary cases.

# Impact

- Async work has a nara-owned task API and a scheduled main-thread apply boundary.
- Image assets now prove the source-change -> reload request -> task -> typed result -> asset state
  -> render prepare path.
- Watcher code is adapter-owned and disabled by default.
- Remaining architecture focus moves to Apply Changes, runtime UI, material/sampler authoring, and
  render-graph forcing use cases.

# Citations

- [Progress](../../progress/2026-07-09-async-hot-reload-foundation.md)
- [Verification](../../verification/2026-07-09-async-hot-reload-foundation.md)
- [Plan](../../../../plans/2026-07-09-003-feat-async-hot-reload-foundation-plan.md)
