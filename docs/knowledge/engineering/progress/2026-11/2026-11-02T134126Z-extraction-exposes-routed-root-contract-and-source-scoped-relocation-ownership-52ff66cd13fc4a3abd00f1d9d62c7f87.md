---
type: "Work Progress"
title: "Extraction exposes routed-root contract and source-scoped relocation ownership"
description: "Actual extraction witness drives typed root targets; shared compiler and retained/moved reference owners are under integrated qualification."
timestamp: 2026-11-02T13:41:26Z
record_id: "52ff66cd13fc4a3abd00f1d9d62c7f87"
producer_id: "codex-foundation-main"
run_id: "extraction-relocation-20261102"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "88fd728"
---

# Summary

The complete foundation Goal remains active. Local main and `refactor/foundation` remain
`88fd728`; this continuation's complete extraction work is uncommitted. The preceding private
cut observation and all current retained/moved reference proposals remain part of that working
tree. Do not discard them or claim complete CP2 from their private tests. No push/PR or change
to vendor/reference checkouts.

Actual extraction pressure exposed an owning root-binding representation gap. A passing public
before/after/forest witness proved that a direct local ID cannot name the same root after its
declaration moves behind the new Q hop. Accepted ADRs0006/0038 and the CP2 plan were reconciled
before replacing that field with `PrefabEntityTarget`. The earlier direct-ID topology
qualification retains its original scope, not blanket coverage of this refinement.

# Details

## Routed Root Selection

`PrefabInstanceBinding::Root { base, root }` and `PrefabInstance::root` now require a typed
`PrefabEntityTarget`, with source/addition origin and exact canonical predicates at every hop.
There is no compatibility alias, inferred mode, retired bare-ID reader or format-version ladder.
The existing compiler admits routed root demand before layout, preserves borrowed lookup for a
huge missing direct root, resolves declaration facets before canonical normalization, and checks
the targeted entity is the actual sole effective root. Root demand's declaration and remaining
cursor participate in cycle state. Component inheritance uses the normalized selected root.

Complete source copying now rewrites root-route predicates too. Raw logical cost, serialized
publication shape and Root Host retention use the complete target. Public positive controls cover
the extraction case, wrong-base/missing/non-root/extra-root refusal, source/addition namesakes,
nested aliases and actual builtin Name/Visibility Native spawn. Independent minimal Transform
and JSON/RON authoring consumers still pass. The public original representation witness passed
as `a2ba16c2-64fd-4f0b-b256-c6e51510cc27` before it was replaced with the positive future control.

## Source-Scoped Reference Proposals

Private `BranchRelocation` borrows the actual `SourceCut`, original canonical key and fresh Prefab
destination. It distinguishes entity continuity from instance-slot continuity: P:k's effective
entity remains the anchor, while P:k's original instance declaration moves into Q:k. Retained
declarations receive new Q hops only where their actual route crosses moved records/slots.
Payload values use their own lexical scope; a binding root uses its selected base. Persistent
references, namespace distinctions and unrelated paths remain unchanged and borrowed.

`MovedReferenceInputs` directly borrows that relocation owner. Its parameter table cannot be
switched to another cut. Moved declarations retain cut-local references in Q; outward references
and original P inputs become explicit Q inputs with arguments in P's retained lexical scope.
Identical resulting arguments reuse a parameter. Later explicit P reentries use the retained
route rewrite. New names are deterministic within this newly constructed destination; no
incumbent input table is merged or shadowed. No fallible admission follows binding-table mutation.

The real compiler consumes those proposals in two manual P/Q fixture workflows. The first uses
retained root binding, property target, original-root instance claim and payload references, with
explicit typed occurrence correspondence and normalized values/hierarchy. The second rewrites
P:x into declared Q inputs plus actual P arguments and proves the lowered target still denotes x.
The manual split does not implement an extraction builder, declaration proof, full corpus capture
or publication. Source-only meaning and hidden-default rewriting remain to be integrated.

Independent review found one P2: deduplication computed a long incumbent argument's `cost()`
before admitting that scan. It now admits work using O(1) route lengths before either cost walk;
borrowed-argument clone-cost scanning has the same gate. Already-owned rewritten arguments move
into the table without duplicate clone charges. An8192-hop prior-argument/empty-input immediate
refusal control checks this boundary and preserved bindings. The reviewer reread the final
branches and closed the assigned finding. Root and retained-relocation reviewers had no assigned
confirmed P1/P2. All agents are closed.

## Actual Verification

All parent verification was serial, jobs1, root target, Windows/MSVC Rust1.95.0:

| Scope | Result |
| --- | --- |
| Scene all-feature and no-default library checks | Pass. |
| Root all-feature library check | Pass with the typed-root/retention production changes. |
| Scene full library plus six affected public root/placement binaries | 689/689, run `075de2eb-8281-478b-97e3-c5fe88d8339a`; covers routed roots and the initial nine retained proposal controls. |
| Integrated retained proposal compiler consumer | 10/10 selected relocation cases, `cf183e54-2ee4-4c14-9756-5a753300e6a8`. |
| Complete-root-copy hop remapping and retained proposal replay | 18/18, `a06f5842-29ed-45d9-b941-741f9cf58ed9`; includes actual copied root-bound compilation. |
| Moved/retained proposals, actual compiler consumers and copy cohort | 24/24, `b9d6a1e4-e1ab-44ed-aabd-af9a2feb2871`; subsequent clone-accounting refinement was separately replayed below. |
| Final frozen moved-reference owner and actual outward compiler consumer | 6/6, `b286913a-557a-4caf-af3a-17c728e7931e`; includes inclusive budgets, late refusal, long-prior-route pre-scan admission and owner-bound arguments. |
| Root runtime-core without serde, real Transform consumer | 1/1, `9b5d4aa1-1bbc-47c5-a2c0-1ba1f5603ad9`. |
| Independent module root-binding editor/persistence/compile/Native consumer | 1/1, `72781e50-5bf6-4173-9319-14207e69c27d`; separate manifest shares root target. |
| Root Host retention, including routed pins in three instance carriers | 9/9, `ceafa650-88d2-4fa1-a344-cea753b419de`. Independent formulas include each hop, origin string and canonical base. |
| Final Scene/Root all-feature/all-target correctness Clippy | Pass,1m54s, `-D clippy::correctness`; general and current unused-WIP warnings remain. Not a `-D warnings` result. |

Initial test builds exposed three overlooked internal Root callers and one fixture moving a
still-borrowed key. The first Root all-target Clippy additionally exposed the old retention test's
Root caller, now migrated and strengthened with routed carrier controls. Those callers/fixture
were corrected before the passing runs; no weaker
constructor, alias or lifetime bypass was added. One worker violated its no-Cargo assignment,
started an unqualified command and stopped it without test results. Parent observed no remaining
compiler/Cargo process before taking verification ownership back; that interrupted run supplies
no evidence. No other project process was killed. General WIP unused warnings remain, because
the complete extraction consumer is not yet wired. No new native trajectory is claimed here.

All parent verification sessions are terminal, including final Clippy session17588. No Nara Cargo
or task artifact helper remains active. Other-project Baozi compilation was left alone. Logs stay under TEMP with prefixes nara-routed-root-* and
nara-extraction-*, rather than tracked raw output. Final Root and independent module format checks,
diff whitespace and memory validation pass;1176local links in11changed Markdown documents have
no missing target. Source is frozen and the index is empty. Last F: free space was about34GiB;
no artifact deletion ran.

# Next Action

Continue the actual extraction owner from these verified source-owned proposal primitives.
Implement the complete physical declaration walk and actual P/Q construction; use existing Reflect
plans for component/property payloads and hidden contextual defaults. Retain source-dependent
default meaning instead of pinning current values. Couple generated input declarations to the
new instance arguments; affected unknown semantics refuse specifically, while unrelated complete
declarations stay equal. Cross-cut hierarchy uses typed structural intent, not reference inputs.

Retain bounded declaration/contributor/slot facts before the original compiler discards them,
then prove injective old/new effective occurrences over dirty/open and closed-source inventory.
Connect borrowed Review, actual observed-absent Q/sidecar creation, Pending/retry recovery and
bind-all-before-Ready installation. Qualify actual new-process interruption and public edit/
Undo/Save/Reopen/Check/Play before committing the complete extraction unit/local main merge.
Do not turn these private proposals into a generic equivalence engine, a second evaluator or a
fake CP1 Schema successor. The whole foundation Goal remains active.

# Citations

- [Current CP2 plan](../../../../plans/2026-11-02-prefab-extraction-plan.md)
- [Prior structural-cut checkpoint](2026-11-02T124903Z-topology-prerequisite-committed-and-source-owned-extraction-cuts-verified-6a6b48f4571b4ce8bf2eb98c10910beb.md)
- [Original topology qualification](../../verification/2026-11/2026-11-02T121917Z-explicit-root-binding-and-source-placement-production-qualification-0eada24ff287416bbfe94ccf511d2380.md)
- [Root identity contract](../../../../architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md)
- [Private relocation owner](../../../../../crates/nara_scene/src/prefab/extraction/relocation.rs)
- [Moved-reference owner](../../../../../crates/nara_scene/src/prefab/extraction/relocation/moved.rs)
- [Actual public root consumer](../../../../../crates/nara_scene/tests/extraction_root_selection.rs)
