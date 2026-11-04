# ADR Implementation Status

This index separates architecture decisions from their recorded implementation coverage.
Decision and implementation states retain the scope of their linked evidence; updating this index
does not requalify product behavior or change an ADR's acceptance.

The November 4 [UI architecture refinement](../ui-foundation-design.md) amends ADRs0093/0107/0109/0118
with independent Editor/game products, persistent UI state and local invalidation, public panel
hosting for user-authored frontends, complex-editor/custom-drawing pressure and UI operation for
tests/future agents. Those additions are **not implemented or qualified** by the selected-panel
results below. Historical implemented cells retain their linked bounded contracts; the expanded
UI targets remain partial until [R11a-f](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r11a-execution-model-dependency-closure-and-work-baseline)
pass their independent/native and measured acceptance. This documentation update supplies no
new performance, toolkit compatibility or platform evidence and requires no official ImGui package.

The component/reload follow-up explicitly leaves custom measurement/drawing, routed canvas
interaction, scoped keyed component state/effects and independent component-library ergonomics
unqualified. Current public panels remain Dear-specific; finite Rect/Text paint and button events
are not the complete successor interface. R11f adds source-built Editor edit/build/reconstruct,
bounded versioned owner/UI recovery, failure refusal/recovery and correct-result latency. It is
unimplemented and separate from Play/checkpoint restoration; declarative composition, preserved
frontend objects during GPU recovery and source-built panel registration do not qualify code reload.

The later November 4 development-route correction in ADRs0107/0118 selects separate reference
and self-drawn Editor compositions. The ImGui implementation validates shared module/UI-model
interfaces; the successor runs independently and replaces it after complete acceptance. Optional
product embedding remains allowed, but mixed first-party panels and reciprocal embedding are
not migration requirements. Existing hosted Nara panel results below remain bounded evidence.
Their removal of original Dear painters means an intact pure-ImGui reference must be explicitly
identified rather than assumed. No code was reverted/rebuilt and no new composition was qualified
by this documentation correction.

The [complete foundation reassessment](../foundation-architecture-reassessment.md) records all
103 baseline ADR dispositions and a staged refactor plan. Strategy/research-scope corrections and
ADR 0119 metadata authority are documentation decisions, not new runtime qualification. Proposed
0120 has private representation, Schema-driven C#/Lua workflows and bounded same-World body/lifetime
evidence. Its narrow closed Scene data-family subset is adopted under ADR 0006; broader executable
publication/replacement remains Proposed. 0121 remains Proposed. Its finite typed-program
prerequisite and stock execution replacements have separate implementation evidence under
ADR0094. Original Source/Processed capture replay, native streams and clipped numeric composition
now have scoped evidence. Shared stock-stage Scene effects have their own complete Runtime,
native Scene View and offscreen qualification below; the default Nara-drawn ordinary Inspector
replacement is recorded below. Exact native binding and stock-render ownership are retained.

The [owned Inspector unit](../../plans/2026-11-04-owned-inspector-and-editable-text-plan.md)
adds a qualified shared text prerequisite: finite ordered immutable font faces, exact fallback
raster identity, selected collection faces, bounded Parley/ICU editing, local Undo/Redo and
field/focus lifetime tokens. Empty committed semantic values and signed clipped text offsets
work through the compiler and AccessKit recording bridge. The independent consumer, actual GPU
glyph readback and neutral galleries retain their distinct scopes. This does not complete the
Nara-drawn Inspector, native IME delivery/candidate placement or broader R11/R12.

Its [native scalar integration](../../knowledge/engineering/verification/2026-11/2026-11-03T192310Z-native-owned-scalar-inspector-and-presented-input-qualification-fc9f002f34ed414e84470bf32a812b8e.md)
now consumes the shared text/widgets through the existing graphics Host and exact window/image
composite authority. The optional `Nara properties` tab retains original field drafts, queued and
admitted result identity, source conflicts and ordinary document history/persistence. Selected
Windows/DX12 backend injection covers Chinese/string and numeric editing, focus retirement,
cross-frame Apply and disk Reopen; shared pointer routing excludes paint-only caret/selection changes
while retaining transform, clipping, propagation and behavior invalidation. Complete panel parity,
the Nara-owned Scene/tool visual and broader native/idle/platform gates remain open. The predecessor
remains until those complete workflows pass.

The [keyboard and font-policy successor](../../knowledge/engineering/verification/2026-11/2026-11-03T202859Z-native-owned-keyboard-and-font-policy-qualification-8df745e066a042c79a397610b8615985.md)
adds enabled-control navigation, logical-key text shortcuts and a shared fallible clipboard owner.
Failed Cut/Paste, composition, queued submission and held-modifier handoff preserve their original
state. Three font-policy controls retain dirty, queued and pending drafts and exact Application
results across control replacement. Nine selected Windows/DX12 trajectories pass; the keyboard
test enters through the production logical-key method and uses an isolated memory clipboard.
Physical OS key/clipboard delivery and keyboard-only entry from the shell remain unqualified.
The same [Inspector unit](../../plans/2026-11-04-owned-inspector-and-editable-text-plan.md)
now includes explicit boolean value buttons with original-basis submission, exact result routing
and retained refusal recovery. Its 19-case owner/native cohort covers repeated activation, source
history and persisted/reopened values. The same unit now adds typed root entity/asset references,
bounded source-local target pages and native wheel scrolling. Clipped text inputs lose text/IME
authority while visible field buttons retain exact draft/result ownership. Its final 32-case
owner/native cohort and 308-test ordinary Editor library pass; the unit records native scope,
review corrections and remaining limits. Complete panel parity and broader R11/R12 qualification
remain open; the supported structured-value successor follows below.

The [structured-field successor](../../plans/2026-11-04-owned-inspector-and-editable-text-plan.md#structured-field-integration)
now projects the existing `RootDraft`/`ValueDraft` into Optional, Record and scalar-item List/Map
controls. Typed addresses retain original child identity, independently incarnated text leaves
and whole-field submission/result ownership. Pending children and invalid local text remain
unpublished; deleting a preceding row preserves the surviving leaf's local history. This advances
the supported value controls with 316 ordinary library tests and the final 16-case owner/native
cohort, including complete Scene/Prefab history and disk Reopen. It does not complete
component/tree parity, the Nara-owned Scene/tool
visual, keyboard-only shell entry or broader native/idle/platform qualification.

The [shared reference picker successor](../../plans/2026-11-04-owned-inspector-and-editable-text-plan.md#shared-reference-picker-integration)
uses one private owner for root and nested Record/Optional/List/Map entity references. Typed leaf
addresses, bounded lazy target pages and captured menu revisions preserve exact draft/result
ownership. Closing/reopening refuses earlier actions while stable keys keep truthful keyboard
focus, including navigation before another paint. Choosing one leaf retires its old text/picker
and preserves unrelated local editing. Its final 321-test ordinary library and 44-case owner/native
cohort include exact Application controls and complete Scene/Prefab history/persistence. The
complete selected Inspector and broader R11/R12
requirements remain open.

The [retained-input recovery successor](../../plans/2026-11-04-owned-inspector-and-editable-text-plan.md#retained-input-and-capacity-recovery)
adds whole-candidate Copy through the existing fallible Host clipboard, including incompatible,
unprojected, text-limit-refused and in-flight drafts. Clipboard feedback preserves the original
validation/submission error and local history. Exact captured-set recovery releases only dirty,
unsubmitted drafts and retains queued/pending operation identities. Copy survives composition
cancellation and unrelated buttons becoming enabled; captured-target disablement, geometry,
mapping, modal and explicit behavior invalidation retain their original protections. The owning
unit records 326 ordinary Editor, 240 shared UI and seven independent-consumer controls, plus
18 distinct selected native trajectories, including a corrected ordinary-case rerun. It retains
the remaining complete-panel, Scene/tool,
virtualization, idle and actual OS/platform requirements.

The [source-tool property successor](../../plans/2026-11-04-owned-inspector-and-editable-text-plan.md#source-tool-property-integration)
now consumes the existing finite integer facet in the Nara-drawn property row. Parameter input
uses its own local text lifetime, while ordinary source drafts retain their original recovery
basis. Original activation/property/request actions share the existing ordered Authoring ledger
and one Session candidate with the ordinary tool panel. The owning unit records actual C# native
review, history, replacement, fallback and Save/Reopen evidence. This is a concrete property
consumer of ADRs0047/0118, without a new public control taxonomy or a second prepared edit owner.
Complete component/tree parity remains unfinished; the Scene successor follows below.

The [owned Scene successor](../../plans/2026-11-04-owned-inspector-and-editable-text-plan.md#owned-scene-integration)
replaces ordinary Scene controls with Nara's compiled toolbar and viewport. Its native adapter
directly samples the original immutable Scene output beside the chrome image in the same Host
and actual window drawing order. Distinct placement occurrences and confirmed native clip/input
mapping preserve source geometry and revoke old or queued rebind-period picks. The selected
Windows/DX12 cohort qualifies real pan/zoom/reset, source picking/edit/Undo, unchanged-output
reuse, retained predecessor pixels after native resize and retirement, source-tool properties,
Scene/Canvas priority, image reimport, independent Scene effects and Game/view lifecycle. This
does not complete selected Inspector parity, virtualization, measured idle/retirement costs or
actual OS IME/accessibility and broader platform qualification.

The [component-control successor](../../plans/2026-11-04-owned-inspector-and-editable-text-plan.md#component-controls-successor)
adds Nara-owned creation choices, removal, folding and unavailable diagnostics through the
original Scene/Prefab commands and ordered Authoring result ledger. Displayed document/Schema,
entity, revision, capabilities and local menu observation fence activation. Folded fields retain
compatible drafts/local Undo while losing text focus and composition; removed-component drafts
remain in the existing recovery path. Its owner and native Scene/Prefab qualification is recorded
in the same unit. Full Inspector/navigation and tree parity, virtualization, costs and actual
OS/platform requirements remain open.

The [default ordinary Inspector replacement](../../plans/2026-11-04-owned-inspector-and-editable-text-plan.md#default-ordinary-inspector-replacement)
adds the selected entity's authored placement and exact Prefab-source navigation, uses Nara
controls in the ordinary Document tab and removes the competing optional tab and Dear whole-panel
painters. Its matched consumer qualification remains recorded in that unit. Source-built
`edit_field` contributions and specialized UI/source-asset/Prefab Intent panels retain their
separate consumers; the shell still owns document/tree/persistence controls and native windows.
Complete caller-owned loops reuse the original public `EditorNativeHost`; the former unattached
`show` entry is replaced by explicit contribution-only `show_contributions`, preserving exact
submission/result routing without pretending to render hosted image panels. This does not
complete tree/workspace replacement, virtualization, costs or broader R11/R12.

The [source-declaration Hierarchy successor](../../plans/2026-11-04-owned-inspector-and-editable-text-plan.md#source-declaration-hierarchy-successor)
replaces ordinary Scene/Prefab source-list, composition and removal-confirmation painters with
Nara controls. It preserves authored routed placement and Prefab identity, exact Session selection,
one document history and the original ordered panel result ledger. Captured source/Schema/selection,
permissions and local observations fence activation and final queue admission. Committed filtering,
preedit, scrolling and Show selection remain panel-local; only visible source/choice rows instantiate
controls and layout. Recycled rows cannot retain input authority. Long legal source IDs use bounded
private display text while typed commands retain full IDs. Matched owner/public/native qualification
and visible-count observations belong to the owning unit. This closes neither specialized UI trees
and workspace replacement nor offscreen keyboard traversal, measured full scanning/idle/allocation/
GPU costs, actual OS IME/accessibility or broader R11/R12/foundation qualification.

The [specialized UI Hierarchy successor](../../plans/2026-11-04-owned-inspector-and-editable-text-plan.md#specialized-ui-hierarchy-successor)
replaces the UI Prefab tree and complete creation/move/delete forms with Nara controls through
the original UI projection, structural lowering and Session history. Typed source/placement and
image-generation facts are revalidated before activation and queue admission; exact results
retain explicit retries and cannot attach to replacement forms. Its shared text prerequisite
publishes opaque complete-source editing snapshots and prepares placed raster-accurate glyph
windows under the existing input/output ceilings, retaining 4,096-byte creation text. Hidden
draws still enforce receiving raster policy. The owning unit records scoped tests and corrected
counterexamples, including actual pending-frame Source/UI replacement and chooser keyboard
ownership. Specialized Inspector/Canvas controls, workspace replacement, measured costs and
actual OS/platform qualification remain open; full R11/R12 remains incomplete.

The [joint motion unit](../../plans/2026-11-03-joint-motion-execution-plan.md) now compares public
sets, a private finite phase plan and Bevy App with the same independent animation/real-solver
consumer. Production automatic schedules explicitly use Bevy's serial executor instead of its
feature-selected default; explicitly driven custom schedules retain their owner policy. The
independent workspace verifies real contacts, borrowed same-tick join, deferred poses, NonSend,
managed fixed/presented samples, Pause/exact Step, and narrow Ack followed by a sticky late fault.
This establishes neither production automatic parallel support nor a frozen R4c phase interface.
The joint discontinuity successor below retains its own native evidence. A closed same-World
prediction/offline-PCM successor qualifies complete persistent solver restoration, logical
entities/RNG/commands/outcomes, same-tick and foreign-session result refusal, confirmed-ancestor
preservation, deferred semantic cue publication and managed audio close. Its
[qualification](../../knowledge/engineering/verification/2026-11/2026-11-03T162556Z-bounded-same-world-prediction-and-confirmed-pcm-effects-preserve-exact-attempt-and-ancestor-ownership-71be3014fb29466a8995909c5e20b8c6.md)
retains final 22/23-test cohorts and the two corrected counterexamples. Its logical custom schedule
does not rewind ordinary FixedTime. The
[authored collision successor](../../plans/2026-11-04-authored-collision-derivation-plan.md)
uses public exact Schema/Scene materialization and a concrete Rapier output owner. Actual native
contacts, collider/debug geometry, edits/history/disk Reopen, duplicate/Prefab attribution,
complete membership/retirement and managed close have
[bounded evidence](../../knowledge/engineering/verification/2026-11/2026-11-03T170523Z-authored-collision-definitions-retain-exact-source-attribution-through-real-rapier-derivation-and-retirement-b550d8d09ee34b72adfa1d18262eec6a.md).
This remains an independent integration; broader R10 and an official production domain remain open.

The same unit adds semantic discontinuation to the existing CPU history lineage. Original clones
cannot be refreshed through late admission, first preparation, target adaptation or retained
candidate submission. Ordinary per-slot reset stays distinct. The independent two-View consumer
joins actual fixed/presented Rapier motion and captured virtual shader time with selective reset,
old-output reads and original submission custody. Its scope is offscreen Windows/DX12; general
cross-thread publication atomicity, camera/material compatibility and rollback remain unqualified.

The [registry ownership unit](../../plans/2026-11-01-component-registry-ownership-plan.md) is
implemented as `7f69d53` under ADR0081. Consuming `ComponentRegistryBuilder` admission publishes
the sole `ComponentRegistrySnapshot`; failed admission returns its original candidate and error.
Private App phases preserve installation identity, provider atomicity and exact managed/standalone
World eligibility, including installed-but-unadmitted refusal. Duplicate native reader wrappers
and phase-only query branches are removed across Scene, Root, Tooling and Editor. Its
[qualification](../../knowledge/engineering/verification/2026-11/2026-11-01T033745Z-consuming-registry-admission-and-exact-app-scene-ownership-qualification-c1678b22b0424f55b3dd73f149d8e7e2.md)
records Reflect395, affected-domain results with one corrected assertion's focused rerun,
Root/Editor/public1668, independent30+7, Reference40, compile-fail docs19 and four actual DX12
trajectories, plus strict consumer lint and portable compilation. This completes the ownership
unit, not coordinated authored evolution, complete R12 or Proposed0120/0121.

The [authored-upgrade unit](../../plans/2026-11-01-authored-schema-upgrade-plan.md) is implemented
and qualified for CP1/S2 under ADRs0035/0043/0047/0090/0110/0114. One live Session/Application
operation retains the complete all-profile inventory and frozen working inputs through proposal,
review, publication and complete Workspace/persistence-lane adoption. Exact guarded Ready
publication precedes infallible installation. Fresh-process recovery and the fixed-v2 consumer
cover interrupted publication, ordinary follow-up editing/persistence and Check/Play, with strict
refusal of incompatible v1 execution bindings. Creation-only/additive adoption keep their narrow
existing contracts.

Compiled Editor source operations use each document's retained Native/Data authoring basis
independently of execution authority. Preview materializes only installed Native coverage and
reports exact omissions. The unreleased UI catalog is baseline1; construction lowers through
actual source templates and declared properties with bounded admission. The
[complete qualification](../../knowledge/engineering/verification/2026-11/2026-11-01T203621Z-reviewed-authored-schema-upgrades-and-fresh-process-recovery-qualification-cf7c8ecab10c4e21b4cfcff4906805de.md)
records exact test cohorts, corrected failures, independent review and Windows/process-interruption
limits. Its predecessor shards preserve the earlier implementation checkpoints. CP2 and the
broader foundation units remain open; this does not promote complete R5 or Proposed0120/0121.

The [CP2 extraction unit](../../plans/2026-11-02-prefab-extraction-plan.md) selects explicit root
binding/forest mounting, source-owned typed placements, inline reference values and separate
flat compiled output. A mixed local/inherited sibling-order counterexample requires replacing
the authored hierarchy contract, not just adding a root flag. The
[pure identity ownership prerequisite](../../knowledge/engineering/verification/2026-11/2026-11-01T215838Z-canonical-source-identity-ownership-prerequisite-25574e28909847799bbcd2c2b1f1d469.md)
is qualified under ADRs0007/0038/0058: Identity owns canonical pure values, Asset compares metadata
and Scene retains interpretation, with772 owner tests,220 Root controls,15 independent consumers,
three public association controls, docs and minimal/portable compilation. The
[final-reference-demand correction](../../knowledge/engineering/verification/2026-11/2026-11-01T231259Z-final-reference-demand-and-preallocated-property-paths-qualification-fdb7dce644a84dcbb5b8b81fee9ec514.md)
is qualified at `74d5fc7`: scoped normalized values survive property composition before lookup,
only surviving references open sources, actual IDs lower last, and retained storage paths are
admitted before copying. Inline reference values, lexical input environments, contextual defaults,
source-only copy admission and complete Native value gates now have their
[production prerequisite qualification](../../knowledge/engineering/verification/2026-11/2026-11-02T031431Z-inline-authored-references-and-contextual-default-production-qualification-6f60b71dc5c6458bbd6c9c63f1df239e.md).
Closed compiled storage and the complete `compile` Interface are now connected through production
Native/Runtime/Host consumers. Their [owner/public cohort](../../knowledge/engineering/verification/2026-11/2026-11-02T053411Z-compiled-scene-owners-and-reviewed-transition-corrections-pass-current-public-cohort-e8f7d20e3e2b49d5b086ce2100f1b9b2.md)
passes1629 tests across Reflect, Scene, Tooling and UI, including contextual conversion and live
export reopen. The [complete compiled prerequisite](../../knowledge/engineering/verification/2026-11/2026-11-02T071856Z-closed-compiled-scene-production-and-independent-delivery-qualification-91cc04f45ba249f881cec4f9f78bd3d1.md)
adds Root/public repairs, independent module281 and ReferenceGame179, real language/tool consumers,
actual cooked Host/Player delivery, selected Windows/DX12 trajectories, minimal configurations,
rustdoc negatives and correctness Clippy. Root/placement production replacement now has its
[scoped qualification](../../knowledge/engineering/verification/2026-11/2026-11-02T121917Z-explicit-root-binding-and-source-placement-production-qualification-0eada24ff287416bbfe94ccf511d2380.md):
source declarations, explicit forest/root binding, the shared layout kernel, private flat
observations, exact placement history and root-bound conversion pass owner/public, independent
module, real Transform/Native, language/tool, cooked Player and selected Windows/DX12 consumers.
Semantic subtree deletion repairs affected surviving sibling pins through the Tooling owner.
Complete extraction correspondence and project publication/recovery are still unfinished.
The [next extraction working tree](../../knowledge/engineering/progress/2026-11/2026-11-02T134126Z-extraction-exposes-routed-root-contract-and-source-scoped-relocation-ownership-52ff66cd13fc4a3abd00f1d9d62c7f87.md) refines root pins from a direct local ID to a complete
`PrefabEntityTarget`: a public extraction pressure case reproduced an outer instance promoting
an entity that moves behind a new Prefab hop. The earlier direct-ID qualification retains its
original scope. Typed routed/addition root selection, finite demand and source-copy remapping
pass the689-test Scene/public cohort and focused real/independent consumers. Private retained/moved
lexical reference proposals pass two actual compiler consumers and bounded owner controls,
including a corrected pre-scan admission defect. The
[actual source-construction successor](../../knowledge/engineering/progress/2026-11/2026-11-02T150300Z-actual-source-construction-preserves-hidden-defaults-and-coupled-placements-be40a32c8613443d990618b7acad12c9.md)
now builds P/Q through complete physical declaration and shared Reflect payload/default traversal.
Its consumers cover JSON/RON source readmission, hidden defaults/Absent and coupled placement
counterexamples. Its
[immutable-corpus successor](../../knowledge/engineering/progress/2026-11/2026-11-02T161112Z-immutable-extraction-corpus-and-actual-occurrence-correspondence-pass-owner-consumers-492ab513e3db4b4a83e7202aeff50dae.md)
retains exact source allocations and asset associations, constructs actual dependants and verifies
injective/surjective occurrence correspondence through original compiler results. Final Scene687
and the corrected Reflect rewrite-admission cohort7 pass; minimal/product library checks and
correctness Clippy pass. The
[physical-context successor](../../knowledge/engineering/progress/2026-11/2026-11-02T174336Z-complete-physical-contributor-mapping-and-bounded-extraction-traversal-d1926e8c625c40b3b66654d70756b2ff.md)
adds complete stored-suffix/placement correspondence, conservative physical and modified-context
dependencies, complete unavailable-contributor preservation and explicit bounded ancestor traversal.
An actual outer-claim counterexample and native depth64 stack overflow drove those owning corrections.
The [final admission correction](../../knowledge/engineering/progress/2026-11/2026-11-02T175153Z-selected-capability-admission-and-legal-cross-layer-unavailable-controls-a24faed38ecd429abad2ebbd8b3e774c.md)
preserves current Scene capability independently of Adapter absence and qualifies legal cross-layer
Properties controls. Final Scene library and three owning integrations745 pass, with correctness
Clippy and minimal/product library checks; scoped independent review has no remaining assigned
P1/P2. That checkpoint did not qualify real Workspace/all-profile Host capture or Session
publication/recovery; private preparation does not qualify complete CP2 or native extraction.
The [real Workspace capture successor](../../knowledge/engineering/progress/2026-11/2026-11-02T182845Z-real-workspace-extraction-captures-and-verified-corpus-binding-share-the-original-work-96c0887ee97045f4960c20d59d5afbf3.md)
now shares private source snapshots/checkpoint mechanics with CP1 while retaining nominal proofs.
It captures actual open source/history observations, prepares the real corpus on a worker and
binds complete member/live validation to the same cumulative Work. Final Scene/Tooling libraries875,
independent Scene8 and real Root104 controls pass, with correctness Clippy and affected library
checks. This completes capture/member validation only: selective source/saved checkpoint
installation, all-profile IO capture, Q/sidecar publication, recovery and native extraction remain
unfinished. Guard-only inputs do not acquire source/history replacement authority.
The [selective source-checkpoint successor](../../knowledge/engineering/progress/2026-11/2026-11-02T190731Z-selective-extraction-source-checkpoints-bind-all-workspace-owners-before-installation-9696f7bec4a64642872d06fa5a3cfe29.md)
adds construction-owned complete source-change facts, original-basis classification and nominal
replacement checkpoints. Complete Workspace source guards bind actual changed Scene/Prefab
owners and unchanged guards before any swap, restoring the same preparations on release. Final
Scene/Tooling libraries and independent capture/checkpoint consumers905 pass; the selected source
scope has no remaining independent-review findings. Saved-current/successor bindings, actual
Project inventory, Q/sidecar publication and extraction recovery/Ready remain unfinished. The
source-only commit intentionally leaves saved evidence, external-reload state and Q opening to
their owners; ADR0047's complete-operation contract is not qualified by this narrower result.
The [selective saved/output successor](../../knowledge/engineering/verification/2026-11/2026-11-02T204731Z-selective-extraction-saves-and-actual-encoded-publication-pass-owning-and-public-save-consumers-45f6e075aa074e539c8dcde3c3700b3c.md)
adds actual immutable captures, owner-issued canonical output and exact receipt association,
ReplaceSource/SaveCurrent/Guard saved destinations, and actual single-leaf publication without
early Host adoption. Shared classification/contextual/default validation and compiler, copy and
Schema-upgrade interpretation consume original allowances, including retries. Final Scene/Tooling
libraries and four independent/integration binaries948 pass; Root100 and public Save18 pass, with2
existing measurements ignored. Correctness Clippy and affected minimal/Root/Editor libraries pass
with general warnings. Complete all-profile capture also requires preserving distinct resolution
environments over unique physical sources. That correction, Q/sidecar publication, Project
Ready/recovery and native extraction remain unfinished.
The [environment-owned capture successor](../../knowledge/engineering/progress/2026-11/2026-11-02T215137Z-environment-owned-physical-extraction-capture-and-scoped-catalogue-admission-f97014dea60849a39edeaac3cb6ed047.md)
implements one physical corpus with actual per-configuration membership, frozen catalogue
visibility and before/after associations. Real Project capture retains disk and actual working
inputs separately, checks observed-absent Q/sidecar and global identity admission, and binds the
original Workspace on its original allowance. Ready revalidation reobserves actual namespace,
objects, ancestors and digests without constructing another encoded corpus. The final scoped
Scene/Tooling917, independent Scene22 and Root197 tests pass, with Root/Editor all-feature and
minimal library checks and correctness Clippy. Independent findings on malformed covered
metadata, temporary catalogue custody and external sidecar spelling are corrected. The separate
[saved-generation comparison](../../research/2026-11-03-profile-resolution-and-saved-generations.md)
requires validating exact reviewed outputs together with original disk guards. That proof,
and its actual Project Review successor now have
[owning and consumer evidence](../../knowledge/engineering/progress/2026-11/2026-11-02T233154Z-saved-generation-proof-and-complete-review-refusal-ownership-pass-actual-consumers-8ca79f0732a34e799c8e58a4e4ba061f.md).
The proof includes complete dormant Guard declarations and masked contextual defaults, while
corrected Review selections retain the original corpus and cumulative Work. That predecessor's
refused-revision policy is superseded by the retained-selection successor below: an unpublished
proposal can be replaced, while confirmed publishers retain all accepted file evidence. Reflect196 and Root207
pass; the domain965-test run has964 passes and one corrected test assertion, with the final12
affected controls passing separately. Complete Session/Application Review/Confirm, Q/sidecar
publication, Ready/recovery and native extraction remain unfinished. The neutral physical
inventory now has its
[separate post-split evidence](../../knowledge/engineering/verification/2026-11/2026-11-02T234852Z-neutral-source-inventory-preserves-strict-authority-and-both-real-consumers-fdd43928e9ab444bac78ed2c16d92bf9.md):
the scanner and native custody are independent from Schema-upgrade recovery generations,
ordinary capture stays strict Ready, and concrete upgrade admission precedes explicit physical
claims. Root209, complete/minimal/CLI feature checks and correctness Clippy pass, with no remaining
scoped independent-review P1/P2. The capture and Review owners have no complete publication
authority; new Q/sidecar and final generation installation remain separate obligations.
The [fixed-output and borrowed stored-generation successor](../../knowledge/engineering/verification/2026-11/2026-11-03T004438Z-fixed-extraction-outputs-and-borrowed-stored-generation-admission-qualification-03bf5d78ca3f406abeb2e28d78c557cd.md)
retains actual canonical checkpoint/closed/Q/metadata outputs across late refusal and cancellation,
and validates stored P/Q/dependant pairs without reconstructing full candidates or reassigning
input names. The actual unknown-contributor reorder control corrects an obsolete array-index
identity requirement while preserving same-source and complete declaration checks. Scene/Tooling979
pass before that final correction; final affected46, stored5 and Root182 pass after it, with
independent review, feature checks and correctness Clippy. Concrete recovery package/transport,
actual process recovery, full Session/Application publication and native extraction remain open.
The [recovery-data and exact-file-observation successor](../../knowledge/engineering/verification/2026-11/2026-11-03T012418Z-extraction-recovery-data-and-exact-file-observation-prerequisite-qualification-dba154acfe934f14bb07b17e610fc16d.md)
adds bounded immutable package data from actual original working encodings and fixed outputs,
shared source decoding with pre-admitted ordering/structure checks, coherent Missing-root facts,
and separate positive/negative observations for partially created Q/sidecar destinations. Ordinary
Ready and CP1 source admission stay strict. Actual Scene785 and Root207, feature checks and
correctness Clippy pass; independent review findings and the executed sidecar-role correction are
closed within that scope. Concrete Pending transport, mixed-generation semantic recovery, final
Project/live installation and complete process/native extraction remain unfinished.
The [code-free transport and actual Pending successor](../../knowledge/engineering/verification/2026-11/2026-11-03T022315Z-code-free-recovery-transport-and-actual-pending-extraction-inventory-qualification-d9300e228ce4473ebe5cea64378e2756.md)
shares typed immutable package transport with CP1 while keeping concrete decoding and Schema
policy separate. Actual extraction Storage retains cancellation custody/orphan receipts; actual
Pending inventory admits one unchanged-bundle basis and shares observation-only revalidation
without a second native corpus. Final Root295, Root/Editor all-feature, minimal/CLI checks and
correctness Clippy pass, including affected CP1 controls. Full recovered stored-pair semantics,
Q/sidecar/dependant publication, future Project/Ready and complete Session/public/process/native
CP2 qualification remain open; the full foundation Goal remains active.
The [stored-generation successor](../../knowledge/engineering/verification/2026-11/2026-11-03T024956Z-stored-extraction-generations-retain-actual-pending-custody-and-pass-semantic-recovery-controls-b1cbbf2054494149a64c7fd2f2285177.md)
now joins actual Pending inventory and one unchanged-bundle basis to existing stored-pair and
independent saved-generation admission. It preserves fixed Q IDs/input names and refused decoded
owners under the original allowances. Final Root303, Root/Editor all-feature/minimal checks and
correctness Clippy pass; independent correctness/test reviews close their assigned scope. This
does not publish Q/sidecar/dependants, establish Ready or qualify complete Session/public/process/
native CP2. Those production stages and the full foundation Goal remain open.
The [concrete file-publication successor](../../knowledge/engineering/verification/2026-11/2026-11-03T031004Z-concrete-extraction-file-publication-preserves-live-checkpoints-and-actual-recovered-outcomes-6f0201d385c54f368e7e5b7473a05e41.md)
publishes fixed Q/sidecar and sparse original outputs from distinct live/recovered origins. Actual
accepted results remain beside the original encoded checkpoints; already-output reads issue no
replacement receipt. Original source/ancestor custody, cleanup and cumulative allowances survive
conflict/refusal. Final Root308 passes, including exact-spelling and native directory-prevention
controls. The [checkpoint-acceptance successor](../../knowledge/engineering/verification/2026-11/2026-11-03T033143Z-actual-extraction-file-results-enter-their-original-checkpoints-without-another-write-feeb247c76364545a38b5f9a2b2a5c6a.md)
consumes actual file results into their original nominal checkpoints without another write,
retaining conversion/acceptance refusal custody and actual Observed versus Published evidence.
It removes the displaced combined write/readmission helpers. Final Root315, Tooling17,
Root/Editor feature checks and correctness Clippy pass; independent rereview closes its sentinel
admission correction with no remaining assigned P1/P2. FilesComplete and CheckpointsComplete
remain Pending; future Project/all-bound Ready, Session/public/new-process/native extraction
and the full foundation Goal remain open.
The [ordinary future native-inventory successor](../../knowledge/engineering/verification/2026-11/2026-11-03T041901Z-ordinary-future-extraction-inventory-retains-exact-native-participation-and-selected-policy-f13e25cb405b450eb945f1eabe557438.md)
admits Q and sidecar without raw-destination exemptions, retaining complete native namespace,
original metadata/Guard incarnations and actual output results. It separates physical claims from
explicit native participation, preserves the original selected policy in both concrete recovery
formats, and requires a present nullable selection field. Final related Root340 passes and
independent rereview closes the assigned P2 findings. The original operation allowances and all
live/recovered owners continue; no future Project/Ready, saved installation or complete CP2
public/new-process/native qualification is implied.
The [private future-Project/final-file successor](../../knowledge/engineering/verification/2026-11/2026-11-03T050808Z-private-extraction-future-project-and-actual-final-ready-publication-preserve-unchanged-schema-ownership-a987c2db3b534dc5a30995e64ff6420d.md)
prepares actual saved startup under the unchanged live/recovered basis and fills a private future
receipt only from its own accepted Ready publication. Ordinary Ready reopen and exact/one-short
controls pass. Final production regression has 346 passing tests; the sole incumbent cleanup-cost
expectation is corrected and passes in an eight-test closure. Shared cleanup admission closes the
assigned private-owner P2. The [installation ownership controls](../../knowledge/engineering/progress/2026-11/2026-11-03T054042Z-ue6-review-tightens-prediction-boundaries-while-extraction-installation-preserves-exact-owners-be5b0a82c37f4807adfd3627302515fa.md)
now move the exact Workspace stage and accepted results into reversible Workspace/leaf guards;
the remaining publisher retains independent Pending/basis/budget access. Final combined controls
pass8/8, including observed results without invented publication evidence. Complete all-guard live
installation, Session/public/fresh-process/native extraction and full CP2 remain open.
The [private live-installation successor](../../knowledge/engineering/verification/2026-11/2026-11-03T060356Z-original-extraction-destinations-bind-before-ready-and-finish-without-a-resumable-publication-cursor-3aee2a2f035e45008b135733cea1d373.md)
now binds the actual complete original Hosts and Workspace across final Ready. Five controls pass
for selective installation, late Host refusal, held final cleanup/retry, whole-operation work
boundaries and distinct root-relative/project paths. Successful custody no longer exposes a
publication cursor. The final related extraction/upgrade/persistence cohort passes158/158;
Root/Editor all-feature, minimal, correctness Clippy and fmt checks pass. Session lane/execution installation and complete public/process/native CP2
remain open; this private scope does not qualify them.
The [Session Review continuation](../../knowledge/engineering/progress/2026-11/2026-11-03T065327Z-session-extraction-captures-real-reviews-with-shared-authoring-maintenance-and-concrete-play-operation-identity-0752d732f6ec4fa4b8af57788cd92062.md)
adds real unpublished Session/Application capture and borrowed Review, accepted-Save draining,
physical-return cancellation, neutral source-operation limits, concrete Play operation identity
and one frontend authoring-maintenance observation. Typed Host capacity causes survive recovery
encoding rather than becoming generic source refusal. Confirmation/durable publication and final
Session installation remain open; no complete public/process/native CP2 claim follows.
Its [live publication successor](../../knowledge/engineering/verification/2026-11/2026-11-03T074415Z-live-session-extraction-confirmation-and-original-publication-custody-pass-real-root-and-application-consumers-c3a619b0f0d34cd9bbee59adf90aa70f.md)
qualifies actual confirmation, exact physical retirement and original Workspace/Host/lane/execution
installation across Ready through real Root and Application consumers. Root198 pass plus the sole
explicit-capacity stress rerun and Editor13/13 pass; SourceLimit and original diagnostic borrowing
remain separate from stale input and generic publication refusal. Complete retained save-selection
revision/retry, public new-process recovery, Check/Play and native extraction remain open.
The [retained-selection successor](../../knowledge/engineering/verification/2026-11/2026-11-03T081849Z-retained-extraction-save-selection-corrects-and-revises-the-original-complete-preparation-74cb3a0454934a70aa008757b2f09c2d.md)
qualifies initial failed-selection correction and unpublished Review revision over the original
corpus, Task/storage custody and cumulative Work. Related Root207/207 and Editor15/15 pass.
ADR0047 distinguishes superseding an unpublished proposal from preserving accepted publisher
evidence; selection closes at confirmation. Public process recovery, strict Check/Play and native
extraction still prevent complete CP2 qualification.
The [public process-recovery successor](../../knowledge/engineering/verification/2026-11/2026-11-03T094705Z-recovered-extraction-process-and-integrated-source-owners-pass-current-qualification-e6d36a7a778d4f3494963eec011be399.md)
now exposes an authenticated read-only Pending dispatcher and the concrete extraction owner to
CLI-only consumers without Tooling or fabricated live state. Actual process boundaries cover
all-old, Q/sidecar/partial original progress, all-output Pending, before Ready and Ready before
outcome acknowledgement. Already-new file identities and original Guard/profile meaning survive;
the actual CLI-only64 refusal precedes a fresh128 recovery. Recovery17/17, process3/3 and the
complete Identity/Reflect/Scene/Tooling library cohort1242/1242 pass. Ordinary compiler capacity
retains SourceLimit classification and original diagnostics. Complete independent extraction,
strict Check/Play and native extraction qualification remain open.
The domain and product integration are now committed locally as `3849d6f` and `3ce9ec7`.
The same evidence records the requested clean/default build, post-clean Root and Editor cohorts,
Scene public integrations, final shared-decoder replay and three actual Windows/DX12 regression
trajectories. Those native regressions do not qualify a new extraction interaction.
The [complete public/native consumer successor](../../knowledge/engineering/verification/2026-11/2026-11-03T115127Z-complete-public-prefab-extraction-and-native-workflow-qualification-c440d6b31843411897951e9ba2221a2e.md)
closes that remaining CP2 consumer scope: independent real-project extraction preserves25 actual
compiled entities, selective history, inherited editing, Revert and complete conversion; actual
starter Check admits P/Q and real supervised Play reaches Running/Stop/Exit. Dear native controls
cover captured-tab targeting, invalid SaveCurrent correction, cancellation, exact publication and
Reopen. Final Editor293/293, native5/5 and independent1/1 pass. The owning observation correction
is reproduced maintenance-only and independently checked; unchanged Review remains quiet.
The CE receipt's one validated P2 is fixed; its reliability-persona coverage remains explicitly
incomplete rather than inferred from green tests. Public child source-receipt digest projection,
dedicated idle WM_PAINT incidence, broader write-back, other platforms and complete R5 remain
outside this qualification. The full foundation Goal remains active.
The generic value-tree and record-owned sidecar alternatives retain their
comparison rationale in the plan; historical prerequisite evidence retains its original scope.

The [November 2 follow-up review](../../audits/2026-11-02-refactor-follow-up-review.md) adds concrete
conversion, cancelled-upgrade retry, same-window interaction and bootstrap-diagnostic transition
checks to the existing owners. The
[bootstrap correction](../../knowledge/engineering/verification/2026-11/2026-11-02T051018Z-failed-bootstrap-diagnostics-retained-through-the-real-project-tool-caller-446882fff4584df1a23d3930a6fda2dc.md)
is qualified at349921f through owner cases and an actual failing-project caller. Same-window
revocation is committed as d6dccbc after actual hidden-window DX12 tests; cancelled-upgrade retry
is committed as080852c after the actual recovery-before-Pending cancellation and unchanged-proposal
adoption. Their focused20-test replay passes in the cohort record above. Conversion and export
are qualified within the linked closed compiled prerequisite; this does not complete CP2.

The first-party processed Host successor is qualified as `44470ed` under the
[ordered asset-delivery plan](../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md).
ADRs 0007/0033/0103 now specify explicit source-execution facets and plan-owned Runtime codec
definitions/agreement. The replacement covers shared startup, later Scene requests, finite Scene
data dependencies, concrete Image/TileSet residency and typed complete-Product CPU collections.
Its [qualification](../../knowledge/engineering/verification/2026-10/2026-10-29T152520Z-first-party-processed-runtime-hosts-and-explicit-source-facets-qualification-c2d2a14cd90243f0a200a287f1ce4d20.md)
records final root/owner1082, public80, independent84, Source/Editor55, Reference6, all three selected
native DX12 trajectories, strict consumer Clippy and minimal features. Source-authored Door
dependencies, processed input, target selection and the packaged Player remain ordered work.
The [source-location successor](../source-location-and-reference-resolution-design.md) is
implemented as `a56a1d3` under ADRs 0007/0038. Its
[qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T015832Z-canonical-source-locations-and-typed-ui-image-binding-qualification-0ce5bafdc65f4893aa0079009668e41a.md)
records canonical/scoped source identities, source-asset/document association, real separated and
nested source roots, independent consumers, watch/reload and actual source CLI Export. The same
unit refreezes ADR 0102's public Image inventory: typed asset bindings and process-local opaque
identities replace string-based inference, with Runtime/Canvas, independent manual/provider IDs,
all three neutral examples and four selected actual Windows/DX12 trajectories. It also qualifies
frame-independent Editor TileSet maintenance after Image publication/revocation. The subsequent
[Source Scene qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T060408Z-finite-source-scene-data-closures-and-retained-editor-observations-qualification-deea8e4f86ac44f3a6faf27aefa53a4e.md)
at `f96951a` implements ADRs 0047/0110's finite source data closure, actual canonical readers,
primary Product travel and demanded open Scene working copies. Root1020, final capture20,
independent12, corrected integration, native6 and final affected Reference18 pass in the recorded
scope. Move-only input observations and separate Host/worker retention replace premature refunds
and retained unrelated selections. Input/target delivery and the complete custom-drawn Editor
remain open; source Scene qualification does not complete those units.

The [Input source unit](../../plans/2026-10-30-input-definition-and-source-admission-plan.md) is
qualified at `ff228af` under amended ADRs 0035/0041/0051. Immutable compiled definitions and actual shared owners
replace mutable live-map serde; context slots, move-only World state and candidate-scope selection
preserve semantic lifetimes. Its [qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T083131Z-immutable-input-definitions-and-global-source-workflow-qualification-f8db4628f79943ac94a90593cef6c736.md)
records final source/Authoring/Export105, independent physical5, owner17, Reference179, samples34,
native3, full workspace check and correctness Clippy, with broad predecessor failure history and
focused repairs. The subsequent [processed Input unit](../../plans/2026-10-30-processed-input-startup-and-process-proof-plan.md)
is qualified at `ffa5795` through its [own evidence](../../knowledge/engineering/verification/2026-10/2026-10-30T100534Z-processed-startup-input-ownership-and-prebuilt-source-free-physical-process-qualification-35c2432f89d04982b10d2be3fa0f9d12.md):
primary Product configuration, strict Runtime codec, actual selected typed acquisition, independent
World state and pre-Startup installation. Root1113, final Input126, source5/runner3, Reference179
and final affected7, final native3, workspace/minimal/Clippy/format checks pass within the recorded
predecessor/final scope. Seven prebuilt child cases run after complete source fixture removal.
Safe budget diagnostics survive both codec and Host bridges. The context/revision allowance still
does not close the [endpoint/contribution gap](../../research/2026-10-30-input-endpoint-admission-and-retirement-review.md);
its finite owner/terminal comparison now selects bounded raw-ID admission with private member
lifetimes, read-only projections and shared terminal credits under amended ADR 0041. The
[progress record](../../knowledge/engineering/progress/2026-10/2026-10-30T105046Z-engine-research-reconciliation-and-bounded-input-terminal-comparison-84d7a4b9cbdf44419a23d851f92b9a7d.md)
records five reproduced failures and the passing private eight-test capacity comparison. Production
owner/projection replacement and total prepared UI invalidation are now qualified by the
[bounded Input/UI successor](../../knowledge/engineering/verification/2026-10/2026-10-30T140508Z-bounded-input-ownership-and-total-ui-presentation-qualification-b97643044e4a4f92b892a3539d2ad415.md).
It refreezes ADR0102's exact completion/candidate inventory with finite member/contribution
ownership, original endpoint claims, terminal-tail and late-copy repairs, actual public gameplay,
neutral examples, four selected Windows/DX12 trajectories and the formal Reference desktop
observation path. The record retains broad-run failure/correction history and precise limits.
Seat/analog/replay, full editable text, target-build and packaged delivery remain separate.

The [reference-validity/current-demand design](../reference-validity-and-load-demand-design.md)
and its [unit plan](../../plans/2026-10-30-reference-validity-and-load-demand-plan.md) select the
next CP3 contract under ADRs0007/0011/0033/0047/0110/0122. Actual mandatory dependency equality,
per-leaf demand and distinct strict validation replace the eager-reference assumption. Its
[production qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T172312Z-reference-validity-and-current-demand-production-replacement-qualification-b16511fa33a74c2b808270394b28bf62.md)
records recursive Schema/derive, exact mandatory admission, once-decoded current preparation,
compact deep validation, saved-source delivery, dirty-working-copy boundaries and actual public
source/processed A-B-C/D travel. It includes strict source Export/relocation, independent native
defenses, root/owner regressions and two selected Windows/DX12 trajectories, with broad-run failure
and correction history retained. The eager encoded base, target cooking and packaged Player
remain separate. The pinned study and prior finite closure evidence keep their original scopes.
The [retained-pack successor](../runtime-content-backing-and-residency-design.md) under ADR0007
is implemented and qualified. It preserves complete captured
availability and compares one exact file plus authenticated temporary member reads without a
general provider/cache framework. Its [plan](../../plans/2026-10-31-retained-runtime-pack-plan.md)
has [production evidence](../../knowledge/engineering/verification/2026-10/2026-10-30T184847Z-retained-runtime-pack-and-independent-encoded-residency-qualification-386d2e5e9c124b66bc4390f69b796053.md):
Root1022, independent50, actual travel/held cancellation, escaped pixels, original generations,
exact raw/decode budgets, corruption/refund and relevant feature/Clippy checks. GPU/streaming
performance, general providers/cache, target cooking and the packaged Player remain separate.
The [local build successor](../../plans/2026-10-31-local-artifact-build-plan.md) is qualified in its finite scope.
Its [prerequisite qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T201802Z-source-import-recipes-and-target-sensitive-cpu-image-cook-prerequisite-qualification-462c0179d20b44fd8545d4c135e814f9.md)
records actual six-field source recipes, removed synthetic artifact records/paths and pure CPU
target-sensitive Image cooking: owner328, downstream219, Root1039, independent22 and selected
nativeDX12 two trajectories, plus minimal/Clippy/format/doctest evidence. The
[design](../local-artifact-build-design.md) now has
[complete local-build evidence](../../knowledge/engineering/verification/2026-10/2026-10-30T231610Z-tracked-local-artifact-builds-and-complete-group-publication-qualification-d3fcd08e15e04d45812ae09d7c81e944.md)
for typed tracked compound/processed Image execution, complete local publication/reuse,
fresh-process/interruption cases and original Runtime custody. Root/Asset1252, final independent57,
owner20, minimal features and workspace/strict-consumer Clippy pass within the recorded scope.
ADRs0007/0103 own this accepted finite contract; the remainder of Proposed0087, general scheduling,
target executable selection and packaged delivery remain separate.

The [explicit Cargo unit](../../plans/2026-10-31-explicit-cargo-build-selection-plan.md) is
qualified under ADRs0110/0116. Shared pure explicit selection replaces fixed decoders; native
execution captures exact executable/input facts, Export stages captured bytes, and the Editor
separates complete machine stdout from diagnostic tails. Its
[evidence](../../knowledge/engineering/verification/2026-10/2026-10-31T035034Z-explicit-cargo-builds-captured-executables-and-supervised-protocol-qualification-4cfff5128fa64423b6f234ea7b312c63.md)
records final owner151, deadline6, actual custom native/foreign2, original processed process,
source CLI Export/relocation and final Windows/DX12 supervised2, plus feature/Clippy/format limits.
Foreign output is build-only; source Export remains source-carrying. The
[cooked Player successor](../../plans/2026-10-31-cooked-player-delivery-plan.md) now has its own
[finite qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T050529Z-cooked-content-assembly-captured-player-binding-and-relocatable-source-free-delivery-qualification-816fbb7867fb4c3288a2b2b34f27e1ff.md)
under ADRs0007/0051/0103/0110: complete production assembly/deep coverage, generated compiled
expectations, exact captured-Cargo binding, bounded three-file publication and relocated source-free
Windows headless execution. Final106 owner tests and actual independent cooked1/original2 consumers
pass; retained A, shared content, global driver-event Input and Deferred travel/corruption preserve
their original owners. Cargo compilation failures retain bounded structured diagnostics. This does
not qualify desktop presentation, Linux/macOS, server stripping, general activation or the rest of
Proposed0086/0088. The full foundation Goal stays active.

The [October 29 rendering review](../../research/2026-10-29-render-architecture-and-refactor-plan-review.md)
and long-horizon refinement prefer a finite explicit resource graph, typed contributions and
runner-owned execution in Proposed 0121. Design covers foreseeable shared/cross-view output,
texture/buffer access, history and Nara-owned drawing before interface freeze. The
[private comparison](../../plans/2026-10-31-render-composition-and-host-execution-plan.md)
now has [scoped executable evidence](../../knowledge/engineering/verification/2026-10/2026-10-31T060722Z-private-render-graph-buffer-history-and-viable-controls-comparison-on-dx12-d6aca1996b2342659bd07933b33e0e64.md):
61 unique CPU cases, seven real DX12 cases and a subsequent actual post-Ready submission-failure
case. Graph, direct-order, two-pass slot and ECS-schedule paths execute the same numeric image/
buffer workload. Semantic-copy compatibility, fixed history groups, callback failure and accepted-
prefix resource custody are tested within the private fixture. The subsequent
[optimized CPU-stage comparison](../../benchmarks/2026-10-31-render-composition.md) runs three
processes with 36,000 candidates and substantial P95 variation; it establishes no cost winner.
The [production device-owner prerequisite](../../knowledge/engineering/verification/2026-10/2026-10-31T064658Z-coherent-wgpu-device-ownership-and-render-cost-comparison-qualification-18760d4fdea7435b90fb9bd2bee64510.md)
is implemented at `48cf9ce` under ADR0078: coherent Ready state and one private acquisition path
replace independent native fields through stock Runtime/Editor consumers. Final library147,
focused6 including two additional native cases, critical Editor3, minimal/Clippy/fmt pass in that
recorded scope. Runner-local storage, production output/stream authority, external Runtime/Scene
View/offscreen Features, exact UI completion and materials remain open. R8/R9's complete production
replacement is unimplemented; 0121 remains Proposed.

The subsequent [finite program prerequisite](../../knowledge/engineering/verification/2026-10/2026-10-31T133536Z-finite-typed-render-programs-through-the-original-host-production-qualification-a32bacec9d8b4572801df1f79fc31c01.md)
implements Accepted ADR0094's typed texture/buffer graph through the same production Host. Exact
definition/instance/capture pairing, enabled device requirements, whole-group native admission,
retained output/imports and flat original custody have owner/public and actual Windows/DX12
evidence. An external package implements blur, a separate effect, raw/normalized statistics and
storage-to-vertex histogram without a private executor. Stock Sprite/UI, Runtime/Editor policy,
View/history, complete content/UI interleaving and Nara-drawn composition remain open; this
prerequisite does not promote full0121 or claim a complete R8/R9 replacement.

The [stock preparation successor](../../knowledge/engineering/verification/2026-10/2026-10-31T144610Z-shared-stock-native-preparation-and-whole-candidate-preflight-qualification-01b79b28279b48618dd19ec377ad2cc8.md)
implements whole-candidate capability/Image/custody preflight and one shared native allocation/upload
owner. Sprite/UI typed adapters execute in production-program native tests; existing Runtime/Editor
consumers use the shared caches, byte guards and original-prefix responsibility. Host-frame cache
age and exact touched-instance cleanup preserve independent producer clocks. The resource-byte
interface replaces output-only names without aliases. Ordinary capture/policy migration and
central DTO/facade deletion remain next work; full0121 is still Proposed.

The [typed policy successor](../../knowledge/engineering/verification/2026-10/2026-10-31T192756Z-original-typed-render-policy-capture-and-complete-consumer-qualification-df7f9377f51142b78060b53617c800a1.md)
now carries original selected captures through Runtime windows, Scene, Canvas, embedded Game and
independent image consumers. Explicit capture/window/stock-policy/domain plugins replace central
payload fields, collection matches and combined facades. Installation close preserves accepted
execution; exact final-image UI reports and original move-only sidecars retain separate custody.
The independent scalar renderer qualifies ordinary ECS/window capture and original-input image
reconstruction without stock drawing or a private executor. Scoped ordinary336/Editor268,
Root745 plus corrected Canvas2, final changed-consumer4/Reference12, native owners19/Editor8,
independent image3/program8 and final alternate window1 supply actual evidence. The record
distinguishes pre-correction runs, minimal/portable compile and final correctness checks. Complete
View/history groups, independent streams and Nara-drawn composition remain open; full0121 stays
Proposed.

The [captured-history successor](../../knowledge/engineering/verification/2026-10/2026-10-31T210034Z-captured-view-histories-and-original-submission-cleanup-production-qualification-5c061376803446c0bff1a95495ca55c7.md)
implements neutral View lifetimes, logical effect declarations, immutable actual bindings and
move-owned candidates with exact predecessor bases. Four independent temporal chains and an actual
Runtime window qualify that selected transaction. Original-prefix cleanup isolates late destructor
faults without losing physical completion or successful publication; Host observation transfers in
one removal decision. Its evidence records broad-run failure/correction history, selected native
and independent scope, and separate candidate-byte versus producer-owner limits. Complete
source/processed interleaving, independently paced two-window/image composition, durable materials
and the editable Nara-drawn Inspector remain open; full0121 stays Proposed.

The [native-composition successor](../../knowledge/engineering/verification/2026-10/2026-10-31T225517Z-native-composition-physical-pump-handoff-and-retained-stock-input-qualification-f0cd93c17a3c47fa905993cee07e6c5c.md)
qualifies public window/read-use scopes and receipt admission under ADRs0032/0040/0111, heap-owned
stable policy slots under0094, and physical-loop handoff under0078/0084/0110. Ordinary Source and
Processed desktop actions retain four original captures through live-store/backing retirement and
interleaved image replay. Actual two-window/image and clipped numeric drawing consumers pass,
along with Winit and selected Editor Windows/DX12 gates. The record retains the incorrectly
selected debug measurement failure and supplies no performance or Linux/macOS execution claim.
The [public stock-composition seam](../render-program-production-design.md#next-production-unit-public-stock-composition)
is qualified by the [shared stock-composition successor](../../knowledge/engineering/verification/2026-11/2026-11-01T013746Z-shared-stock-composition-original-captures-and-native-scene-effects-qualification-2f250ecb33b24b5091f28a141cd6b244.md)
at `c836fdb`. Ordinary external blur/grade packages reuse world/Canvas/UI lowering across Runtime,
actual Scene View and offscreen. Shared stages replace the default policy's fixed graph body;
policy-independent collection transfers original UI sidecars, and recipe-selected adapters
remove Scene picking's default-policy downcast. Exact images retain their original program
resource and semantic interpretation. Headless activation, final-definition pairing, UI
backpressure recovery and original-input numerical replay have owner/public evidence. The 701
ordinary, 39 affected Root, 79 final public/native, 9 independent, 10 Reference Game and 5 selected
native tests pass in their recorded scopes; counts overlap. Minimal/portable and correctness/
strict-consumer checks also pass.
Full0121, durable materials, broader View/time facts, reproducible cost attribution and the editable
Nara-drawn Inspector remain open. The separately reviewed registry builder/snapshot plan is
unimplemented R12 work, not qualification from these render tests.

The [stock image successor](../../knowledge/engineering/verification/2026-10/2026-10-31T153613Z-stock-image-consumers-and-declared-worldcanvas-through-shared-programs-qualification-f6cfe4185aad459587db367d807aeb60.md)
migrates real Scene, authoring Canvas, embedded Game and independent image consumers to the
same typed-program native transaction. Ordered Sprite Views have one writer; mapped Game
WorldCanvas uses declared graph textures and shared nearest composition before full-resolution UI.
Private final-target binding shares actual byte custody and preserves initial storage on pure
preflight refusal. Final affected88, no-default14, public program8/image3 and selected native
Editor8 qualify their recorded source scopes. Ordinary Runtime/window capture/policy migration,
central payload/facade deletion, View/history and complete custom composition remain open.

The [original-result/logical-close prerequisite](../../knowledge/engineering/verification/2026-10/2026-10-31T082117Z-original-ui-result-custody-and-logical-close-qualification-6e1965c6428841ec9138088bef20f90c.md)
is implemented at `8e9d840` under ADR0102. Nominal pre-admitted result batches, retained original
receivers, replacement/backpressure guards and Root UI/Input close settlement now have final85,
neutral and selected native evidence. CPU WGPU observations carry exact submission/presentation
facts without native custody. Statistics cannot block logical settlement. Its native successor
below removes the remaining GPU ECS storage and scheduled first-present observation; complete
output/producer/Feature work stays separate.

Its [late-result successor](../../knowledge/engineering/verification/2026-10/2026-10-31T091550Z-late-presentation-retirement-admission-and-exact-step-outcomes-817901432b684d17b69c2f87f4427408.md)
at `525cd6f` closes the first-Stop and unavailable-close-port publication windows. Exact lifecycle
admission accepts original transport for retirement without reviving UI/World presentation, and
the private consumed result prevents a pending Step from reporting a false presentation success.
Final Root37, native critical3, no-UI build and correctness checks pass in the recorded scope.
The [shared concrete GPU drain](../../knowledge/engineering/verification/2026-10/2026-10-31T084653Z-shared-concrete-gpu-queue-retirement-qualification-ab68d8adf25f406d9222df6b3b2fcc70.md)
at `96bd0bf` removes the toolkit-specific drain owner with actual Editor adapter8/native6 evidence.
ADRs0032/0040/0042/0078's
[owned native desktop successor](../../knowledge/engineering/verification/2026-10/2026-10-31T105632Z-owned-native-desktop-execution-and-post-host-presentation-qualification-4db921de03c94598b2ee20d7e82508c5.md)
is implemented at `37ca8bb`: retained original Runtime/Host/registry/windows/pump, authenticated
CPU transfers, post-Host UI/READY/startup, finite telemetry seal and distinct logical/native budgets.
Wait maintenance advances without simulation. The obsolete World retirement operation is deleted;
a plain CPU presentation requirement carries product intent only. Final ordinary227, Root58,
Reference57, native owners7, native Editor4, actual surface mutations/candidate seal, independent
minimal consumers and correctness checks supply scoped evidence. This closes the stock serial
window-backed native transition, including the three earlier Astra/max design requirements.
Immutable generated outputs, producer-prefix retirement, public offscreen/external Features and
complete R8/R9a remain open; Proposed0121 is not promoted.

The reviewed recommendation to amend/replace another ADR is not silently counted as implementation.
The October 27 [UI](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md) and
[language/extension](../../research/2026-10-27-language-extension-product-boundaries.md) studies
add source evidence and experiment priorities only. The [closed Scene native/data
design](../scene-component-representations-design.md) separates that implemented storage subset
from remaining product/language-owner work. Neither a managed tools SDK nor a replacement Editor
frontend is implemented by those documents.
Its [metadata qualification](../../knowledge/engineering/verification/2026-10/2026-10-27T102723Z-backend-free-owner-schema-snapshots-and-clarified-owned-editor-ui-sequencing-7e4b079a81b64d149b8f031b8b2c84ea.md)
records no-default admission, bounded bundles, exact identity/content distinctions and migrated
Scene/root/Editor/independent consumers. It does not qualify mixed runtime materialization.
The [closed selection/compiler successor](../../knowledge/engineering/verification/2026-10/2026-10-27T105601Z-closed-scene-binding-selection-and-shared-strict-data-compilation-263aa2e60c62429db4a8862d2df6e059.md)
adds explicit native/data catalog selection, strict owner/version checks and shared production
Prefab compilation, with nested/default/reference and actual source-demand counterexamples.
The [runtime-family successor](../../knowledge/engineering/verification/2026-10/2026-10-27T125421Z-production-scene-native-data-runtime-and-managed-authority-qualification-294f613511bb48e49fed958c05a1a469.md)
adds managed data selection, aggregate native/data preflight, atomic complete carrier publication,
guarded membership/mutation, current-live export and native claims retry through the existing
owners. Public scheduled Rust and file-backed Editor/source/Prefab consumers exercise that family.
App domain guards now compose; ordinary/advanced native definitions and actual export coverage
have their owning-contract corrections. The
[product selection successor](../../knowledge/engineering/verification/2026-10/2026-10-27T141025Z-mixed-scene-product-selection-and-content-to-runtime-admission-qualification-7c90588821aa4e8da374958ef2e436e4.md)
adds recipe-owned admissions through strict Check/content capture, startup, Restart and Scene
replacement, including public source witness retention and outer-composition counterexamples.
The selected language execution owner remains a separate integration obligation; no SDK or
complete R3 claim follows.
The [EnemyStats consumer successor](../../knowledge/engineering/verification/2026-10/2026-10-27T152916Z-concrete-enemystats-semantic-proposals-through-the-public-project-session-1acb9412a2544e98b059654fe5ec2ca7.md)
adds concrete C# metadata/proposals through the public window-free Session, with actual
Attach/edit/Spawn, review/commit, history, JSON/RON persistence and Rust data reads. Borrowed
authoring metadata queries let that consumer verify the required owner and full component
declaration. Original query, code-publication and newest-request identities fence both review
stages. That experiment remains historical consumer evidence. The
[finite-tool production successor](../../knowledge/engineering/verification/2026-10/2026-10-27T180941Z-production-finite-source-tools-and-exact-physical-close-observations-fc2363973364434096d350ae4a8d762c.md)
qualifies explicitly enabled trusted child proposals in ordinary Project Session/Application
under ADRs 0047/0118. Actual C# metadata, review/acceptance, history, persistence and public Rust
data access pass; deterministic gates separately prove physical spawn/IO retirement and complete
Close observations. Concurrent attempt/request failures retain independent results. The old
managed tool bridge and fixture executor are removed. The
[inline facet successor](../../knowledge/engineering/verification/2026-10/2026-10-27T193225Z-finite-source-tool-property-facets-and-shared-native-review-qualification-c47581dc5dfa45038a38a6aea5b2d4be.md)
adds one admitted i64 property parameter and the small Source Tool panel through the same immutable
Tooling view and ordered action/result ledger. Actual C# and Windows/DX12 source/history/Save/fresh
Open workflows exercise the shared candidate. Generic dirty input retains independent recovery;
only tool parameters retire with their original route. Exact revocation remains available during
Close decisions. Local installation, general inline contributions, executable gameplay publications
and the custom-drawn frontend remain open; no SDK or complete R1b/R3 claim follows.
The [local release preparation successor](../../knowledge/engineering/verification/2026-10/2026-10-27T201403Z-code-free-local-finite-tool-release-capture-and-lifecycle-owner-design-1f12c7706bd44248adec1203402d167e.md)
adds code-free strict-capability capture of a selected manifest and complete declared artifact/
content bytes, with actual C# payload and source-drift consumers. It admits bounded exact local
relationship declarations and generation-one Schema through Reflect; it does not resolve an
installed graph or approve execution. The
[artifact publication successor](../../knowledge/engineering/verification/2026-10/2026-10-27T210523Z-local-tool-artifact-publication-with-a-final-ready-receipt-and-continuous-file-custody-4443d7b18348430884f7015082c9e304.md)
adds preview/cancel, an empty no-replace candidate root, exclusive member creation with continuous
read-handle custody, and a final no-replace ready receipt. Actual Windows and independent C#
consumers exercise bounded complete inventory, incomplete/post-commit observations and fresh
code-free inspection. The failed populated-directory rename experiments select receipt-last
publication; whole-tree atomicity, stable contents and parent-directory durability are not claimed.
There is no implicit or public orphan deletion path. The
[local lifecycle plan](../../plans/2026-10-28-local-source-tool-release-plan.md) keeps installed
graph/ownership recovery, approved runtime custody, evolved-Schema integration, durable selection
and safe dependent-aware removal in progress. Artifact publication alone is not a Package lifecycle.
The [installed runtime authority study](../../research/2026-10-28-installed-tool-runtime-authority.md)
selects a Host-approved machine runtime prerequisite and a captured-managed-image bootstrap
experiment, retaining the package artifact/input boundary. The root Host's narrow Windows
`ExactProcessImage` replaces the private Play copy of native path/mapped-stream evidence;
Play retains suspended spawn, resume, Job containment and physical retirement. This extraction
does not implement the managed bootstrap, installed activation or durable enablement. Its actual
native tests and retained scope belong to the
[shared image evidence](../../knowledge/engineering/verification/2026-10/2026-10-27T213708Z-shared-windows-process-image-evidence-and-explicit-installed-tool-runtime-trust-partition-814f24baa3964ef5ae5708602aaca655.md).
The [captured managed tool successor](../../knowledge/engineering/verification/2026-10/2026-10-27T221624Z-captured-managed-tool-images-through-the-session-and-exact-finite-child-custody-b852454a4fe841548bed563ca97d071e.md)
implements explicit Windows launcher/bootstrap approval, bounded captured managed-image transport
and strict pre-load reference/entry admission. Approved definitions use the existing Session probe,
review and source history; one finite process custody handles native suspended startup, mapping,
resume, cancellation and actual pipe return. Package bytes are not reread from installation paths.
Actual installed V1/V2 and native Editor scopes belong to that linked evidence, including their
real failure and platform limits. The first admitted subset has preexisting project metadata,
generation-one catalogs and no unresolved installed requirements. Durable selection, fresh-project
metadata adoption, evolved lineage, dependent-aware update/removal and full Package UI remain open.
The [continuous source-custody successor](../../knowledge/engineering/verification/2026-10/2026-10-27T233313Z-continuous-file-publication-and-editor-source-custody-qualification-d9a0d6253fc3473ab82093b446048d9c.md)
qualifies the shared file Host's accepted leaf lifetime, temporary reader handoff, actual
authoring/Reopen handles and pending Image metadata custody under ADRs 0047/0070/0115. Historical
catalog data now enters through fresh domain admission. The old bare-ID interfaces, unused receipt
cache and redundant startup transfer wrapper are removed. Root/public persistence 672, independent
language 22, actual managed/Application and three selected Windows/DX12 native trajectories pass.
Read handles remain mutable-content observations; existing replacement remains detect-only.
Windows parent-directory rename can require physical Close. This prerequisite does not implement
durable tool selection, all source-capture identity lifetimes or complete the local lifecycle plan.
The [durable selection successor](../../knowledge/engineering/verification/2026-10/2026-10-28T012206Z-durable-installed-source-tool-selection-and-independent-paired-attempt-qualification-3a4a90d3cc3042e5b89e8fa20e314c22.md)
adds bounded project-local desired selection, code-free Open/reconciliation, exact approved
installation provenance and ready -> file publication -> route promotion through the same Session.
Separate operation/Disable observations preserve both co-terminal outcomes; stale returned-probe
admission, replaced parents, held IO, Close and abnormal-Drop cleanup have focused regressions.
Root/public persistence 702, filesystem 87, Tooling 86, Editor 245 and rebuilt independent language
24 cases pass; the actual C# Application and selected durable native Inspector/panel trajectory
pass. The selected evidence retains its exact platform and final-check scope. Metadata is
pre-admitted; fresh-project adoption/evolved lineage, installed dependent closure, protected content,
safe removal and complete Package UI remain open. This is not full R1b or foundation completion.
The [owner-preserving refresh successor](../../knowledge/engineering/verification/2026-10/2026-10-28T014807Z-recipe-schema-refresh-preserves-independently-admitted-owner-lineage-8afcd1ede3dc4451ac719a1a236e9c85.md)
corrects ADR 0110's compiled schema refresh to retain complete owners absent from the current
recipe, reject invalid/conflicting existing metadata and preserve semantically unchanged leaf
bytes. Focused owner/protocol and public Check evidence belong to that record. The
[adoption plan](../../plans/2026-10-28-authoring-owner-adoption-plan.md) separately records reviewed
history, cumulative-work, successor-receipt and whole-open-set requirements. The
[source/history successor](../../knowledge/engineering/verification/2026-10/2026-10-28T032419Z-source-owned-schema-reclassification-with-exact-retained-history-and-bounded-interpretation-becb85b221e241aebeb4845c692a7379.md)
qualifies additive Scene/Prefab classification, exact retained history in both directions,
nominal commit guards and cumulative-work/peak-retention admission under ADRs 0026/0090. It reuses
actual source preparation, with Reflect pre-allocation interpretation hooks and no source revision
or history replacement. Dormant malformed overlay payloads remain preserved. The
[metadata/current-source handoff](../../knowledge/engineering/verification/2026-10/2026-10-28T041431Z-snapshot-preserving-bundle-successors-and-prepared-current-source-capture-74dd681a584541178fb81b346b53e878.md)
adds snapshot-preserving bundle composition, actual accepted leaf/target attachment and current
Scene capture using the original dirty Prefab Arcs under one prepared nominal basis. Complete
lineage no-op and stale/wrong-target/budget refusals retain actual observations. ADR 0114 adopts
the metadata successor boundary. The later saved-project receipt and whole-Workspace units have
their own evidence below; neither source proof nor this internal handoff closes Session adoption.
The [whole-Workspace successor](../../knowledge/engineering/verification/2026-10/2026-10-28T050925Z-whole-workspace-schema-preparation-and-shared-immutable-published-sources-dc568b9954884122a24a9f8a25491451.md)
adds one move-only candidate for the exact complete open Scene/Prefab set, all-incumbent admission
before successor classification and exclusive all-source binding before infallible swaps. Foreign
workspaces, stale final sources, changed bindings/reopen and unsupported owners refuse without
partial publication; source/history/selection/saved checkpoints remain in place. Published sources
now share immutable document/classification Arcs through fork and Session transfer, with independent
source identities and history. ADRs 0047/0118 own the Workspace cut; ADR 0114 distinguishes saved
startup observations from dirty working captures. The linked evidence qualifies this source unit,
not the matching project receipt or installed Session adoption.
The [saved-project successor](../../knowledge/engineering/verification/2026-10/2026-10-28T054209Z-same-basis-saved-project-receipt-before-whole-session-schema-adoption-a796d5f92d5e4fb4b1ae7cbdca4d2741.md)
shares ordinary startup reading/classification/capture under the prepared basis, keeps saved and
dirty roots distinct, joins the complete actual source/asset/database/lookup closure to the accepted
owner leaf and creates a new bundle-bound index. Cumulative capture and validation budget, final
usage, stale observations and exact attachment failure have actual-file evidence. ADRs
0090/0047/0114/0118 correct the earlier all-historical-composition requirement: exact source history
is preserved; a later Undo may expose a diagnosed composition conflict and Redo its repair. No
Cartesian dependency-history validator is required. Whole Session publication/reconciliation,
immediate preview invalidation and installed-owner/native adoption remain unimplemented.
The [owned-source preparation successor](../../knowledge/engineering/verification/2026-10/2026-10-28T061611Z-owned-source-history-and-whole-workspace-schema-preparation-a4021ad50ca24035b6530ae233b9dff3.md)
adds bounded frozen Scene/Prefab history inputs and a complete Workspace request for finite Host
Tasks. Direct and owned paths share classification/history proof; output pins the original source
Arc, and exact original Workspace/source checks still fence publication. Actual worker and Compute
Task consumers cover immutable snapshots, stale live edits, source Drop, calibrated budgets and
both real conversion-history branches. This does not implement the Session operation, its physical
IO/Close coordination or native installed adoption. Caller-side capture scans/copy have no measured
responsiveness claim; source and project budget models remain logical rather than RSS deadlines.
The [whole-Session adoption successor](../../knowledge/engineering/verification/2026-10/2026-10-28T080917Z-whole-session-installed-schema-owner-adoption-and-explicit-publication-recovery-5a02037cad1b4af49cbec2b24306524a.md)
implements the bounded missing generation-one owner operation through code-free installed
inspection, physical quiescing, complete dirty-source/history preparation, review, actual bundle
publication and matching project/Workspace adoption. Known published-but-unadopted and uncertain
results retain their facts; explicit reconciliation rechecks current dirty sources and attaches
an actual observed successor without another write or synthetic receipt. Same-turn preview
invalidation, index-generation handoff and strict read-only capability derivation have owning
corrections. Root737, filesystem/Tooling190 and Editor245 pass, with existing explicit skips;
real C# Application and Windows/DX12 missing-owner-to-Inspector/panel/history/Save/fresh-Open
consumers pass. This is native Application adoption plus existing widgets, not a new installer
UI, existing-owner migration, installed dependency/removal closure, language SDK or full R1b.
The full foundation Goal remains active; the linked evidence retains platform/performance limits.
The [terminal failure successor](../../knowledge/engineering/verification/2026-10/2026-10-28T085355Z-schema-adoption-failure-provenance-and-truthful-cancellation-b8fcde768b2e4e36a93355796a5f6224.md)
retains the original Workspace source/history report, source diagnostics, operation/phase and typed
task rejection through Session/Application. Pure cancellation creates no failure, while actual
worker failure and publication facts remain independently observable. This closes diagnostic
transport for the bounded adoption operation, not installed dependency/removal or Schema evolution.
The [installed-graph successor](../../knowledge/engineering/verification/2026-10/2026-10-28T095430Z-exact-installed-source-tool-graph-and-dependency-safe-withdrawal-0c433a90f253423da28da2b3e275bff8.md)
replaces the unreleased single-member selection record with bounded exact installed relationships
and independent desired intent. Disabled dependents block ordinary withdrawal; reviewed cascade
stales on graph/route drift and stops implicated execution independently of publication and
physical return. Shared capture admission, same-facts external/native distinction and retained
positive route association have actual public/managed evidence. Root/Tooling857, independent28,
C# Application and selected Windows/DX12 paths pass within the linked source/platform scope.
Withdrawal does not reclaim files, recover created-content custody, migrate existing Schema or
complete the local installation lifecycle. Full R1b and the foundation Goal remain active.
The [protected-content successor](../../knowledge/engineering/verification/2026-10/2026-10-28T114302Z-exact-object-file-unlink-and-durable-installed-content-protection-995675b08109422ab0f90b15d6004546.md)
adds bounded independent management facts, actual live installation custody, sticky sample
adoption/observed modification, exact prepared publication and explicit protection repair.
Reserved metadata objects stay outside source-document admission before decoding succeeds;
content task/terminal facts enter ordinary frame polling. Fs supplies concrete explicitly
authorized same-object unlink on the qualified Windows/NTFS baseline, without claiming unchanged
bytes or installation ownership. Native98, root/Tooling887, Editor246 and independent29 pass,
with their explicit skips; actual managed/Application and selected Windows/DX12 paths pass.
Default-stream custody still permits ADS changes, so complete protected whole-file reclamation
and the full local lifecycle remain open. No custom-drawn UI, performance improvement or native
cross-platform unlink qualification follows.
The [accepted-document protection successor](../../knowledge/engineering/verification/2026-10/2026-10-28T132217Z-accepted-document-protection-and-repairable-installed-content-close-qualification-8a03235372c74fcbb4a02f7ee23eaca2.md)
adds immediate sticky member protection at existing source acceptance seams, confirmed known facts
independent of publication success, exact automatic successors and explicit repair. Persistence
polling progresses unknown classification before its original checkpoint can publish. Failed
first registration, unavailable initial records and retired file Hosts retain honest repair/Close
semantics. Final root/Tooling/Editor libraries pass 1,144 with 38 explicit skips; independent30,
actual managed cancellation/Application and selected Windows/DX12 pass. Reclamation, all-stream
proof, replacement-opening failure injection and interactive locality retain the linked limits.
The [retained-source and physical-observation successor](../../knowledge/engineering/verification/2026-10/2026-10-28T150643Z-retained-source-objects-and-truthful-source-tool-physical-cleanup-qualification-13bc9f4599ef4e839fd608dcfd7f4aed.md)
protects actual retained document/metadata leaves through rename, unlink and uncertainty while
keeping current namespace checks. Existing HostCustody supplies read-only cleanup observations
for superseded Hosts; source-tool pending and sticky executor cleanup failure stay distinct.
Focused owner tests, the independent 30-test consumer, actual managed/Application and selected
Windows/DX12 workflows pass within the linked evidence. Generic unlink is unchanged.
The [managed artifact and text-import replacement](../../knowledge/engineering/verification/2026-10/2026-10-28T163225Z-managed-source-tool-artifact-store-and-project-owned-text-template-replacement-7c31339a78064eb8bc385a9bd82b372a.md)
implements the amended ADRs 0020/0035/0047/0070/0118: fixed private installation publication,
finite artifact inventory, strict source isolation and explicit project-owned text import.
It removes the old mixed-content record/reader, adoption/touch interception and optional-ledger
Save/Reopen/Close gates. Exact graph requirements, source history, actual publication and physical
return remain with their existing owners. The linked evidence owns independent/public/native
results and the reviewed alias corrections. Generic unlink remains unchanged; native-format
templates, reviewed intent/unlink/partial recovery and the full local lifecycle are outside that
predecessor's qualification.
The [reviewed registered-artifact successor](../../knowledge/engineering/verification/2026-10/2026-10-28T185322Z-reviewed-registered-artifact-unlink-and-truthful-partial-recovery-qualification-4c85687a35ef4b54b24b3fc98b6e5083.md)
implements original-basis disposal review, actual graph/source/user checks, pre-deletion intent,
real per-member unlink, first-failure/cancellation preservation and result-publication recovery.
Exact directory namespace custody permits child publication and deletion; completed installation
releases creation-time DELETE access through a live-object handover. Sticky Tasks cleanup failure
cannot become deletion permission. Close retains unpublished mixed results, and Application keeps
every failed Session receipt. Final affected libraries pass 1,316 with 38 explicit skips. The
linked evidence owns actual independent/native results and limits. Incomplete installation
publication recovery, native templates, existing-owner evolution and the full lifecycle remain open.
The [incomplete-publication successor](../../knowledge/engineering/verification/2026-10/2026-10-28T194040Z-incomplete-artifact-publication-custody-and-unified-reviewed-recovery-e4c2d017cc7f4000ac1b38588809102a.md)
replaces the ready-only artifact inventory with actual Ready/Incomplete publication facts. The
publisher retains the actual created subset, receipt temporaries and cleanup-only objects;
exact reader handover and refusal-preserving registration keep that custody through stale
confirmation and failed publication. Retained-object retry reuses the same Session owner, while
fresh Open cannot recover authority from serialized IDs. Both artifact kinds use the existing
review, intent, real unlink and partial-result recovery. Final affected libraries pass 1,239
with 38 explicit skips; all 31 independent language consumers and the separately selected real
managed cancellation, Application and Windows/DX12 workflows pass. The linked evidence owns
static checks and platform limits. Native-format template identity/remapping, existing-owner
Schema evolution, complete management controls and the full foundation Goal remain open.
The [native Prefab successor](../../knowledge/engineering/verification/2026-10/2026-10-28T211402Z-native-prefab-template-identity-remapping-and-retained-publication-qualification-288aa15af1244e52b81c3c364689d633.md)
adds source-owned closed Prefab collection copying/import. Reflect rewrites observed declared
leaves; Scene remaps complete source/overlay/reference identities through its production compiler;
Host fixes target identities, sidecars and encoded bytes against original project/Schema/index
observations. The existing import Tasks lane publishes an absent directory, freshly admits its
members and retains actual partial or published-without-admission custody through physical return
and explicit owner transfer. Independent consumers verify two copies, ordinary source history and
Save/Reopen after actual artifact withdrawal/unlink. Its linked evidence owns final build, test,
review and selected native scope. Final source libraries pass 592; Host/Tooling/Editor/filesystem
libraries pass 1,258 with 38 explicit skips. All 33 independent consumers and separately selected
actual managed/Application/Windows-DX12 workflows pass; correctness Clippy, minimal feature and
root/fixture formatting checks pass. Scene/image collections, external bindings, existing-owner
Schema evolution, full management controls and the complete R1b/foundation Goal remain open.
The [creation-metadata successor](../../knowledge/engineering/verification/2026-10/2026-10-28T223626Z-installed-creation-only-owner-successor-preserves-sources-history-and-separate-execution-approval-57dec00a6525410f9abb79ec24a99e69.md)
adds explicit Add/CreationMetadata intent, complete immediate catalog capture, strict unchanged
stored definitions and freshly bound data Adapter evidence. Source/Workspace reuse the same
whole-open-set/history preparation and nominal basis. Session requires affected execution Disable
and actual return, keeps publication/adoption/cancellation distinct and approves V2 separately.
Final six-library qualification passes 1,887 with 39 explicit skips. All 37 independent consumers,
actual managed cancellation, both C# Application selections and the selected Windows/DX12
facet/panel trajectory pass. Correctness Clippy, independent Scene integration, minimal features
and root/fixture format checks pass. The linked evidence owns exact source/review/platform limits.
This bounded creation-only operation does not establish stored-field/value migration, general
Schema owner evolution, graphical upgrade controls or completion of the foundation Goal.
The [documentation verification](../../knowledge/engineering/verification/2026-10/2026-10-23T115037Z-foundation-architecture-rebaseline-documentation-checks-and-adversarial-closure-33c8c794baf142ee964468a01248af17.md)
records inventory/link/status checks and the scoped adversarial correction/recheck. No Cargo,
performance or native trajectory was re-executed for this design-only unit.

The [R4a local-close qualification](../../knowledge/engineering/verification/2026-10/2026-10-25T135422Z-owner-local-close-custody-and-bounded-completion-dependency-comparison-3a6ad83ba4f649ec973982d9de3a0ec9.md)
removes the concrete obligation ledger's former `Send` promise and admits local Rust controllers
through the existing owner-thread retirement/quarantine path. Exact rejected registration,
startup failure, finite retry, normal Drop and unwind have public evidence. The same safe
two-node consumer compares typed composite and sealed completion dependencies; fixed aggregates
select the composite, with a separate renamed-root consumer. ADRs 0003/0010/0042/0082/0084 adopt
the scoped ownership contract. Actual Editor graphics/SDK migration, arbitrary dependency graphs,
R4b/R4c and full R4a remain open; this is not native cleanup qualification.

The [retained native-graphics successor](../../knowledge/engineering/verification/2026-10/2026-10-25T145420Z-retained-editor-native-graphics-retirement-and-physical-completion-qualification-2665523023e84aba979458cba3f2affc.md)
replaces the Editor's best-effort drop/reinstall path with retained partial/live graphics custody,
an exact device/queue drain token and completion-gated renderer/view/platform phases. Semantic
Exit keeps the frontend through dirty decisions and retains maintenance wakes; only observed
semantic readiness and physical graphics completion permit exit. Eleven selected actual
Windows/DX12 trajectories pass, including SDK completed-error retry, pre-presentation exit,
dirty Exit during queued reconstruction, partial initialization and the existing secondary/device-recovery workflow. Adapter tests retain
separate GPU/readback scope. ADRs 0078/0118 adopt this concrete Host contract. Arbitrary SDK
exceptions, detached outstanding epochs and every native failure branch are not qualified; R4b,
R4c, dynamic-code custody and full foundation rebaseline remain open.

The [activation-scoped fault-delivery correction](../../knowledge/engineering/verification/2026-10/2026-10-26T052536Z-retained-runtime-fault-delivery-and-reference-game-exit-driver-correction-19b2541631464d8798e3fa47ace8bfca.md)
separates an already delivered sticky execution fault from retained cleanup failure under
ADR 0084. Actual typed fault fields survive outcome capture, Stop and Desktop lowering; a new
generation and independent cleanup failures remain observable. The Reference Game Exit caller
also continues physical maintenance after Workspace retirement. Exact verification and remaining
Application progress ergonomics belong to the record; no general fault bus or lifecycle redesign
is introduced by this correction.

The [ordinary Application progress successor](../../knowledge/engineering/verification/2026-10/2026-10-26T063246Z-one-bounded-application-no-action-pass-through-open-and-physical-retirement-f8c6241efa2b48f8af54f1603482cd39.md)
provides one public bounded no-action pass across Open, Session and physical retirement.
Native maintenance shares it without changing effects-only wake ownership; real submissions
retain their own evidence and drive. Retained supervised owners progress during replacement,
and failure cancellation publishes on the next pass rather than recursively driving again.
Ordinary/public/Reference Game and selected actual Windows/DX12 scope belong to the linked
record. Existing ADRs 0047/0118/0084 own these contracts; no universal lifecycle kernel is added.

The [R4b activity-custody qualification](../../knowledge/engineering/verification/2026-10/2026-10-25T155728Z-independent-task-activity-custody-and-shared-image-runtime-qualification-13caef5f8c39471aa58529789ae9c530.md)
adds optional Host-owned Tasks facilities and independently closed activities. ADRs 0008/0052
adopt scope-local admission/coalescing/cutoffs and physical-return accounting; 0082/0084 distinguish
that evidence from shared worker joins. Seventeen scope tests and two actual same-AssetId Image
consumers are included in the 189-test all-feature Tasks/Image cohort; one direct/renamed-root
consumer also passes. Independent review found and rechecked the corrected disposal-reaper
restart gap. No default product allocation changed. Lazy per-Runtime versus shared workload
measurement, trusted Editor deployment, R4c and the full rebaseline remain open; the new counters
and timing sample are not performance qualification.

The [R4b demand-activation and matched comparison](../../knowledge/engineering/verification/2026-10/2026-10-25T164306Z-demand-activation-custody-and-matched-lazy-shared-image-comparison-c5d3a638a3884807b489a85f0b7b0b1a.md)
adds optional per-kind first-use workers while retaining partial startup and in-flight OS spawn
through actual join. The 198-test all-feature Tasks/Image cohort, two independent consumers,
workspace check and focused Clippy pass. A separate release comparison at matched two-worker
capacity confirms sibling PNG progress with lazy private facilities and shared occupancy while
two closures block. ADRs 0008/0052 adopt explicit activation costs/failure and distinguish custody
from progress isolation; no product default changes. Actual Editor failure/refresh/close owners
and deployment qualification remain open, not replaced by the bounded Image evidence.

The [retained authoring replacement qualification](../../knowledge/engineering/verification/2026-10/2026-10-25T172648Z-retained-authoring-replacement-and-physical-retirement-qualification-f93efd28e9324ff4accf45fc3d38311c.md)
adds a bounded Session-owned predecessor or rejected post-start Runtime, independent retirement
maintenance and exact file-Host transfer before successor publication. A held predecessor permits
successor Save/Reopen but gates another refresh; Session close observes late-result disposal and
aggregates disjoint physical reports once, with sticky cleanup failure. The four real Prefab
owner tests and final 420-case root/198-case ordinary Editor cohorts pass, together with eleven
selected Windows/DX12 trajectories including the critical group and refreshed Canvas source.
ADRs 0052/0118 adopt this boundary. The initial App/Open custody successor has its own evidence
below. Trusted Play's complete Session close, lazy-private product deployment, R4c and the full
plan remain separate open work.

The [initial construction/Open custody qualification](../../knowledge/engineering/verification/2026-10/2026-10-25T191539Z-initial-authoring-construction-and-failed-open-physical-custody-qualification-ad41f2c430ac4e56bdaf88fbb457f1cd.md)
retains raw App configuration, admission, failed Startup and unpublished Runtime owners through
physical return. Public Open failure preserves that owner; Application maintains a bounded failed
attempt independently of Recovery, gates Retry/replacement, waits on Exit and exposes sticky
cleanup errors through the frontend and final runner result. A later process Exit upgrades an
existing Close Project intent while preserving cleanup. ADRs 0003/0010/0118 adopt these boundaries.
The full seven-reviewer round confirms lost Exit and missing real Startup evidence; both have
separate fixes and independent scoped closure. Real held-body/late-result owner proofs, source
refusal, complete root425, expanded Editor247 and selected Windows/DX12 native10 pass. Final
integration limits remain bound to the linked record; this does not qualify held authoring native
quiet cycles, trusted Play's complete Session close, lazy product deployment or all R4b.

The [native metadata source-fact correction](../../knowledge/engineering/verification/2026-10/2026-10-25T194117Z-native-document-metadata-in-supervised-play-source-closure-facts-0fb93990d7d44da9a8111b48f82aee59.md)
adds already admitted Scene/Prefab sidecars to the same source closure used by supervised Play.
It shares identity/digest/read accounting, retains catalogue revalidation and exact handshake
comparison, and passes direct Editor receipt/strict Runtime digest comparisons for repeated and
mixed path/stable-ID Prefab references. This corrects the source-confirmed cause of the generated
starter's runtime-facts refusal; its actual Ready/Stop/clean Editor Exit rerun passes within the
247-case Editor cohort. This does not qualify full source-free delivery or CAS.

The [private-lazy Editor and whole-Session close successor](../../knowledge/engineering/verification/2026-10/2026-10-25T213241Z-private-lazy-editor-tasks-and-whole-session-physical-close-qualification-4c62c82789e4449dba02bb60f145760c.md)
deploys explicit owned-lazy Tasks for authoring and trusted Play, retaining physical participants
and canonical activation identity. Irreversible Session close owns prepared, failed and active
Play retirement without gameplay or successor Restart; existing activation/candidate return stays
accountable. Disjoint reports aggregate once, and completed cleanup errors survive Session release,
Application replacement and the native final result. ADRs 0003/0008/0052/0118 own these contracts.
The linked evidence distinguishes actual owner/public/native checks from the original review
findings and their closure. Shipping/default Tasks remain eager; worker counts are not latency
or CPU/memory measurements. Actual product worker-start failure injection, standalone Open Host
retry/final shutdown, held native quiet-close, R4c and the remaining foundation waves stay open.

The separate [Reopen preparation-fidelity correction](../../knowledge/engineering/verification/2026-10/2026-10-25T220113Z-reopen-preparation-failure-fidelity-before-native-source-admission-9e51c9e94971464ab4876bdf2ff2ff07.md)
preserves already determined file/decode/validation failures instead of masking them with an
independent source probe. Successful Scene/Prefab candidates prove admitted/read/current identity
before native callbacks and retain final publication revalidation. Unchanged public failure
assertions, actual callback negative/positive controls and scoped source/test reviews qualify the
bounded ADR 0047 correction; they do not claim strong filesystem CAS or a new persistence protocol.

The [standalone startup-owner successor](../../knowledge/engineering/verification/2026-10/2026-10-25T232708Z-startup-preparation-owner-retry-and-final-application-shutdown-4281deb3281042fc9095f7cf2e247c4b.md)
implements a reusable lazy Open Host facility outside every Session, exact pre-admission rejection
and unadmitted cleanup evidence, retained partial-worker retirement, explicit same-Host retry and
irreversible final Application shutdown. Close Project and dirty Cancel preserve actual reuse.
The standalone single-poll and blocking drivers share the original physical owner without holding
its mutex across blocking waits. ADRs 0008/0052/0118 own this bounded contract. The linked evidence
records real first-submission/partial-worker proof, independent review findings and closure,
passing owner/public/integrated cohorts, and the final native qualification results and limits.
It does not extend the predecessor's historical qualification, select shared product queues,
qualify every trusted authoring/Play activation failure or complete R4b/R4c and the foundation plan.

The [R5c disposable projection qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T225800Z-disposable-scene-projection-epochs-and-stable-native-preview-qualification-2478d897135549daacbc8d1fbc90b9d9.md) binds `207bbcc` to complete-domain Scene epoch ownership, unchanged shared authoring facilities, a stable preview renderer producer and fresh presentation authority after identical-content recovery. Held Image/Save work, the joined public edit/history/Save/Reopen/picking/Close consumer, independent Prefab consumers and six actual Windows/DX12 trajectories pass. This closes the selected preview lifetime mismatch, not incremental projection, durable property addressing or the full foundation rebaseline.

The [canonical native-source admission checkpoint](../../knowledge/engineering/verification/2026-10/2026-10-24T-project-content-source-admission-and-prefab-binding-qualification.md)
adds explicit location/stable Scene and Prefab keys at the content owner, exact sidecar closure
reconciliation, canonical Prefab ancestry and bounded provenance allocation. Delivery refuses
conflicting stable source identities across its finite admitted union. The separate
[exact-spelling qualification](../../knowledge/engineering/verification/2026-10/2026-10-24T095418Z-exact-stored-spelling-through-filesystem-capabilities-79d51b36d2944aa0bc70384eb2d11b86.md)
records filesystem-native spelling evidence and platform limits. This does not complete required
source-bound materialization, occurrence claims, Browser/Editor admission or the Runtime/Canvas
identity refreeze; the active rebaseline remains open.

The [required Prefab-binding checkpoint](../../knowledge/engineering/verification/2026-10/2026-10-24T105758Z-required-canonical-prefab-source-binding-qualification-6afeb3f651f5456fba96e8e1e279c46c.md)
removes the bare-document resolver/default-binding route, optional projection keys and request-based
cycle fallback for every Prefab expansion consumer. The in-memory adapter owns one document per
canonical key, with explicit canonical replacement across aliases and no retained source history.
Project conflict admission remains independent of that author-owned publication policy. This does
not bind the containing Scene source, add occurrence ownership or change shared UI key equality.

The [required Scene-source and occurrence implementation](../../knowledge/engineering/verification/2026-10/2026-10-24T114901Z-required-scene-sources-and-active-occurrence-ownership-qualification-e249da9a1f404677a72e52457ac990c4.md)
has scoped qualification. Pure expanded documents are now distinct from required source-bound
materialization candidates; Scene owns explicit World-local active namespace/slot association
through exact publication, replacement, end and disposable projection clear. Product startup,
preview and each opened Editor document carry their own source keys. The five product review
defects and the native handle-peak undercount are fixed. Final Rust 1.95.0 Scene/UI/tooling 463,
root/public 567, independent module 35 and reference-game 31 tests pass, as do six selected
Windows/DX12 trajectories. This qualifies the bounded source/occurrence replacement, not ordinary
no-entity preparation, shared Runtime/Canvas identity refreeze or the complete foundation goal.

The [unbound prebinding prerequisite](../../knowledge/engineering/progress/2026-10/2026-10-24T141552Z-unbound-prebinding-admission-and-ordinary-scene-preparation-design-checkpoint-c5f43007b24245da8e1c9dd009b7fa33.md)
qualifies Reflect's fresh-target revalidation over already prepared native values, preserving the
original World and touched asset baseline without allocating targets or rerunning codecs. The
570-test Reflect/Scene cohort passes. That prerequisite did not qualify the ordinary Scene prepare
API or its managed definition, occurrence and complete public-consumer integration.

The subsequent [ordinary Scene preparation qualification](../../knowledge/engineering/verification/2026-10/2026-10-24T164025Z-owned-ordinary-scene-preparation-and-complete-support-admission-qualification-e94d2a95cbf94dd7a0cbbd371aedf55c.md)
implements move-only unbound native-value custody, world-free cancellation and consuming commit
under ADRs 0038/0058/0081/0098. Ordinary execution uses the managed definition; explicit typed preview
retains separate execution and Host composition bases without bypassing authority. Final checks
are nonflushing, and shared ECS/Hierarchy admission includes cached resource targets, generation
insertion and completion removal under ADR 0100. The final-source owner857, root/public588,
independent47 and Reference Game31 cohorts pass, as do six actual Windows/DX12 trajectories,
workspace/minimal compilation, scoped correctness lint, eight Scene doctests and three workspace
format checks. The subsequent [complete-key conformance](../../knowledge/engineering/verification/2026-10/2026-10-24T185535Z-complete-persistent-ui-identity-and-neutral-conformance-successor-0cf84bdfd2f54766be84f3ca206f7931.md) qualifies Runtime/Canvas adoption,
neutral/public consumers and ten actual Windows/DX12 trajectories. Neither unit completes the
foundation rebaseline.

The [R1a qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T123523Z-workflow-driven-foundation-waves-and-r1a-owned-application-submission-qualification-5e5f6c3eb3b84150bc3a5e06e9862abf.md)
adds owned per-action direct Application submission observations under ADR 0118. The relevant
214-test Editor/public cohort, 11 independent consumer tests and all three selected Windows/DX12
native trajectories pass. Closing filters, whole-batch rejection, retained observations and
dispatch versus eventual Save completion remain explicit. The five-owner target architecture
and seven workflow waves are an execution direction, not completion of R1, prepared editing,
dynamic tools, Behavior or render composition. Their unimplemented scope remains unqualified.

The [R1/F2 persistence-attempt qualification](../../knowledge/engineering/verification/2026-10/2026-10-25T081443Z-exact-persistence-attempts-and-terminal-observation-qualification-332f36f3c0f94f93ae1eb2721e981359.md)
implements one concrete Idle/Pending/Terminal owner under ADRs 0047/0118, with the exact admitted
attempt carried through dispatch, physical-return-aware publication and observed-ID acknowledgment.
Published rejection, nonpublishing refusal and retry supersession remain distinct; Close retains
its matching success while Exit consumes its exact Save before advancing. Old, foreign and duplicate
observations cannot clear successors, including a real two-document Exit handoff. Final-source
root/Editor/tooling 667, independent panel 7, independent source 11 and Reference Game 2 tests pass,
as do four selected actual Windows/DX12 trajectories and all 12 final serial qualification groups.
Complete nine-lens review has no actionable finding; both advisory proof gaps have a separately
reviewed correction and final-source rerun. This is observation ownership, not file reconciliation,
physical shutdown completion, full R1 or foundation qualification. The attempts do not implement
useful semantic tooling, compiled-tool diagnostic transport or local install lifecycle; the next
qualification records the separate F3 diagnostic replacement.

The [R1/F3 compiled-failure qualification](../../knowledge/engineering/verification/2026-10/2026-10-25T100841Z-compiled-domain-failure-projection-and-truthful-publication-observations-8b995496a82b411184e95f6f462b48f8.md)
replaces the Display/source string boundary with bounded domain code/severity/summary records,
explicit loss and read-only admitted repair hints. Canonical tool protocol 1 synchronizes both
endpoints, metadata and current consumers without old readers. Schema's typed owner preserves
publication uncertainty and independent cleanup; Repair retains actual postcheck and nested
content failures. Complete nine-lens review and independent validation identify five P2 items;
the caller's corrections have a separate zero-actionable source follow-up. Final root416,
all-eight real CLI cases, workspace/Reference Game all-target compilation, minimal CLI,
correctness/suspicious lint and format checks pass. Actual failed-cleanup injection and typed
uncertainty projection are distinct proofs, not physical uncertainty reproduction. The useful
semantic tool, finite child experiment, local install lifecycle and complete R1 remain open.

The [source-built Snap qualification](../../knowledge/engineering/verification/2026-10/2026-10-25T112059Z-source-local-snap-public-consumer-and-installer-qualification-f89f21479f7d4c06a81ab5bc63b5ff87.md)
adds a real local Transform2d command through two bounded observations, a pure planner and the
existing Source-owned candidate/inverse/history. The actual Dear ImGui panel and an independent
inactive-document Session consumer agree through exact results, one Undo and Save/Reopen.
Complete nine-lens review's confirmed P2 installer-proof gap is corrected with a real
installer-composition test and independent follow-up. Final consumer42 and focused6 pass; scoped
Clippy and both format checks pass. No new engine API or ADR contract is introduced. These tests
do not create a native window/GPU; child-process tools, inline Inspector, local install lifecycle,
supplemental panel recovery coverage and full R1 remain open.

The [finite child Snap qualification](../../knowledge/engineering/verification/2026-10/2026-10-25T122634Z-finite-child-snap-proposal-and-physical-custody-qualification-ce23b00e49cc4a758017b5a2eba63913.md)
adds the explicitly enabled trusted helper experiment without an engine process API. Fixed frames
carry only grid/coordinates; the parent retains source basis and physical custody until child reap
and all I/O joins. Two reproduced error-fact losses are fixed. Complete nine-lens review's P2
Reopen proof gap is corrected and independently rechecked. Final focused nextest13, feature-on
all-target Clippy, default-off all-target check and both format checks pass. Held return, cancel,
timeout, malformed output and actual inactive-document history/Save/Reopen retain their scope.
This is trusted no-descendant Windows execution, not sandboxing, arbitrary process-tree cleanup,
native Editor/GPU qualification, a runner-owned installed-tool lane or full R1/R1b completion.

The [R2 source-owner qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T140847Z-prepared-source-editing-and-non-active-entity-query-qualification-d08c0cb5ab2c4f798dd7cd98c2a2e6c5.md)
adds owned Scene/Prefab prepare/commit, mixed construction and bounded non-active direct-entity
queries under ADRs 0026/0047/0118. Final serial evidence passes 826 library cases, 10 prepared
integration cases, 30 separately compiled consumers and 33 selected Application/native cases,
including four actual Windows/DX12 trajectories. Eleven regressions cover the raw Session close
cut, including gestures and queued Apply Changes; Stop/Cancel remain available for retirement.
This is explicit-ID construction and direct source enumeration, not all of R2 or delivery wave 1.
Transaction-local allocation, durable nested properties, full field queries, prepared-edit panel
ergonomics and cost comparisons remain unqualified. That evidence predates the bounded custody
replacement below; it does not qualify the later additive Open/source-lifetime changes.

The [R2 ordinary source-owned creation qualification](../../knowledge/engineering/verification/2026-10/2026-10-24T203944Z-source-owned-entity-creation-and-generated-product-qualification-c5fd5454223046e38fc1761338ddffb0.md)
removes Workspace ordinal/reservation choreography. Scene/Prefab own generated IDs, validation and
history publication, including separate Published and registry/database-aware native entries.
The final 383-owner, 398-root, 183-Editor, six-preview and five-independent consumer cohorts pass,
as do three selected actual Windows/DX12 trajectories and the generated starter Check/build/export
workflow. Workspace/all-target, correctness Clippy, portable wasm and format checks pass. Generated
identity is not an absolute historical non-reuse promise. This is ordinary creation, not the retained
transaction-local draft, full field queries, panel preparation, cost comparison or full R2.

The [retained construction qualification](../../knowledge/engineering/verification/2026-10/2026-10-24T224939Z-retained-source-construction-pinned-public-and-native-qualification-e1813acad6f04a93898a1e342989fa44.md)
adds the bounded source-owned draft, opaque local references and fixed prepared/successful mapping
through Workspace, project Session and direct Application adapters. Explicit Rust 1.95.0 passes
owner 393, root/Editor 581, four independent workflows, the feature-off UI guard, no-default creation
and direct Session lifecycle rejection. Three selected actual Windows native regressions and final
workspace/correctness/portable checks pass. The complete eight-lens CE receipt's one P2 test gap is
fixed and independently rechecked. The bounded debug comparison states its ID-length/timer limits
and corrects earlier unpinned toolchain labeling. Field discovery/reading, registered-panel
preparation and full R2 are not implemented by this qualification.

The [read-query qualification](../../knowledge/engineering/verification/2026-10/2026-10-25T002933Z-bounded-authoring-query-complete-review-and-aggregate-budget-qualification-893b5a3ffa104ec8bc7c7413c12e0c5d.md)
records the bounded successor: actual component/root metadata pages and separately bounded
selected-root values reuse one document-query basis and existing action/outcome routing. Explicit
Rust 1.95.0 owner tests and the independent non-active Scene/Prefab discover/read/computed-edit/
history/persistence consumer pass, as do the root/Editor libraries, three selected affected Windows
native regressions and workspace/correctness/format checks. Portable wasm compile and feature-off
UI checks also pass. Five scoped simplification lanes and complete nine-lens CE review ran. The
one confirmed P2 multi-row budget proof gap is closed by literal two-row exact/under-budget
cases for Scene/Prefab, five passing focused tests and an independent targeted follow-up.
Production sources were unchanged during that review correction. Populated/degraded index
maintenance costs remain unmeasured. This is not full R2 or prepared panel submission.

The [prepared-panel qualification](../../knowledge/engineering/verification/2026-10/2026-10-25T024527Z-prepared-panel-complete-review-and-scoped-workflow-qualification-49e27d9779054a5b8fda1142add06519.md)
records the bounded successor: finite read-only context preparation, complete query-basis
admission and move-only candidate commands reuse existing owners and panel result routing.
Actual registered non-active Scene/Prefab panels now complete computed edits and local-reference
construction through history and persistence. Owner/public/root, selected existing Windows/DX12
and static/portable evidence retain their recorded scope. Actual complete nine-lens review found
two P2 proof gaps and one P3 guide contradiction, all corrected with an independent targeted recheck.
Nine added regressions, 597 root/Editor cases and four final public workflows pass; the three
selected actual Windows/DX12 cases precede only test/documentation corrections. Five
source-simplification lanes ran; built-in original-slot result observers now borrow outcomes instead
of cloning an intermediate full report. The
[two-run cost comparison](../../benchmarks/2026-10-25-prepared-panel.md) separates preparation,
enqueue and consuming dispatch. The
[subsequent phase attribution](../../benchmarks/2026-10-25-reference-preparation-phases.md) records
two instrumented and three control runs with all temporary source changes removed. The populated
instrumented format boundary occupies about 97% of public preparation wall time; every observed
call uses local classification and declines reference-shape certification. Absolute disturbance
remains unisolated; no latency improvement, permanent profiling API or new admission proof is
qualified by that diagnosis. The subsequent
[finite reference certificate](../../benchmarks/2026-10-25-reference-shape-certificate.md) extends
the private shape proof to both typed reference forms while retaining full fallback and exact
encoder byte checks. Direct counters observe zero parser-preflight calls in 576 owner calls;
two clean-source candidate runs meet the selected populated-case comparison target, with control
drift and no fixed causal speedup claim. Complete document traversal and both encodings remain.
The earlier read-query receipt remains separate; these bounded units do not complete R2 or the
foundation rebaseline.

The [selector-directed read qualification](../../knowledge/engineering/verification/2026-10/2026-10-25T051125Z-selector-directed-property-reads-and-indexed-root-retirement-qualification-bd267ec01e4d44d3ad5f2a2d4543514d.md)
replaces unreleased root-only read APIs with existing stable selectors, independent lookup/copy
limits and borrowed explicit/default provenance. A real non-active Scene/Prefab panel computes
edits from selected nested values through history and persistence. Review's P2 hidden root-tombstone
scan is corrected with admitted sorted-slice lookup and independent follow-up. Final focused41,
public5 and static evidence are separate from the earlier broad/native cohorts; their exact source
and limits are retained. At that checkpoint defaulted-root mutation was unimplemented; its separately
qualified successor follows. Nested discovery and full R2 stay open.

The [first-write qualification](../../knowledge/engineering/verification/2026-10/2026-10-25T060130Z-source-owned-first-property-writes-and-bounded-selector-error-qualification-0dfaa18252de473b90ff57ef150e64b0.md)
uses the existing Source candidate/root-patch/inverse/history chain to initialize only a selected
omitted default root for nested writes, or assign an explicit root directly. Undo restores authored
absence; unrelated defaults remain omitted. Static invalid-ID errors and append-time construction
ID limits remove the observed unadmitted-identity copies. Actual public defaulted read/computed-edit/
Undo/Redo/clean-Save refusal/dirty Save/Reopen evidence retains its scope. Complete templates still
cannot omit fields, and missing storage ancestors/list slots remain refused; no new framework,
default projection policy, full R2 or foundation completion is implied.

The [independent direct-member successor](../../knowledge/engineering/verification/2026-10/2026-10-26T024129Z-pre-budget-query-ownership-and-independent-direct-member-panel-workflow-1acbda3fb8cc47218dd9b54bcd59f367.md)
removes a caller-controlled projection clone before metadata admission. Borrowed rows become
owned only after the full page/continuation budget gate. An actually separately compiled
registered panel discovers a direct Record member in a non-active Scene, performs a bounded
scalar read and retained prepared edit, receives exact accepted/stale results, and completes
Undo/Redo and Save/Reopen without moving selection or preview. The earlier direct-member receipt
described an internal headless panel test as independent; the successor and the
[Exit-driver correction](../../knowledge/engineering/verification/2026-10/2026-10-26T022556Z-public-exit-drivers-retain-maintenance-after-workspace-retirement-2c1f097eeed04d499d81adc33d3232dd.md)
correct that claim without rewriting historical evidence. This is not recursive/collection
discovery, Prefab-overlay, native-window or full R2 qualification.

The [observation-bound preparation successor](../../knowledge/engineering/verification/2026-10/2026-10-27T155752Z-complete-observation-bases-at-public-authoring-preparation-ingress-d45451eaca544ad2b50a55a66a0ce496.md)
replaces revision-only Workspace/Session/Application preparation with complete retained query
bases, including construction preparation. Panel adapters delegate that check to the common
ingress; source-local Scene/Prefab candidates and commit-time admission remain unchanged.
Real public negative consumers distinguish equal document/revision with missing or independently
readmitted equivalent Schema, and preserve source/history/navigation/disk on refusal. Current
observations cannot authorize an obsolete construction draft. Evidence and native scope belong
to that successor; the complete foundation and extension workflows remain open.

The [private Prefab intent comparison](../../knowledge/engineering/verification/2026-10/2026-10-26T025005Z-private-prefab-property-intent-comparison-and-next-owner-replacement-70ca642be09045ab909751b87ed5164d.md)
separately demonstrates the difference between real root-patch lowering and retained scalar leaf
intent. Its six test-private cases and Proposed design do not replace ADR 0038's current overlay
format, implement structural/reference rebasing or qualify conflict persistence.
The [structural/reference successor](../../knowledge/engineering/verification/2026-10/2026-10-26T034052Z-private-prefab-structure-and-reference-comparison-with-owner-boundary-counterexamples-c823420a09444940993ed9d948a28273.md)
adds real nested expansion, explicit reference-slot binding, normalized placement, additions and
suppression comparisons. Default-reference, mixed-owner child and optional-container
counterexamples correct or bound the private compiler; complete component admission, production
addition provenance, closure publication and source repair/history remain open. The
[Unity/Godot editor study](../../research/2026-10-26-unity-godot-prefab-editor-ownership.md)
informs the Proposed editor workflow, not an ADR amendment or native implementation claim.
The [typed-origin production successor](../../knowledge/engineering/verification/2026-10/2026-10-26T041116Z-typed-prefab-instance-addition-provenance-and-complete-consumer-propagation-331ec66de92c4315971800928d8f2dc7.md)
reconciles 0038/0058/0102 and replaces base-source attribution for instance additions with tagged
ancestry, including added instance anchors. Runtime retains encapsulated provenance; root custody
and opaque UI lowering distinguish both origins. Current durable patch storage, flat effective-ID
constraints and the separate closure owner remain. This is not declarative-overlay, Apply/Revert,
repair-history or conflict-persistence completion; actual verification limits belong to the record.

The [full-value comparison successor](../../knowledge/engineering/verification/2026-10/2026-10-26T061353Z-complete-value-projection-comparison-without-source-history-or-templates-047bc67b5dec41c2a940091b39ceedba.md)
shares production source classification's complete-value validation core with the private
declarative compiler. Direct whole-value construction preserves lawful absent versus empty
containers and no longer needs source edit/construction history, a creation template or an Edit
Adapter. Its strict whole-document entry remains test-only; aggregate effective-value checks
include defaults but do not prove full pre-copy admission. This closes the private lowering
counterexample, not durable overlay storage, source repair/history or Host publication.

The [mixed-origin/property successor](../../knowledge/engineering/verification/2026-10/2026-10-26T074532Z-complete-mixed-origin-overlay-targets-and-payload-relative-property-claims-0949791c549242739236d36eb00d4fa2.md)
combines complete nested source/addition targets with per-hop canonical-base predicates and
bounded Schema-relative property claims in the private compiler. Explicit Add/Replace modes,
overlap/key rules, Absent/default binding and empty reference slots have executable coverage.
Reflect reuses its declared-reference traversal; the resolved-base trace and compiler remain
test-private. Production patch storage, bounded source/conflict decoding, source history/repair
and captured-closure activation remain separate work, without an Accepted ADR format change.

The [exact instance transaction successor](../../knowledge/engineering/verification/2026-10/2026-10-26T083507Z-exact-source-owned-prefab-instance-transactions-and-complete-snapshot-custody-85b9ca4381b94cad9c61fb829f479247.md)
adds containing-source declaration replacement through existing prepared Scene/Prefab edits,
exact inverse/history and actual Host scanners. ADRs 0026/0038/0043/0090 adopt comparison,
snapshot migration and unavailable-preservation rules. Both sides remain complete retained input;
source preparation does not imply resolution, activation, nested preview or conversion. Production
durable overrides remain patches, with partial-field asset dependency capture still a separate gap.

The [field-asset fact successor](../../knowledge/engineering/verification/2026-10/2026-10-26T092102Z-source-owned-field-asset-facts-and-actual-authoring-closure-capture-198247ca51f4465880fb2cb1f3240ccb.md)
closes that current Authoring dependency gap through source-classified current field/default
references, bounded aggregate projection and actual Host capture. It reuses Reflect's declared
visitor and the existing catalogue/receipts without fake complete-record facts. Unknown/version-
incompatible data remains preserved without inferred dependencies. Reference-bearing preparation
uses full classification; this is a correctness scope, not an unchanged-latency or durable-overlay
activation claim. Verification and the conservative unavailable-manifest copy charge belong to
the linked evidence.

The [canonical source codec successor](../../knowledge/engineering/verification/2026-10/2026-10-26T103001Z-canonical-source-identity-codec-and-complete-nested-overlay-round-trips-3ce62b507a1a445582c811954dac28c6.md)
adds source-owned strict JSON/RON identity values under ADRs 0038/0043. Complete kind and origin
replace the private comparison's second representation; real mixed-origin target, binding,
placement and conflict round trips pass. Scene's 429-test cohort, no-default build and affected
correctness Clippy pass. A decoded identity remains a claim, not Host/source admission. Durable
overlay storage, actual effective closure capture, source repair/history and conversion remain
open; the new pre-recursion declaration design is Proposed, not production qualification.

The [nested repair baseline](../../knowledge/engineering/verification/2026-10/2026-10-26T103859Z-nested-instance-repair-baseline-and-effective-source-demand-replacement-design-a6dbd3843c104c40b6ba817e428478f7.md)
adds five public current-expansion regressions in both feature configurations. Discarded sources
are not resolved after exact replacement/detach or complete subtree removal; stale full
declarations reject without publication. This does not qualify actual Host admission. The
Proposed successor now separates source preservation from selected activation and retains
ADR 0101 child ordering; those reviewed obligations are not new implemented contracts.

The [declarative production replacement](../prefab-overlay-design.md) is now in progress under
ADRs 0006/0026/0038/0043/0090. It removes patch-as-overlay storage and the private comparison
compiler in favor of complete typed source intent and one Scene compiler. The first integrated
cohorts are superseded by the [shared-owner/Host checkpoint](../../knowledge/engineering/sessions/2026-10/2026-10-26T163304Z-shared-prefab-compiler-owner-and-host-verification-before-staged-structural-demand-replacement-45d427ddb20d40b0acde05d288566775.md):
Reflect passes 291/291; Scene passes 389/390 with one executed structural-demand counterexample;
the Host passes 47 selected cases (434 filtered); no-default Scene library compilation passes.
Shared Schema authority, selected source capture and explicit source repair have this bounded
evidence. The [staged production successor](../../knowledge/engineering/progress/2026-10/2026-10-26T175942Z-staged-prefab-structural-demand-and-selected-value-program-qualification-19ce2446522e4c28a5fbd70d31ab827f.md)
then replaces the interleaved compiler with shell/structural demand, layered layout and selected
value programs; raw Prefab capture no longer requires eager standalone component publication.
Scene 497/497, actual-file Host 52/52, selected independent consumer 18/18 and no-default compilation
pass. Three independent allocation/prepare observations remain ignored; no native/performance
qualification or merge is implied. Complete second-document Editor capture, Host/native workflows,
nested preview fencing, conflict observations, conversion and repeated-descriptor work remain open.
Earlier evidence above retains its original pre-replacement scope; the full foundation Goal is active.

The [independent-capture checkpoint](../../knowledge/engineering/progress/2026-10/2026-10-26T192056Z-independent-authoring-capture-and-preview-dependency-stamp-integration-c31cf6ef3eaa457a87528419c09ea49c.md)
extracts reusable actual-demand acquisition from startup, retains immutable Scene/Prefab working
copies with exact source and Schema receipts, and distinguishes session content from file publication.
Root/metadata/omission retention corrections and dependency-aware preview payloads have 33 selected
root regressions; current Scene passes 507/507 with three existing ignored observations. The
complete root library passes 501/501 with three existing ignored observations, and the Editor
compilation check passes; none is native preview execution. At that checkpoint, the
ordinary Editor still used its direct-only preview helper: async capture scheduling, persistence
receipt refresh, dependency selection invalidation, new Image publication and native input/pixel
authorization remain unqualified. This checkpoint does not close the remaining workflow above.

The [Editor capture qualification](../../knowledge/engineering/verification/2026-10/2026-10-26T221436Z-captured-editor-scene-preview-and-ordered-image-publication-qualification-bd36086e52524bb1b227dbd21da81056.md)
connects actual-demand working copies, background composition and revalidation, persistence receipts,
captured asset databases and complete native content stamps. ADR 0047 adopts the preview policy;
0115 clarifies ordered Image evidence reconciliation. Final root/Editor libraries pass 754 cases;
Scene/Tooling/UI pass 677; the combined focused/native cohort passes 84, including six actual
Windows/DX12 trajectories. Independent consumers pass 18 and the selected Reference Game cohort
passes 13. Workspace all-feature/all-target compilation and the public overlay example pass.
Prefab geometry selects its containing anchor; Image changes require new captured evidence.
These are overlapping scoped cohorts, not full foundation or all-platform qualification.
Conversion, structured conflict repair, inherited Inspector selection, the direct UI Canvas path
and repeated Schema descriptor work remain open; this checkpoint does not merge the branch.

The [source-intent review/Revert successor](../../knowledge/engineering/verification/2026-10/2026-10-27T012127Z-source-owned-prefab-intent-review-and-revert-qualification-4b14705bceb545e79f34b7afbc5144bf.md)
qualifies bounded source declaration observations, exact source-owned Revert, actual failing-source
locations, rejected composition observations and the Inspector's selection-independent Prefab
Intent review/cancel/commit path. ADRs 0026/0038/0047/0090 adopt that narrow repair; 0011/0038
separate explicit owner value limits from selector admission. Reflect/Scene/Tooling pass 912 cases,
root/Editor libraries pass 770, independent consumers pass 19 and the selected Reference Game
passes 13. Six selected actual Windows/DX12 trajectories, minimal Scene and workspace target
checks pass. The cohorts overlap. Inspector mouse controls have owned ImGui-host evidence, not
the complete native nested Revert/history/persistence chain. Canonical foreign-source navigation,
convert-to-local, cross-source Apply, shared Canvas capture, inherited Inspector selection and
aggregate repeated Schema/value work remain open. Earlier checkpoints keep their original scope;
this successor does not complete R5 or the full foundation Goal.

The [complete-conversion comparison](../../plans/2026-10-27-prefab-conversion-plan.md) now has
[three executed source-constraint probes](../../knowledge/engineering/progress/2026-10/2026-10-27T014146Z-complete-prefab-conversion-source-constraints-and-comparison-plan-48bc7f5499704cd39c2f617a29916625.md):
semantic/native component retention differs, unavailable records already have omission facts,
and an unrelated missing instance rejects whole-document expansion after a healthy-anchor
control. Independent review clarified selected-data refusal and unchanged history eviction.
No production conversion, selected-demand entry, replay footprint or checkpoint is implemented
by those tests. ADR amendments and the complete public/native workflow remain open.

The [private selected-compiler/replay successor](../../knowledge/engineering/verification/2026-10/2026-10-27T022733Z-selected-prefab-compiler-and-scoped-source-replay-private-qualification-c55e3532dbfd4367990424c40afe75c1.md)
adds selected instance demand inside the same compiler and a closed footprint in the existing
source history. Scene all-feature tests pass 557 cases with three skipped, the focused cohort
passes 24 and minimal compilation passes. Static independent reviews found no remaining defect
in those assigned scopes. The history fixtures supply manually localized records; actual local
identity/reference translation, production consumers, contextual dependency fencing and the
Editor conversion workflow are still absent. The private working changes are uncommitted and
have unused-helper warnings until the real source owner connects them; they are not public
conversion support or a native/product qualification.

The [complete source-conversion successor](../../knowledge/engineering/verification/2026-10/2026-10-27T041718Z-complete-source-owned-prefab-conversion-and-current-stable-lookup-qualification-cbce190af0c4488b97895e68905573a6.md)
qualifies `58a3eea` and `989793c`: nominal owned Scene/Prefab background preparation, actual
selected localization/reference translation, non-extractable semantic dependency fencing and
one existing source history entry through review, commit, Undo/Redo and disk Reopen. It also
corrects current Authoring validation to re-resolve used stable queries against a fresh bounded
catalogue, detecting ambiguity from new metadata. Source and Host reviews close their assigned
P2 findings. Scene/Reflect pass 885 cases with three skipped; final affected source tests pass
33, root library tests pass 556 with three skipped, independent consumers pass two, and workspace,
minimal Scene and correctness Clippy checks pass. Cohorts overlap and retain the record's exact
source limits. ADRs 0026/0038/0047/0090 adopt these boundaries. At that checkpoint the Editor
command, contextual ticket, participant/file fence and native interaction remained open.

The [complete Editor successor](../../knowledge/engineering/verification/2026-10/2026-10-27T064545Z-complete-editor-prefab-conversion-and-source-admission-qualification-bc6d938e4ea5421fa42d0028230a11dc.md)
qualifies `bf8e2d6` and `6da9980`: retained nominal conversion, actual file/working-copy fences,
fresh validation after review, bounded human-readable mapping and physical cancellation/Close.
Pre-copy admission now reaches complete candidate classification and retained format buffers;
incremental/full classification agrees under independent traversal and retained-reference quotas.
Source/Tooling/Reflect pass 983 cases, Root/Editor pass 808, final affected/native tests pass 29,
independent consumers pass four and Reference Game passes 88. Cohorts overlap and retain the
record's exact source/feature/platform limits. This closes the selected complete-conversion
workflow, not outer-only unpack, cross-source Apply or the full foundation Goal.

The [R5c custody qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T163942Z-per-document-persistence-custody-qualification-30cc4d7af3a445d18d9adf5b795886e2.md)
implements typed per-document Scene/Prefab file custody under ADRs 0047/0118. Save retains its
admission-time Host through acknowledgement/uncertainty; Reopen validates exact retained Host
identity and reconciles under that Host's borrow. Exact Close and held-cleanup isolation pass.
Final production code passes 831 library cases, 30 independent consumers and four actual
Windows/DX12 trajectories; a later test-only extension passes the final 34-case custody cohort.
Private multi-owner fixtures do not qualify public additive Open. At that source, alias convergence,
multi-Scene preview/navigation, multiple Prefab refresh obligations and Play-source retirement
continuity remained Proposed; the dedicated CE full receipt was unavailable, with bounded native
reviews recorded.

The subsequent [source/preview qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T180456Z-common-source-admission-and-retained-scene-preview-qualification-45ffac37614a4edaa792f08d650f292e.md)
binds `7e68ad2` to 845 passing library cases, 30 independent consumers and seven actual Windows/DX12
trajectories. Common current-target alias exclusion now covers every concrete document lane;
Browser/anchored Prefab exact-source activation preserves its slot/history. Immutable frames carry
the retained last-explicitly-selected Scene, separate from the active Inspector; native receipts,
inactive input exclusion, Back to Scene and Canvas/Close retirement follow that binding. Trusted
Running divergence resolves its exact source document. At that source, additive Open, multi-source
Prefab refresh and cancellation/retirement source retention were unqualified; their separate
successor evidence follows. No second source registry or Runtime per document is added.

The [additive Open qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T202313Z-additive-scene-and-prefab-open-qualification-d0b1fc894dd2419fa1cf43c9e39eb79d.md)
binds `1bbaae5` to 851 passing library cases, 17 persistence integrations, 31 independent consumers
and eight selected actual Windows/DX12 trajectories. Same-kind Browser and anchored Open now add
concrete per-document Hosts without predecessor Close choreography; explicit Close retains its own
Save/Discard/Cancel, cleanup and retry. The synchronous ResultPending Close Save path has a later
passing focused regression. The eight-persona review's P2 coverage finding is addressed without
changing production after review. That evidence does not qualify Play source continuity through
cancellation/retirement; the later dedicated qualification below does. The foundation plan is incomplete.

The [multi-source refresh qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T192721Z-multi-source-prefab-refresh-obligation-qualification-47f6266123094e7bb2ea046f42131843.md)
binds `883bc19` to 850 passing library cases, 39 content integrations, 31 independent consumers
and eight selected actual Windows/DX12 trajectories. ADRs 0047/0113 now retain all source-keyed
Prefab obligations through Close, retry and Runtime retirement, with canonical configured-root
observations and budget-before-copy Loader validation. One successor must satisfy every digest
and closure membership before Current. The completed eight-persona review's P2 recovery-coverage
finding is addressed by four no-requeue recovery combinations and independent follow-up. At that
source, additive Open and exact Play source retention were unimplemented; their later evidence is
separate. This prerequisite alone does not qualify multi-open or complete R5c/the foundation plan.

The [R5c Play-source qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T210921Z-trusted-play-source-continuity-and-source-local-close-qualification-61897d1b17744e2aaff6fc6efb1594a7.md) binds `ebc0d48` to 597 passing library cases,
40 public Play/persistence cases, 21 headless cases, 31 independent consumers and six selected
actual Windows/DX12 trajectories. One exact source survives every real non-Empty trusted owner,
including cancellation, Game activation, physical retirement and incomplete cleanup/retry.
Unrelated Close preserves that owner; associated Close/Exit still waits for actual return.
Runtime Edit/Apply Changes remain Active-only and global Scene Reopen exclusion is unchanged.
Seven Codex review personas found no confirmed defect or unresolved gate; no cross-model
independence is claimed. Durable addressing, projection/history and remaining waves stay open.

The [post-boundary U3 evidence](../../knowledge/engineering/verification/2026-10/2026-10-23T054535Z-post-boundary-u3-capability-write-cursor-windows-qualification-89d81288b77f48d79370b7a9dbf9b089.md)
qualifies ADR 0070's capability-owned write cursor on Windows: mixed reads/seeks cannot reposition
continuing writes, including lock/release and exact-handle name replacement. The combined App/FS
211-test cohort passes, with three pre-existing privileged filesystem cases ignored. This does not
extend earlier Linux lock evidence to the new cursor implementation or qualify performance.

The [post-boundary U4 evidence](../../knowledge/engineering/verification/2026-10/2026-10-23T055642Z-post-boundary-u4-post-flush-persistent-admission-qualification-e1266d181d7043a3b9ce33b60697d53c.md)
qualifies ADR 0081's post-flush support insertion facts while retaining original asset baselines.
Queued first apply, direct/batch conflicts, support hooks and asset interference pass in the
670-test full-feature root/domain cohort. Existing platform/native limits remain unchanged.

The [post-boundary U1/U2 evidence](../../knowledge/engineering/verification/2026-10/2026-10-23T055844Z-post-boundary-u1-u2-lossless-outcomes-and-disposal-close-qualification-3f13b79a544a478b974d062c99662a12.md)
qualifies lossless scope outcomes, typed owned input return, real Game View/Image/edit bookkeeping,
admitted task disposal custody and terminal cleanup-failure propagation through Stop and Host
diagnostics. The full-feature root/domain670, public Runtime/Editor90 and selected three actual
Windows/DX12 native trajectories pass. ADRs 0052/0076/0117 clarify those contracts without claiming
arbitrary unwind recovery, escaped-output cleanup, cross-platform native qualification or performance.

The [post-boundary U5 evidence](../../knowledge/engineering/verification/2026-10/2026-10-23T070352Z-post-boundary-u5-image-owned-prepared-capture-qualification-a532b06bf3d94666bc5f1c2748052ef5.md)
qualifies ADRs 0033/0037's Image-owned immutable prepared association. Independent identity/pixel
pairing and the unused generic prepared store are removed. Image92, render/Sprite/UI/WGPU244,
root Image Host13, direct/renamed-root Image consumer2 and selected Reference Game38 pass, together
with two actual Windows/DX12 Image/Canvas trajectories and a negative constructor doctest. These
results do not qualify U6 recovery continuity or broaden native/performance claims.

The [post-boundary U6 evidence](../../knowledge/engineering/verification/2026-10/2026-10-23T072629Z-post-boundary-u6-bounded-image-recovery-continuity-qualification-1f48883676614cde8d61d8a59e3cef2d.md)
qualifies ADR 0037's bounded Image removal-recovery continuity separately from exact attempt
baselines. Asset/Image/TileSet252, no-default-feature Image78 and selected root Host/Scene37 pass.
Independent review's post-Spawn unavailable-invalidation counterexample is fixed with exact state
revision advancement; foreign consumer authority, external writes, empty-slot ABA and replaced
stores cannot restore eligibility. U7/U8, unmeasured costs and other native platforms remain separate.

The [post-boundary U7 evidence](../../knowledge/engineering/verification/2026-10/2026-10-23T074124Z-post-boundary-u7-borrowed-and-exclusive-scene-consumer-qualification-24bb1ff684fa45c48abb28ebc9aded12.md)
qualifies root-owned borrowed `CurrentScene` and scoped `ProjectScenes` access under ADR 0117.
The root selected37, three rustdoc examples and independent two-level24 pass, including actual
travel/Restart and fresh-process Save/restore. Scheduled/scoped predicates and cancelling-slot
custody remain shared; no independent source lease, implicit flush or reverse Scene dependency is
introduced. U8 and final integration retain their separate gates.

The [post-boundary U8 evidence](../../knowledge/engineering/verification/2026-10/2026-10-23T075151Z-post-boundary-u8-explicit-bevy-ecs-entry-qualification-b54c09bf3ae54a5dbc48b382336ea785.md)
qualifies the single `nara_ecs::bevy_ecs` / `nara::ecs::bevy_ecs` complete dependency entry under
ADRs 0002/0044. ECS/derive23, the negative old-alias doctest, both actual renamed dependency
fixtures and workspace all-feature/all-target compilation pass. Independent review found no
boundary blockers; ordinary preludes, original types, six derives and provenance stay intact.

The [final post-boundary integration and requirement audit](../../knowledge/engineering/verification/2026-10/2026-10-23T075653Z-post-boundary-u1-to-u8-final-integration-and-requirement-audit-c11cf626f4494238bf3d1a2532e40634.md)
closes the approved U1-U8 plan against runtime source `96461f9`: all-feature libraries2002,
public Runtime/Host90, complete independent modules77, selected Reference Game38, two-level24 and
FS contracts37 pass. Five Windows/DX12 native trajectories actually execute, and the current
positive/negative rustdocs, standalone desktop compilation, scoped lint and eight workspace format
checks pass. Existing ignored cases, warnings, performance gaps and other native platforms remain
explicitly unclaimed. Broader partial ADR statuses are not promoted by this bounded completion.

The [low-level integration](../../knowledge/engineering/verification/2026-10/2026-10-22T193724Z-u11-low-level-refactor-public-and-native-consumer-integration-ddb983fa80454657955de17ff65d1772.md) qualifies the bounded U1-U11 owner refactor: phase spatial cuts, retained asset intent,
unique lock/task/close owners, typed codecs, facade derives, World identity operations and exact
Gameplay success. Final owner329, root267/integration78, independent module77, Reference87/desktop6,
five Windows/DX12 Editor trajectories and a real compiled Check/Run/Repair/Export pass. Broader ADRs
remain partial where their own domain/platform scope is unimplemented; spatial source-scan cost and
existing lint warnings are retained in the linked evidence.

The [ADR boundary qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T022246Z-adr-boundary-and-frozen-codec-execution-verification-b210d1cf09c94203b04bffcfa1dbe236.md)
at `05531a0` moves codec execution onto the existing frozen snapshot and makes ECS authoring,
upstream and hidden internal imports explicit. Core607, root/domain529, root integration141,
independent module77 and the selected renamed/game consumers pass; two affected Windows/DX12
Editor trajectories, the all-target workspace check and scoped correctness/suspicious lint pass.
ADRs 0037/0097 narrow unvalidated future mechanisms without changing current ownership guarantees.
The record retains the observed in-process parallel ImGui test failure, later serial acceptance,
existing warnings and the bounded native/platform scope.

The [Tile locality qualification](../../benchmarks/2026-10-20-tile-visibility.md) at `15e6564`
retains complete extraction/material admission while streaming candidates and conservatively
clipping per view under ADR 0117. Source and actual pixel oracles plus three optimized processes
support the bounded correction without cross-frame caches. The [CI portability repair](../../plans/2026-10-20-ci-portability-repair-plan.md)
at `fce2e70` adds Linux handle enumeration, terminal consumer observation and compile-only portable
wasm checks. Local Linux execution is separate from hosted/platform release and browser support.

The [Rust/Play attribution](../../benchmarks/2026-10-20-rust-play.md) at `2c26c92` qualifies an
ordinary public Play/Restart and compiler-failure recovery consumer under ADR 0116. Windows
Cargo input spelling is stabilized after canonicalization; same-parameter ordinary CLI builds
reuse all three project artifacts. Complete before/after observations retain mixed timing
changes and do not select profile/linker changes or a native hot-patch ABI.

The [Browser preparation qualification](../../benchmarks/2026-10-20-browser-preparation.md) at
`7f78c59` implements finite source-index workers and explicit request-specific document Open
under ADRs 0047/0118. Exact predecessor/index admission, dirty-close decisions and physical
Host/task retirement remain with their current owners. Public, focused native and timing
evidence separates reduced owner occupancy from unchanged total filesystem work.

The [scalar preparation locality investigation](../../benchmarks/2026-10-20-scalar-preparation.md)
implements private publication-shape proof with full fallback and field-only classification reuse
at `601b467`. Exact JSON/RON byte limits, schema/default/reference validation, unavailable
preservation and history publication remain with their existing owners. Differential and ordinary
refusal tests support this implementation without changing ADRs 0026/0047/0049/0118 or introducing
asynchronous editing. The report separates measured improvement from remaining whole-document work.

**Published-backend qualification baseline:** 2026-10-17, Dear ImGui 0.21 implementation `6a89a77`.
The [published-backend acceptance](../../knowledge/engineering/verification/2026-10/2026-10-17T135132Z-dear-imgui-0-21-published-backend-acceptance-5cf11eaec8ea42a986d4953c4a4a00b8.md)
covers the selected Windows/DX12 trajectories, registered texture/window mapping and upstream IME
composition. Manual OS candidate-window behavior, native Linux/macOS release support and complete
accessibility are not established by that run. The earlier
[foundation correctness acceptance](../../knowledge/engineering/verification/2026-10/2026-10-17T115334Z-foundation-correctness-closure-u1-u5-verification-4490cc23b14742749a37e7a9cb1c7cbf.md)
covers removal revocation, displayed-frame input, saved-source Restart admission and quiet external
Play maintenance at `4589b5a`. Neither record extends an earlier run's toolkit or platform scope.

The [2026-10-18 foundation audit and focused repairs](../../audits/2026-10-18-foundation-contract-audit.md)
record stable-asset context preservation during advanced Scene replacement and atlas-owner isolation
in the shared glyph cache. Its 264 scoped tests do not replace native Editor product qualification.

The [component/plugin and asset contract evidence](../../knowledge/engineering/verification/2026-10/2026-10-18T164837Z-component-plugin-and-asset-contract-refactor-b4c2497eff5d4bdb90cebe1de995ceb3.md)
records completed U1鈥揢5 within the selected runtime identity/type, direct-App provider closure,
file-backed TileSet extent and render-target migration scope. Owner, root/public-consumer,
workspace/feature-minimal and selected Windows/DX12 native checks pass. The native selection used
Rust 1.99; it does not replace the full Rust 1.95 release qualification or extend platform claims.

The 2026-10-19 foundation corrections add focused Rust 1.95 owner, public-consumer and selected
Windows/DX12 evidence in the affected rows. The [final integration record](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-correction-completion.md)
captures the passing locked workspace check, configured external-consumer lint, native critical
trajectories and completed independent review. U1-U15 are complete within their correction scope.
Release qualification retains its platform limits; G1-G9 are separate follow-ups.

The [S2 authoring verification](../../knowledge/engineering/verification/2026-10/2026-10-19T153834Z-authoring-s2-exact-results-and-retained-field-editing-2b09e865afc64b7990e2adc737fa2aa7.md)
adds exact source-built panel results and the retained default field editor under ADRs 0047/0118.
Scene projects read-only source/schema admission facts; the frontend keeps stable widget identity,
strict revision/conflict recovery and one result recipient. Ordinary owner/application tests,
independently compiled field tools and selected native Windows/DX12 trajectories establish this
scope.

The [S3 component authoring verification](../../knowledge/engineering/verification/2026-10/2026-10-19T162634Z-authoring-s3-component-constraints-diagnostics-and-inventory-64aacf18a3af432584d306af27d32eb3.md)
records finite scalar intervals, safe component-domain diagnostic facts and one explicit inventory
under ADRs 0045/0081, implemented in `64e8484` and `2ca6771`. Evidence includes 537 shared-owner
tests, 210 final Scene tests, focused interval/derive checks, actual strict Check/compiled Editor
domain codes, three independent
component workflows, the renamed root-only facade and locked bundle verification. All 172
Editor/application tests and the two affected Windows/DX12 native field/Canvas trajectories pass.
App plugin-error codes, broader async interfaces and multiple-document ownership remain outside
that scope; the selected native runs do not expand platform or full release qualification.

The separate [N05 App plugin-error follow-up](../../knowledge/engineering/verification/2026-10/2026-10-19T170951Z-n05-safe-plugin-failure-facts-and-invocation-context-d01f7391b84348a8827a0daa760dd1a4.md)
is qualified under ADRs 0010/0068 at `40bcdc6`. It adds
checked static plugin-domain code/summary, framework-owned invocation facts and product lowering
through the existing primary/shutdown reports. Direct App addition and `seal` errors retain
invocation context and Editor publishes terminal shutdown facts once during an incomplete physical close.
All 121 App tests, the selected 133-test product cohort and the locked all-features/all-targets
workspace check pass; independent review has no remaining findings. These separate cohorts do
not change S3's historical evidence or add native platform/release qualification.

The [2026-10-22 workflow integration](../../knowledge/engineering/verification/2026-10/2026-10-22T130424Z-engine-workflow-refactor-final-integration-ac0a299fafd64a6dba6179531a1f5cff.md)
adds bounded evidence for ADRs 0084, 0090, 0117 and 0118. Runtime close distinguishes physical
retirement from final failure and projects current pending participants independently of historical
causes. Ordinary product Hosts own source/bootstrap preparation and retained cleanup, while the
advanced run owners keep embedded event-loop policy. Managed Scene travel retains two independent
native-domain attempts and retries only unfinished work. Safe content failure facts, ordinary
nonmodal input routing, per-instance tint
batching and complete image-demand admission pass their affected owner/consumer tests. Local
EntityRef widgets complete history/Save/Check/Reopen; an independent dialogue owner shares
workspace revisions, Host persistence, dirty-close decisions and panel retirement. Current file
identity excludes cross-namespace/case/hard-link source aliases, including atomic Save and retained
parent-directory changes. The ledger retains earlier platform/release limits and the integration
record distinguishes later corrections from older executed cohorts. Its
[bounded architecture review](../../audits/2026-10-22-post-refactor-ownership-and-ergonomics-review.md)
separates remaining measurements and concrete-consumer triggers from correctness repairs.

Rows retain their own last-qualified source and scope. Some older broad decisions remain partial
or proposed while narrower Accepted ADRs implement particular workflows. Read the relevant Accepted
refinement before treating an older gap as the current product's behavior. Links are selected
implementation and verification entry points, not an exhaustive proof inventory or a requirement
to rerun every historical gate for each change.

Plans describe scope and sequence; execution progress and resumable context belong in engineering
memory when useful. This index does not authorize work or require a new plan, registration, ADR or
verification record for a change that already fits an existing contract. Update affected rows when
their decision or verified coverage changes. Historical unit timelines, review receipts and full
evidence lists remain in Git:

```text
git show 9d50cf9:docs/architecture/adr/implementation-status.md
```

## Schema

| Field | Meaning |
|---|---|
| ADR / Decision | Stable ADR identity and its recorded `proposed`, `accepted`, `superseded` or `rejected` status. |
| Implementation | `not-started`, `partial`, `implemented` or `superseded`, within the recorded scope. |
| Owner | The responsible crate or architecture domain. |
| Code / Verification | Selected existing implementation and test/configuration/evidence entry points. Not-started proposals do not invent anchors. |
| Coverage and remaining gap | Implemented scope and unresolved limits. An implemented row with `-` has no remaining gap in its selected scope. |
| Revisit trigger | The concrete consumer, evidence or platform qualification needed to extend the recorded scope; not an automatic roadmap commitment. |
| Last qualified | The original inspection date and source, not the date this index was edited. |

The [Runtime configuration prerequisite](../../knowledge/engineering/verification/2026-10/2026-10-29T041356Z-admitted-exact-runtime-configuration-and-owner-initialization-qualification-106a7693310949e980e8cda1e52c5548.md)
qualifies immutable admitted runtime-only values under ADR 0035, exact duration records and shared
validation, source-input refusal, final-profile preset invariants and actual initial clock/diagnostic
owners. Final Project 77, root boundary/startup 99, Reference Game 57 and all three selected native
Windows/DX12 critical cases pass. The record distinguishes an earlier 956-case root run from the
final exact-duration rerun and records Clippy/minimal/doctest scope. Source candidates keep real
lineage; no source-free catalog or processed Scene is implemented by this configuration unit.

The [processed Scene and Product-binding successor](../../knowledge/engineering/verification/2026-10/2026-10-29T060803Z-processed-runtime-scene-and-complete-product-binding-qualification-cd202f5ff4514802a3c5f57a32b43b7a.md)
extends ADRs 0006/0007/0038/0122 at `89d9bc8`. The
[Scene-owned format](../../../crates/nara_scene/src/runtime_scene.rs) preserves complete provenance,
explicit effective data and exact definitions through ordinary native/data publication. Complete
Product indices and privately staged bindings replace Runtime's source-database dependency;
actual new values and Scene publish through the existing transaction. Asset134, domain1334,
independent21, final root70, Reference Game57 and three selected native DX12 cases pass. The record
separates earlier pre-correction root997 evidence. Weak binding does not prove membership/residency;
same-base catalog/startup/later acquisition, other Runtime resource codecs and the broader Proposed
0088 remain open. This supplements the corresponding rows' earlier source/runtime-family scope.

The [immutable local-base successor](../../knowledge/engineering/verification/2026-10/2026-10-29T072753Z-immutable-local-runtime-base-and-shared-processed-scene-lifecycle-qualification-140b1b89e14c4130a72ac1c6d8f3de89.md)
at `a3b235f` adds shared source-free executable composition, independently expected strict-v1
catalog capture, one Product/member index, actual Required dependency checks and complete decoded
identity. The same current-Scene owner retains real processed candidates and their original base
through startup/replacement/retirement. Final owner45, independent48, Asset/Scene918, Reference
Game57 and all3 native DX12 critical cases pass; the record retains earlier root1084 scope and
boundary corrections separately. This extends ADRs0006/0007/0035/0122 narrowly. First-party
processed Runtime Host/request entry, automatic native family codecs/residency, empty source-facet
composition, input maps and target build selection remain open. R6/R7 and Proposed0088 are not
complete, and R8/R9 remain unimplemented.

The [actual-output successor](../../knowledge/engineering/verification/2026-10/2026-10-29T094316Z-actual-runtime-decoded-output-charging-and-shared-pixel-retention-qualification-14f023431b4c462dbeb1051f1739662e.md)
adds the required concrete codec cost query and checked actual-output refund under ADR0007.
Implementation is `a049e15`.
Full maximum work/output still precedes decode; successful acquisitions retain actual product
and wrapper costs through the same shared owners. Asset/Image271, Host30 and independent64 pass,
including32 small allocations, a held4MiB output, overlapping work/retention, query failure/panic
and original native Scene/pixel custody. Minimal features and owner/root/independent correctness
Clippy pass. This resolves whole-format retained charging, not first-party residency, TileSet
content/dependency admission or the remaining packaged delivery and rendering work.

The [TileSet and resource-baseline successor](../../knowledge/engineering/verification/2026-10/2026-10-29T114236Z-processed-tileset-ownership-and-exact-scene-resource-baselines-qualification-e5b31550eacc4fd1a3d4b6168fb4df52.md)
at `7247273` adds strict source-independent TileSet descriptors/codecs, actual required Runtime
dependency inventory and one shared Source/Product native publication owner. Complete typed
bindings, actual Image identity/extent, atlas barriers, cell membership and original escaped pixel
custody pass without synthetic source metadata. Scene product resources now require concrete
incarnation/content baselines through native waits and final commit; both real games use bounded
mutation generations. Asset/Image290, Scene781, root926, TileSet/SpriteRender131, independent73,
public11, WGPU146, Reference71, two-level30 and all three selected native DX12 trajectories pass.
The record states exact default skips, feature/platform scope, correctness Clippy and minimal
checks. First-party processed Host/request/stock residency, empty source-facet composition, input
and target delivery remain open. The separate rendering refinement `eb35d7a` amends Proposed0121
and R8/R9 contracts; it implements no rendering graph or custom-drawn Editor UI.

The [first-party Host successor](../../knowledge/engineering/verification/2026-10/2026-10-29T152520Z-first-party-processed-runtime-hosts-and-explicit-source-facets-qualification-c2d2a14cd90243f0a200a287f1ce4d20.md)
at `44470ed` closes that bounded startup/request/stock-residency and explicit source-facet gap.
Exact codec selection precedes callbacks; canonical family/member order preserves shared-budget
agreement. Required dependent Scenes remain flat admitted data, while public travel still needs
a declared root. Passive unpublished mutation baselines, value/state association guards,
pre-admitted catalogue copies and actual Tilemap reference/selection scratch replace the reviewed
failure paths. Earlier qualification retains its own scope; full input/build/package delivery,
third-party native publication and rendering/custom UI are not implied complete.

## Implementation Index

| ADR | Decision | Implementation | Owner | Code | Verification | Coverage and remaining gap | Revisit trigger | Last qualified |
|---|---|---|---|---|---|---|---|---|
| [0001](0001-runtime-workspace-boundaries.md) | accepted | implemented | Workspace and root facade architecture | [Cargo.toml](../../../Cargo.toml); [src/lib.rs](../../../src/lib.rs) | [examples/hello_world.rs](../../../examples/hello_world.rs); [tests/module_consumer_boundary.rs](../../../tests/module_consumer_boundary.rs) | - | - | 2026-08-04 @ authority simplification |
| [0002](0002-use-bevy-ecs-as-ecs-substrate.md) | accepted | implemented | `nara_ecs` | [crates/nara_ecs/Cargo.toml](../../../crates/nara_ecs/Cargo.toml); [crates/nara_ecs/src/lib.rs](../../../crates/nara_ecs/src/lib.rs); [internal support](../../../crates/nara_ecs/src/__private.rs) | [authoring-surface contract](../../../crates/nara_ecs/tests/authoring_surface.rs); [renamed ECS fixture](../../../tests/fixtures/derive-dependencies/renamed-ecs/src/lib.rs); [renamed root fixture](../../../tests/fixtures/derive-dependencies/renamed-root/src/lib.rs); [renamed schedule extension](../../../tests/fixtures/schedule-extension/renamed-root/src/lib.rs); [U7 derive evidence](../../knowledge/engineering/verification/2026-10/2026-10-22T190137Z-u7-facade-aware-bundle-and-systemparam-extraction-f14b220201744f71a90a123e476b1769.md) | Ordinary authoring exports are explicit and independent of Bevy prelude growth. Complete version-coupled substrate is under `bevy_ecs`; generated/internal lifecycle-free transaction support is under hidden `__private`. Selected derives resolve renamed Nara-only dependencies while preserving original Bevy runtime types. This refinement's workspace check and independent renamed ECS/root/schedule fixtures pass. | Upgrades must review exact released expansions and execute both renamed fixtures; broader Bevy compatibility is not promised. | 2026-10-23 @ explicit facade boundary and renamed-consumer verification; prior U7 evidence retained |
| [0003](0003-own-app-plugin-and-schedule-lifecycle.md) | accepted | implemented | `nara_app` and schedule-set-owning domains | [crates/nara_app/src/lib.rs](../../../crates/nara_app/src/lib.rs); [crates/nara_app/src/runtime.rs](../../../crates/nara_app/src/runtime.rs) | [U6/U7 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u6-u7-managed-schedules.md); [schedule registry](../../../crates/nara_app/tests/schedule_registry.rs) | All installed graphs are fallibly initialized after plugin finish and before Ready/Startup. Managed explicit custom-schedule execution rejects built-in/missing labels and unavailable states, retains authority/fault checks and leaves automatic order and frame clocks unchanged. Owner, complete domain and public/renamed-root consumers pass. | - | 2026-10-19 @ `5419b10`; earlier `7830053` |
| [0004](0004-use-bevy-reflect-backed-component-metadata.md) | superseded | superseded | [ADR 0119](0119-schema-first-semantic-metadata-authority.md) | - | - | Original reflection-authority decision is historical; Schema-first authority is now owned by ADR 0119. Existing native behavior was not requalified by this documentation change. | - | 2026-10-23 @ decision supersession only; earlier evidence retained in history |
| [0005](0005-dimension-aware-runtime-with-2d-first-authoring.md) | accepted | partial | 2D/3D spatial and render domains | [crates/nara_transform/src/lib.rs](../../../crates/nara_transform/src/lib.rs); [crates/nara_render/src/lib.rs](../../../crates/nara_render/src/lib.rs) | [crates/nara_transform/src/lib.rs](../../../crates/nara_transform/src/lib.rs); [crates/nara_sprite_render/src/tests.rs](../../../crates/nara_sprite_render/src/tests.rs) | Completed inherited 2D affine snapshots and hierarchy propagation are implemented. No concrete 3D hierarchy/render consumer has proved the additive 3D boundary. Future persistent local-affine and 3D representations remain Proposed, not accepted implementation debt. | A concrete 3D hierarchy/render workflow validates the separate dimension boundary and selects any required schema/source migration under ADR 0097. | 2026-10-23 @ spatial decision-scope refinement; prior U4 evidence retained |
| [0006](0006-scene-and-prefab-data-model.md) | accepted | implemented | `nara_scene`, `nara_reflect`, persistent component providers and product composition | [document](../../../crates/nara_scene/src/document.rs); [shared preflight](../../../crates/nara_scene/src/validation.rs); [data carrier](../../../crates/nara_scene/src/data_components.rs); [publication](../../../crates/nara_scene/src/spawn/publication.rs); [recipe](../../../src/product.rs) | [runtime-family evidence](../../knowledge/engineering/verification/2026-10/2026-10-27T125421Z-production-scene-native-data-runtime-and-managed-authority-qualification-294f613511bb48e49fed958c05a1a469.md); [product selection evidence](../../knowledge/engineering/verification/2026-10/2026-10-27T141025Z-mixed-scene-product-selection-and-content-to-runtime-admission-qualification-7c90588821aa4e8da374958ef2e436e4.md); [public product consumer](../../../tests/product_data_components.rs) | Closed explicit native/data storage, shared aggregate admission, exact managed selection, atomic publication, guarded member access and complete current-live export. Recipe-owned data admissions pass through Check/content capture, startup and replacement; public project sources retain the exact metadata witness. | Actual language-owner adoption remains in the Scene plan/Proposed 0120; pure data is not executable readiness. | 2026-10-27 @ runtime-family and product-selection evidence; original document foundation retained |
| [0007](0007-asset-identity-and-import-pipeline.md) | accepted | implemented | `nara_asset` and concrete asset/content owners | [Product server](../../../crates/nara_asset/src/server.rs); [storage](../../../crates/nara_asset/src/storage.rs); [selected codecs](../../../src/runtime_content/codecs.rs); [base](../../../src/runtime_content.rs) | [Host/source-facet qualification](../../knowledge/engineering/verification/2026-10/2026-10-29T152520Z-first-party-processed-runtime-hosts-and-explicit-source-facets-qualification-c2d2a14cd90243f0a200a287f1ce4d20.md); [binding predecessor](../../knowledge/engineering/verification/2026-10/2026-10-29T060803Z-processed-runtime-scene-and-complete-product-binding-qualification-cd202f5ff4514802a3c5f57a32b43b7a.md) | Checked process-unique complete Product/type bindings remain separate from content and residency. Explicit source facets no longer accompany native storage. The original plan freezes codec definitions/limits and the captured base retains finite Scene data dependencies. Passive unpublished baselines, actual value/state association and pre-admitted catalogue copies preserve failure/cancellation without publishing claims. Raw mint/retype stays private; detached render identities grant no residency. | Source locator correction and complete input/target/package delivery remain ordered work. Actual native-family publication retains its concrete owner. | 2026-10-29 @ `44470ed`; predecessor identity scopes retained |
| [0008](0008-runtime-concurrency-and-task-pools.md) | accepted | implemented | `nara_app`, `nara_tasks`, task-consuming domains | [crates/nara_app/src/lib.rs](../../../crates/nara_app/src/lib.rs); [crates/nara_tasks/src/runtime.rs](../../../crates/nara_tasks/src/runtime.rs) | [crates/nara_tasks/src/tests/execution.rs](../../../crates/nara_tasks/src/tests/execution.rs); [crates/nara_tasks/src/tests/close.rs](../../../crates/nara_tasks/src/tests/close.rs); [U2 return evidence](../../knowledge/engineering/verification/2026-10/2026-10-22T181737Z-u2-task-return-and-physical-running-accounting-ce30f14820ee4350931de3355bb0c955.md) | - | - | 2026-10-23 @ U2 return evidence; prior integration evidence retained |
| [0009](0009-diagnostics-errors-and-logging.md) | accepted | partial | `nara_diagnostic`, diagnostic producers | [crates/nara_diagnostic/src/report.rs](../../../crates/nara_diagnostic/src/report.rs); [crates/nara_diagnostic/src/field.rs](../../../crates/nara_diagnostic/src/field.rs) | [crates/nara_diagnostic/src/contract_tests.rs](../../../crates/nara_diagnostic/src/contract_tests.rs); [crates/nara_project/src/tests.rs](../../../crates/nara_project/src/tests.rs) | Current scene, asset, project, and tooling reports use static summaries and classified fields; remaining CLI/editor/AI consumption and runtime producer bridge convergence are incomplete. | A concrete workflow or safety defect identifies a missing diagnostics consumer or producer bridge. | 2026-07-10 @ 6a70847 |
| [0010](0010-plugin-lifecycle-dependencies-and-failure.md) | accepted | partial | `nara_app`, product start ownership | [plugin lifecycle](../../../crates/nara_app/src/plugin.rs); [resolution](../../../crates/nara_app/src/plugin/resolve.rs); [domain facts](../../../crates/nara_app/src/plugin/domain_error.rs); [product lowering](../../../src/project_host/runtime_diagnostics/plugin.rs) | [N05 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19T170951Z-n05-safe-plugin-failure-facts-and-invocation-context-d01f7391b84348a8827a0daa760dd1a4.md); [App domain tests](../../../crates/nara_app/tests/plugin_domain_errors.rs); [recipe runtime tests](../../../tests/project_runtime_boot.rs); [Editor Play tests](../../../tests/workspace_play_runtime.rs) | Static declarations, pure closure, repeatable preparation/closed commit, explicit shutdown obligations, sealed Apps and sticky mutation guards remain implemented. Checked plugin-domain code/summary replaces factory-only unchecked codes and also serves hooks. Direct App addition/seal and retained product owners preserve actual invocation and primary/shutdown facts, including retryable first preflight. Product lowering preserves safe identity; Editor publishes shutdown failures once while physical close is pending. Focused owner/product tests and the all-features/all-targets workspace check pass. Final desktop/native integration and future admitted replacement contracts remain incomplete in this broader decision scope. | Desktop/native lifecycle parity or a concrete replacement workflow supplies the missing evidence without weakening closed commit or retained-owner cleanup. | 2026-10-20 @ `40bcdc6` and N05 evidence; earlier `5ddbf18` retained |
| [0011](0011-component-schema-ids-and-migrations.md) | accepted | implemented | `nara_reflect`, schema-owning domains | [crates/nara_reflect/src/schema.rs](../../../crates/nara_reflect/src/schema.rs); [crates/nara_reflect/src/migration.rs](../../../crates/nara_reflect/src/migration.rs) | [crates/nara_reflect/tests/registry_contract.rs](../../../crates/nara_reflect/tests/registry_contract.rs); [crates/nara_reflect/tests/catalog_format.rs](../../../crates/nara_reflect/tests/catalog_format.rs); [property lineage](../../../crates/nara_reflect/tests/property_lineage.rs); [property resolver](../../../crates/nara_reflect/tests/property_addressing.rs); [R5a verification](../../knowledge/engineering/verification/2026-10/2026-10-24T024954Z-r5a-final-owner-diagnostics-and-workspace-qualification-f58a0ab130cb47a8bc2829e32f1d0a86.md) | Root identities, native bindings, freeze and migrations retain their existing evidence. R5a qualifies sibling-scoped Record member IDs, root-relative declaration-path tombstones, explicit keyed-list selectors and exact current-Schema/version resolution. Reflect/Scene passes 532 tests, and four independent Scene/Prefab consumer tests cover migration, reorder, atomic editing, history and persistence. This is Schema-relative addressing, not complete source-document/entity/occurrence identity. | A new accepted Schema shape or identity-evolution requirement reopens this contract. Complete source/document/occurrence addressing is a separate owner obligation, not a missing part of the Schema-relative selector. | 2026-10-24 @ R5a verification; earlier f91e2e0 evidence retained |
| [0012](0012-render-crate-boundaries.md) | accepted | implemented | Render authoring, extraction, and backend crates | [crates/nara_render/src/lib.rs](../../../crates/nara_render/src/lib.rs); [crates/nara_sprite_render/src/extract.rs](../../../crates/nara_sprite_render/src/extract.rs) | [crates/nara_render/src/lib.rs](../../../crates/nara_render/src/lib.rs); [crates/nara_sprite_render/src/tests.rs](../../../crates/nara_sprite_render/src/tests.rs) | - | - | 2026-07-13 @ e276402 |
| [0013](0013-platform-window-and-runner-boundaries.md) | accepted | implemented | `nara_app`, `nara_window`, platform adapters | [crates/nara_app/src/lib.rs](../../../crates/nara_app/src/lib.rs); [native mode lowering](../../../crates/nara_winit/src/window_mode.rs) | [runner tests](../../../crates/nara_winit/src/tests.rs); [actual Windows mode tests](../../../crates/nara_winit/src/window_mode/native_tests.rs); [boundary qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T044452Z-module-goal-alignment-and-concrete-boundary-repairs-qualification-f7676d2222234fd39b8b565dbcaf996f.md) | Startup applies windowed/borderless/exclusive mode; exclusive selects the exact configured extent or explicitly fails before creation. 53 ordinary Winit and 4 Windows native mode/retirement tests pass. | Dynamic mode switching, monitor-selection UX and native Linux/macOS mode qualification are outside this evidence. | 2026-10-20 @ `d718412` |
| [0014](0014-testing-ci-and-compatibility-policy.md) | accepted | partial | Repository verification and CI | [.github/workflows/ci.yml](../../../.github/workflows/ci.yml); [.github/workflows/reference-game-candidate.yml](../../../.github/workflows/reference-game-candidate.yml) | [tests/ci_policy.rs](../../../tests/ci_policy.rs); [tests/product_capabilities.rs](../../../tests/product_capabilities.rs) | Formatting, all-target/feature/example checks, complete suites, direct architecture review and three-workspace CI exist. The recorded hosted Windows/Linux matrix and checkout-free candidate run are complete. ADR 0099 retires prose parsers, collector/ingest/approval protocols and custom release machinery; candidate packaging/product smoke remain. Recorded Redirects retain decision-local evidence rather than treating static/smoke proof as author-facing measurement. | Author-facing product measurements are required before publication is reconsidered; retired collector/delivery protocols need not be restored. | 2026-08-04 @ authority simplification |
| [0015](0015-editor-tooling-and-dogfooding-boundary.md) | accepted | partial | `nara_tooling`, editor UI adapters, and the private Editor product | [src/project_host/runtime/editor.rs](../../../src/project_host/runtime/editor.rs) | [tests/scene_inspector.rs](../../../tests/scene_inspector.rs); [tests/scene_play_mode.rs](../../../tests/scene_play_mode.rs) | UI-agnostic tooling, Host-owned Play/persistence, one immutable frame, one ordered toolkit-neutral action ingress, revision-bound Dear ImGui drafts and a private dockable native Editor Host, bounded Scene Composition, a runtime-rendered edit-world Scene View, exact picking, one complete startup-Scene authoring/check/run/export workflow, and one embedded in-process Game View are implemented. The selected frontend adapter still owns no document, selection, persistence, Runtime, or GPU authority. | Native platform, accessibility/IME, live-edit or Nara UI dogfood expansion needs its own observed workflow; ADR 0116 owns supervised Play and ADR 0118 owns the selected frontend. | 2026-08-17 @ `b5697f3` |
| [0016](0016-extension-seams-for-backends-and-domain-modules.md) | accepted | partial | Domain plugins and native/backend owners | [crates/nara_render_wgpu/src/backend.rs](../../../crates/nara_render_wgpu/src/backend.rs); [crates/nara_asset_watch/src/backend.rs](../../../crates/nara_asset_watch/src/backend.rs) | [crates/nara_render_wgpu/src/backend.rs](../../../crates/nara_render_wgpu/src/backend.rs); [crates/nara_asset_watch/src/tests.rs](../../../crates/nara_asset_watch/src/tests.rs) | Render, watcher, and task owners prove several isolation/lifecycle shapes. One clean-room schema-owning specialized plugin and its ordinary product contribution remain unproven; no Nara-owned cross-implementation domain Interface is currently admitted or required. | A named workflow admits a bounded concrete integration or placement experiment. Portable compatibility freeze requires demonstrated semantic variation through two implementations or one implementation plus a real portability consumer; fakes alone do not prove replaceability. | 2026-07-20 @ ADR 0095 refinement |
| [0017](0017-render-graph-policy.md) | accepted | partial | `nara_render` and render domains | [crates/nara_render/src/lib.rs](../../../crates/nara_render/src/lib.rs); [crates/nara_render_wgpu/src/host.rs](../../../crates/nara_render_wgpu/src/host.rs) | [crates/nara_render/src/lib.rs](../../../crates/nara_render/src/lib.rs); [crates/nara_sprite_render/src/tests.rs](../../../crates/nara_sprite_render/src/tests.rs) | Explicit views and targets remain, while ADR 0105 refines Phase 1 ordering to a closed engine-owned code path. ADR 0111 resolves the first offscreen/cross-target workflow with one private serialized fixed edge; it does not justify a logical resource scheduler or graph. | A later computed dependency or intermediate/history lifetime that exceeds both the closed plan and ADR 0111's fixed edge requires a new focused alternatives decision. | 2026-08-16 @ `1f97c48` |
| [0018](0018-coordinate-units-and-time.md) | accepted | partial | `nara_transform`, `nara_render`, `nara_app` | [crates/nara_transform/src/lib.rs](../../../crates/nara_transform/src/lib.rs); [crates/nara_render/src/lib.rs](../../../crates/nara_render/src/lib.rs) | [crates/nara_transform/src/lib.rs](../../../crates/nara_transform/src/lib.rs); [crates/nara_render/src/lib.rs](../../../crates/nara_render/src/lib.rs) | 2D transform, camera and fixed-time conventions exist. Physics and 3D consumers have not validated cross-domain unit/axis interoperability; no post-affine representation is required by the narrowed ADR 0097. | The first physics and 3D slices validate fixed-step world units, coordinate conversion and their admitted spatial representation. | 2026-10-23 @ spatial decision-scope refinement |
| [0019](0019-physics-strategy.md) | superseded | superseded | Historical physics abstraction | - | - | Replaced by ADR 0095. | - | 2026-07-20 @ ADR 0095 |
| [0020](0020-project-layout-and-package-format.md) | accepted | partial | `nara_project`, build/export hosts | [crates/nara_project/src/manifest.rs](../../../crates/nara_project/src/manifest.rs); [crates/nara_project/src/path.rs](../../../crates/nara_project/src/path.rs) | [crates/nara_project/src/tests.rs](../../../crates/nara_project/src/tests.rs); [crates/nara_project/tests/fixtures/complete_v1.toml](../../../crates/nara_project/tests/fixtures/complete_v1.toml) | Authorized project-manifest discovery/consumption and startup scene/prefab/image content boot are implemented. Ordinary Cargo application discovery, export layout, cooking, package catalogs, and checkout-free runtime consumption remain incomplete. | A concrete executable or packaged-project workflow satisfies the selected scope under ADR 0086/0088. | 2026-07-18 @ f341255 |
| [0021](0021-scripting-and-wasm-boundary.md) | superseded | superseded | Historical scripting boundary | - | - | Replaced by ADR 0093. | - | 2026-07-13 @ ADR 0093 |
| [0022](0022-3d-coordinate-system.md) | accepted | not-started | Future 3D transform, render, physics, and importer domains | - | - | No concrete 3D render, importer or physics consumer validates the selected coordinate conventions. Transform3d field encoding and local-affine representation remain Proposed candidates rather than accepted schema obligations. | A focused 3D hierarchy/render/import workflow proves basis, winding, depth and explicit 2D interoperability; any new representation requires its own accepted decision. | 2026-10-23 @ coordinate and representation scope refinement |
| [0023](0023-event-message-and-command-model.md) | accepted | partial | `nara_app` and communicating runtime domains | [crates/nara_gameplay/src/queue.rs](../../../crates/nara_gameplay/src/queue.rs); [crates/nara_asset/src/reload.rs](../../../crates/nara_asset/src/reload.rs) | [crates/nara_gameplay/src/lib.rs](../../../crates/nara_gameplay/src/lib.rs); [crates/nara_asset/src/lib.rs](../../../crates/nara_asset/src/lib.rs) | Typed gameplay, asset, and authoring queues plus a real semantic-command headless consumer now exist, but no reusable event/request abstraction or complete domain-wide lifetime classification exists. | A second shared channel-metadata consumer motivates OQ-013; desktop input preserves the gameplay communication boundary. | 2026-07-19 @ db511a7 |
| [0024](0024-determinism-fixed-update-and-replay-policy.md) | accepted | partial | `nara_app`, `nara_gameplay`, `nara_tasks`; independent motion/PCM fixture | [crates/nara_app/src/lib.rs](../../../crates/nara_app/src/lib.rs); [crates/nara_gameplay/src/queue.rs](../../../crates/nara_gameplay/src/queue.rs); [bounded consumer](../../../tests/fixtures/motion-execution/consumer/src/prediction.rs) | [crates/nara_app/src/lib.rs](../../../crates/nara_app/src/lib.rs); [crates/nara_gameplay/src/lib.rs](../../../crates/nara_gameplay/src/lib.rs); [prediction controls](../../../tests/fixtures/motion-execution/consumer/src/prediction/tests.rs) | Per-tick fixed clocks, bounded command admission, ordered async integration and a deterministic wave remain implemented. The independent 32-logical-tick model restores complete persistent solver state and selected World state in the same Runtime, rejects stale/foreign attempts and confirmed-prefix replacement, and defers sample-clock PCM effects until exact confirmation. App clocks are not rolled back. Canonical seeded RNG, general replay/checkpoint, physical audio devices and machine-level floating-point parity remain future scope. | R4c retains remaining complete-consumer gates. Broader R10 collision/affine authoring, arbitrary Rust/native recovery and cross-platform simulation need their own evidence. | 2026-11-04 bounded consumer qualification; fixed runtime evidence 2026-07-19 @ db511a7 |
| [0025](0025-runtime-ui-system.md) | accepted | partial | `nara_ui`, `nara_ui_kernel`, `nara_ui_widgets`, `nara_ui_render`, and input adapters | [crates/nara_ui/src/authoring.rs](../../../crates/nara_ui/src/authoring.rs); [crates/nara_ui/src/codec.rs](../../../crates/nara_ui/src/codec.rs) | [crates/nara_ui/src/adapter_tests.rs](../../../crates/nara_ui/src/adapter_tests.rs); [crates/nara_ui_kernel/tests/compile_order.rs](../../../crates/nara_ui_kernel/tests/compile_order.rs) | Bounded text, structured Scene/Prefab UI identity and hierarchy order produce immutable per-surface frames, actions and resource-complete image/glyph packets. Presented session/behavior/input authority, atomic dispatch, root-local last-good fallback and pause-safe cursor delivery are implemented. Host-owned slot/target uniqueness, suspension, stale-attempt cancellation and balanced accessibility endpoint generations are covered. Editor convergence, advanced layout, editable text and virtualization remain trigger-gated. | A concrete Editor, advanced layout, editable-text or virtualization workflow needs the missing capability; the bounded Runtime UI product baseline is already recorded. | 2026-08-10 @ `d28efcf` |
| [0026](0026-editor-command-patch-and-undo-model.md) | accepted | partial | `nara_scene`, `nara_reflect`, `nara_tooling` | [crates/nara_scene/src/patch.rs](../../../crates/nara_scene/src/patch.rs); [crates/nara_scene/src/authoring.rs](../../../crates/nara_scene/src/authoring.rs) | [tests/scene_authoring_session.rs](../../../tests/scene_authoring_session.rs); [tests/scene_patch_transactions.rs](../../../tests/scene_patch_transactions.rs); [coupled-field regressions](../../../crates/nara_scene/src/tests.rs); [R5c projection qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T225800Z-disposable-scene-projection-epochs-and-stable-native-preview-qualification-2478d897135549daacbc8d1fbc90b9d9.md); [property edit limits](../../../crates/nara_scene/tests/property_edit_limits.rs); [R5a verification](../../knowledge/engineering/verification/2026-10/2026-10-24T024954Z-r5a-final-owner-diagnostics-and-workspace-qualification-f58a0ab130cb47a8bc2829e32f1d0a86.md); [source-owned creation qualification](../../knowledge/engineering/verification/2026-10/2026-10-24T203944Z-source-owned-entity-creation-and-generated-product-qualification-c5fd5454223046e38fc1761338ddffb0.md); [source conversion and lookup qualification](../../knowledge/engineering/verification/2026-10/2026-10-27T041718Z-complete-source-owned-prefab-conversion-and-current-stable-lookup-qualification-cbce190af0c4488b97895e68905573a6.md); [Editor conversion qualification](../../knowledge/engineering/verification/2026-10/2026-10-27T064545Z-complete-editor-prefab-conversion-and-source-admission-qualification-bc6d938e4ea5421fa42d0028230a11dc.md) | Atomic one-shot patches retain per-operation structural/schema/budget checks and validate native business invariants on the final candidate. Exact preconditions, failure-atomic inverse publication, branch-safe content-state identity and bounded combined Undo/Redo retention remain implemented. Source-local EntityRef assignment, clearing and nested/optional fields now use ordinary revision/history transactions; cross-source references and broader Prefab closure remain outside that slice. Owned prepared source edits now share the exact candidate, inverse and admitted cost with direct apply; complete forward/inverse replay limits precede publication. A real continuous gesture still needs its own qualified workflow. Disposable integrated preview now uses a Scene-owned epoch without another authoring history; ordinary same-domain authoring sync remains. The joined public field/history/structural/Save/Reopen/picking/Close consumer passes. R5a ordered property replacements resolve against one evolving private candidate, bound raw operation/selector/value work before root coalescing, and reuse the owner-created inverse and existing Scene/Prefab history. Indexed failure diagnostics omit private keys and values; source and external Application consumers pass; registered-panel preparation remains separate R2 work. R2 ordinary source-owned creation now owns generated IDs, validation and one history publication, returning exact identity/report without an exposed reservation; native registry/database and Published paths retain distinct admission. Complete conversion now uses immutable owned requests, fixed typed remaps, semantic dependency-fenced tokens and the existing exact history footprint. Public and independent Scene/Prefab review, commit, replay and disk Reopen pass. Complete Editor conversion now retains the exact nominal token through actual dependency validation and one source history publication. Complete candidate and format-buffer copies are admitted before allocation. | A real continuous Inspector/gizmo/text gesture reopens preview and exactly-one-undo semantics; cross-source/Prefab reference operations need their own closure proof. | 2026-10-27 @ `bf8e2d6` and `6da9980`; earlier scoped evidence retained |
| [0027](0027-save-game-and-runtime-persistence.md) | accepted | partial | Game-owned checkpoint consumer; future shared persistence domain | [sample save owner](../../../samples/two-level-game/src/save/mod.rs); [travel and restore](../../../samples/two-level-game/src/gameplay/travel.rs) | [consumer qualification](../../knowledge/engineering/verification/2026-10/2026-10-21T025657Z-two-level-checkpoint-consumer-and-complete-frame-headless-qualification-72c5ef2705f0461e90916bb60e1bb345.md) | Versioned bounded DTOs persist logical level instances, pickup/deletion facts, progress and dynamic marker IDs. Precommit validation preserves the current game on rejection. Cooperative file publication covers conflicts, uncertain readback and retained temporary cleanup. Actual Windows save and fresh-process restore pass, including the release export outside the checkout. | A shared save framework, arbitrary-tick snapshots and native physics/audio reconstruction require their own consumers. Linux failed-candidate cleanup remains explicitly unsupported by the current exact-identity filesystem adapter. | 2026-10-21 @ first checkpoint consumer |
| [0028](0028-networking-and-replication-scope.md) | accepted | partial | Future networking domain plus identity/schema/gameplay owners | [crates/nara_identity/src/domain.rs](../../../crates/nara_identity/src/domain.rs); [crates/nara_reflect/src/schema.rs](../../../crates/nara_reflect/src/schema.rs) | [crates/nara_identity/src/tests.rs](../../../crates/nara_identity/src/tests.rs); [crates/nara_reflect/tests/registry_contract.rs](../../../crates/nara_reflect/tests/registry_contract.rs) | Stable identity, schemas, and authoritative commands provide readiness, but no replication eligibility, authority model, snapshot/delta format, or transport adapter exists. | A concrete multiplayer/replication workflow admits its record, authority, interest, and transport contracts without changing core ECS identity. | 2026-07-13 @ e276402 |
| [0029](0029-animation-strategy.md) | accepted | not-started | Future concrete animation plugin | - | - | No animation consumer, plugin-owned clip/controller/target model, stable persistent target, or evaluation schedule exists. Generic component-field binding, graphs, and one universal clip type remain unadmitted candidates. | A real sprite or other animation workflow selects the first complete plugin-owned contract while proving stable persistent identity and transient evaluation ownership; later domains need not share its API. | 2026-07-20 @ ADR 0095 refinement |
| [0030](0030-audio-strategy.md) | superseded | superseded | Historical audio abstraction | - | - | Replaced by ADR 0095. | - | 2026-07-20 @ ADR 0095 |
| [0031](0031-text-and-font-strategy.md) | accepted | partial | `nara_text`, UI compiler/widgets/planner and WGPU | [font engine](../../../crates/nara_text/src/engine.rs); [editing](../../../crates/nara_text/src/editing.rs); [default settings](../../../src/product/default_settings.rs) | [editing tests](../../../crates/nara_text/tests/editing.rs); [fallback/collection tests](../../../crates/nara_text/tests/fonts.rs); [independent field](../../../tests/fixtures/ui-module-consumer/tests/editable_text.rs); [U12 profile-default evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u12-profile-defaults.md) | Finite ordered caller-selected faces preserve actual shaping/raster identity, default variation instances and immutable collection-face selection. Bounded Parley/ICU local editing adds grapheme movement/deletion, bidi geometry, preedit restoration and local history with atomic rejection. Actual GPU fallback masks and independent field compilation pass. Earlier ProductRecipe/default-profile, compiled Check, Export and native selected-face evidence retains its original scope. | Complete Nara Inspector, OS IME delivery/candidate placement, project font assets, custom variable axes, localization and world text remain separate. | 2026-11-04 @ bounded text prerequisite; U12 `ff64897` and native/export `39fba8cf` retained |
| [0032](0032-render-backend-integration-boundary.md) | accepted | partial | platform/render adapters | [crates/nara_window/src/backend.rs](../../../crates/nara_window/src/backend.rs); [crates/nara_render_wgpu/src/host.rs](../../../crates/nara_render_wgpu/src/host.rs) | [tests/window_surface_retirement.rs](../../../tests/window_surface_retirement.rs); [crates/nara_render_wgpu/src/host/imgui/tests.rs](../../../crates/nara_render_wgpu/src/host/imgui/tests.rs) | Safe owning surfaces, scoped main-thread retirement, resize, generations and at-most-once acquire/submit/present are implemented. Resource-complete packets avoid live World/asset reads. ADR 0117 admits bounded serial views/offscreen sequencing; ADR 0118 adds concurrent Editor views, detach/reopen and device recovery on Windows/DX12. Arbitrary target coordination and native Linux/macOS release qualification remain outside the evidence. | Measure packet/workload pressure before widening coordination; qualify each additional native platform with its actual lifecycle and recovery trajectory. | 2026-10-17 @ foundation `ca3d803`, selected Editor `f4ffdd18` |
| [0033](0033-asset-import-and-render-resource-preparation-seam.md) | accepted | partial | `nara_asset`, `nara_image`, `nara_tilemap`, content residency and render backends | [Image content](../../../crates/nara_image/src/asset.rs); [TileSet codec](../../../crates/nara_tilemap/src/runtime.rs); [publication](../../../crates/nara_tilemap/src/content/publication.rs); [Host residency](../../../src/project_host/runtime/content_residency.rs) | [Host/residency qualification](../../knowledge/engineering/verification/2026-10/2026-10-29T152520Z-first-party-processed-runtime-hosts-and-explicit-source-facets-qualification-c2d2a14cd90243f0a200a287f1ce4d20.md); [TileSet/native predecessor](../../knowledge/engineering/verification/2026-10/2026-10-29T114236Z-processed-tileset-ownership-and-exact-scene-resource-baselines-qualification-e5b31550eacc4fd1a3d4b6168fb4df52.md); [retained-output predecessor](../../knowledge/engineering/verification/2026-10/2026-10-29T094316Z-actual-runtime-decoded-output-charging-and-shared-pixel-retention-qualification-14f023431b4c462dbeb1051f1739662e.md) | Optional Product/provenance stays distinct from actual Image content. Source/processed startup and travel share stock Image/TileSet stores, exact association/content baselines, actual Required dependencies, atlas/cell checks and one publication tail. Native pixels retain the original decoded-output and metadata owners through retirement. Source execution is explicit. Earlier DX12 atlas readback remains its predecessor scope; the successor adds actual stock extraction/preparation and the selected native Editor regressions. | Third-party native publication, durable CAS/import-cache, streaming and complete input/target/package delivery remain open. CPU decode alone grants no native residency. | 2026-10-29 @ `44470ed`; earlier Image/output/TileSet scopes retained |
| [0034](0034-editor-play-mode-world-boundary.md) | accepted | implemented | `nara_tooling`, concrete Editor Host | [crates/nara_tooling/src/play.rs](../../../crates/nara_tooling/src/play.rs); [src/project_host/runtime/editor.rs](../../../src/project_host/runtime/editor.rs) | [R5c Play-source qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T210921Z-trusted-play-source-continuity-and-source-local-close-qualification-61897d1b17744e2aaff6fc6efb1594a7.md); [R5c source/preview qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T180456Z-common-source-admission-and-retained-scene-preview-qualification-45ffac37614a4edaa792f08d650f292e.md); [tests/scene_play_mode.rs](../../../tests/scene_play_mode.rs); [tests/workspace_play_runtime.rs](../../../tests/workspace_play_runtime.rs) | Trusted Play retains exact document/incarnation/revision through every real non-Empty phase, including start reservation, Game activation, cancellation and physical retirement/retry. Current revision observes that source rather than active navigation. The dedicated source-local Close/public/native qualification passes without widening Active-only execution. | Unrelated document Close preserves Play; associated Close/Exit still waits for actual return. The six Windows/DX12 trajectories do not qualify Linux/macOS, monitor scanout or the remaining projection/history redesign. | 2026-10-24 @ Play-source `ebc0d48`; earlier scoped evidence retained |
| [0035](0035-project-manifest-and-runtime-settings-authority.md) | accepted | partial | `nara_project`, facade composition and concrete source/Runtime Hosts | [manifest](../../../crates/nara_project/src/manifest.rs); [effective settings](../../../crates/nara_project/src/effective.rs); [Runtime configuration](../../../crates/nara_project/src/runtime_configuration.rs); [composition](../../../src/project_host/composition.rs) | [Source authority](../../knowledge/engineering/verification/2026-10/2026-10-24T065424Z-project-source-authority-carrier-and-creation-identity-qualification-6cdfe4a580b14b8da78e659010497632.md); [Input source](../../knowledge/engineering/verification/2026-10/2026-10-30T083131Z-immutable-input-definitions-and-global-source-workflow-qualification-f8db4628f79943ac94a90593cef6c736.md); [processed Input](../../knowledge/engineering/verification/2026-10/2026-10-30T100534Z-processed-startup-input-ownership-and-prebuilt-source-free-physical-process-qualification-35c2432f89d04982b10d2be3fa0f9d12.md) | Bounded manifest/profile admission, source identity and shared source-independent Runtime composition retain their linked qualification. Source projection preserves an unfulfilled Input path; strict Runtime projection refuses it. Processed configuration separately admits an optional complete primary Product, round-trips exact values and cannot select code. Actual catalog/recipe admission requires membership, Input plugin and selected typed decoder before startup. Real source Check/Export and prebuilt processed execution have separate evidence. | Remaining semantic capability/default and file-backed specialized-plugin configuration choices retain their concrete consumer triggers. Target-build/package selection and complete Editor/frontend delivery remain separate; no path-to-Product guess or source-authority fallback is admitted. | 2026-10-30 @ explicit source obligation and processed Product configuration; prior source authority scope retained |
| [0036](0036-event-message-and-resource-queue-lifetime.md) | accepted | partial | runtime domains | [source changes](../../../crates/nara_asset/src/reload/source_changes.rs); [reload requests](../../../crates/nara_asset/src/reload/requests.rs); [Input owner](../../../crates/nara_input/src/timeline/owner.rs) | [source-change tests](../../../crates/nara_asset/src/reload/tests_source_changes.rs); [request tests](../../../crates/nara_asset/src/reload/tests_requests.rs); [bounded Input qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T140508Z-bounded-input-ownership-and-total-ui-presentation-qualification-b97643044e4a4f92b892a3539d2ad415.md) | Gameplay, diagnostic, reload and watcher queues retain their recorded bounds and cleanup, including full-rescan recovery. Input now owns finite topology/member/log storage and terminal credits. Projection/observation/action copies keep original records charged until actual completion, including late next-frame copies. Independent mutation queues are removed. | Other producers still need concrete cleanup/diagnostic/replay declarations. The Input slice does not qualify arbitrary queues or all replay formats. | 2026-10-30 @ bounded Input successor; `c54d366` scopes retained |
| [0037](0037-asset-load-request-cache-and-lifetime-policy.md) | accepted | partial | `nara_asset`, concrete asset domains | [crates/nara_asset/src/state.rs](../../../crates/nara_asset/src/state.rs); [crates/nara_asset/src/storage.rs](../../../crates/nara_asset/src/storage.rs); [crates/nara_asset/src/reload/resolution.rs](../../../crates/nara_asset/src/reload/resolution.rs); [crates/nara_image/src/import/publication.rs](../../../crates/nara_image/src/import/publication.rs) | [crates/nara_asset/src/reload/tests_requests.rs](../../../crates/nara_asset/src/reload/tests_requests.rs); [crates/nara_asset/src/reload/tests_resolution.rs](../../../crates/nara_asset/src/reload/tests_resolution.rs); [contract evidence](../../knowledge/engineering/verification/2026-10/2026-10-18T164837Z-component-plugin-and-asset-contract-refactor-b4c2497eff5d4bdb90cebe1de995ceb3.md); [R0 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19T021717Z-image-lifecycle-contract-repairs-95901fe367a149bc90ec922ad0384a73.md); [U5 asset qualification](../../knowledge/engineering/verification/2026-10/2026-10-22T193928Z-u5-asset-association-desired-recipe-and-runtime-owned-watch-rescan-e081ec487db94262b392fc8b7d7ff3ef.md) | Managed value/state association precedes baseline capture; common admission rejects stale success, failure, removal, resource replacement and empty-slot ABA. Image retains desired metadata independently from attempts and last-good pixels, binding retained recipes to publication/slot/metadata observation. Watcher scope loss retains runtime-owned rescan with weak Host receipts. Final combined 329-test cohort passes, including retained relative removal/metadata order across rejection and coalescing; independent Image/Watcher rereviews have no remaining blocker. Identity, concrete residency ownership and retention policy remain separate; no mixed cache modes or generic lease/eviction mechanism are reserved. Broader acquisition/dependency workflows remain unqualified. | Existing public content/native consumers retain their recorded scope. A complete acquisition/dependency/retirement workflow identifies missing concrete owners and snapshot requirements; shared execution additionally requires measured duplication. | 2026-10-23 @ identity/residency/policy scope refinement; U5 evidence retained |
| [0038](0038-scene-prefab-authoring-identity-and-provenance.md) | accepted | partial | `nara_scene`, `nara_tooling`, and projection-aware consumers | [compiled.rs](../../../crates/nara_scene/src/compiled.rs); [prefab.rs](../../../crates/nara_scene/src/prefab.rs); [provenance.rs](../../../crates/nara_scene/src/provenance.rs) | [compiled production qualification](../../knowledge/engineering/verification/2026-11/2026-11-02T071856Z-closed-compiled-scene-production-and-independent-delivery-qualification-91cc04f45ba249f881cec4f9f78bd3d1.md); [root/placement qualification](../../knowledge/engineering/verification/2026-11/2026-11-02T121917Z-explicit-root-binding-and-source-placement-production-qualification-0eada24ff287416bbfe94ccf511d2380.md); [complete extraction consumer](../../knowledge/engineering/verification/2026-11/2026-11-03T115127Z-complete-public-prefab-extraction-and-native-workflow-qualification-c440d6b31843411897951e9ba2221a2e.md) | Canonical source/occurrence identities, closed flat compiled values and provenance, source-owned placements and explicit root binding have qualified production owners. Complete conversion preserves declared references and hidden defaults through exact history and Save/Reopen. Extraction adds typed routed roots, complete correspondence, saved-generation guards, selective publication and CLI-only recovery; independent inheritance/Revert/conversion, strict Check, actual Play and native controls qualify the selected complete unit. | Broader write-back, additional platforms and complete R5 remain open. Keep source/occurrence lowering under ADR0102; this unit does not accept all of ADR0083. Public child-receipt projection and idle native incidence retain explicit evidence limits. | 2026-11-03 @ complete extraction consumer; topology `88fd728`; earlier scoped evidence retained |
| [0039](0039-main-loop-time-pause-and-runtime-state.md) | accepted | partial | `nara_app`, `nara_input` | [crates/nara_app/src/lib.rs](../../../crates/nara_app/src/lib.rs); [crates/nara_input/src/timeline.rs](../../../crates/nara_input/src/timeline.rs) | [crates/nara_app/src/lib.rs](../../../crates/nara_app/src/lib.rs); [crates/nara_input/tests/timeline_runtime.rs](../../../crates/nara_input/tests/timeline_runtime.rs) | Real/virtual/fixed/render clocks, pause, exact Step, managed Winit driving, cleanup, App exit and truthful close are implemented. The normalized timeline routes UI pause-safely, retains action edges and resumes only unconsumed actions. Typed gameplay-state transition queues/stages with OnExit/transition/OnEnter and broader background/redraw policy remain incomplete. | OQ-034 requires a real boot/menu/gameplay/pause/game-over topology before selecting the minimum state and scoped-lifetime surface; a measured desktop background-policy failure separately admits the remaining runner policy. | 2026-08-05 @ `c54d366` |
| [0040](0040-render-resource-lifetime-and-submitter-ownership.md) | accepted | partial | render domains and WGPU Host | [output leases](../../../crates/nara_render_wgpu/src/output.rs); [original prefixes](../../../crates/nara_render_wgpu/src/submission.rs); [Host](../../../crates/nara_render_wgpu/src/host.rs) | [immutable-output qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T121248Z-immutable-render-outputs-owning-producers-and-original-native-prefix-qualification-1b4de7f4305740698abba87b589f9459.md); [history and cleanup qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T210034Z-captured-view-histories-and-original-submission-cleanup-production-qualification-5c061376803446c0bff1a95495ca55c7.md) | Exact outputs, shared byte custody, producer-local retirement and every original prefix retain their separate responsibilities. Stock windows and images share the finite program owner. Captured history publication, late flat cleanup isolation and stable fault/abandonment observation are qualified within the linked scope; callbacks never publish success. | Complete cross-stream/custom composition and measured retention work; browser-local async execution and external replacement authority remain separate. | 2026-11-01 @ captured-history and submitted-cleanup successor; prior evidence retained |
| [0041](0041-input-routing-actions-text-focus-and-accessibility.md) | accepted | partial | Input definition/state, finite ingress, source/Runtime owners and UI adapters | [definitions](../../../crates/nara_input/src/definition.rs); [state](../../../crates/nara_input/src/state.rs); [ingress](../../../crates/nara_input/src/timeline/owner.rs); [Runtime custody](../../../src/runtime_content/input.rs) | [Source qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T083131Z-immutable-input-definitions-and-global-source-workflow-qualification-f8db4628f79943ac94a90593cef6c736.md); [processed qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T100534Z-processed-startup-input-ownership-and-prebuilt-source-free-physical-process-qualification-35c2432f89d04982b10d2be3fa0f9d12.md); [bounded owner/UI qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T140508Z-bounded-input-ownership-and-total-ui-presentation-qualification-b97643044e4a4f92b892a3539d2ad415.md) | Immutable definitions, actual owner-backed bindings and move-only state preserve aggregation, context/rebind lifetimes and pre-Startup source/processed selection. Finite endpoint/pointer/member storage, one physical/synthetic log, M*K contributions and reserved terminal credits now replace the confirmed growth/lost-release mechanisms. Late copies retain original events; real terminal batch tails settle after ordinary refusal. Exact private endpoint claims protect replacements and total UI completion cannot restore cancelled interaction. Independent gameplay, source-free children and selected native paths pass in linked scope. | Seat-scoped maps, gamepad/axis holds, replay, broader priority and complete text/IME remain separate. Raw labels require adapter drain/nonreuse; current generation lookup does not recover missing event origin. | 2026-10-30 @ bounded owner/UI successor; source/processed/U14 scopes retained |
| [0042](0042-runtime-service-and-backend-boundary.md) | accepted | partial | Native/threaded domain owners and runtime lifecycle | [crates/nara_tasks/src/runtime.rs](../../../crates/nara_tasks/src/runtime.rs); [crates/nara_window/src/backend.rs](../../../crates/nara_window/src/backend.rs) | [crates/nara_tasks/src/tests/execution.rs](../../../crates/nara_tasks/src/tests/execution.rs); [crates/nara_tasks/src/tests/close.rs](../../../crates/nara_tasks/src/tests/close.rs) | Typed cross-thread integration, native ownership, target lifecycle, finite truthful close, terminal evidence, and retained incomplete task cleanup are implemented for their concrete owners. A universal generation-scoped service session, common queue/ack/poison model, and second native service are no longer completion requirements. Browser/device recovery and future long-lived owners still need domain-specific lifecycle evidence. | Each future native/threaded owner proves only the stages, affinity, queue/batch, and synchronous or retryable close semantics its implementation actually needs. A shared session Interface requires a separate evidence-backed ADR. | 2026-07-20 @ ADR 0095 refinement |
| [0043](0043-scene-prefab-and-patch-document-migration-policy.md) | accepted | implemented | Scene, Reflect and explicit project-upgrade owners | [Scene format](../../../crates/nara_scene/src/format.rs); [Reflect migration](../../../crates/nara_reflect/src/migration.rs); [authored upgrade](../../../src/project_host/runtime/editor/authored_upgrade.rs) | [format contract](../../../crates/nara_scene/tests/format_contract.rs); [Scene tests](../../../crates/nara_scene/src/tests.rs); [CP1/S2 qualification](../../knowledge/engineering/verification/2026-11/2026-11-01T203621Z-reviewed-authored-schema-upgrades-and-fresh-process-recovery-qualification-cf7c8ecab10c4e21b4cfcff4906805de.md) | Reviewed authored upgrades transform complete declarations and establish explicit saved/history checkpoints; ordinary Open does not execute project conversion. | CP2 extraction and occurrence remapping have a separate representation and qualification boundary. | 2026-11-02; earlier format evidence retained |
| [0044](0044-root-facade-and-prelude-layering-policy.md) | accepted | implemented | Root `nara` facade and `nara_ecs` authoring surface | [src/lib.rs](../../../src/lib.rs); [crates/nara_ecs/src/lib.rs](../../../crates/nara_ecs/src/lib.rs) | [tests/product_capabilities.rs](../../../tests/product_capabilities.rs); [authoring-surface contract](../../../crates/nara_ecs/tests/authoring_surface.rs); [renamed root fixture](../../../tests/fixtures/derive-dependencies/renamed-root/src/lib.rs); [renamed schedule extension](../../../tests/fixtures/schedule-extension/renamed-root/src/lib.rs) | Root and ECS preludes remain explicit selections; complete substrate imports are opt-in and hidden transaction helpers are not ordinary extension APIs. Root-only renamed derives, custom scheduling and complete workspace targets compile and execute against the original Bevy types. | Upgrades must review exact released expansions and execute both renamed fixtures; broader Bevy compatibility is not promised. | 2026-10-23 @ explicit authoring/full-dependency/internal boundary and renamed-consumer verification; prior facade evidence retained |
| [0045](0045-component-schema-capability-metadata.md) | accepted | implemented | `nara_reflect`, `nara_scene`, `nara_tooling` | [numeric intervals](../../../crates/nara_reflect/src/numeric_interval.rs); [effective.rs](../../../crates/nara_reflect/src/effective.rs); [validation.rs](../../../crates/nara_scene/src/validation.rs); [authoring_decode.rs](../../../crates/nara_scene/src/authoring_decode.rs) | [S3 verification](../../knowledge/engineering/verification/2026-10/2026-10-19T162634Z-authoring-s3-component-constraints-diagnostics-and-inventory-64aacf18a3af432584d306af27d32eb3.md); [interval tests](../../../crates/nara_reflect/tests/numeric_intervals.rs); [U10 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u10-closed-vec2.md); [U11 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u11-recursive-inspector.md) | Recursive shapes, bounded defaults, separate reference-work budgets, raw-preserving projection and source-local reference closure remain implemented. S3 adds typed signed/unsigned/finite-float intervals, F32-materialized comparison, schema admission and canonical fingerprints; absent interval metadata preserves previous identity. Shared validation covers values, defaults/templates and native preflight. Independent component workflows and native rejection/correction pass. Vec2 and recursive Inspector evidence retain their earlier scope. | Cross-field and arbitrary native invariants remain owner-specific; a new shape or aggregate effective-work policy needs a concrete consumer. | 2026-10-20 @ S3 verification; U10 `b0648b6` and U11 `7bec88f` retained |
| [0046](0046-plugin-metadata-and-default-plugin-groups.md) | accepted | partial | `nara_app`, `nara_reflect` and product/plugin recipes | [crates/nara_app/src/plugin.rs](../../../crates/nara_app/src/plugin.rs); [crates/nara_app/src/plugin/definition.rs](../../../crates/nara_app/src/plugin/definition.rs); [crates/nara_reflect/src/plugin.rs](../../../crates/nara_reflect/src/plugin.rs) | [crates/nara_app/tests/plugin_composition.rs](../../../crates/nara_app/tests/plugin_composition.rs); [tests/plugin_composition.rs](../../../tests/plugin_composition.rs); [contract evidence](../../knowledge/engineering/verification/2026-10/2026-10-18T164837Z-component-plugin-and-asset-contract-refactor-b4c2497eff5d4bdb90cebe1de995ceb3.md) | Static declarations, configuration identity, groups, closure, schema selection and provenance are implemented. Before direct-App schema authority publication, actual provider receipts must equal the complete installed declarations' provider union, including multiple installation calls. ProductRecipe/SchemaContribution delegation, standalone registries and asset-free/staged codecs remain supported; diagnostic PluginId is not durable ownership. Package distribution, file-backed settings and missing-package recovery remain unproven. | OQ-045 proves the remaining external package lifecycle and file-backed/missing-package workflows without introducing a universal package or provider registry. Cross-provider replacement remains outside this ADR unless separately accepted. | 2026-10-18 @ contract evidence linked above; earlier `56264181873249c18d5477ce2170c8b1` |
| [0047](0047-editor-workspace-and-scene-document-state.md) | accepted | partial | Tooling, Scene authoring and Editor persistence owners | [workspace](../../../crates/nara_tooling/src/workspace.rs); [owned Save](../../../src/project_host/runtime/editor/persistence/background.rs); [source readmission](../../../src/project_host/persistence/reopen.rs); [runtime custody](../../../src/project_host/runtime/editor/persistence/retirement.rs) | [R5c Play-source qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T210921Z-trusted-play-source-continuity-and-source-local-close-qualification-61897d1b17744e2aaff6fc6efb1594a7.md); [R5c additive Open qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T202313Z-additive-scene-and-prefab-open-qualification-d0b1fc894dd2419fa1cf43c9e39eb79d.md); [R5c multi-source refresh qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T192721Z-multi-source-prefab-refresh-obligation-qualification-47f6266123094e7bb2ea046f42131843.md); [R5c source/preview qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T180456Z-common-source-admission-and-retained-scene-preview-qualification-45ffac37614a4edaa792f08d650f292e.md); [ordinary structure qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T051819Z-ordinary-structural-authoring-and-captured-source-reopen-qualification-f9b317e0ceda49a6b37f9000c61af8d7.md); [Host lifetime qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T030524Z-editor-host-lifetime-cleanup-custody-qualification-12c6b025d6c74ed1bf6900c8b0413877.md); [Owned Reopen](../../knowledge/engineering/verification/2026-10/2026-10-20T022302Z-owned-editor-reopen-preparation-and-publication-qualification-557ea95db99b4b8a87829870db1d6b13.md); [Owned Save](../../knowledge/engineering/verification/2026-10/2026-10-20T013307Z-owned-editor-save-custody-native-observation-and-measurements-9828019f2f934061a1773899ce356ffb.md); [R5c projection qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T225800Z-disposable-scene-projection-epochs-and-stable-native-preview-qualification-2478d897135549daacbc8d1fbc90b9d9.md); [direct-member and panel query qualification](../../knowledge/engineering/verification/2026-10/2026-10-26T004155Z-bounded-direct-member-discovery-and-panel-query-workflow-7003450c49204d8eba92ba90db765dc0.md); [source-owned creation qualification](../../knowledge/engineering/verification/2026-10/2026-10-24T203944Z-source-owned-entity-creation-and-generated-product-qualification-c5fd5454223046e38fc1761338ddffb0.md); [source conversion and lookup qualification](../../knowledge/engineering/verification/2026-10/2026-10-27T041718Z-complete-source-owned-prefab-conversion-and-current-stable-lookup-qualification-cbce190af0c4488b97895e68905573a6.md); [Editor conversion qualification](../../knowledge/engineering/verification/2026-10/2026-10-27T064545Z-complete-editor-prefab-conversion-and-source-admission-qualification-bc6d938e4ea5421fa42d0028230a11dc.md); [CP1/S2 qualification](../../knowledge/engineering/verification/2026-11/2026-11-01T203621Z-reviewed-authored-schema-upgrades-and-fresh-process-recovery-qualification-cf7c8ecab10c4e21b4cfcff4906805de.md) | Save captures checkpoint/basis and acknowledges only that revision; later edits remain dirty. Retained original file Hosts survive failed cleanup, abandonment and runtime replacement. Reopen keeps exact revision/source/schema admission and owner-native affinity. Exact observed or authoring-validated uncertain source bytes now restore captured topology after later unsaved moves; external bytes retain exact unavailable preservation. The 20-test persistence cohort and ordinary public/native workflows pass. R2 adds exact non-active direct-entity/component/root pages, bounded selected-root reads, one-level direct-member pages, owner-bound prepared publication, retained transaction-local construction, selection-independent Session/panel query forwarding and a shared close-admission fence. R5c retains typed file lanes by document and admission-time Hosts through Save/Reopen, uncertainty and exact cleanup; public same-kind Open is additive and retains distinct file Hosts. Common source admission now checks current retained targets across Scene/Prefab/extension lanes. Explicit Scene selection owns the retained preview binding; failed publication restores exact prior navigation without rebinding preview. Canonical Prefab-domain observations and source-keyed refresh obligations survive Close; complete successor validation preserves last-good on failure. Exact Play-source continuity now keeps unrelated Close source-local through cancellation, Game handoff and physical retirement/retry. Requested and accepted preview source/revision/omissions remain distinct across Scene epochs. Clear preserves shared authoring custody; physically held Image import and Save continue through repeated publications. Ordinary creation now uses the source result, with no Workspace ordinal, candidate-ID helper, success increment or Reopen reset; failed creation preserves selection and saved Reopen retains the actual stored ID. Authoring source validation now retains used stable-query observations and resolves a fresh bounded catalogue, detecting new metadata ambiguity. Source conversion tokens have separate qualification; contextual Editor custody remains open. The integrated Host now retains one reviewed conversion operation with actual file/working-copy participants, fresh post-review physical validation, correlated terminal results and physical cancellation/Close custody. Complete-set Schema upgrade adoption binds exact captured persistence predecessors, source/history checkpoints and lane observations before final Ready. | Public two-Scene/two-Prefab non-active editing, Save/Reopen and exact Close now pass; External merge and broader source mutation remain unqualified; Play-source continuity has its separate public/native evidence. Independent extension lanes are qualified separately by the 2026-10-22 workflow integration. No second saved document/manifest or relaxed owner publication is introduced. | 2026-11-02; `bf8e2d6`, `6da9980` and earlier scoped evidence retained |
| [0048](0048-runtime-diagnostics-and-observability-bus.md) | accepted | partial | `nara_diagnostic`, composition | [crates/nara_diagnostic/src/runtime.rs](../../../crates/nara_diagnostic/src/runtime.rs); [crates/nara_diagnostic/src/plugin.rs](../../../crates/nara_diagnostic/src/plugin.rs) | [crates/nara_diagnostic/src/contract_tests.rs](../../../crates/nara_diagnostic/src/contract_tests.rs); [crates/nara_asset_watch/src/tests.rs](../../../crates/nara_asset_watch/src/tests.rs) | The bounded privacy, dedupe, retention, pressure, and headless plugin core is present, and watcher loss bridges to structured diagnostics and pressure. Asset, task, window, render, project, and editor runtime producer bridges remain absent. ADR 0099 makes offline product evidence decision-local rather than part of this runtime bus contract. | A workflow requires a named runtime producer bridge; offline product observations remain decision-local under ADR 0099. | 2026-08-01 @ ADR 0099 refinement |
| [0049](0049-untrusted-project-input-and-parse-budget-policy.md) | accepted | partial | persistent format and concrete external-reader owners | [crates/nara_core/src/limits.rs](../../../crates/nara_core/src/limits.rs); [crates/nara_core/src/serde_shape.rs](../../../crates/nara_core/src/serde_shape.rs) | [crates/nara_reflect/tests/catalog_format.rs](../../../crates/nara_reflect/tests/catalog_format.rs); [crates/nara_scene/tests/format_contract.rs](../../../crates/nara_scene/tests/format_contract.rs) | Scene, prefab, patch, schema-catalog, project-manifest, canonical asset metadata, the audited static non-interlaced PNG path, and the aggregate authorized startup closure enforce their implemented byte, shape, domain, and publication budgets. Other codecs and persistent formats, time/journal budgets, and complete ADR 0048 bridges remain incomplete. ADR 0099 removed the unused generic offline evidence ingester. | Each newly admitted external reader or format proves its own byte, shape, path, and publication budget; no reader-independent evidence framework is required. | 2026-08-01 @ ADR 0099 refinement |
| [0050](0050-asset-root-symlink-junction-and-package-trust-policy.md) | accepted | partial | filesystem/platform adapters and file-backed domains | [crates/nara_fs/src/capability.rs](../../../crates/nara_fs/src/capability.rs); [crates/nara_fs/src/platform/windows.rs](../../../crates/nara_fs/src/platform/windows.rs) | [U8 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u8-unix-leaf-admission.md); [filesystem contract](../../../crates/nara_fs/tests/filesystem_contract.rs) | Manifest/content closure and bounded source indexing use host-issued root identity. Unix leaf opens use O_NONBLOCK plus existing no-follow controls, then reject non-regular types on the same handle. Linux FIFO/regular/symlink cases pass in both trust modes using original source via a temporary standalone manifest; this is not root locked-workspace Linux qualification. Windows locked crate coverage also passes. Remaining watcher/cache migrations and broader Host trust bindings remain incomplete. | Rename/persistence workflows admit remaining callers without duplicate platform authority; root locked Linux and other Unix qualification remain separate. | 2026-10-19 @ `eff95c2`; earlier `f341255` retained |
| [0051](0051-persistent-file-envelope-migration-and-golden-fixtures.md) | accepted | partial | persistent format owners | [Core envelope](../../../crates/nara_core/src/format.rs); [Scene format](../../../crates/nara_scene/src/format.rs); [Runtime Image framing](../../../crates/nara_image/src/runtime.rs) | [Core contract](../../../crates/nara_core/tests/format_contract.rs); [Scene contract](../../../crates/nara_scene/tests/format_contract.rs); [Image qualification](../../knowledge/engineering/verification/2026-10/2026-10-29T090832Z-anonymous-and-processed-image-content-with-retained-native-publication-qualification-3e529af056d34a5caa2c12924c0c1014.md) | Canonical source/Schema/asset metadata and processed Scene/catalog kinds retain their owning evidence. Runtime Image v1 uses the same kind/version/engine/generator admission in bounded metadata plus exact raw RGBA; a nonempty binary golden and malformed/unknown/duplicate/positional/length cases pass. No compatibility reader is introduced. | Other remaining formats retain their own concrete workflow triggers. Binary content need not expand into JSON arrays or Base64; source metadata and Runtime pixels retain separate format owners. | 2026-10-29 @ compact Runtime Image qualification; prior envelope scopes retained |
| [0052](0052-task-backpressure-cancellation-and-long-running-diagnostics.md) | accepted | implemented | `nara_tasks`, task-consuming domains | [task runtime](../../../crates/nara_tasks/src/runtime.rs); [asset integration](../../../crates/nara_asset/src/reload.rs) | [cancellation and measurement evidence](../../knowledge/engineering/verification/2026-10/2026-10-19T182252Z-editor-responsiveness-measurements-and-standalone-task-cancellation-0e2521d0010a4d94940d83fa865f3d0c.md); [execution](../../../crates/nara_tasks/src/tests/execution.rs); [close](../../../crates/nara_tasks/src/tests/close.rs); [U2 return evidence](../../knowledge/engineering/verification/2026-10/2026-10-22T181737Z-u2-task-return-and-physical-running-accounting-ce30f14820ee4350931de3355bb0c955.md) | Per-task return observes executor-owned capture and late-result disposal independently of terminal/output lifetime; caught unwinds report CleanupFailed. Physical running count/age remains until final worker-owned disposal. Scene/Browser/Open replace local flags with this observation. Tasks 51 and affected root 24 tests pass, with gated reaper observation separately passing. Caller-thread rejected/coalesced capture or external terminal-result disposal can still block; no nonblocking destructor guarantee is claimed. | G9/O2: an actual measured disposal-latency consumer can justify an owner-policy change; a future Future consumer needs its own execution/cancellation contract. | 2026-10-23 @ U2 return evidence; prior execution evidence retained |
| [0053](0053-visibility-culling-and-tilemap-render-cache.md) | superseded | superseded | Historical render scaling topology | - | - | Replaced by ADR 0096. | - | 2026-07-20 @ ADR 0096 |
| [0054](0054-gpu-upload-budget-and-buffer-allocation-policy.md) | accepted | partial | render backend | `crates/nara_render_wgpu/src/budget.rs`; `crates/nara_render_wgpu/src/backend.rs` | [U4 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u4-instance-budget.md) | Finite cumulative instance reservations cover each Runtime frame and Editor Host transaction, with typed rejection and RAII rollback/commit. The 122-test WGPU cohort, mixed Sprite/UI readback, two native Editor targets and old-completion/new-scope regression pass. Broader upload statistics, reusable staging and measured resource-class overload policies remain open. | A reproducible workload exceeds a measured threshold, justifying the next private mechanism and resource-class-specific overload policy under ADR 0096. | 2026-10-19 @ `964d25c`; initial `0b078a0` |
| [0055](0055-feature-matrix-boundary-checks-and-compatibility-fixtures.md) | accepted | partial | repository CI | [Cargo.toml](../../../Cargo.toml); [.github/workflows/ci.yml](../../../.github/workflows/ci.yml) | [tests/product_capabilities.rs](../../../tests/product_capabilities.rs); [tests/ci_policy.rs](../../../tests/ci_policy.rs) | Local dependency/feature checks and locked root/reference-game/module-consumer workflows compile all targets and run supported complete suites without a custom documentation parser. The recorded six-cell Windows/Linux hosted matrix passed at f098876. Candidate policy retains product/security semantics; invalid refs/repeated attempts fail explicitly. Broader profiles remain unqualified. | Broader capability profiles remain evidence-triggered. | 2026-08-04 @ authority simplification |
| [0056](0056-headless-runtime-and-dedicated-server-readiness.md) | accepted | partial | facade/runtime composition | [src/product.rs](../../../src/product.rs); [examples/headless_server.rs](../../../examples/headless_server.rs) | [src/lib.rs](../../../src/lib.rs); [crates/nara_gameplay/src/lib.rs](../../../crates/nara_gameplay/src/lib.rs) | Bounded threaded work, preserve-debt time, headless diagnostics/pressure, tick-aware commands, compiled ceilings, server isolation and pre-mutation closure are implemented. Sticky faults and Host-owned startup/publication/retirement drive a deterministic movement/combat/wave and stable CLI. Metrics bridges, hosted artifact proof, networking and dedicated-server delivery remain incomplete in this recorded scope. | A delivery or desktop-parity consumer proves the missing scope without reopening lifecycle ownership. | 2026-07-19 @ db511a7 |
| [0057](0057-authoritative-fixed-tick-and-command-ingress.md) | accepted | implemented | `nara_gameplay` | [crates/nara_gameplay/src/command.rs](../../../crates/nara_gameplay/src/command.rs); [crates/nara_gameplay/src/queue.rs](../../../crates/nara_gameplay/src/queue.rs) | [U10 execution and external-consumer evidence](../../knowledge/engineering/verification/2026-10/2026-10-22T193136Z-u10-exact-gameplay-batch-success-and-reporter-identity-380c336814b2450abe14023d43cf6253.md); [tests/runtime_instance.rs](../../../tests/runtime_instance.rs) | Exact generation/tick/reporter execution completion gates Capture and Ack after deferred boundaries. Failure quarantines unacknowledged work without rollback or automatic replay; Stop remains reachable. Owning 138-test cohort and renamed-root 9-test consumer pass. | Crash transactions, persistent replay and panic containment remain outside this contract. | 2026-10-23 @ U10 evidence linked above |
| [0058](0058-stable-runtime-identity-and-entity-references.md) | accepted | partial | `nara_identity`, `nara_scene`, identity consumers | [domain.rs](../../../crates/nara_identity/src/domain.rs); [export.rs](../../../crates/nara_scene/src/export.rs); [epoch proof](../../../crates/nara_identity/src/domain/scene_projection_epoch.rs); [projection owner](../../../crates/nara_scene/src/spawn/projection.rs) | [identity tests](../../../crates/nara_identity/src/tests.rs); [durable_references.rs](../../../crates/nara_reflect/tests/durable_references.rs); [durable_identity.rs](../../../tests/fixtures/module-consumer/tests/durable_identity.rs); [U6/U8 evidence](../../knowledge/engineering/verification/2026-10/2026-10-22T191516Z-u6-typed-codec-and-u8-identity-consumer-qualification-0c76d56bc4b040118283f1024fe2b788.md); [R5c projection qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T225800Z-disposable-scene-projection-epochs-and-stable-native-preview-qualification-2478d897135549daacbc8d1fbc90b9d9.md); [public instance-end qualification](../../knowledge/engineering/verification/2026-10/2026-10-24T031111Z-public-exact-scene-instance-end-and-source-context-design-review-a91caf241959454f8674b9b04eabc141.md); [source/occurrence qualification](../../knowledge/engineering/verification/2026-10/2026-10-24T114901Z-required-scene-sources-and-active-occurrence-ownership-qualification-e249da9a1f404677a72e52457ac990c4.md) | Durable logical records and checked runtime incarnations are separate. Unloaded may reactivate without historical growth; Deleted is terminal. A separately compiled nested explicit/default and required/optional consumer preserves durable values across source publication, spawn, Prefab and patches, while live operations reject unloaded/deleted or old-incarnation targets. Export remains strict and distinguishes residency outcomes. First-target adoption admits Add/Insert observers before marker mutation. Exact retirement prepares through shared domain reads; ordinary World functions own borrowing and short index publication. Identity/Scene 311 and external durable/manual 10 tests pass. Complete-domain disposable Scene epochs prove all predecessor history, logical records and live markers before preparation and final admission. Foreign responsibility rejects; fresh domains retain N+1 claims without continuing-domain eviction or Deleted readmission. The public exact Scene-instance end operation now covers empty and individually emptied activations, stale/foreign receipt refusal and lifecycle admission through the same authoring retirement path. Scene serde287, the independent parse/spawn/end consumer and three rustdoc tests pass; Scene now owns active source/occurrence association in the same exact publication/end cut. Empty instances retain occupancy, limits reject without eviction, and final registration revalidates the exact domain and allocator baseline. | L02/A03 and E05 are corrected and independently reviewed. Independent history budgets and future restore/streaming proofs retain their existing scope. | 2026-10-24 @ active occurrence qualification; public instance-end qualification; projection `207bbcc`; earlier scoped evidence retained |
| [0068](0068-global-resource-budgets-metrics-and-diagnostic-privacy.md) | accepted | partial | `nara_core`, `nara_diagnostic`, runtime producers | [limits](../../../crates/nara_core/src/limits.rs); [diagnostic identities](../../../crates/nara_diagnostic/src/identity.rs); [App source-fact validation](../../../crates/nara_app/src/failure_metadata.rs); [plugin lowering](../../../src/project_host/runtime_diagnostics/plugin.rs) | [N05 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19T170951Z-n05-safe-plugin-failure-facts-and-invocation-context-d01f7391b84348a8827a0daa760dd1a4.md); [diagnostic contracts](../../../crates/nara_diagnostic/src/contract_tests.rs); [recipe privacy tests](../../../tests/project_runtime_boot.rs) | Shared scalar units, bounded runtime privacy and pressure primitives, asset ingress/request ceilings, sticky bounded observation recovery and image-domain RAII admission metrics remain present. N05 shares checked static source-fact validation between App plugin errors and RuntimeFaultDetail without a diagnostic dependency. Product bridges preserve domain code/summary and bounded invocation context while legacy dynamic text, private paths and forged owner claims remain absent from DiagnosticReport Debug/JSON. The U9 oracle stays decision-local. Image faults and other named producers still lack complete ADR 0048 bridges. | A named producer needs a runtime bridge; product observations remain decision-local under ADR 0099. | 2026-10-20 @ `40bcdc6` and N05 evidence; ADR 0099 scope retained |
| [0070](0070-capability-oriented-filesystem-substrate.md) | accepted | partial | `nara_fs` | [crates/nara_fs/src/capability.rs](../../../crates/nara_fs/src/capability.rs); [crates/nara_fs/src/directory_enumeration.rs](../../../crates/nara_fs/src/directory_enumeration.rs) | [U8 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u8-unix-leaf-admission.md); [filesystem contract](../../../crates/nara_fs/tests/filesystem_contract.rs); [path contract](../../../crates/nara_fs/tests/path_contract.rs); [U3 owned-lock evidence](../../knowledge/engineering/verification/2026-10/2026-10-22T192527Z-u3-owned-file-lock-capabilities-on-windows-and-linux-3ffaeab4142942bd9e4761ce4cf19af4.md); [exact unlink/content evidence](../../knowledge/engineering/verification/2026-10/2026-10-28T114302Z-exact-object-file-unlink-and-durable-installed-content-protection-995675b08109422ab0f90b15d6004546.md) | Capability-bound open/identity/temp/replace/sync/lock/digest, finite-tree staging and exact publication have public conformance. Unix FIFO leaf admission now returns a same-handle type rejection without waiting for a peer; focused Linux original-source and locked Windows evidence pass, with the Linux manifest limitation linked. Handle-bound enumeration remains qualified only for Windows x86-64 local NTFS. Bounded source indexing retains lineage and last-good refresh. Explicit same-live-object unlink now has Windows x86-64/1809+/local-NTFS qualification and honest namespace/durability receipts; protected orphan reclamation remains open because generic deletion does not prove unchanged whole-file content. L07 is corrected: owned non-cloneable locks return file/locked custody on failure, borrowed positional readers issue no duplicate release authority, imported/exported native aliases cannot relock. Windows 71, Linux 77 and compile-fail 7 owned-lock tests pass; root persistence and repair consumers pass. Opt-in exact descendant-spelling opens now add bounded Windows normalized-handle-name evidence and Linux native-entry inode association; Windows FS83 and WSL Linux FS89 tests pass. Ordinary lookup remains unchanged; namespace snapshots, casefold-volume deployment and other exact-name adapters are not qualified. | Owned-lock correction is qualified independently on Windows/Linux. Editor generation/native Save and sample save integration remain in U11; rename/persistence and other filesystem/platform claims retain existing triggers. Generic unlink is not installation policy; ReFS/Unix unlink and complete all-stream reclamation remain unqualified. | 2026-10-28 @ exact unlink/content checkpoint; earlier source-specific evidence retained |
| [0076](0076-play-runtime-debug-control-and-observation.md) | accepted | partial | `nara_app`, `nara_tooling`, identity/interpreter/replay domains | [crates/nara_app/src/lib.rs](../../../crates/nara_app/src/lib.rs); [crates/nara_app/src/runtime.rs](../../../crates/nara_app/src/runtime.rs) | [crates/nara_app/src/lib.rs](../../../crates/nara_app/src/lib.rs); [tests/runtime_instance.rs](../../../tests/runtime_instance.rs) | Exact paused fixed-tick stepping, clocks, command capture/acknowledgement, sticky faults, generation-scoped controls, terminal close evidence, stable identity-only observations, local Inspector filtering, Editor command/view control, generation/revision-stamped one-shot runtime edits, and bounded selected Apply Changes are implemented. The accepted Host/runtime topology remains bounded; disclosure policy, schema-aware component observation, remote payloads, execution cursors, system trace, checkpoints, forward replay, native hot patching, and universal Host APIs remain incomplete or unaccepted. | Later observation/debug/replay slices follow their concrete triggers; replacement Host/Runner roles require their own evidence-backed admission. | 2026-07-23 @ 5ebc45e |
| [0077](0077-render-pipeline-recipes-graph-compilation-and-backend-encoding.md) | superseded | superseded | Historical render architecture | - | - | Replaced by ADR 0094. | - | 2026-07-16 @ ADR 0094 |
| [0078](0078-render-host-affinity-webgpu-initialization-and-device-recovery.md) | accepted | partial | `nara_render_wgpu`, platform runners | [device owner](../../../crates/nara_render_wgpu/src/device.rs); [native Host](../../../crates/nara_render_wgpu/src/host.rs); [window authority](../../../crates/nara_window/src/backend.rs) | [owned native Session](../../knowledge/engineering/verification/2026-10/2026-10-31T105632Z-owned-native-desktop-execution-and-post-host-presentation-qualification-4db921de03c94598b2ee20d7e82508c5.md); [image/prefix qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T121248Z-immutable-render-outputs-owning-producers-and-original-native-prefix-qualification-1b4de7f4305740698abba87b589f9459.md) | One coherent device aggregate, runner-owned native Session and CPU capture/result connection replace GPU ECS storage and borrowed execution. The public native image Host shares normal device acquisition and stock drawing with the Editor; independent offscreen consumers, retained original input through loss, immutable outputs, frozen producer prefixes and direct-Drop fail-stop now have scoped evidence. | External Feature/policy and complete output/history integration remain ordered work. Browser agent-local asynchronous initialization and broader recovery claims require their own actual qualification; native synchronous results do not imply browser support. | 2026-10-31 @ owned native/output successors; earlier scoped evidence retained |
| [0079](0079-root-product-capabilities-and-placeholder-domain-retirement.md) | accepted | partial | root facade, project composition | [Cargo.toml](../../../Cargo.toml); [src/lib.rs](../../../src/lib.rs) | [crates/nara_project/src/tests.rs](../../../crates/nara_project/src/tests.rs); [tests/product_capabilities.rs](../../../tests/product_capabilities.rs) | Optional features, layered exports, settings lineage, product/plugin closure, frozen schema input, startup content and the supported hosted matrix are implemented. Content/presentation are separate; compile features provide availability while requested contributions install WGPU/Winit behavior. Ordinary file-backed Hosts use ProductRecipe. Backend-named manifest capabilities and parse-only asset-watch remain transitional. | A real watcher workflow installs observable behavior or removes parse-only asset-watch; manifest selection and external packages retain their owning decisions. | 2026-08-13 @ `56264181873249c18d5477ce2170c8b1` |
| [0080](0080-domain-owned-task-update-integration-sets.md) | accepted | implemented | `nara_app`, `nara_tasks`, `nara_asset`, `nara_asset_watch`, `nara_image` | [crates/nara_app/src/lib.rs](../../../crates/nara_app/src/lib.rs); [crates/nara_tasks/src/runtime.rs](../../../crates/nara_tasks/src/runtime.rs); [crates/nara_image/src/reload.rs](../../../crates/nara_image/src/reload.rs) | [crates/nara_tasks/src/tests/execution.rs](../../../crates/nara_tasks/src/tests/execution.rs); [crates/nara_asset_watch/src/tests.rs](../../../crates/nara_asset_watch/src/tests.rs); [U2/U3 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u2-u3-recovery-and-device-admission.md); [U2 return evidence](../../knowledge/engineering/verification/2026-10/2026-10-22T181737Z-u2-task-return-and-physical-running-accounting-ce30f14820ee4350931de3355bb0c955.md) | Image logical supersession retires obsolete publication streams without waiting for physical work. Same-generation order, Poll cutoff and TaskPools close ownership remain intact. | - | 2026-10-23 @ U2 return evidence; prior integration evidence retained |
| [0081](0081-schema-source-stable-identity-catalog-and-runtime-binding.md) | accepted | partial | `nara_reflect`, `nara_reflect_derive`, schema owners, runtime composition | [authoring](../../../crates/nara_reflect/src/authoring.rs); [inventory](../../../crates/nara_reflect/src/inventory.rs); [snapshot execution](../../../crates/nara_reflect/src/registry.rs); [UI owner](../../../crates/nara_ui/src/codec.rs); [Editor runtime edit](../../../src/project_host/runtime/editor.rs) | [S3 verification](../../knowledge/engineering/verification/2026-10/2026-10-19T162634Z-authoring-s3-component-constraints-diagnostics-and-inventory-64aacf18a3af432584d306af27d32eb3.md); [current UI schema](../../../crates/nara_ui/src/component_schema_tests.rs); [external inventory workflow](../../../tests/fixtures/module-consumer/tests/component_inventory.rs); [snapshot codec tests](../../../crates/nara_reflect/src/tests.rs); [renamed manual codec consumer](../../../tests/fixtures/module-consumer/tests/manual_component_codec.rs); [U6/U8 evidence](../../knowledge/engineering/verification/2026-10/2026-10-22T191516Z-u6-typed-codec-and-u8-identity-consumer-qualification-0c76d56bc4b040118283f1024fe2b788.md); [R5a verification](../../knowledge/engineering/verification/2026-10/2026-10-24T024954Z-r5a-final-owner-diagnostics-and-workspace-qualification-f58a0ab130cb47a8bc2829e32f1d0a86.md); [untargeted native preparation](../../knowledge/engineering/verification/2026-10/2026-10-24T060059Z-untargeted-native-preparation-before-scene-entity-allocation-c2218c6824cd4aa7a8b9b657fedc9630.md) | Stable IDs, generated codecs, exact owner-local lineage, bindings and bounded data-only bundles remain implemented. Codec execution is available only from a successfully frozen `ComponentRegistrySnapshot`; the snapshot retains its exact definition after registry drop, rejects unknown schema or missing native binding explicitly, and keeps target-World admission in apply. Editor distinguishes absent components from encode failures. Reflect's full 199-test suite, renamed/manual codec consumers and affected workspace targets pass. Existing lifecycle, admission and publication owners remain unchanged. R5a adds scoped nested identity, rooted member-path tombstones and keyed-list policy to the canonical v1 catalog/bundle envelopes and semantic fingerprint v3. Previous envelope revisions reject without a compatibility reader. The current 532-test Reflect/Scene cohort and independent Scene/Prefab editing consumers pass. Native values now prepare in their owning scratch batch after topology/identity admission and before Scene target allocation; binding preserves asset association and cannot replay preparation. Allocator/order regressions and retained asset-bearing retry/cancellation pass. Final Reflect/Scene542, public27, workspace check, focused Clippy correctness and rustdoc qualify this internal prerequisite, not ordinary public Scene preparation or source/occurrence identity. | R5a Schema-relative metadata, lineage and owner edits are qualified; complete source/product/occurrence identity and durable Prefab overlays remain outside that slice. E01 typed registration remains implemented. Historical membership/migrations and broader package provenance remain explicit under OQ-044/OQ-045. | 2026-10-24 @ canonical schema-envelope reset and untargeted preparation qualification; earlier scoped evidence retained |
| [0082](0082-process-host-authority-and-runtime-construction-topology.md) | accepted | partial | Product composition and executable hosts | [src/project_host/runtime.rs](../../../src/project_host/runtime.rs); [src/project_host/runtime/action.rs](../../../src/project_host/runtime/action.rs) | [tests/project_host_boundary.rs](../../../tests/project_host_boundary.rs); [reference-game/tests/host_parity.rs](../../../reference-game/tests/host_parity.rs) | Concrete already-compiled Host-trusted paths prove coherent inputs, publication, parent retention, three-Host semantics, embedding and recipe/runtime registry coherence. Shared process services, package/build activation, replacement Render Host authority and a universal Host/service Interface remain unaccepted. | A future replacement Platform/Runner or Render Host must prove role-specific selection, sole authority, close ordering, and clean-room conformance; package/native-code activation remains under OQ-031 or an Accepted successor. | 2026-07-29 @ refresh 27cbd12 |
| [0083](0083-durable-project-asset-and-document-entity-identity.md) | proposed | not-started | `nara_asset`, `nara_identity`, `nara_scene`, persistent document owners, authoring hosts | - | - | Existing source asset UUIDs and private Prefab locators do not implement the proposed durable product/source/projection identity types or complete continuity/remap workflow. Identity semantics are separable from full packaging, artifact publication and Editor recovery. | First prove the scoped identity/continuity slice with repeated/nested projections and authored subproduct mappings surviving cache deletion. Artifact publication, package collision/copy and multi-file migration qualify subsequent operations; their whole proposals are not circular admission prerequisites. | 2026-07-21 @ primary-source audit f7e5ee2 |
| [0084](0084-executable-runtime-ownership-and-isolation.md) | accepted | partial | `nara_app` and concrete executable hosts | [crates/nara_app/src/runtime.rs](../../../crates/nara_app/src/runtime.rs); [src/project_host/runtime.rs](../../../src/project_host/runtime.rs) | [R5c Play-source qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T210921Z-trusted-play-source-continuity-and-source-local-close-qualification-61897d1b17744e2aaff6fc6efb1594a7.md); [Workflow integration](../../knowledge/engineering/verification/2026-10/2026-10-22T130424Z-engine-workflow-refactor-final-integration-ac0a299fafd64a6dba6179531a1f5cff.md); [U6/U7 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u6-u7-managed-schedules.md); [owner-thread tests](../../../crates/nara_app/tests/runtime_ownership.rs) | One App owns frozen behavior, runtime-local faults, fresh reconstruction and external managed driving. All World/candidate/failure/retirement wrappers remain thread-bound; definitions and control metadata stay transportable. Explicit custom schedules now run through the Runtime owner with state/control, authority and fault admission, without exposing mutable World or advancing clocks. Consecutive query change detection and public/renamed-root consumers pass. Replay/checkpoint/system stepping, panic containment and universal Runner/Host abstractions remain deferred. The R5c Editor refinement retains source association through physical retirement and distinguishes associated-source/Exit stop-first from unrelated document Close. | [U1 qualification](../../knowledge/engineering/verification/2026-10/2026-10-22T175315Z-u1-rejected-close-registration-preserves-runtime-custody-0b351d9e2f324d8fb56fa85b6d2da7fb.md) closes L01: rejected App registration retains cleanup custody through incomplete close/retry; fresh App 132-test cohort passes. [U10](../../knowledge/engineering/verification/2026-10/2026-10-22T193136Z-u10-exact-gameplay-batch-success-and-reporter-identity-380c336814b2450abe14023d43cf6253.md) closes E06 for Gameplay with exact-batch success gates after deferred effects; schedule errors still do not imply intra-schedule abort or rollback. Embedded authority changes and thread-affine SDK polling retain their concrete-consumer triggers. | 2026-10-24 @ Play-source `ebc0d48`; earlier scoped evidence retained |
| [0085](0085-hierarchy-transform-and-visibility-semantics.md) | proposed | not-started | Persistent scene hierarchy, authoring, visibility, UI projection, physics, and future 3D owners | - | - | ADR 0100 owns the dedicated non-linked runtime relation and completed 2D projection. ADR 0101 owns ordered persistent placement, exact mutation/inverse, and the ordered runtime projection. Visibility, linked lifetime, physics, 3D, and broader hierarchy semantics remain unaccepted. | A concrete product proves one remaining visibility, linked-lifetime, physics, 3D, or broader hierarchy contract without reopening ADR 0100 or ADR 0101. | 2026-08-04 @ architecture admission |
| [0086](0086-rust-project-build-and-executable-generation.md) | proposed | not-started | Executable hosts, build tooling, and Rust project composition | - | - | Current starter tooling builds and activates source-carrying native executables, but the proposed general Host-tool/target-executable/build-selection and qualification separation is not implemented. Existing checkout-free and compiled-project evidence retains its narrower scope. | A bounded explicit Cargo selection produces exact Host tools and target artifacts, rejects stale candidates and distinguishes build-only foreign targets from real target-runner activation. Independently qualify last-good executable plus compatible data across relevant failures. | 2026-07-16 @ feasibility review |
| [0087](0087-asset-dependency-import-product-and-artifact-publication-graph.md) | proposed | partial | `nara_asset`, importer-owning domains, and authoring hosts | [finite build Host/store](../../../src/asset_build.rs) | [local-build evidence](../../knowledge/engineering/verification/2026-10/2026-10-30T231610Z-tracked-local-artifact-builds-and-complete-group-publication-qualification-d3fcd08e15e04d45812ae09d7c81e944.md) | ADR0103 accepts and qualifies the finite typed tracked-input/closed-recipe/local-group slice; ADR0122 separately owns authored Product continuity. Seed hints, successful recipes, complete captured members and actual publication/uncertainty evidence have real compound/Image consumers. This does not accept the general dependency/reconciliation/scheduling graph or portable crash guarantees. | A concrete general scheduling, reconciliation or remote-provider consumer proves additional contracts without widening the finite slice by documentation. | 2026-10-31 @ finite local-build qualification |
| [0088](0088-target-build-cook-package-and-runtime-content-catalog.md) | proposed | not-started | Product build hosts, `nara_asset`, cook providers, and runtime content adapters | - | - | Nara has source projects and imported artifacts but no immutable target plan, dependency closure, cook generation, capability stripping, package manifest, runtime content catalog, or atomic mount/patch activation. | First qualify a local immutable base runtime catalog, explicit target closure/stripping and source/cache-inaccessible boot. Patch/DLC, remote authenticity and overlay rollback are independent later qualifications; bounded build capture does not require the whole Editor journal. | 2026-07-13 @ e276402 |
| [0089](0089-runtime-scene-instance-loading-activation-unload-and-travel.md) | proposed | partial | `nara_scene`, `nara_identity`, executable runtimes, and scene-consuming services | [current owner](../../../src/startup_scene.rs); [CPU residency](../../../src/project_host/runtime/content_residency.rs); [owned requests](../../../src/project_host/runtime/scene_requests.rs) | [ownership qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T172653Z-current-scene-ownership-and-decoded-generation-custody-qualification-0bf781c53d9e4cc3950bfb77b98d2b8a.md); [residency/request qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T183137Z-hosted-scene-residency-and-owned-preparation-qualification-d8d63279f87c4b34bc83eeeccdc7550e.md); [game qualification](../../knowledge/engineering/verification/2026-10/2026-10-21T025657Z-two-level-checkpoint-consumer-and-complete-frame-headless-qualification-72c5ef2705f0461e90916bb60e1bb345.md) | One current source/receipt owner supports protected Restart and direct data replacement. Original-Host threaded preparation admits declared roots and requires an explicit commit with preflighted CPU residency. Earlier domain/root/public/Editor/Reference cohorts remain qualified in their recorded scope. The public two-level game now proves travel, distinct logical instances, retained-source Retry and fresh-process restore, also through exported binaries. Full-frame headless execution advances command and task integration without changing exact stepping. | Additive active sets, arbitrary native rollback and the complete proposed lifecycle remain unaccepted. First presentation is not a complete interactive desktop control trajectory; other platforms retain their own qualification requirements. | 2026-10-21 @ bounded two-level consumer; earlier evidence retained |
| [0090](0090-unavailable-schema-and-lossless-authoring.md) | accepted | partial | Reflection, Scene, tooling and persistent document owners | [classification](../../../crates/nara_scene/src/authoring_decode.rs); [structural admission](../../../crates/nara_scene/src/authoring_decode/structure.rs); [source readmission](../../../src/project_host/persistence/reopen.rs) | [Workflow integration](../../knowledge/engineering/verification/2026-10/2026-10-22T130424Z-engine-workflow-refactor-final-integration-ac0a299fafd64a6dba6179531a1f5cff.md); [ordinary structure qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T051819Z-ordinary-structural-authoring-and-captured-source-reopen-qualification-f9b317e0ceda49a6b37f9000c61af8d7.md); [S3 verification](../../knowledge/engineering/verification/2026-10/2026-10-19T162634Z-authoring-s3-component-constraints-diagnostics-and-inventory-64aacf18a3af432584d306af27d32eb3.md); [authoring tests](../../../crates/nara_scene/tests/authoring_decode.rs); [structural tests](../../../crates/nara_scene/tests/structural_authoring.rs); [source conversion and lookup qualification](../../knowledge/engineering/verification/2026-10/2026-10-27T041718Z-complete-source-owned-prefab-conversion-and-current-stable-lookup-qualification-cbce190af0c4488b97895e68905573a6.md); [Editor conversion qualification](../../knowledge/engineering/verification/2026-10/2026-10-27T064545Z-complete-editor-prefab-conversion-and-source-admission-qualification-bc6d938e4ea5421fa42d0028230a11dc.md) | Complete/Degraded classification, raw-preserving fields/history and safe component-domain diagnostics remain. Stable-ID reparent/reorder now preserves unknown payload/identity/reference/cardinality while accounting for explicit topology placement changes. Complete documents without Prefab anchors admit source-local subtree Delete after effective/default reference validation; Restore remains history-only. Save/Reopen and the strict execution boundary remain source/schema/revision owned. Domain, public Scene/Prefab and Windows widget evidence pass. Complete source conversion preserves unavailable direct records and classifies their new placements. Unavailable projected survivors refuse; native-unbound current semantic records copy with exact Schema-owner claims. Conversion reuses immutable unchanged classification while preserving placements and complete unavailable records. Per-record traversal and aggregate retained references have independent quotas; same-limit full readmission and JSON/RON Reopen pass. | Unknown-record deletion/remapping, unresolved Prefab deletion closure and executable extensions remain outside ordinary structural authority. External manifest drift still rejects; byte proof is exact readback of already validated source, not a substitute for edit/Save manifest comparison. | 2026-10-27 @ `bf8e2d6` and `6da9980`; earlier scoped evidence retained |
| [0091](0091-editor-persistence-recovery-and-concurrent-writer-policy.md) | proposed | partial | `nara_tooling`, persistent document owners, `nara_fs`, and editor hosts | [crates/nara_tooling/src/workspace.rs](../../../crates/nara_tooling/src/workspace.rs); [crates/nara_fs/src/replace.rs](../../../crates/nara_fs/src/replace.rs) | [crates/nara_fs/tests/filesystem_contract.rs](../../../crates/nara_fs/tests/filesystem_contract.rs); [tests/editor_persistence.rs](../../../tests/editor_persistence.rs) | Known-schema Save uses a workspace-bound non-cloneable checkpoint and exact replacement receipt; ambiguity cannot advance saved state and remains DetectOnly rather than strong CAS. Accepted 0090 separately owns bounded unavailable-data authoring. The proposed recovery journal and old-or-new multi-document filesystem publication remain absent. | Qualify receipt-backed per-document Save, cross-file transaction/recovery and build-source capture independently. Cross-file claims require corruption/rebinding and every relevant crash/failure point; simpler Save/capture must state its own consistency envelope. | 2026-07-28 @ 9dcc8cf |
| [0092](0092-sdr-color-space-alpha-and-output-encoding.md) | accepted | partial | `nara_core`, `nara_image`, material/render domains, render backends | [crates/nara_core/src/lib.rs](../../../crates/nara_core/src/lib.rs); [crates/nara_image/src/asset.rs](../../../crates/nara_image/src/asset.rs) | [crates/nara_image/src/tests.rs](../../../crates/nara_image/src/tests.rs); [crates/nara_material/src/lib.rs](../../../crates/nara_material/src/lib.rs) | The implemented sRGB/linear texture mapping, linear shader path, straight-alpha source blend state, and current sRGB surface view establish the baseline, but public `Color` semantics, explicit authoring conversions, finite-value validation, durable semantic fixtures, rendered linear-composition evidence, and fail-closed handling for a surface without the declared sRGB-capable view remain incomplete. | A focused implementation slice documents the public API, adds explicit sRGB conversion, validates persistent values, and proves texture/tint/clear/blend/output plus supported/unsupported surface-format cases through codec and backend tests before claiming full implementation. | 2026-07-13 @ architecture audit |
| [0093](0093-rust-authoring-hot-iteration-and-optional-scripting-adapters.md) | accepted | partial | Rust build/runtime hosts, Editor Host and optional adapter packages | [crates/nara_asset/src/reload.rs](../../../crates/nara_asset/src/reload.rs); [crates/nara_tooling/src/play.rs](../../../crates/nara_tooling/src/play.rs) | [crates/nara_asset/src/lib.rs](../../../crates/nara_asset/src/lib.rs); [tests/scene_play_mode.rs](../../../tests/scene_play_mode.rs) | Domain data reload and rebuild-oriented Play foundations exist. Editor extension edit/build/reconstruct and owner/UI recovery are unimplemented R11f targets, separate from game/Play restoration. Broader change classification, executable activation and optional hot-patch scope retain their own evidence. | Qualify changed Editor code, dirty-content/history/draft compatibility, safe refusal/last-good recovery and correct-presentation latency. Named optional-language/installable-tool comparisons and Rust hot-patching retain their separate consumer/lifetime/target requirements. | 2026-11-04 Editor iteration design refinement; earlier bounded evidence retained |
| [0094](0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md) | accepted | partial | `nara_render`, native Feature packages and WGPU Host | [programs](../../../crates/nara_render_wgpu/src/program/mod.rs); [policy bindings](../../../crates/nara_render_wgpu/src/policy.rs); [owned candidates](../../../crates/nara_render_wgpu/src/candidate.rs); [history](../../../crates/nara_render_wgpu/src/history.rs); [Host](../../../crates/nara_render_wgpu/src/host.rs) | [independent program](../../../tests/fixtures/render-program-consumer/README.md); [independent policy/history](../../../tests/fixtures/render-policy-consumer/README.md); [typed-policy qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T192756Z-original-typed-render-policy-capture-and-complete-consumer-qualification-df7f9377f51142b78060b53617c800a1.md); [history qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T210034Z-captured-view-histories-and-original-submission-cleanup-production-qualification-5c061376803446c0bff1a95495ca55c7.md) | Typed texture/buffer programs, original policy captures, alternate scalar rendering and stock Runtime/Editor/image consumers share one Host. Move-owned candidates pin complete history bases, retain actual Feature owners and publish coherent slots/watermarks. Independent four-chain numerics, a Runtime window, reset/refusal/loss and cleanup evidence qualify the selected history transaction. Central payloads and combined facades are removed. | Complete source/processed interleaving, independently paced two-window/image and Nara-drawn composition remain required before full0121 acceptance. General scheduling, external native imports and dynamic unload remain separate. | 2026-11-01 @ captured View/history successor; earlier scoped evidence retained |
| [0095](0095-plugin-owned-specialized-domains-and-project-configuration.md) | accepted | partial | Specialized plugins, root product composition, schema-owning packages | [crates/nara_app/src/plugin.rs](../../../crates/nara_app/src/plugin.rs); [crates/nara_app/src/plugin/resolve.rs](../../../crates/nara_app/src/plugin/resolve.rs); [concrete collision owner](../../../tests/fixtures/motion-execution/physics/src/authored.rs) | [crates/nara_app/tests/plugin_composition.rs](../../../crates/nara_app/tests/plugin_composition.rs); [tests/plugin_composition.rs](../../../tests/plugin_composition.rs); [independent authored collision workflow](../../../tests/fixtures/motion-execution/consumer/src/collision/tests.rs) | Plugin-owned ECS/systems, conflicts and owner-local schema registration remain implemented. ProductRecipe and typed schema contributions share repeatable direct/file-backed configuration. The independent collision fixture additionally proves an exact persistent binding, post-spawn native derivation, complete selected membership, structured source/property/revision attribution, native contacts/debug and managed close. It is not an official production Physics plugin. Package installation, file-backed specialized settings, missing-package recovery and manifest backend vocabulary retain their separate gaps. | OQ-045 retains independently versioned package lifecycle and degraded-authoring work. A production specialized domain qualifies its own complete product workflow; no second solver is required unless a portable cross-provider Interface is proposed. | 2026-11-04 @ authored collision qualification; earlier recipe evidence 2026-08-01 @ 8b4e7f3 |
| [0096](0096-evidence-gated-render-scaling-and-upload-policy.md) | accepted | partial | `nara_render`, render domains, `nara_render_wgpu` | [crates/nara_render/src/lib.rs](../../../crates/nara_render/src/lib.rs); [crates/nara_render_wgpu/src/backend.rs](../../../crates/nara_render_wgpu/src/backend.rs) | [U2/U3 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u2-u3-recovery-and-device-admission.md); [U4 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u4-instance-budget.md) | Closed render ownership, device invalidation, Sprite/UI encoding and resource-complete image/glyph packets are implemented. Live-device texture admission distinguishes content rejection from retryable allocation failure; cumulative instance reservations cover Runtime/Editor scope demand. Target-plan primitive/resource/atlas/retention budgets remain. Measured scaling thresholds and generalized chunking/ring/staging mechanisms remain unimplemented. | A reproducible measured workload, rather than a general scaling roadmap, justifies the next private mechanism. | 2026-10-19 @ `ebbe005`, `964d25c`; scoped U3/U4 evidence |
| [0097](0097-future-capable-2d-3d-spatial-transform-model.md) | accepted | partial | `nara_transform`, scene/schema, render, import, tooling, and physics domains | [crates/nara_transform/src/lib.rs](../../../crates/nara_transform/src/lib.rs); [crates/nara_transform/src/propagation.rs](../../../crates/nara_transform/src/propagation.rs) | [crates/nara_transform/src/lib.rs](../../../crates/nara_transform/src/lib.rs); [crates/nara_render/src/lib.rs](../../../crates/nara_render/src/lib.rs); [U4 spatial qualification](../../knowledge/engineering/verification/2026-10/2026-10-22T194451Z-u4-immutable-spatial-cuts-and-exact-hierarchy-publication-4023754775ba445a887c9056715584e0.md) | Authored 2D TRS and phase spatial cuts replace Global Components; direct non-spatial parents terminate inheritance. U4 covers finite publication, exact World association, source edits and interpolation. Future post-affine/3D fields and KeepWorld normalization are [Proposed candidates](../spatial-affine-representation-design.md), not accepted implementation debt. Separate 3D and physics consumer boundaries remain unqualified. | Preserve the supported 2D snapshot contract. A real non-TRS import or world-preserving edit compares representation, precision, Inspector and undo; a focused 3D consumer admits its fields. Explicit source/schema migration is allowed. | 2026-10-23 @ spatial decision-scope refinement; U4 implementation evidence retained |
| [0098](0098-schema-owner-lineage-and-active-runtime-composition.md) | accepted | implemented | `nara_reflect`, schema-owning plugins, and product composition | [provider](../../../crates/nara_reflect/src/provider.rs); [Scene selection](../../../crates/nara_scene/src/component_bindings.rs); [recipe](../../../src/product.rs); [composition](../../../src/project_host/composition.rs) | [registry contract](../../../crates/nara_reflect/tests/registry_contract.rs); [product consumer](../../../tests/product_data_components.rs); [selection evidence](../../knowledge/engineering/verification/2026-10/2026-10-27T141025Z-mixed-scene-product-selection-and-content-to-runtime-admission-qualification-7c90588821aa4e8da374958ef2e436e4.md) | Owner-local lineage remains separate from composition. Explicit native/data selection has complete semantic identity, exact owner admissions and metadata-only content witnesses; generated recipe identity includes lineage facts. | Language execution/publication remains Proposed 0120. | 2026-10-27 @ product-selection evidence; original 9e3ae84 retained |
| [0099](0099-decision-local-product-evidence-and-publication-admission.md) | accepted | implemented | Product measurement owners and release operations | [.github/workflows/reference-game-candidate.yml](../../../.github/workflows/reference-game-candidate.yml); [tests/measurement_policy.rs](../../../tests/measurement_policy.rs) | [tests/measurement_policy.rs](../../../tests/measurement_policy.rs); [tests/ci_policy.rs](../../../tests/ci_policy.rs) | - | - | 2026-08-01 @ 74b15a3 |
| [0100](0100-runtime-structural-hierarchy-and-completed-2d-transform-projection.md) | accepted | partial | Runtime hierarchy, `nara_transform`, scene materialization, runtime UI, snapshots, render extraction, and reference-game product proof | [crates/nara_hierarchy/src/relation.rs](../../../crates/nara_hierarchy/src/relation.rs); [crates/nara_hierarchy/src/writer.rs](../../../crates/nara_hierarchy/src/writer.rs); [crates/nara_hierarchy/src/lib.rs](../../../crates/nara_hierarchy/src/lib.rs) | [U4 spatial qualification](../../knowledge/engineering/verification/2026-10/2026-10-22T194451Z-u4-immutable-spatial-cuts-and-exact-hierarchy-publication-4023754775ba445a887c9056715584e0.md); [scale comparison](../../benchmarks/2026-10-23-low-level-spatial-cuts.md) | One non-linked runtime relation owns ordered projection. Private Historical/Fixed/Presented cuts bind World/runtime, full Entity, role/time and captured order; readers need no completion-token join. L03/L04/E04 and A01 are repaired, including exact hierarchy publication and immutable consume-then-edit behavior. Spatial/render191 and final combined329 tests pass. Final-source Reference headless87, desktop render6 and five Windows/DX12 Editor trajectories pass; Root library267/integration78, actual compiled CLI Export, scoped Clippy and format/doc checks also pass in U11. Writer sparse baselines and Scene no-flush windows remain unchanged. Visibility, linked lifetime, physics and 3D remain unaccepted. | Residual source-value scan cost is measured, not hidden by change ticks. U11 retains exact root/consumer coverage and no native Linux/macOS claim. Prepared batches retain the no-flush/no-hierarchy-mutation window; later domains/platforms keep their triggers. | 2026-10-23 @ `89fdf6f` and U4 evidence |
| [0101](0101-ordered-persistent-placement-and-runtime-projection.md) | accepted | partial | Persistent Scene/Prefab/Patch documents, runtime hierarchy, authoring tools, and UI projection | [crates/nara_scene/src/document.rs](../../../crates/nara_scene/src/document.rs); [crates/nara_scene/src/prefab.rs](../../../crates/nara_scene/src/prefab.rs); [crates/nara_hierarchy/src/writer.rs](../../../crates/nara_hierarchy/src/writer.rs) | [crates/nara_scene/src/tests.rs](../../../crates/nara_scene/src/tests.rs); [crates/nara_scene/src/product_transaction_tests.rs](../../../crates/nara_scene/src/product_transaction_tests.rs); [crates/nara_hierarchy/src/tests.rs](../../../crates/nara_hierarchy/src/tests.rs) | Canonical ordered roots/children support exact invertible document/tooling mutations. Spawn, nested Prefabs, export, replace, Retry, retirement and UI consume the same completed order and structured provenance; runtime-only hierarchy roots retire atomically with Scene replacement. Runtime writer batches now use a private sparse candidate/baseline path while preserving full private Scene validation. | Future Editor UI and other consumers must preserve the completed hierarchy and structured provenance without introducing a UI-local order or parsed identity authority. Prepared batches retain their owner-controlled no-flush/no-hierarchy-mutation requirement. | 2026-10-18 @ F11 trusted-generation evidence |
| [0102](0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md) | accepted | partial | Text, UI kernel/widgets, Runtime/Editor adapters, input and presentation | [compiler](../../../crates/nara_ui_kernel/src/compiler.rs); [field](../../../crates/nara_ui_widgets/src/text_field.rs); [UI Runtime](../../../crates/nara_ui/src/runtime.rs) | [U9 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u9-text-content-box.md); [complete-key successor](../../knowledge/engineering/verification/2026-10/2026-10-24T185535Z-complete-persistent-ui-identity-and-neutral-conformance-successor-0cf84bdfd2f54766be84f3ca206f7931.md); [typed Image qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T015832Z-canonical-source-locations-and-typed-ui-image-binding-qualification-0ce5bafdc65f4893aa0079009668e41a.md); [completion/candidate qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T140508Z-bounded-input-ownership-and-total-ui-presentation-qualification-b97643044e4a4f92b892a3539d2ad415.md); [original-result/close qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T082117Z-original-ui-result-custody-and-logical-close-qualification-6e1965c6428841ec9138088bef20f90c.md); [text guide](../../guides/editable-text.md) | Immutable frame/session/packet authority, complete keys, typed Images and total original-result/close custody retain their prior qualification. The bounded text successor adds exact finite fallback faces, local editable buffers, field-incarnation/focus tokens, shared clipped drawing/pointer/candidate geometry and committed TextInput semantics. Owning, independent and actual GPU consumers qualify this prerequisite separately from a complete Editor panel. | Complete Editor toolkit, native editable-text/IME/accessibility workflows, virtualization, render output/mirror completion and Linux/macOS remain separate. Preserve UI rejection without Scene rollback; bounded primitives do not imply whole-toolkit parity. | 2026-11-04 @ bounded text prerequisite; `8e9d840` and earlier scopes retained |
| [0103](0103-concrete-domain-owned-asset-import-execution.md) | accepted | implemented | `nara_asset`, concrete domains and project/build Hosts | [source/import](../../../crates/nara_asset/src/import.rs); [optional build facet](../../../crates/nara_asset/src/build.rs); [finite executing Host](../../../src/asset_build.rs); [Editor facet](../../../src/project_host/runtime/editor/source_asset_import.rs) | [original extraction](../../knowledge/engineering/verification/2026-10/2026-10-28T233934Z-source-import-contracts-below-project-hosts-with-typed-read-errors-e617bfdd9a104fb69bdf3a20936f7d57.md); [local-build qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T231610Z-tracked-local-artifact-builds-and-complete-group-publication-qualification-d3fcd08e15e04d45812ae09d7c81e944.md) | Pure source import and optional serializable/normalized build settings stay below root/Scene/2D. Root owns actual capabilities, tracked current inputs, operation budgets, complete local groups and observable publication receipts. Real compound and processed Image builds feed existing Runtime delivery; concrete native publication keeps its owner. No erased registry or global budget manager is introduced. | General scheduling/reconciliation, target executable selection, complete packaged delivery and broader native/platform qualification remain separate. | 2026-10-31 @ finite local-build qualification; prior source/Editor/native evidence retains its scope |
| [0104](0104-host-owned-editor-workspace-installation.md) | accepted | implemented | Process Hosts, `nara_tooling`, Editor sessions, and UI adapters | [crates/nara_tooling/src/workspace.rs](../../../crates/nara_tooling/src/workspace.rs); [src/project_host/runtime/editor.rs](../../../src/project_host/runtime/editor.rs) | [crates/nara_project/src/tests.rs](../../../crates/nara_project/src/tests.rs); [crates/nara_tooling/src/workspace.rs](../../../crates/nara_tooling/src/workspace.rs) | - | - | 2026-08-04 @ `92ee7f9` |
| [0105](0105-closed-first-party-render-plan.md) | accepted | partial | `nara_render`, first-party render domains, UI packet lowering, and WGPU | [crates/nara_render/src/lib.rs](../../../crates/nara_render/src/lib.rs); [crates/nara_sprite_render/src/types.rs](../../../crates/nara_sprite_render/src/types.rs); [tests/project_runtime_boot.rs](../../../tests/project_runtime_boot.rs) | [crates/nara_sprite_render/src/tests.rs](../../../crates/nara_sprite_render/src/tests.rs); [crates/nara_ui_render/src/packet_tests.rs](../../../crates/nara_ui_render/src/packet_tests.rs); [tests/project_runtime_boot.rs](../../../tests/project_runtime_boot.rs) | Sprite/Tilemap share per-view layer/sort-key order, optional foot-Y and stable ties; only adjacent compatible items batch. UI keeps overlay/painter order. Immutable resource-complete packets reach WGPU without live World/asset lookup, with exclusive targets, bounded views, offscreen sequencing and independent Editor views. Saved file-backed TileSets now reach textured static render batches with verified Image pixels. Multipart sorting groups and a general graph remain unimplemented. | Use a saved/reopened real map and measured cost to admit only necessary sorting/cache/culling work. Broader execution authority still requires a concrete consumer and focused decision. | 2026-10-18 @ [Tilemap slice](../../plans/2026-10-18-tilemap-file-backed-foundation-plan.md); 2026-10-17 @ foundation `ca3d803`, selected Editor `f4ffdd18` |
| [0106](0106-content-and-presentation-product-composition.md) | accepted | partial | Root product composition, project capabilities, reference-game Hosts, and presentation adapters | [Cargo.toml](../../../Cargo.toml); [src/product.rs](../../../src/product.rs) | [crates/nara_project/src/tests.rs](../../../crates/nara_project/src/tests.rs); [tests/product_capabilities.rs](../../../tests/product_capabilities.rs) | 2D/UI content and presentation are separate capabilities. Explicit WGPU/Winit contributions preserve base-plugin behavior under unrelated compile features; ProductRecipe owns ordinary file-backed Host composition. | The bounded Runtime UI product evidence is published. External backend replacement, dynamic contribution discovery and generic package distribution remain outside this ADR. | 2026-08-13 @ `56264181873249c18d5477ce2170c8b1` |
| [0107](0107-shared-ui-foundation-and-independent-product-layers.md) | accepted | partial | UI Foundation, Runtime UI, Editor Domain, Editor Host and toolkit adapters | [crates/nara_ui_kernel/src/compiler.rs](../../../crates/nara_ui_kernel/src/compiler.rs); [editor/src/frontend/imgui/mod.rs](../../../editor/src/frontend/imgui/mod.rs) | [crates/nara_ui/src/adapter_tests.rs](../../../crates/nara_ui/src/adapter_tests.rs); [editor/tests/authoring_loop.rs](../../../editor/tests/authoring_loop.rs) | Shared UI Foundation and independent Runtime/Editor layers remain. Qualified Nara-selected panels coexist with the Dear workspace predecessor over the original document/action and Host owners; bounded results are linked above. | R11a-e must qualify dependency closure, keyed component state/scoped effects, repeatable preparation, custom layout/drawing, public frontend attachment and measured invalidation costs. Whole Editor/game toolkit convergence remains optional; native IME/accessibility requires matching evidence. | 2026-11-04 component-interface design refinement; earlier bounded evidence retained |
| [0108](0108-ui-surface-presented-session-and-atomic-interaction.md) | accepted | implemented | UI session/interaction, Input, Winit accessibility and Host Surface lifecycle | [interaction session](../../../crates/nara_ui_kernel/src/interaction/session.rs); [Surface state](../../../crates/nara_ui/src/runtime/state.rs); [paint lowering](../../../crates/nara_ui_render/src/planner/lowering.rs) | [interaction tests](../../../crates/nara_ui_kernel/tests/interaction.rs); [adapter tests](../../../crates/nara_ui/src/adapter_tests.rs); [paint boundary qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T044452Z-module-goal-alignment-and-concrete-boundary-repairs-qualification-f7676d2222234fd39b8b565dbcaf996f.md); [completion/claim qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T140508Z-bounded-input-ownership-and-total-ui-presentation-qualification-b97643044e4a4f92b892a3539d2ad415.md) | Paint baselines precede clipping and preserve half-open mapping in the original35-test scope. Prepared completion now checks Input invalidation and the live UI basis before publication; C1 consumes old-basis candidates. Original Input claims survive suspension/geometry and protect same-label replacements. Restore clears predecessor interaction only for an actual lifetime handoff. Late completion preserves current effects and backend pixel facts without false interactive publication. | These bounded lifecycle/paint scopes do not qualify full editable text, OS IME delivery, native accessibility parity, font fallback or monitor scanout. | 2026-10-30 @ total completion/endpoint claims; `40d3f87` / `8582427` scopes retained |
| [0109](0109-root-local-ui-compilation-and-presentation-closure.md) | accepted | partial | UI authoring/compiler, text limits, UI presentation planner and WGPU | [crates/nara_ui_kernel/src/authoring.rs](../../../crates/nara_ui_kernel/src/authoring.rs); [crates/nara_ui_kernel/src/compiler.rs](../../../crates/nara_ui_kernel/src/compiler.rs) | [crates/nara_ui/src/adapter_tests.rs](../../../crates/nara_ui/src/adapter_tests.rs); [crates/nara_ui_kernel/examples/layout_gallery.rs](../../../crates/nara_ui_kernel/examples/layout_gallery.rs) | Original root failure/fallback, bounded compilation and presentation closure remain qualified. The November 4 incremental-reuse/cost contract is a design target. | R11a/R11e establish measured work budgets, selective invalidation and bounded cache bytes. Last-good root retention does not qualify those additions. | 2026-11-04 design refinement; `c4df323` scope retained |
| [0110](0110-first-party-project-workflow-and-source-carrying-export.md) | accepted | implemented | Root project workflow, `nara_project`, `nara_fs`, project content and Runtime Hosts | [src/project_workflow/starter.rs](../../../src/project_workflow/starter.rs); [selected delivery owner](../../../src/project_content/delivery.rs); [export](../../../src/project_tooling/export.rs) | [creation](../../../tests/project_workflow_creation.rs); [delivery](../../../tests/project_content_delivery.rs); [real compiled CLI](../../../tests/project_workflow_cli.rs); [export source matching](../../../src/project_workflow/export/source_tests.rs); [compiled domain execution](../../verification/2026-10-21-compiled-asset-domain-export.md); [source authority verification](../../knowledge/engineering/verification/2026-10/2026-10-24T065424Z-project-source-authority-carrier-and-creation-identity-qualification-6cdfe4a580b14b8da78e659010497632.md) | Capability-owned creation, exact selected Scene closures, finite declared delivery, presentation-free headless execution, desktop startup verification, Cargo artifact selection, checkout-free source-carrying export and startup repair are implemented. Check/Export cover the exact union of startup and declared Scene dependencies, including compiled source-domain auxiliary members. Focused source-publication tests retain synthetic observations; the Windows CLI trajectory now builds the actual external domain into both release targets, accepts two real product observations, and verifies detached headless delivery plus missing-auxiliary rejection before gameplay. Startup executable agreements remain distinct and reject retargeting to freshly checked changed startup content. The unreleased compiled-tool contract now uses canonical protocol 1 across metadata, starter and current consumers. Its bounded structured-failure replacement has separately scoped [F3 qualification](../../knowledge/engineering/verification/2026-10/2026-10-25T100841Z-compiled-domain-failure-projection-and-truthful-publication-observations-8b995496a82b411184e95f6f462b48f8.md), including typed Schema publication/cleanup, inspectable repair hints and real compiled domain failures; historical protocol-2 evidence retains its original scope. The starter now emits a required source universe and stable startup Scene metadata as exact prepared members; publication rejects post-publication metadata tampering while retaining the published directory. | Full backups, cooked/source-free delivery, automatic rebinding and complete reproducible build-input closure require separate concrete workflows. Export startup observations do not establish full native presentation/input or other platforms. Current-level activation, residency retirement and game saves follow the separate persistent-game plan. | 2026-10-24 @ source authority carrier and creation identity checkpoint; earlier evidence retained |
| [0111](0111-editor-scene-view-execution-and-picking-authority.md) | accepted | implemented | Native Editor Host, `nara_render`, and `nara_render_wgpu` | [render packets](../../../crates/nara_render/src/lib.rs); [native execution](../../../crates/nara_render_wgpu/src/host.rs) | [immutable-output qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T121248Z-immutable-render-outputs-owning-producers-and-original-native-prefix-qualification-1b4de7f4305740698abba87b589f9459.md); [earlier projection qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T225800Z-disposable-scene-projection-epochs-and-stable-native-preview-qualification-2478d897135549daacbc8d1fbc90b9d9.md) | Configuration, exact physical outputs, display occurrences and revision-bound picking are separate production owners. Actual retained A1/A2 pixels, registrations, Scene Reimport, no-window public use, producer-local close, original input through loss and native Scene/Canvas source/mapping authority pass. Stable configuration no longer aliases future pixels. | Graph/Feature and broader transfer/color/history support remain under the current R8/R9 unit and Proposed0121. Native evidence is Windows/DX12; browser and other platform execution are not implied. | 2026-10-31 @ immutable-output qualification |
| [0112](0112-editor-game-view-runtime-presentation-and-input-authority.md) | accepted | implemented | Native Editor Host, executable Runtime owner, Runtime UI presentation, `nara_render`, and `nara_render_wgpu` | [Game bridge](../../../src/project_host/runtime/editor/game_view.rs); [native consumer](../../../editor/src/platform/imgui.rs) | [designated-display qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T121248Z-immutable-render-outputs-owning-producers-and-original-native-prefix-qualification-1b4de7f4305740698abba87b589f9459.md); [Runtime tests](../../../tests/workspace_play_runtime.rs) | Borrowed final target activation preserves coordinated unpublished Runtime start. Actual M-only presentation preserves D's original UI permit; same-window occurrences, later D, partial window results, close/loss and compatible input capture/fixed Step pass without replay. Output leases carry no duplicate UI permission. | Broader remote/transport placement, graph/Feature integration, platform accessibility and live edit retain their separate decisions and consumers. | 2026-10-31 @ designated-display qualification |
| [0113](0113-editor-ui-prefab-authoring-and-canvas-authority.md) | accepted | implemented | `nara_scene` Prefab authoring, `nara_tooling` workspace, project Host persistence and content refresh, effective project settings, the selected Dear ImGui authoring frontend, and private Editor Canvas | [crates/nara_scene/src/authoring.rs](../../../crates/nara_scene/src/authoring.rs); [crates/nara_scene/src/patch.rs](../../../crates/nara_scene/src/patch.rs) | [R5c additive Open qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T202313Z-additive-scene-and-prefab-open-qualification-d0b1fc894dd2419fa1cf43c9e39eb79d.md); [R5c multi-source refresh qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T192721Z-multi-source-prefab-refresh-obligation-qualification-47f6266123094e7bb2ea046f42131843.md); [crates/nara_scene/src/tests.rs](../../../crates/nara_scene/src/tests.rs); [editor/src/frontend/imgui/interaction_tests.rs](../../../editor/src/frontend/imgui/interaction_tests.rs) | Ordered Prefab structure, finite schema/brush catalogs, exact image choices, source-local preflight and atomic allocator/history/selection publication are implemented. Stale picks, gesture cancellation, focus and Canvas retirement are covered; real-source Save/Reopen, Runtime restart and successor presentation pass. R5c replaces singleton refresh requests with source-keyed obligations, canonical configured-root observations and budgeted complete-set validation. Retry and retirement recovery retain every obligation after Close; 850 library, 39 integration, 31 consumer and eight selected actual native cases pass. ADR 0118 owns the current frontend/window migration; toolkit convergence, responsive authoring and live Runtime mutation remain separate. | Additive anchored Prefab Open, two-source exact Save/refresh and the separately scoped Play-source retirement continuity now pass; see ADRs0034/0118 for that continuity evidence. Responsive authoring, another complete toolkit consumer or live Runtime mutation needs separate evidence; ADR 0118 owns current frontend/windows. | 2026-11-03 status reconciliation; 2026-10-24 @ R5c additive Open; earlier Prefab/Canvas evidence retained |
| [0114](0114-standalone-generic-editor-authoring-and-execution-boundary.md) | accepted | partial | Private Editor, project Host, project/reflect/Scene/tooling and workflow owners | [generic Editor](../../../src/project_host/runtime/editor.rs); [source Browser](../../../crates/nara_tooling/src/project_browser/state.rs); [ordinary structural consumer](../../../editor/tests/support/ordinary_starter_structure.rs) | [ordinary structure qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T051819Z-ordinary-structural-authoring-and-captured-source-reopen-qualification-f9b317e0ceda49a6b37f9000c61af8d7.md); [strict Check tests](../../../tests/project_workflow_check.rs); [external Play consumer](../../../editor/tests/supervised_play_clean_room.rs); [CP1/S2 qualification](../../knowledge/engineering/verification/2026-11/2026-11-01T203621Z-reviewed-authored-schema-upgrades-and-fresh-process-recovery-qualification-cf7c8ecab10c4e21b4cfcff4906805de.md) | Generic Open, bounded data-only schema admission, degraded preservation, Recovery and strict Runtime separation remain. Existing Browser/Image import and source publication evidence retains its scope. Ordinary Scene/Prefab structure now works without game Rust loaded; a generated desktop starter performs create/arrange/remove/history/Save/Reopen, strict public Check and actual supervised Play/Stop/Exit. Compiled Editor authoring uses retained source Native/Data authority independently of Runtime bindings; the Root native-only source fallback is removed. Windows widget and external application evidence are separate. | Native Linux/macOS, broader source mutation/Prefab closure, shader/animation authoring, watchers or dynamic extension hosting needs a selected workflow. The source-local editing proof does not establish full production maturity. | 2026-11-02; `e673693`, selected Editor `f4ffdd18` and original `ec64e4c` evidence retained |
| [0115](0115-editor-image-import-metadata-and-authoring-publication.md) | accepted | implemented | `nara_asset`, `nara_image`, project Host image authoring, UI-neutral tooling, and private Editor adapters | [crates/nara_image/src/import.rs](../../../crates/nara_image/src/import.rs); [src/project_host/runtime/editor/image_import/transaction.rs](../../../src/project_host/runtime/editor/image_import/transaction.rs) | [crates/nara_image/tests/image_settings.rs](../../../crates/nara_image/tests/image_settings.rs); [editor/src/platform/imgui/scene_image_tests.rs](../../../editor/src/platform/imgui/scene_image_tests.rs); [R0 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19T021717Z-image-lifecycle-contract-repairs-95901fe367a149bc90ec922ad0384a73.md); [Image custody qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T035244Z-editor-image-metadata-publication-cleanup-custody-qualification-c097c716d6f3480692d800285ec53544.md) | Canonical metadata, receipt-bound publication, retained-pixel leases, bounded hydration and R0 identity revocation remain implemented. Metadata writers register before mutation; runtime custody retains failed cleanup through terminal failure, close, session Drop and post-publication unwind. Successful publication retires the writer while pending decode retains exact fresh-read authority and binding. Focused engine/public 116, Editor/application 186 and Windows/DX12 native 2 tests pass; workspace check and independent integration reviews pass. | - | 2026-10-20 @ `bae0440`; earlier R0 evidence and `ec64e4c` |
| [0116](0116-standalone-supervised-editor-play.md) | accepted | partial | Private `nara-editor`, project Host, executable product Hosts, UI-neutral tooling and platform process adapters | [editor/src/supervised_play/owner.rs](../../../editor/src/supervised_play/owner.rs); [src/project_host/runtime/editor/external_play.rs](../../../src/project_host/runtime/editor/external_play.rs) | [src/project_host/runtime/editor/external_play.rs](../../../src/project_host/runtime/editor/external_play.rs); [editor/src/platform/imgui/external_cadence_tests.rs](../../../editor/src/platform/imgui/external_cadence_tests.rs); [storage and diagnostics evidence](../../knowledge/engineering/verification/2026-10/2026-10-20T152924Z-reflection-declaration-checks-and-linux-supervised-play-storage-corrections-efc879a2a8974c989725df994b86e5db.md) | Committed Windows evidence covers exact saved generations, child READY, lifecycle, build failure/Run Last Good, cancellation, quiet observation and physical cleanup. Restart rejects changed or unavailable saved-source closures before retirement. Bounded preparation/launch failure facts and quiet repeated-error projection pass 175 Editor library tests. Linux generation storage now uses the standard user cache: six actual-source Linux tests and ext4 publication pass; explicit tmpfs remains rejected before staging. These storage results do not qualify the complete hosted Linux Play trajectory. | Complete native Linux/macOS process/window release trajectories before platform release claims. Retain ADR 0116's exact generation, receipt and cleanup guarantees; observe successor hosted Linux clean-room/reference Play using the new bounded diagnostics. | Earlier `ca3d803`, `ab211bc`, `0b3f569`; 2026-10-20 @ diagnostics `258c1ab`, Linux cache `ce49696` |
| [0117](0117-2d-foundation-ownership-and-authoring-contracts.md) | accepted | partial | App, content/Scene, render, tooling and concrete product Hosts | [managed runtime](../../../crates/nara_app/src/runtime.rs); [Image publication](../../../crates/nara_image/src/content.rs); [TileSet publication](../../../crates/nara_tilemap/src/content.rs); [View lifecycle](../../../crates/nara_render/src/view.rs); [world canvas](../../../crates/nara_render/src/canvas.rs); [project content](../../../src/project_content.rs); [product runtime](../../../src/project_host/runtime.rs) | [workflow integration](../../knowledge/engineering/verification/2026-10/2026-10-22T130424Z-engine-workflow-refactor-final-integration-ac0a299fafd64a6dba6179531a1f5cff.md); [TileSet publication](../../knowledge/engineering/verification/2026-10/2026-10-19-tileset-publication-and-recovery.md); [canvas qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T070752Z-fixed-world-canvas-and-native-ui-qualification-e7c7f1ee29e4459688d51f1ca20282d8.md); [View/history qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T210034Z-captured-view-histories-and-original-submission-cleanup-production-qualification-5c061376803446c0bff1a95495ca55c7.md) | Existing evidence retains managed Runtime, checked content/GPU publication, fixed WorldCanvas and presentation-gated input within its recorded scope. Neutral captured Views now have non-reused process lifetimes independent of Entity/order/extent, stable through filtering or missing targets and replaced on camera removal/readdition. Public packets reject duplicate IDs; original temporal View owners reclaim slots independently of shared effect declarations. | The low-level plan's phase-cut and managed mutation successors retain their own evidence. Joint motion/solver ownership, physics/audio, tile authoring/durable IDs, scaling, telemetry and native Linux/macOS expansion remain separate. | 2026-11-01 @ captured View lifetime successor; prior scoped evidence retained |
| [0118](0118-editor-workspaces-frontends-and-creation-templates.md) | accepted | partial | Private Editor, tooling, reflection and document-domain owners | [Application submissions](../../../editor/src/app.rs); [field editor](../../../editor/src/frontend/imgui/field_editor.rs); [ordinary tree controls](../../../editor/src/frontend/imgui/authoring.rs); [structural admission](../../../crates/nara_scene/src/authoring_decode/structure.rs); [native Host](../../../editor/src/platform/imgui.rs); [Open preparation](../../../src/project_host/runtime/editor/open/preparation.rs) | [Prepared-panel qualification](../../knowledge/engineering/verification/2026-10/2026-10-25T024527Z-prepared-panel-complete-review-and-scoped-workflow-qualification-49e27d9779054a5b8fda1142add06519.md); [R5c Play-source qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T210921Z-trusted-play-source-continuity-and-source-local-close-qualification-61897d1b17744e2aaff6fc6efb1594a7.md); [R5c additive Open qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T202313Z-additive-scene-and-prefab-open-qualification-d0b1fc894dd2419fa1cf43c9e39eb79d.md); [R5c multi-source refresh qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T192721Z-multi-source-prefab-refresh-obligation-qualification-47f6266123094e7bb2ea046f42131843.md); [R5c source/preview qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T180456Z-common-source-admission-and-retained-scene-preview-qualification-45ffac37614a4edaa792f08d650f292e.md); [R1a qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T123523Z-workflow-driven-foundation-waves-and-r1a-owned-application-submission-qualification-5e5f6c3eb3b84150bc3a5e06e9862abf.md); [Workflow integration](../../knowledge/engineering/verification/2026-10/2026-10-22T130424Z-engine-workflow-refactor-final-integration-ac0a299fafd64a6dba6179531a1f5cff.md); [ordinary structure qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T051819Z-ordinary-structural-authoring-and-captured-source-reopen-qualification-f9b317e0ceda49a6b37f9000c61af8d7.md); [Owned startup qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T032446Z-owned-startup-closure-publication-and-responsiveness-qualification-f187b61104d4443187cea8b9bc392460.md); [Host lifetime qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T030524Z-editor-host-lifetime-cleanup-custody-qualification-12c6b025d6c74ed1bf6900c8b0413877.md); [S3 verification](../../knowledge/engineering/verification/2026-10/2026-10-19T162634Z-authoring-s3-component-constraints-diagnostics-and-inventory-64aacf18a3af432584d306af27d32eb3.md); [direct-member and panel query qualification](../../knowledge/engineering/verification/2026-10/2026-10-26T004155Z-bounded-direct-member-discovery-and-panel-query-workflow-7003450c49204d8eba92ba90db765dc0.md); [source-owned creation qualification](../../knowledge/engineering/verification/2026-10/2026-10-24T203944Z-source-owned-entity-creation-and-generated-product-qualification-c5fd5454223046e38fc1761338ddffb0.md) | Owned direct submissions retain one exact observation per original action plus a separate drive update; the 214-test related cohort, 11 independent consumer tests and three selected Windows/DX12 trajectories pass. The first R2 prepared source transaction and non-active direct-entity query workflow is qualified through its separate evidence. R5c adds typed per-document persistence custody, common current-target source admission, explicit retained Scene preview and exact operation/cleanup routing. R5c multi-source refresh now retains every canonical source obligation through Close, retries and retirement, with one budgeted successor validation. Public additive Open now passes with two Scenes/two Prefabs and bounded actual native source navigation; exact Play-source retirement continuity now has its separate 597-library/40-public/21-headless/31-consumer/six-native qualification. Retained transaction-local construction, selected-root discovery/reads, bounded direct-member discovery and source-built prepared-panel submission have separate qualification; Selector-directed property reads, synchronous read-only panel observations and the integrated non-active nested discover/read/prepare/retain/commit consumer have separate qualification; full R1, recursive/collection nested discovery and shared-owner preparation locality remain open. R5a Schema-relative properties have separate qualification. Earlier field/result/domain, owned Save/Reopen/startup and full Host lifetime custody remain qualified within linked scopes. Ordinary tree controls now admit source-local Move/Reorder and closed subtree Delete through exact document/history transactions. Unknown semantic records survive authorized placement changes. The 271-test Scene/tooling, 176-test Editor/public and 65-test shared cohorts pass; two actual Windows/DX12 widget trajectories and a generated strict Check/external Play consumer pass. Existing v4 phase measurements stay frozen. Ordinary source-owned creation and generated starter Check/build/export retain their separate qualification; native component-bearing and Published empty workflows remain distinct. | R11b-d public panel/component hosting, custom drawing and UI operations, plus R11f Editor code reconstruction/recovery, remain unimplemented. Native Linux/macOS, OS IME candidate windows, accessibility, inline tools, unknown/Prefab closure deletion and dynamic plugins need their own workflows. Pixel presentation and measured interaction/iteration costs remain separate; Tile tools stay deferred. | 2026-11-04 component/iteration design refinement; earlier bounded direct-member/panel evidence retained |
| [0119](0119-schema-first-semantic-metadata-authority.md) | accepted | partial | Semantic Schema and native reflection adapters | [provider](../../../crates/nara_reflect/src/authoring.rs); [registry](../../../crates/nara_reflect/src/registry.rs) | [native contract](../../../crates/nara_reflect/tests/registry_contract.rs); [non-Reflect consumer](../../../tests/fixtures/module-consumer/tests/support/nested_schema.rs); [review](../../audits/2026-10-23-foundation-architecture-coverage.md) | Current native providers already use explicit Schema/codec contracts without Reflect. This round corrects authority and supersedes 0004; it does not remove optional reflection storage, implement new domain descriptors or re-execute runtime tests. Dynamic representation/reload remains Proposed under 0120. | Retain non-Reflect/native complete-binding consumers when changing code; qualify any new metadata adapter/descriptor against its concrete operation. | 2026-10-23 @ source/decision review only; no fresh runtime qualification |
| [0120](0120-schema-publications-and-execution-representations.md) | proposed | not-started | Schema, Scene materialization and concrete gameplay adapters | [public data consumer](../../../tests/fixtures/module-consumer/tests/scene_data_components.rs); [language consumer](../../../tests/fixtures/behavior-languages/README.md); [raw storage probes](../../../tests/fixtures/module-consumer/tests/behavior_storage_comparison.rs) | [representation comparison](../../research/2026-10-27-behavior-representation-comparison.md); [body/lifetime comparison](../../research/2026-10-27-behavior-code-iteration-and-lifetime.md); [public-language evidence](../../knowledge/engineering/verification/2026-10/2026-10-27T145608Z-public-scene-language-consumer-and-obsolete-representation-runtime-removal-67783a7cb8514ce9bbb7957670b4ec37.md); [finite-tool evidence](../../knowledge/engineering/verification/2026-10/2026-10-27T180941Z-production-finite-source-tools-and-exact-physical-close-observations-fc2363973364434096d350ae4a8d762c.md) | Experimental C#/Lua execution uses the adopted public Scene data family and real window-free Session editing/history/persistence. The obsolete preview/post-commit materialization/export Runtime is deleted. Finite trusted child tools now have their separate production Session/Application contract under ADRs 0047/0118. Same-World body switching and VM/collectible-context leases remain scoped execution probes; this executable publication/migration/cohort proposal is not adopted. | Integrate an executable gameplay owner through product admission, then cross-owner compatibility, in-flight work, Stop-first, catalog migration and complete physical retirement. Inline tool facets and installation remain separate R1b workflows. | 2026-10-28 @ finite-tool qualification; separately adopted ADR 0006 and bounded language evidence retained |
| [0121](0121-composable-render-programs-and-host-execution.md) | proposed | partial | Render Feature/domain adapters and local GPU Host | [independent stock consumer](../../../tests/fixtures/stock-composition-consumer/README.md); [production integration](../render-program-production-design.md) | [shared stock qualification](../../knowledge/engineering/verification/2026-11/2026-11-01T013746Z-shared-stock-composition-original-captures-and-native-scene-effects-qualification-2f250ecb33b24b5091f28a141cd6b244.md); [native-composition qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T225517Z-native-composition-physical-pump-handoff-and-retained-stock-input-qualification-f0cd93c17a3c47fa905993cee07e6c5c.md); [cost method/results](../../benchmarks/2026-10-31-render-composition.md) | Finite programs, original typed captures, selected View/history, Source/Processed replay and independent native/image streams use the same Host under Accepted0094. Shared world/Canvas/UI stages now carry external blur/grade through Runtime, actual native Scene View and offscreen, with exact HUD, retained resource semantics and policy-independent picking. Optimized controls establish no stable cost winner; the complete proposal remains unqualified. | Durable material generations, broader View/time facts, the editable Nara-drawn Inspector and measured geometry retain their separate gates. Attribute reproducible production costs before interface freeze; full0121 remains Proposed. | 2026-11-01 @ shared stock-composition successor; Proposed status unchanged |
| [0122](0122-scoped-imported-product-identity-and-authored-continuity.md) | accepted | implemented | `nara_asset`, Reflect/Scene reference owners, project content and concrete metadata facets | [product identity](../../../crates/nara_asset/src/product_identity.rs); [families/codecs](../../../crates/nara_asset/src/runtime_codec.rs); [canonical reference](../../../crates/nara_reflect/src/asset_reference.rs); [prepared-product checks](../../../src/project_content/domains.rs); [compound owner](../../../tests/fixtures/module-consumer/asset-domain-consumer/src/bundle.rs) | [continuity/format qualification](../../knowledge/engineering/verification/2026-10/2026-10-29T004054Z-compound-product-continuity-and-independent-runtime-format-qualification-a2d6d4837fd44218b7cb87f67cb38063.md); [declared-reference qualification](../../knowledge/engineering/verification/2026-10/2026-10-29T023522Z-declared-product-references-and-prepared-content-target-admission-qualification-37c1b3f6eeca448f928a6961f1c7ab6b.md); [compound project and cached Editor Play](../../../tests/fixtures/module-consumer/asset-domain-consumer/tests/compound_workflow.rs); [Editor transition](../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/compound_import.rs) | Scoped IDs, authored continuity, deterministic Runtime text format and predecessor-aware Editor publication pass. The successor preserves complete Product references through existing nested/default Schema traversal, rewrite, Scene export and Prefab copy, validates actual prepared targets and rechecks edited cached Play. Primary-only consumers refuse auxiliary selection; JSON/RON records remain strict. Known-retired preservation remains relative to the admitted lineage. | Runtime catalog delivery, CAS/group publication and project-wide migration remain separate work; the scoped decision does not adopt them. | 2026-10-29 @ `c3c6d33`; declared references and prepared target qualification |
