---
type: "Memory Event"
title: "Decision: Dynamic non-Rust ECS lowering requires concrete evidence"
description: "Per-World dynamic ComponentIds and RuntimeSchemaRecord remain contingent implementation options for a real adapter, not the default project component representation."
timestamp: 2026-07-11T15:28:41Z
record_id: "2b925547ca63414d8a31ab34ba0b11eb"
producer_id: "codex-root"
run_id: "session-019f4ede-b40a-77c3-8336-c6f713f3fa86"
event_kind: "Decision"
---

# Event

Per-World dynamic `ComponentId` values backed by a fixed-layout `RuntimeSchemaRecord` remain a
contingent implementation option for a real scripting or data adapter. Rust-defined components are
the default, and Nara does not build dynamic lowering merely to preserve language neutrality.

# Impact

# Citations
