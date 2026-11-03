---
type: "Verification Evidence"
title: "Independent joint motion comparison selects explicit serial automatic execution"
description: "Real borrowed solver, three compositions, feature-unified executor regression and narrow Ack evidence; shared temporal and prediction gates remain open."
timestamp: 2026-11-03T13:06:31Z
record_id: "dda9dc3f7d2e45d6baf30927cce70fc7"
producer_id: "codex-foundation-main"
related_plan: "docs/plans/2026-11-03-joint-motion-execution-plan.md"
git_branch: "refactor/foundation"
---

# Verification

Windows 11, Rust 1.95.0, `CARGO_BUILD_JOBS=1`, root `target`; all Cargo work ran serially.
Base HEAD was `906bd31`; the paths below identify the reviewed successor, not that base's contents.

| Gate | Actual result |
| --- | --- |
| Independent consumer, default features | 11/11, nextest `4834d263-705c-4f5d-aed7-48daad0ab4a9` |
| Same consumer, dependency-enabled Bevy `multi_threaded` | 12/12, `43bdac1d-ab7d-4a8b-a24f-2f3475e8dbdb` |
| Root App, Transform and Gameplay | 235/235, 6 ignored native tests outside the selected gate, `6b60a699-b53d-4d03-bdc7-0d498f3065fb` |
| Fixture workspace correctness Clippy | Passed on the final join-state successor with the existing Reflect dead-code warning |
| Formatting | Root and independent workspace `cargo fmt --all` passed |
| Independent reviews | Two read-only reviewers found no confirmed P1/P2 in the App/fixture scope; final reviewer inspected the successful-join state and nonempty command regression |

## Result

The independent animation and physics crates run the same finite inputs through public sets,
a private three-segment plan and Bevy App::empty. Direct and scoped borrowed Rapier 2D 0.33.0
produce equal same-build poses with actual floor contact. A held worker blocks join/pose/publication;
an unsuccessful borrow cannot mark its attempt joined or run the successful callback. The managed
consumer uses the actual ordered hierarchy and Transform snapshots for zero/multiple fixed ticks,
Pause/exact Step and parent translation changes. A real command is consumed and acknowledged,
then a later failure leaves the drive sticky while retaining its Ack and deferred effects.

The feature-unified RED run `1c67bd2f-2334-424e-9710-764c5bda972f` observed automatically driven
Startup/Core systems moving to Bevy workers. The corrected App explicitly creates every automatic
schedule with Bevy's serial executor. The private cleanup is covered by the construction inventory;
the independent thread probe directly observes the public Startup/Core stages. An explicitly
driven custom schedule still uses its owner-selected parallel executor after seal.

## Limits and next action

This is not solver-internal parallelism, a throughput measurement, cross-platform bitwise
determinism or production automatic-parallel admission. The private finite plan checks its segment
inventory, not provider correctness. Its shared raw projection oracle is not an exhaustive equality
proof against production snapshots. The raw failed-borrow regression does not qualify complete
managed solver-failure publication suppression. No new phase interface is frozen.

Continue the shared temporal interleaving: a pre-cut original capture first prepared after the cut
must not adopt current history. Reuse the existing history lineage and native two-View fixture.
Bounded prediction/effect admission, full rollback, authored collision and broader R10 qualification
remain open. Keep the full foundation goal active.

## Sources

- [Unit and comparison decisions](../../../../plans/2026-11-03-joint-motion-execution-plan.md)
- [Independent consumer](../../../../../tests/fixtures/motion-execution/README.md)
- [Owning execution contract](../../../../architecture/adr/0003-own-app-plugin-and-schedule-lifecycle.md)
