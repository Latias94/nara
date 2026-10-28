---
type: "Work Progress"
title: "Native Prefab template copy and publication integration"
description: "Records source-owned copying, actual publication integration and independently reviewed custody corrections before final qualification."
timestamp: 2026-10-28T21:00:26Z
record_id: "261a364ec6af4ce5bae5c11cd70b1dc8"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
git_branch: "refactor/native-prefab-template-import"
related_plan: "docs/plans/2026-10-29-native-prefab-template-import-plan.md"
---

# Scope

Continue the full active foundation Goal. Local main was fast-forwarded to `ba6dc1e` after
filesystem query `c5bee11`, actual incomplete artifact recovery `22499a3` and documentation
qualification `ba6dc1e`. No remote push/PR. The new branch is
`refactor/native-prefab-template-import`; all its source changes are currently uncommitted.
Only original untracked `vendor/` predates this unit and remains untouched. Cargo is serial,
Rust 1.95.0, `CARGO_BUILD_JOBS=1`, root target reused.

The [native Prefab plan](../../../../plans/2026-10-29-native-prefab-template-import-plan.md)
continues the managed-artifact lifecycle through actual project-owned source copies. A strict
Content descriptor states original universe, identity policy, explicit `prefabs_root`, entry and
at most 32 JSON/RON source members. The destination uses actual project settings, fresh stable
sidecars or independent location keys. All nested declarations, base/hop constraints and declared
payload/default references are remapped by Scene/Reflect. Local entity IDs and Schema owners
remain unchanged. Closed Prefabs are the initial subset; Scene/image/external bindings remain.

# Implementation

Reflect owns collected-leaf rewriting, validates all original paths/references before callbacks
and admits replacement allocation. Scene owns bounded complete source-copy preparation and
validates every copied source together through the production schema-only Prefab compiler.
Host captures actual installation Content and metadata, fixes keys/bytes, then uses the existing
template-import Tasks lane. Windows staging closes child handles before exact directory
publication and freshly admits current files afterwards, like project creation; closed creation
IDs never supply continuity. Native pending fences ordinary source admission. Actual publication,
failed staging and Close observations are distinct. Session/Application expose the same operation.

Exact unresolved native publication custody must transfer explicitly before another import;
otherwise the common import guard refuses `RequiresReconcile`. Borrowed observation does not
discard an orphan. The move-only take operation transfers actual directory/member/error facts.

# Executed Evidence

- Reflect new helper: 14 passed, 126 outside selection; 0.321s; build 21.84s;
  `9ccb6210-5491-4fd6-8ba1-84cb9acb6fcc`.
- Scene new source preparation: 11 passed, 441 outside selection; 0.456s; build 1m08s;
  `ccc5b623-7da0-4f13-8046-01c9f2fb51ad`.
- Host import/installation: 42 passed, 821 outside selection; 7.131s; build 1m19s;
  `bf53267b-4d34-4866-a669-11f1aa498001`.
- Host subsequent actual post-publication drift/explicit handover cohort: 15 passed,
  849 outside selection; 4.148s; `3f5c0c3c-9055-4bec-a6fd-74b8fd14daec`.
- First full independent runner: 32/33 passed;
  `69c825c4-ef08-4609-9a77-5781dcf5080e`. Both new native-template consumers passed, including
  double import, different source root, fresh sidecars and all internal references, Browser
  editing/history/Save/Reopen, artifact withdrawal/unlink and fresh project Open. The older
  actual ACL incomplete-artifact test called reclamation before separate source-index work
  returned, receiving `Busy`. Its settlement helper now includes actual public pending fields;
  the production refusal stays unchanged. The runner stopped before managed/Application/native
  stages, which therefore have no new qualification from this failed run.

Initial root test compilation found helper visibility, missing test imports and Rust 2024 opaque
return lifetime capture. Use existing exact struct fields and a concrete gate owner; no product
boundary change was needed. The initial source descriptor hardcoded `prefabs/`; require explicit
original `prefabs_root` and preserve arbitrary original configured coordinates instead.

# Independent Review And Current Corrections

Reflect reviewer `01a18438-9495-7991-ba31-15106b04d5f0` found no confirmed issue in the bounded
leaf rewrite and is reviewing frozen Scene copy separately. Host reviewer
`01a18441-0107-7390-a344-1dccf1e5a5a6` found four concrete defects:

1. Source-index `AssetMeta` classification omitted native Scenes/Prefabs sidecars, which are
   indexed as Unknown. Include metadata whose corresponding indexed source is Scene/Prefab,
   check its kind/path and pin digest. Reuse `index.entry` for bounded lookup.
2. Later imports could overwrite failed native custody and Close facts. The common refusal and
   explicit owner transfer above correct this.
3. Plain file/directory exclusive creation could fail after creation while losing custody.
   File staging now uses existing `create_temp`/cleanup transfer and retains actual child
   directories. Filesystem worker `01a1844b-aaf4-79e1-a8e0-819d956ed25b` is adding the concrete
   `create_child_directory_retained` failure owner, reusing the existing internal create outcome.
4. Final tree enumeration discarded regular-leaf identity. `tree::verify_observed` now compares
   the actual enumerated object with the still-held fresh-admission handle. Completed installation
   verification uses that same correction.

These final corrections and regression tests are not yet recompiled. Do not commit/merge or
claim the whole unit qualified until actual checks and independent recheck complete. Host input
decoding remains finite under document/encoded/per-document format limits; Scene's aggregate
copy budget does not claim a bound on earlier input decode RSS or a streaming output encoder.

# Next Action

Poll the current Reflect/Scene full-library run (exec 49329; log in TEMP
`nara-native-prefab-scene-reflect-libraries.log`) before starting another Cargo. Wait for FS
worker source-ready, then compile/retest actual Host corrections and new FS tests. Recheck the
four Host findings independently, resolve any Scene finding, run affected libraries and the
actual consumer runner serially, static/minimal/fmt/doc checks, then precise commits and local
ff main. Keep the full foundation Goal active.

# Citations

- [Full foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Managed artifact plan](../../../../plans/2026-10-28-managed-source-tool-artifacts-plan.md)
- [Native Prefab plan](../../../../plans/2026-10-29-native-prefab-template-import-plan.md)
- [Artifact ownership](../../../../architecture/source-tool-artifact-ownership-design.md)
- [Finite source-tool guide](../../../../guides/finite-source-tools.md)
