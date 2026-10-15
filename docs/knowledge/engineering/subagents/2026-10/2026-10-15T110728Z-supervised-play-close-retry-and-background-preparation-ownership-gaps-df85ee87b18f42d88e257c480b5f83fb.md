---
type: "Subagent Finding"
title: "Supervised Play close retry and background preparation ownership gaps"
description: "Subagent Finding for Supervised Play close retry and background preparation ownership gaps."
timestamp: 2026-10-15T11:07:28Z
record_id: "df85ee87b18f42d88e257c480b5f83fb"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
subagent_id: "/root/u5_editor_runtime"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
---

# Finding

The carried SPL work already owns processes, output, handshakes, generations, and late spawn
results. U12 should integrate it. Three remaining product gaps are unreachable close retry,
unchanged observations that request redraw, and large preparation work still running on the UI.

# Evidence

The Closing dispatch filter in editor/src/app.rs accepts only WorkspaceIntent::Resolve. The
read-only egui toolbar disables Play actions. CleanupIncomplete offers RetryRetirement while the
close-requested owner accepts RetryClose. Fixing only one layer leaves retry unreachable.

owner.rs returns a Running observation on every quiet advance; the Host applies an accepted attempt
as changed unconditionally, and the platform wake handler requests redraw unconditionally. A None
result means retain the existing result, so observation comparison must preserve that semantics.

advance_build invokes publish_candidate and generation/store/publication synchronously. Full
digest/copy/revalidation/sync work is followed by supervisor launch revalidation on the UI thread.
Source supersession checks also hash inputs. The existing spawn worker already demonstrates
sync_channel(1), JoinHandle, wake, and late-result physical retirement; reuse that concrete pattern.

# Recommendation

Admit a narrow cleanup retry during Closing without enabling document editing. Publish observations
only when their effective model changes. Move generation preparation and exact launch validation to
one bounded private worker phase that returns the same store/candidate/cleanup owner. Cancellation
sets a flag and is observed between chunks; close/replacement waits for physical completion and
cleanup, including an in-progress sync. Keep exact generation, handle, digest, nonce, and first-present
guarantees. Add no generic filesystem job system or second supervisor.

# Disposition

Read-only investigation for U12 while U5 consumer verification runs. No U12 production change or
verification is claimed by this record. The implementation must exercise normal Closing retry,
quiet Running, and injected pending publication before the existing native canary.

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md U12
- docs/architecture/adr/0116-standalone-supervised-editor-play.md
- editor/src/app.rs: dispatch_close_resolution_actions
- crates/nara_tooling_egui/src/editor.rs: toolbar read_only gate
- src/project_host/runtime/editor/external_play.rs: cleanup actions and observation application
- editor/src/supervised_play/owner.rs: advance and close-requested action admission
- editor/src/supervised_play/generation/store/publication.rs
- editor/src/supervised_play/supervisor.rs: revalidate_for_launch
- editor/src/supervised_play/process/spawn.rs
