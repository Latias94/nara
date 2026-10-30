# Editor, Extensibility, And Rust Iteration Direction

Date: 2026-10-30. Status: research and recommendations, not an accepted design or implementation plan.

The semantic Editor foundation is worth keeping. The consequential pre-1.0 decisions are where
third-party contributions attach, which contributions can coexist, and what survives code replacement.
Replacing Dear ImGui alone will not answer those questions. Conversely, neither a universal extension
ABI nor a mandatory scripting language is needed to answer them.

## Evidence And Scope

| Source | Inspected revision |
| --- | --- |
| Nara | `75e1284ed28bc5a96686609f7dae8805d1924788`, `main` |
| Bevy, read-only `repo-ref/bevy` | `ae6152152d6fb8308ad6038314b48ef295e80fbc`; manifest says `0.20.0-dev` |
| Godot, read-only `repo-ref/godot` | `7be59b463a4798834d72d6d0a2942ddd01d920ee`; `version.py` says `4.8 beta` |
| Fyrox, read-only `repo-ref/fyrox` | `5495d012b32145674501af2e67d2fd06a06f04dd`; manifest says `2.0.0-rc.1` |

These are mechanism comparisons against fixed source revisions, not claims that the three checkouts
are stable releases or identical to Nara's dependencies. Reference checkouts had no tracked changes.
Official documentation was checked online on this date; mutable documentation is distinguished from
the source pins. Nara file links identify local files with explicit line ranges at the revision above;
they are not immutable public permalinks.

Read [Strategy](../../../STRATEGY.md), the [architecture map](../../architecture/README.md), the
[October 27 language/extension study](../2026-10-27-language-extension-product-boundaries.md), and
Accepted ADRs [0046](../../architecture/adr/0046-plugin-metadata-and-default-plugin-groups.md),
[0047](../../architecture/adr/0047-editor-workspace-and-scene-document-state.md),
[0093](../../architecture/adr/0093-rust-authoring-hot-iteration-and-optional-scripting-adapters.md),
[0114](../../architecture/adr/0114-standalone-generic-editor-authoring-and-execution-boundary.md),
[0116](../../architecture/adr/0116-standalone-supervised-editor-play.md), and
[0118](../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md).
[ADR 0120](../../architecture/adr/0120-schema-publications-and-execution-representations.md) remains
Proposed; this report does not treat its executable-publication design as implemented authority.

The following subsequent work changes the October 27 baseline and is **not reported as missing**:

- The [finite source-tool owner](../../plans/2026-10-28-finite-source-tool-owner-plan.md) and
  [inline facet](../../plans/2026-10-28-inline-source-tool-facet-plan.md) now place activation,
  proposal review, exact acceptance and physical child custody in the Session, with shared panel
  and Inspector observations.
- The [installed graph](../../plans/2026-10-28-installed-source-tool-graph-plan.md) distinguishes
  exact installations, disabled dependents and reviewed withdrawal. The
  [local release plan](../../plans/2026-10-28-local-source-tool-release-plan.md) is not the sole
  current implementation inventory: its [managed-artifact successor, lines 25–36](../../plans/2026-10-28-managed-source-tool-artifacts-plan.md#L25-L36)
  records actual artifact inventory, reviewed unlink, incomplete-publication recovery and native
  Prefab import successors. Their remaining product scope does not erase those implementations.
- Source-built field editing, prepared edits, exact dispatch results and independent document
  ownership already exist. The issue is where additional consumers can use them, not an absent
  Undo system or absent semantic Editor API.

No Cargo, builds, tests, formatting, subagents, source edits, ADR edits, index edits or memory writes
were performed. Historical test results are attributed, not requalified. CI, general performance
observability and platform-layer risks belong to the coordinating research lane.

## Findings At A Glance

| ID | Classification | Decision pressure | Priority |
| --- | --- | --- | --- |
| E1 | Architecture risk | Semantic separation is real; frontend distribution and interaction reuse are still incomplete. | Before expanding the custom-drawn frontend |
| E2 | Architecture risk | A useful independent command still needs a panel, a custom shell, or an Editor integration change. | Before expanding the tool SDK |
| E3 | Intentional gap | Rebuilding Rust and resuming a useful gameplay situation are not yet one ordinary Editor workflow. | Next Rust production workflow |
| E4 | Architecture risk | A multi-installation graph fronts one active component-tool route. | Before freezing installed contribution semantics |
| E5 | Intentional gap | Static native extension objects do not establish a reloadable binary/code-lifetime contract. | Before advertising native or managed in-process tool reload |

No new **confirmed defect** is claimed. The constraints below are source-confirmed; architecture risks
are consequences under the named workflows. Intentional gaps are unimplemented or explicitly
unsupported product promises, not violations of the current bounded contracts.

## E1. Preserve The Semantic Separation; Budget For A Real Frontend Port

**Trigger.** Move the existing Inspector, Snap panel and a dirty dialogue document into Nara's
custom-drawn Editor, preserving preview, exact acceptance, Undo, Save and project replacement.

**Evidence and caller cost.** `nara_tooling` has no Dear/Winit/WGPU dependency
([manifest:8–18](../../../crates/nara_tooling/Cargo.toml#L8-L18)). Session exposes document queries and
property reads independently of painting
([prepared_edit.rs:32–64](../../../src/project_host/runtime/editor/prepared_edit.rs#L32-L64)).
Application submission returns owned outcomes and drives the existing owners
([app.rs:978–1009](../../../editor/src/app.rs#L978-L1009)). Thus document authority is actually separated.

The native integration contract is more specific: factories produce `ImguiEditorPanel`,
`show` receives `&dear_imgui_rs::Ui`, and `EditorNativeComposition::build` returns
`(EditorApplication, ImguiEditorFrontend)`
([workspace.rs:25–59](../../../editor/src/frontend/imgui/workspace.rs#L25-L59),
[composition.rs:59–83](../../../editor/src/platform/composition.rs#L59-L83)). Even using Application
from another crate currently imports a package with unconditional Dear/Winit/WGPU dependencies
([editor/Cargo.toml:7–23](../../../editor/Cargo.toml#L7-L23)). This is a compilation/distribution
boundary, not evidence that document mutations depend on ImGui.

The port also includes behavior beyond drawing: field drafts retain conflict, compatibility and
pending-result state
([field_editor.rs:155–164](../../../editor/src/frontend/imgui/field_editor.rs#L155-L164)); source-tool
controls retire input on changed activation/basis or disappearance
([source_tool.rs:50–108](../../../editor/src/frontend/imgui/source_tool.rs#L50-L108)). Third-party
panels will need a new painter and an adapted interaction controller. Reusing only `EditorAction`
does not preserve those user-visible behaviors automatically.

**Counterevidence.** ADR 0118 deliberately assigns transient drafts to the frontend and explicitly
allows replacement of widgets, focus, docking and layout
([lines 25–36 and 68–75](../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md#L25-L75)).
This is not a discovered violation of the ADR. Nor is arbitrary property reading absent from
Application: `QueryDocumentEntities`, `QueryDocumentAuthoring` and `ReadDocumentProperty` already have
typed action outcomes
([frame.rs:700–713](../../../crates/nara_tooling/src/frame.rs#L700-L713),
[frame.rs:925–934](../../../crates/nara_tooling/src/frame.rs#L925-L934)). Missing direct convenience
methods do not make a neutral consumer impossible.

**Reference and recommendation.** Godot's dock registration accepts its concrete `Control` type
([pinned editor_plugin.cpp:95–119](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/editor/plugins/editor_plugin.cpp#L95-L119));
Fyrox's Editor plugins consume its concrete Editor and UI messages
([pinned plugin.rs:23–78](https://github.com/FyroxEngine/Fyrox/blob/5495d012b32145674501af2e67d2fd06a06f04dd/editor/src/plugin.rs#L23-L78)).
Neither precedent promises toolkit-independent plugin painters. Use them to justify a clear
semantic/painter split, not a universal widget language.

Port one complete external panel plus default field editing first. Keep source authority and
publication unchanged; extract only genuinely shared draft transitions into private toolkit-free
helpers when both frontends need them. Separate optional frontend dependencies if a second buildable
frontend or external Application consumer actually needs that boundary. Require the same dirty-input,
stale-result, close and persistence trajectories before retiring the predecessor.

**Cost of postponement.** Each additional ImGui-specific public panel/helper becomes another migration
site and another behavior to requalify. Establishing this seam now limits that growth; creating an
Editor kernel or widget IR now would add a different maintenance burden without proving the port.

## E2. Commands Need A Contribution Lifetime Independent Of Panel Visibility

**Trigger.** A studio adds “Snap selected entity to grid” to the normal Editor menu or a shortcut.
It must remain available when the Snap panel is closed, use the current document safely, and produce
one Undo entry. The same operation should later work from a Nara-drawn menu without modifying engine
source.

**Evidence and caller cost.** The independent Snap consumer already computes proposals and prepares
one owner transaction. Its public installation entry registers a panel
([snap_local_position.rs:183–226](../../../tests/fixtures/module-consumer/editor-panel-consumer/src/snap_local_position.rs#L183-L226));
its interaction starts from `ImguiEditorPanelContext`
([lines 282–344](../../../tests/fixtures/module-consumer/editor-panel-consumer/src/snap_local_position.rs#L282-L344)).
The composition accepts panels and source-asset domains, while the panel descriptor carries kind,
title, multiplicity, placement and initial visibility
([composition.rs:46–83](../../../editor/src/platform/composition.rs#L46-L83),
[panels.rs:10–35](../../../editor/src/workspace/panels.rs#L10-L35)). There is no corresponding contributed
command/menu/shortcut registration in this interface. The actual top-level menu enumerates panel
descriptors to offer “Open” actions
([imgui/mod.rs:575–599](../../../editor/src/frontend/imgui/mod.rs#L575-L599)).

The panel trait has painting and completion hooks; hiding or closing retires its instance
([workspace.rs:25–36](../../../editor/src/frontend/imgui/workspace.rs#L25-L36)). A tool author can add
widgets or key handling while a panel is painted, but that does not provide the named closed-panel
workflow. Integration with the ordinary shell requires a root Editor frontend/composition change,
or the caller must own a custom shell. This is a concrete extension-placement cost, not a complaint
about action-enum size or file length.

**Counterevidence.** The semantic work needs no new mutable-World escape hatch. Public query actions,
prepared property edits and exact Application submission already suffice to implement it in a custom
Host. Source-built panels are explicitly the current supported contribution; a global command
palette was not promised by the completed finite-tool plan. The existing field helper also already
removes substantial ordinary Inspector boilerplate. This finding concerns discoverable activation
outside panel painting, not missing editing operations.

**Reference and recommendation.** Godot separately exposes tool-menu callbacks and context-menu
plugins, rather than requiring a dock for every tool
([pinned editor_plugin.cpp:242–253](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/editor/plugins/editor_plugin.cpp#L242-L253),
[525–530](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/editor/plugins/editor_plugin.cpp#L525-L530)).
Its [official plugin guide](https://docs.godotengine.org/en/stable/tutorials/plugins/editor/making_plugins.html)
also separates plugin activation and cleanup from the UI it contributes. These are breadth and
lifetime precedents, not an endorsement of unrestricted callbacks into Nara's document owners.

Use the existing Snap consumer to add one finite source-built semantic command contribution:
identity, label, applicability observation and invocation through current read/prepare/commit ports.
Menu and shortcut placement should refer to that command; they should not own its editing semantics
or duplicate its Undo behavior. Keep project/source incarnation and result identity explicit.
Do not turn the action vocabulary into a string RPC bus or build a general workflow language.

**Cost of postponement.** Game teams otherwise accumulate tiny custom Editor forks or permanent panels
for ordinary commands. Implementing menus independently in each frontend would duplicate command
applicability and result handling. A single closed-panel Snap workflow is a better next API investment
than adding more package roles without an activation consumer.

## E3. Make Rebuild-And-Resume A Rust Product Workflow Before Promising Universal Hot Reload

**Trigger.** A developer reaches a reproducible combat or puzzle situation, changes a Rust function
body, then adds a component/resource field. They want to inspect the changed behavior at that situation
without manually playing through the level again, with explicit failure and fallback behavior.

**Evidence and caller cost.** Ordinary supervised Play has a trustworthy fresh-build boundary, but
`Restart` and `RunLastGood` go directly to `activate_last_good`; only the other start path invokes
`start_build`
([owner.rs:526–581](../../../editor/src/supervised_play/owner.rs#L526-L581)). The launch context contains
project, profile, executable generation and source receipt facts, not a gameplay checkpoint handoff
([launch.rs:129–147](../../../src/project_host/product_launch/wire/launch.rs#L129-L147)). The ordinary
supervisor/launch path does not integrate capture, compatibility validation and restoration of a
game-owned runtime checkpoint. A game can implement restoration itself; that is not yet the named
Editor workflow.

Use the **final** historical cohort at `2c26c92a42229d4b80f18ee43628ad1225d81a38`, not its preceding
exploratory/baseline cohort
([Rust Play report:103–131](../../benchmarks/2026-10-20-rust-play.md#L103-L131)):

| Workflow | Retained samples | Median seconds |
| --- | ---: | ---: |
| No-edit Play | 5 | 3.295 |
| Body edit then Play | 5 | 13.961 |
| Runtime layout edit then Play | 5 | 10.700 |
| Restart admitted executable | 5 | 1.499 |

These are old, small, uncontrolled cohorts, **not measurements of `75e1284e`**, stable P95 estimates,
or evidence that layout edits are intrinsically faster than body edits. Restart latency cannot be
presented as Rust recompilation latency. The unmeasured user cost also includes returning to the
interesting gameplay state.

**Counterevidence.** This is a deliberately bounded v1 protocol, not a broken Restart command.
Accepted ADR 0093 already distinguishes data reload, optional compatible patching, structural rebuild
and validated restoration. The two-level game has a real game-owned checkpoint consumer: its test
launches separate save and restore processes
([gameplay.rs:65–85](../../../samples/two-level-game/tests/gameplay.rs#L65-L85)); its format explicitly
tests version migration
([checkpoint.rs:66–85](../../../samples/two-level-game/tests/checkpoint.rs#L66-L85)). Therefore
“Nara cannot save runtime state” would be false. The missing part is joining those owners into the
ordinary development loop. Existing compilation failure and last-good evidence also remain valid.

**Reference and recommendation.** Bevy separates its development dynamic-linking facility
([pinned bevy_dylib:7–30](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_dylib/src/lib.rs#L7-L30))
from its Dioxus/Subsecond integration
([pinned hotpatch.rs:20–58](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_app/src/hotpatch.rs#L20-L58)).
The [official setup guide](https://bevy.org/learn/quick-start/getting-started/setup/#enable-fast-compiles-optional)
describes faster iterative compilation, not arbitrary state migration. Fyrox offers a different
model: serializable plugin entities are reconstructed across reload, with an explicitly experimental
CHR workflow in its [official book](https://fyrox-book.github.io/beginning/hot_reloading.html).

Join the existing game checkpoint owner to an explicit development capture/rebuild/restore operation.
Keep game-specific serialization and migration in the game or concrete adapter, and define what
happens when capture, compilation or restoration fails. Qualify one body edit and one structural edit
at the same useful gameplay situation. Compare optional coarse Rust function patching against that
baseline; do not make an ABI freeze or another language a prerequisite. Ordinary gameplay already
uses direct ECS systems
([Collect Game:127–145](../../../samples/collect-game/gameplay-plugin/src/lib.rs#L127-L145)); retain
that path instead of routing per-frame gameplay through finite Editor-tool IPC.

**Cost of postponement.** Games invent incompatible resume arguments and state files, while fast
restart numbers conceal repeated setup work. Once game-specific development launch protocols spread,
integrating restoration requires migrating every consumer. A bounded checkpoint handoff is useful
even if safe in-place Rust patching never becomes a supported feature.

## E4. The Installed Graph Must Not Freeze A Single-Tool Product Model

**Trigger.** Install an EnemyStats balancing tool and an independent quest-property tool in one
project. Both Inspector contributions should remain available; disabling or updating one should
not remove the other's route or pending review. Their processes need not run concurrently.

**Evidence and caller cost.** The installed set can retain many exact members and requirements, but
`EditorSourceTools` holds one `active`, one `probe`, one `invocation` and one `reviewed` candidate
([source_tools.rs:245–263](../../../src/project_host/runtime/editor/source_tools.rs#L245-L263)).
Promotion retires the current invocation and overwrites `active`
([selection.rs:925–943](../../../src/project_host/runtime/editor/source_tools/selection.rs#L925-L943)).
An invocation must match that one activation
([source_tools.rs:591–630](../../../src/project_host/runtime/editor/source_tools.rs#L591-L630)).
Consequently, successfully activating B replaces A; failed replacement retains the predecessor.
This is not merely a global process-concurrency limit.

The release/definition also selects one component, while the wire description supplies at most one
integer-property facet
([source_tools.rs:153–186](../../../src/project_host/runtime/editor/source_tools.rs#L153-L186),
[434–479](../../../src/project_host/runtime/editor/source_tools.rs#L434-L479)). New unrelated schemas
within that supported shape can use the existing path. Multiple simultaneously available tools,
or a richer contribution kind, require changes to the owning contract rather than only a new
package manifest. A requirement on A records an installation relationship; it does not activate A's
contribution or provide an assembly lookup path.

**Counterevidence.** These are explicit decisions in
[ADR 0118:478–513](../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md#L478-L513)
and the [installed-graph plan:18–40](../../plans/2026-10-28-installed-source-tool-graph-plan.md#L18-L40).
The graph is necessary for exact identity and dependency-safe withdrawal even with one selected tool;
its existence is not overengineering. Native source-built composition already supports multiple
panels, and ordinary editing remains available when a finite tool is disabled. The risk is freezing
this deliberately narrow route as the general installed-tool SDK, not a current correctness failure.

**Reference and recommendation.** Godot's Inspector retains multiple registered plugins and removes
the specified one
([pinned editor_inspector.cpp:4185–4211](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/editor/inspector/editor_inspector.cpp#L4185-L4211));
dock and Inspector registrations are separate capabilities
([pinned editor_plugin.cpp:515–522](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/editor/plugins/editor_plugin.cpp#L515-L522)).
That illustrates coexistence, not proof of transactional reload or Nara's stronger source custody.

Before broadening package roles or publishing a stable SDK, run the two-independent-tool workflow.
Replace the active singleton with bounded contribution-owned activation/request/review state if that
workflow is selected; process execution may remain serialized. Define facet conflicts explicitly.
Keep package identity, exact installation, Schema owner, contribution and invocation distinct, using
the existing lifecycle owners rather than introducing a resolver or universal plugin kernel.
The complete graph and safe reclamation work can be reused.

**Cost of postponement.** The singleton assumption is already visible in Tooling observations,
frontend drafts, desired selection and exact request routing. More public consumers will multiply
the migration surface. Failing to qualify coexistence before API freeze would encourage tool authors
to aggregate unrelated tools into one monolithic child or repeatedly switch activation themselves.

## E5. Treat Code Retirement As A Different Contract From Tool Disable

**Trigger.** Turn the existing source-built dialogue document/panel into an independently shipped,
reloadable native or managed extension. Replace it while a view, command, document history and
pending persistence work still exist. Decide whether Undo survives, whether restart is required,
and when old executable code may actually be reclaimed.

**Evidence and caller cost.** The current native document API deliberately accepts Rust-owned typed
commands/views and document-owned history
([extension.rs:10–36](../../../crates/nara_tooling/src/workspace/extension.rs#L10-L36)). Erased commands
retain `TypeId` and `Arc<dyn Any + Send + Sync>`; views retain another erased `Arc`; decoded candidates
own a trait object
([extension.rs:153–228](../../../crates/nara_tooling/src/workspace/extension.rs#L153-L228)). The independent
Dialogue owner retains its own Undo/Redo state
([dialogue.rs:29–35 and 67–115](../../../tests/fixtures/module-consumer/editor-panel-consumer/src/dialogue.rs#L29-L115)).
Native panel factories and instances likewise carry Rust callbacks and destructors.

These are suitable static Rust interfaces. If their implementation came from unloadable code,
rejecting a stale command would not make destruction of its erased payload safe after library
unload. Old frame views can also outlive the active document. Serializing the document alone cannot
retire every retained callback, vtable or drop function. A future binary adapter must either keep its
code owner alive through these lifetimes or replace the crossing values with an adapter-owned
contract that has an explicit retirement policy.

**Counterevidence.** Nara does not currently claim that these types are a binary ABI or that static
plugins can unload. ADR 0118 explicitly excludes that claim
([lines 178–188](../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md#L178-L188)).
The implemented finite child path avoids placing package-owned Rust objects in the Editor. Its
worker returns only after the direct child is reaped and acquired pipe workers are joined
([process.rs:65–68](../../../src/project_host/runtime/editor/source_tools/process.rs#L65-L68)), and
disable retains physical custody
([source_tools.rs:543–558](../../../src/project_host/runtime/editor/source_tools.rs#L543-L558)). Those
are real improvements since the October 27 research. They qualify finite source proposals, not
resident gameplay scripts or arbitrary native Editor plugins.

**Reference and recommendation.** Fyrox's dynamic entry returns `Box<dyn Plugin>` and keeps the
library alive after its plugin object
([pinned dylib.rs:41–72](https://github.com/FyroxEngine/Fyrox/blob/5495d012b32145674501af2e67d2fd06a06f04dd/fyrox-impl/src/plugin/dylib.rs#L41-L72)).
More significantly, its Editor explicitly clears scene command stacks and removes plugin property
editors before reload because they can contain plugin objects
([pinned editor/lib.rs:3217–3250](https://github.com/FyroxEngine/Fyrox/blob/5495d012b32145674501af2e67d2fd06a06f04dd/editor/src/lib.rs#L3217-L3250)).
This is a concrete user-facing reload cost, not evidence that “Rust DLL reload is solved.” Godot can
close the old extension before opening the replacement fails
([pinned gdextension_manager.cpp:180–206](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/core/extension/gdextension_manager.cpp#L180-L206)).
Its [official extension configuration](https://docs.godotengine.org/en/stable/engine_details/engine_api/gdextension/gdextension_file.html)
also makes reloadability binding-dependent. Bevy's `Plugin::cleanup` is a pre-schedule hook, not
unload
([pinned plugin.rs:74–79](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_app/src/plugin.rs#L74-L79)).

Keep direct source-built Rust and the finite child proposal path as different products. For a
resident optional gameplay adapter, place execution in the Player at explicit phases and give that
adapter its own state/reload policy; do not extend the short-lived Editor child into a universal
behavior host. For an independently shipped native binary with an actual native-authority need,
evaluate an explicit versioned C boundary and code custody. The
[Rust Reference](https://doc.rust-lang.org/reference/items/external-blocks.html#abi) provides no stable
Rust ABI guarantee. C function tables would still need allocation, destruction, callback and thread
rules. A managed adapter likewise needs lifetime evidence:
[.NET unloading is cooperative](https://learn.microsoft.com/en-us/dotnet/standard/assembly/unloadability),
and executing stacks or external strong references can prevent completion.

Use the existing Dialogue consumer with a retained old view/command and pending work as the next
lifetime probe. Explicitly select keep-old-code-until-release, reconstruct-and-retire, or restart;
state any history loss. Do not retrofit code-owner tokens throughout healthy static Rust APIs before
an actual reloadable consumer establishes the necessary crossing.

**Cost of postponement.** Deferring a universal ABI is beneficial. Deferring lifetime/support semantics
while advertising unload is expensive: plugin authors will expose arbitrary native objects and
history entries, making a later data boundary or restart requirement a pervasive API break.

## Direction And Decision Order

The source comparisons support a layered direction, not a prediction that every mature engine will
converge on one language or extension technology. Preserve Nara's direct Rust gameplay path, a
toolkit-independent document/operation domain, frontend-specific presentation, finite isolated source
tools, and optional adapter-owned resident gameplay. Their distinct lifetimes are useful boundaries.

The next investments should close ordinary author workflows in this order:

1. Qualify the neutral command plus its external Snap consumer, and port that same semantic operation
   through one custom-drawn panel/Inspector slice. This exercises E1/E2 without replacing document
   authority or designing a general UI protocol.
2. Join the existing game checkpoint and supervised Play owners into an explicit Rust rebuild/resume
   workflow. Compare compatible patching only against that complete experience.
3. Prove two installed contributions can coexist before freezing the installed SDK. Keep the existing
   graph, artifact custody and source transactions; change only the contribution assumptions that
   the workflow disproves.
4. Require the retained-document/code-lifetime probe before offering resident tool reload or binary
   unload. Optional scripting research may proceed independently; neither an official second language
   nor native hot-unload is a prerequisite for the first three workflows.

The public-API criterion is concrete: how many engine-private edits, frontend-specific adapters and
owner-lifecycle decisions does an independent author need to add a useful command, document or game
iteration path? Crate count, source length, and the number of safety checks do not answer that question.

## Research Trace

Local evidence: read-only PowerShell, Git and `rg`; no reference checkout changes. Online evidence:
Search Service `system2_search_query`, `open` and `find`, queried for official Bevy setup, Fyrox code
reload, Godot Editor/GDExtension APIs, the Rust ABI reference and .NET unloadability. Three discovery
queries were followed by targeted primary-page reads; there were no web retries or backoff. Domains
used: `bevy.org`, `fyrox-book.github.io`, `docs.godotengine.org`, `doc.rust-lang.org`,
`learn.microsoft.com`; immutable source links use the three inspected GitHub commits above.
Retrieval date: 2026-10-30, Asia/Shanghai. Only this report was written by this research lane.
