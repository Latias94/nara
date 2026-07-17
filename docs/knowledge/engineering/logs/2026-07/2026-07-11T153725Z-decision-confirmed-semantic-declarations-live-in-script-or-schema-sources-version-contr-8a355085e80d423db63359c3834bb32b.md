---
type: "Memory Event"
title: "Decision: Rust declarations and project data own schema semantics"
description: "Rust declarations and persistent project data formats own semantic declarations; stable IDs are explicit where practical and sidecars are reserved for sources that cannot retain them."
timestamp: 2026-07-11T15:37:25Z
record_id: "8a355085e80d423db63359c3834bb32b"
producer_id: "codex-root"
run_id: "session-019f4ede-b40a-77c3-8336-c6f713f3fa86"
event_kind: "Decision"
---

# Event

Rust declarations and persistent project data formats own semantic declarations. Stable IDs and
tombstones are explicit in their source where practical; generated or adapter-owned sources may use
version-controlled sidecars when they cannot retain identity safely. The catalog is derived and
rebuildable.

# Impact

# Citations
