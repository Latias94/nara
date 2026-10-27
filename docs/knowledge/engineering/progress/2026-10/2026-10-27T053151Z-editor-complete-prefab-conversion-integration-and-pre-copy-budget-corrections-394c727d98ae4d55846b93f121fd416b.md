---
type: "Work Progress"
title: "Editor complete Prefab conversion integration and pre-copy budget corrections"
description: "Shared actual-demand acquisition and Editor conversion custody are integrated; source pre-copy budget fixes, independent consumers and native qualification remain in progress."
timestamp: 2026-10-27T05:31:51Z
record_id: "394c727d98ae4d55846b93f121fd416b"
producer_id: "codex-root"
run_id: "editor-prefab-conversion-2026-10-27"
---

# Summary

The full foundation rebaseline Goal remains active. Work is on
`refactor/editor-prefab-conversion`, based on `c407ddf`; current changes are uncommitted.
The source conversion predecessor is already on local main. No remote operation is authorized.
Untracked `vendor/` remains untouched. Cargo is serial with Rust 1.95.0,
`CARGO_BUILD_JOBS=1` and the root target.

# Implementation And Review

The current integration retains a nominal Scene/Prefab conversion token in one Host operation,
extracts concrete actual-demand acquisition from preview capture, and routes exact Workspace
publication without an extractable ordinary edit. Review pagination keeps fixed local IDs.
Fresh validation after review includes physical source/Schema/metadata observations, stable
lookup resolution and actually used file-versus-working-copy lineages. Cancel and whole-Session
Close retain pending work until physical return. The Inspector has explicit complete-conversion
wording and correlated pending/result observations.

Peirce (`01a17bba-e693-7a21-b547-4bb4acfd05a0`) is fixing two accepted source P2 findings:
complete containing-candidate copying was admitted after allocation, and published dependency
classification retention was omitted. The work must propagate the last loader-tightened
allowance through localization and full preparation, reserve actual copies before allocation,
and keep normal edit/history admission intact. Source files remain worker-owned until frozen.

The Host now reserves review display work before compilation, including old/new pages and the
current String-to-Arc row copy. Participant observation copies are charged before publication.
The parent also enabled cancellation during final validation, matching the existing Host
capability, and extended the actual UI input regression.

# Verification

The intermediate Root cohort `3f9911b1-4cb8-4e69-81ca-165ae715e1b8` passed all 13 selected
tests: eight Host and five acquisition cases. Build took 1m18s; execution took 3.424s.
It covers real Open, prepare/review/commit, source history, Save/Reopen, stale file/StableId/
working-copy changes, unrelated navigation, cancellation and physical Close custody.
It sampled unfinished Source fixes and is not final qualification.

The preceding retention run had three passes and one missing-Inspect fixture failure; the
fixture was corrected but has not been rerun. The earlier Workspace four-case run passed.
No final full suite, independent public Host consumer, actual native conversion trajectory or
performance claim is established yet.

# Parallel Work

- Parfit (`01a17bd3-15af-7642-a777-be542acacf93`) independently reviews Host acquisition,
  custody and UI read-only.
- Gibbs (`01a17bd2-eb9f-74a0-9b98-e324d467c9b7`) owns only a new ignored native conversion
  trajectory and its module declaration.
- Sagan (`01a17bd3-b71d-7a10-8747-65fc4a2c3854`) owns only the independent public Host consumer
  test file in the module-consumer workspace.
- Hume's eight Host tests are integrated and Hume is closed. All verification stays with
  the parent; workers run no Cargo, formatter or commit.

# Next Action

Integrate Source fixes, independently re-review them with another reviewer, freeze writers,
inspect diffs, format and run focused Source/Workspace/Host/UI cohorts. Then qualify the
independent public Host and selected actual native trajectory, update evidence/ledger and
create precise local commits only for coherent verified work. Keep the full Goal active.

# Citations

- [Conversion plan](../../../../plans/2026-10-27-prefab-conversion-plan.md)
- [Foundation rebaseline plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Source predecessor evidence](../../verification/2026-10/2026-10-27T041718Z-complete-source-owned-prefab-conversion-and-current-stable-lookup-qualification-cbce190af0c4488b97895e68905573a6.md)
- [Host conversion](../../../../../src/project_host/runtime/editor/prefab_conversion.rs)
- [Actual-demand acquisition](../../../../../src/project_host/authoring/project/capture/prefabs.rs)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
