# Owned Editor Reopen Preparation

Status: Completed in `8f5003b` within the
[verification scope](../knowledge/engineering/verification/2026-10/2026-10-20T022302Z-owned-editor-reopen-preparation-and-publication-qualification-557ea95db99b4b8a87829870db1d6b13.md).

Baseline: `4356001`. Follow the qualified owned Save slice and the measured synchronous
Reopen cost. Keep ordinary Reopen actions and exact owner publication authority.

## Contract

- Admission captures the original document, kind, base revision, source binding and data-only
  authoring basis. A later edit or replacement cannot be overwritten by a prepared result.
- Background work owns a capability-bound read source, bounded decoding and pure authoring
  classification/preservation. It does not own the Workspace, original publication Host or
  native registry callbacks. Compiled component codecs remain on their owner thread.
- The original Host remains in the session. Only successful owner publication updates its
  identity/digest conflict baseline. Failed/stale/cancelled preparation leaves document, history,
  source incarnation, saved state and that baseline unchanged.
- Opening is visible across frames. Busy guards cover source-opening navigation, conflicting persistence,
  runtime replacement and Play admission; publication rechecks the Runtime exclusion.
  Activating another already open document remains allowed; it cannot retarget the captured operation.
- Logical task terminality alone cannot prove the read call returned. Close waits for physical
  preparation and existing runtime task retirement; dropping the session transfers running
  closure custody to the existing TaskPlugin/runtime quarantine. No new public Cancel action.
- Preserve the existing authoring preservation policy and uncertain-Save reconciliation.
  Do not add journals, a generic async operation service or async Plugin/codec/panel traits.

## Lane Map

Mode: execute_direct. Shared branch `refactor/foundation-contract-hardening`, base `4356001`.
Root is the sole Cargo/format/stage/commit owner. No concurrent Cargo; use shared target and
`CARGO_BUILD_JOBS=1`. Preserve unrelated `vendor/` and `myeasylog.log`; no push or PR.

| Lane | Writable scope | Responsibility |
| --- | --- | --- |
| Lower owner | `src/project_host/persistence.rs`, narrowly needed `file_publication.rs` read-source capture | Owned read/parse/pure authoring preparation, owner-thread native admission and focused tests |
| Root | `runtime/editor` session/background integration, plan/docs/measurement | Exact request capture and stale-result admission, lifecycle, serialized verification |
| Native/consumer | `editor/`, root `tests/` except measurement, `reference-game/tests/`, module-consumer tests | Bounded terminal waits and native close/replacement/completion behavior |
| Adversarial review | Read-only source and mature local references | Failure scenarios and affinity/source/revision/lifecycle constraints |

## Verification And Delivery

1. Prove the owned input/output on a real thread, while native callbacks execute only on owner
   admission. Preserve bounded capability reads and data-only lossless authoring behavior.
2. Use deterministic held preparation for later edits, late completion, file changes, Play,
   close, task rejection/cancellation/panic and abandonment. Check actual state and source bytes.
3. Migrate public/native/reference consumers and qualify affected Windows/DX12 trajectories.
   A policy test or source review is not native presentation qualification.
4. Record v3 first/max owner drive and end-to-end Reopen observation separately, using the
   existing optimized empty-entity workload. Keep v1/v2 data frozen and report native callbacks
   and owner projection costs as limits. No SLA or total CPU claim follows.
5. Review independently, update ADR/ledger/guides and the two standalone HTML reports outside
   docs, record verification and create precise Conventional Commits.

## Next Ownership Repair

Source review identified a pre-existing gap outside read preparation: once Save returns its Host
to a session, operation-scoped custody no longer retains failed temporary cleanup if that session
is dropped. Reopen read-task quarantine does not own it. Prioritize one private retirement owner
for the full Scene/Prefab lane lifetime before more startup/source latency work. Operations should
borrow that owner; an idle Host must not masquerade as a `SaveWork` with an empty request.

Decide cleanup authority, close admission, lane replacement and the already-Stopped runtime case
together. Prove terminal failed Save -> Drop, failed/stale Reopen -> Drop, replacement and explicit
close/retry with held cleanup. No successful-read Drop test establishes those compound cases.
