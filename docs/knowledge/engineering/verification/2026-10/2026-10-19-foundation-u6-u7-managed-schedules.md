---
type: Verification Evidence
title: Foundation U6 and U7 schedule admission and managed custom execution
description: Fallible installed-graph sealing and explicit custom schedule driving preserve Runtime authority.
timestamp: 2026-10-19T09:05:00Z
record_id: b7c213e5158b484c8eac3e28542b39ed
producer_id: codex-root
run_id: foundation-correction-20261019
related_plan: docs/plans/2026-10-19-1335-refactor-foundation-correction-plan.md
git_branch: refactor/foundation-contract-hardening
---

# Contract

After plugin finish and before Ready/Startup, App fallibly initializes every installed schedule
using Bevy's own graph admission. `ScheduleGraphError` identifies the actual interned label.
Public-anchor automatic-deferred compatibility remains a distinct policy and error type. Cycles
and configured ambiguity errors cannot defer their first failure until a custom schedule runs.

`RuntimeInstance::run_schedule` explicitly executes an installed custom label between owner drives.
It requires Running with no pending control, rejects built-in/missing labels, and retains managed
authority validation and sticky fault capture. Its exclusive borrow prevents reentry. It does not
advance frame/tick clocks, alter the automatic sequence, or publish a presented frame on its own.

# Serial Verification

Windows, Rust 1.95.0, `CARGO_BUILD_JOBS=1`, one Cargo invocation at a time:

- Initial graph-admission selection `8bdc7bbb-f0cb-45c4-bfbe-8550076b6e14`: 14 passed.
- Managed ownership/registry/compatibility selection `85160d25-5b48-4b29-8fb0-f30571a91a95`:
  23 passed, including legitimate Startup Commands observed by Update and prior-fault rejection.
- The root library/content/runtime boot cohort passed 138 tests with the new graph error mapping.

Cases cover Startup/Update/custom cycles, ambiguity errors, a graph added during plugin finish,
retained incomplete-close ownership after failed sealing, independent explicit custom labels,
built-in/missing/state/control rejection, deferred writes, authority faults and ordinary Stop/Close.
Independent review found no actionable correctness issue. Consecutive custom-query change
detection was identified as a coverage improvement; a four-run regression verifies first run,
unchanged second run, Update mutation, and an unchanged subsequent run. It passed in the complete
cohort below.

The prior-fault assertion initially expected a newly raised execution fault. Runtime already
derives Faulted state from its reporter, so the correct result is `Unavailable(Faulted)` before
execution. The assertion was corrected; this was not a production fault-admission defect.

# Remaining Evidence

Run `ce1fe22a-6ede-46b8-869c-0c94cf18824b` passed all four selected root tests. Three verify the
renamed-root dependency boundary, explicit custom execution through the ordinary public root,
and the full independent renamed-root fixture including its own managed execution. The fourth
is the Scene public boundary trajectory recorded under U5.
Complete App/Input/Gameplay/Tilemap all-features run `7f6e5032-bf1f-4cdf-8f1b-d5eef0ab784e` passed
all 242 tests, including the U12 slot-resolution cases and U14/U15 domain regressions. Together
with the public-consumer run, U6/U7 focused acceptance is complete. The final workspace gate
remains a separate plan-wide requirement.
