---
type: "Subagent Finding"
title: "Stable source catalogues need accepted import updates and keep namespace and reopen limits explicit"
description: "Subagent Finding for Stable source catalogues need accepted import updates and keep namespace and reopen limits explicit."
timestamp: 2026-10-15T18:06:45Z
record_id: "26cb59198abd437f94828c59109b95d4"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
status: "accepted"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
subagent_id: "/root/u7_generic_asset_references"
---

# Finding

Making the generic preview consume its admitted identity database is necessary for stable UUID
references, but a database frozen only at Open can reject a newly imported image that was accepted
later by the existing Editor image owner. The accepted image publication point must update the
private preview identity context before subsequent Sprite decoding can rely on it.

# Evidence

`AssetRef::resolve` cannot resolve UUIDs without `ProjectAssetDatabase`, even if AssetServer already
reserved the image handle. The generic preview now explicitly receives admitted identities.
`EditorImagePublicationPort` is the concrete trusted publication point for imported images and
already owns the image's UUID/path after Host metadata validation. Reflecting over AssetServer
would lose source-kind/admission authority and is unnecessary.

Two narrower limits remain explicit: live source-index Refresh does not readmit an entire startup
closure, so source rename is established through reopen; the existing raw AssetPath database
requires distinct logical paths across admitted roots and fails collisions instead of silently
cross-wiring records. The two-source consumer uses explicit source-scoped paths. Neither limit
authorizes a general mount manager or watcher in this slice.

# Recommendation

Use the same ordinary import publication critical section: validate the new record before image
commit, then update the private copy-on-write preview database only after commit succeeds. Reuse
the existing database insertion checks through a small actual-consumer method rather than clone
the full database on every import. Precharge the initial copy in existing admission residency;
bound later accepted records through the existing last-good-image owner. Same identity is
idempotent; conflicting metadata does not partially publish a new identity.

# Disposition

The U7 agent is implementing this correction and a real Import-to-Sprite preview regression.
No new AssetManager, process-global registry, proof receipt, source parser, or generic transaction
framework is admitted. Generic nested Prefab geometry still has its existing explicit safe-preview
omission; successful data-only closure admission does not claim otherwise.

# Citations

- src/project_host/runtime/editor/authoring_runtime.rs
- src/project_host/runtime/editor/preview.rs
- src/project_host/authoring/project.rs
- crates/nara_asset/src/database.rs
- crates/nara_scene/src/authoring_preview.rs
