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

The November 5 framework comparison reopens direct adoption and maintenance of the user's
Open-GPUI extraction. The actual development baseline is `89cb76ba` on
`optimize/docking-graph-transaction`, distinct from the older `repo-ref/open-gpui` snapshot.
ADRs0107/0118 permit that Editor-local execution model while retaining Nara's document/history,
game-UI and exact native View contracts. The
[R11a comparison](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#maintained-open-gpui-comparison)
and [engine-fit study](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-5-maintained-open-gpui-and-engine-fit)
record existing reuse, missing controlled selection/local text history, dependency consumption and
native-render integration work. Source and framework tests alone select no production default,
qualify no Nara engine View and do not close the fork's in-progress canonical docking replacement.
Its first maintenance successor, Open-GPUI `8c58cca8`, supplies an independently locked public
consumer and compatible dependency selection. The Windows/Rust 1.95 consumer and actual WGPU
renderer compile, a disposable Nara-plus-framework graph compiles, and 2,590 selected framework
library tests pass. `bce3a902` records the upstream correctness queue. Its subsequent
[arena/native-frame unit](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-5-arena-and-native-frame-maintenance)
implements separate allocation, nested cleanup, App-bound clear tokens and post-borrow Windows
maintenance at published Open-GPUI `15cc4a7d85`, with allocator Miri and real dual-HWND evidence.
The subsequent [TextSystem ownership unit](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-5-textsystem-lifetime-and-native-reconstruction)
removes the pool reference cycle and exercises public headless/native reconstruction, including
the corrected WinRT settings-factory lifetime, at published Open-GPUI `229723e620` after native
correction `432d079675`. The specific touchpad timing comparison remains open.
Complete Editor integration, full native input, costs and game-runtime selection
remain unqualified by these framework prerequisites.
The [diff-based refresh record](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-5-diff-based-upstream-baseline-and-maintenance-skill)
at Open-GPUI `2d66e3ff88f` fixes the comparison target and maintenance workflow; source inventory
and the repository skill add no implementation qualification or toolkit-default decision.
The subsequent [cached-dependency qualification](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-cached-dependencies-and-deferred-opacity)
at published Open-GPUI `7a1bf6d73d4` adds nested-draw read isolation, paint/deferred subscriptions,
stable deferred replay and admitted opacity convergence. Core 873, affected consumers 2,964 and
selected Windows 165 pass, with four existing interactive cases skipped; the native cache matrix
uses actual Submitted frames and retirement. At that checkpoint, text layout and pending-input/blur
remained the two initial units before the broader refresh. This framework evidence establishes no Nara toolkit
default, complete engine View or measured runtime budget.
The following [native text-layout unit](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-native-text-layout-successor)
is verified locally at Open-GPUI `0861f8d52b1`, without publication in this continuation. It
preserves native tabs, Unicode separator baselines, default/explicit typography and wrapped
hit-test errors. Core 877, affected consumers 2,964 and selected Windows 173 pass; four existing
interactive cases remain skipped. The subsequent
[native-resource successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-native-upload-and-color-readback-successor)
is verified locally at Open-GPUI `f1850835fbb`, without publication. It adds shared checked
atlas admission, lease-preserving refusal, mapped color-staging release and transparent glyph
target initialization. Focused resources 13, affected Core 54 and complete Windows 186 pass,
with four existing interactive cases skipped. The following
[pending-input/native-handler successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-pending-input-and-native-handler-successor)
is verified locally at Open-GPUI `cde23d44ada`, after cache-fixture correction `fb196f8a9c8`.
It preserves admitted focus/context ownership, modifier routing, original deadlines and exact
completed-action observation. Core 901, affected consumers 2,964 and Windows 187 pass, with
four existing interactive cases skipped. All five initial correctness units have executed scope;
the wider framework refresh, Nara integration, live OS IME, non-Windows execution and measured
performance remain open. The following
[GPU-record/color-transfer successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-gpu-records-and-windows-color-transfer)
is verified locally at Open-GPUI `871c012c96c`, without publication. It adds initialized shared
flag storage, direct DirectX sprite upload and corrected Windows
transfer powers. Core/WGPU 941, affected consumers 2,964 and Windows 188 pass, with four
existing interactive cases skipped. Actual Windows pixels, cached/deferred replay and exact
release pass through both debug runtime shaders and embedded FXC artifacts. These results do
not qualify macOS header/Metal, WGPU scene pixels, Nara rendering integration, measured costs
or a new Editor/game default.
The following [font-installation/reporting successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-runtime-font-installation-and-missing-glyphs)
is verified locally at Open-GPUI `ed2a55383f411fec6eb6a87b410c49fbeb83aadb`, without publication.
Descriptor/layout/width generations, bounded App-owned missing-grapheme delivery, stable Cosmic
face identities and exact public-context TextSystem ownership retain candidate recovery and
release. Core/WGPU 999, affected consumers 2,964 and Windows 189 pass; four existing interactive
cases remain skipped. Native existing-family reselection and Windows/CoreText missing reporting
remain open, as do the remaining framework refresh, Nara integration and measured budgets.
The subsequent [native-font successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-native-font-reselection-and-retained-faces)
is verified locally at Open-GPUI `b803d542b674af34feea0ca277b2ac6721de9a0e`, without publication.
DirectWrite selection/fallback refresh and retained actual native faces pass focused 9 and complete
Windows 197 cases, with four existing interactive cases skipped. CoreText source and five tests
include configured cascade identity and reviewed attribute/traits corrections; Mac compilation/
execution remains unqualified. The earlier shared Core/WGPU/consumer results keep their original
scope. Variable-axis reconstruction, OS font watching, the remaining refresh, Nara integration
and measured budgets remain open.
The subsequent [Cosmic paragraph successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-cosmic-paragraph-shaping-and-public-paint)
is verified locally at Open-GPUI `f48d20d27862e45ff27fcd7ba4f261fd9119d0f7`, following Core
correction `3d0714f44e2`, without publication. It adapts `c214057e08`, preserving original
run/glyph geometry and full-source missing-grapheme reporting through B-class segments.
Its public consumer also qualifies a bounded Core terminal
closest-hit correction. Complete Core/WGPU/Windows plus public paragraph consumer passes
1,207/1,207 with four existing interactive Windows cases skipped. Affected UI core/components,
docking and Canvas pass 2,964/2,964 with no skips. Actual Cosmic Hebrew raster and public
paint/reconstruction retain headless/software scope. Full RTL caret, configured
fallback priority, Unicode wrapping, remaining platforms, Nara integration and measured budgets
remain open.
The subsequent [Unicode wrapping/indentation/truncation successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-unicode-wrapping-indentation-and-truncation)
is verified locally at Open-GPUI `b466037e87e17f11e7acce70706b7866edda3ca1`, following separate
compiler corrections `8d35827da9a` and `09fe9d2ec46`, without publication. Existing wrapper,
layout/style and TextRun owners now handle UAX opportunities, explicit indentation, Middle and
whole-grapheme cuts, with real paragraph budgets and candidate recovery. Core/WGPU/Windows plus
public text consumers pass 1,250/1,250 with four existing interactive Windows cases skipped;
affected UI/component/docking/Canvas consumers pass 2,964/2,964 and the independent public API
tests pass 3/3. Selected Windows framework/independent checks pass on actual Rust 1.92.0.
Cosmic headless/software paint and actual Windows Submitted frames keep their own scope.
Remaining fixed-target text/layout/API/platform work, full script/caret support, numeric budgets,
Nara rendering integration and complete R11/R12 remain open; no production default changes.

The subsequent [font preparation/inventory/raster successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-font-preparation-inventory-and-raster-reuse)
is verified locally at Open-GPUI `1d148d66dfdf89115046e08d6e01923b4b2e2165`, following separate
native alias-identity correction `19a4ed3434a`, without publication. Explicit prewarming shares
existing resolution/match owners; backend-only family inventory and bounded Cosmic measured-image
handoff preserve source, old glyphs and exact release. Final Core/WGPU/Windows plus public text
passes 1,268/1,268 with four existing interactive Windows cases skipped; affected components/
docking/Canvas pass 2,964/2,964 and independent public API tests pass 5/5. Locked compilation,
correctness Clippy, formatting and actual Windows Rust 1.92.0 affected-framework/consumer checks
pass. Native Submitted and Cosmic software/cold pixel scopes remain distinct. At that checkpoint,
the next text split/layer/underline unit had source-grounded acceptance rather than execution.
Remaining framework/platform work, complete fallback/script support, numeric costs, canonical docking,
Nara View integration and R11/R12 remain open; no production default changes.

The subsequent [source-fragment/layer/underline successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-source-fragments-text-layers-and-underline-exclusions)
is verified locally at Open-GPUI `55ddbcde39527629a28555b5eadb678cddfccd71`, without publication.
Shared raw/shaped source partition, explicit text layers and projected underline exclusions use
existing layout/Window/clip owners; corrected underflow and actual fragment/cover ordering retain
their reproduced regressions. Final Core/WGPU/Windows plus public text passes 1,281/1,281 with
four existing interactive Windows cases skipped; affected components/docking/Canvas passes
2,964/2,964 and independent API tests pass 6/6. Compilation, correctness Clippy, formatting and
actual Windows Rust 1.92.0 checks pass. Actual primitive order, software raster and native
Submitted evidence keep distinct scope. General bidi reflow/caret, existing ink overhang,
numeric costs, remaining framework/platform work, canonical docking, Nara View and R11/R12
remain open; no production default changes.

The following [window-text ownership successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-window-local-text-ownership-and-source-cache)
is verified locally at Open-GPUI `1f01e811c101e5a81e52e955942f6a24a8974902`, without publication.
It adopts official Rc/RefCell/Cell window storage and removal of unused hash APIs. Generation-coherent
cache invalidation, candidate recovery and backend reentry remain explicit fork adaptations.
A reproduced clamp-key defect shared by the starting fork and official target now has an owning
content-key correction. Corrected Core/WGPU/Windows plus four public text consumers pass
1,292/1,292, with four existing interactive Windows cases skipped; notify-only redraw and actual
native Submitted/reconstruction retain their complete source and lifetime checks. Remaining
framework/platform work, numeric costs, Nara integration and complete R11/R12 stay open.
Affected components/docking/Canvas pass 2,964/2,964 and independently locked public text tests
pass 7/7 without skips. Locked compilation, affected correctness Clippy, formatting and actual
Windows Rust 1.92.0 framework/consumer checks pass. This changes no execution/default ADR.

The following [Taffy/text/root/grid successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-taffy-text-measurement-and-rootgrid-sizing)
is verified locally at Open-GPUI `11d9a96033ea8425b07f64fdf2bad6503f1720dc`, without publication.
It adopts official leaf/alignment APIs, shaped-overflow and truncation measurement corrections,
MaxContent list probes, auto window/prompt roots, row sizing and opt-in stack growth.
The fork's additional ordinary-wrap cache correction has a reproduced finite-to-MaxContent
regression; an old portal expectation now preserves the root's authored inset without changing
portal/transform production. Corrected Core/WGPU/Windows plus five public consumers pass
1,312/1,312 with four existing interactive Windows cases skipped. Native kerning/Submitted,
headless paint and direct API evidence keep separate scope. Stacker-disabled owner/portal
cases pass 33/33, affected components/docking/Canvas pass 2,964/2,964 and independently locked
public text/grid tests pass 9/9 without skips. This selects no new layout owner,
numeric cost result, broader platform qualification or Editor/game default. Locked compilation,
affected correctness Clippy, formatting and actual Windows Rust 1.92.0 framework/independent
checks pass; official mechanisms and the fork inherited-cache correction remain distinct.

The following [shared image successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-shared-image-decoding-and-animation)
is verified locally at Open-GPUI `995ac8576362e43f36364616b33cc45c9560fab7`, without publication.
Official EXIF/GIF/WebP helpers replace inherited duplicate decoding; contextual error behavior
and the fork's explicit dependency minimum/executor-clock/deadline adaptations retain separate
provenance. Isolated loading notifications, deterministic animation/static idle and actual Windows
Submitted/GPU-readback/replacement/retirement keep distinct public/native scope. Complete affected
framework/public, component/docking/Canvas and independent suites pass 1,328/1,328,
2,964/2,964 and 12/12; four existing interactive Windows cases remain skipped. Locked checks,
correctness Clippy, formatting and actual Windows Rust 1.92.0 framework/consumer checks pass.
This changes no execution/default ADR. Remaining image features, API/platform/external-loop/
renderer lifetime, numerical costs, canonical docking, Nara View and full R11/R12 remain open.

The following [externally driven App successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-externally-driven-application-lifetime)
is verified locally at Open-GPUI `e4364085634906472d23dab0efe0b9305fab7a04`, without publication.
Official strong ApplicationHandle/run_embedded use existing launch ownership; fork updates use
the qualified App transaction, with explicit weak/delayed-launch/reentry/shutdown behavior.
The Windows visual-test host qualifies actual stateful Submitted/readback and observed native/App
release, not production Windows external-loop support, all renderer resources or OS-input/timing.
Complete affected framework/public, component and independent suites pass 1,339/1,339,
2,964/2,964 and 14/14; four existing interactive Windows cases remain skipped. Locked checks,
correctness Clippy, formatting and actual Windows Rust 1.92.0 framework/consumer checks pass.
Owning asset loads, display observation, conditional hot reload, other closure differences,
numeric costs, canonical docking, Nara integration and full R11/R12 remain open. No default ADR
changes.

The owner's [official-baseline clarification](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-official-framework-baseline-and-reusable-additions)
defaults to retaining official mechanisms, refactoring fork-owned changes around them and
keeping additions usable through ordinary public modules. Local Open-GPUI `bf3e0d2f7c6`
records the maintenance rule; it adds no runtime qualification. The following
[owning asset-load successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-owning-asset-loads-and-view-completion)
is verified locally at `1855a94c5eeb7e0a97e69479232211827efc3942`, without publication.
Official CachedLoad/Option/opaque-entry ownership serves App, Window, retain-all and the actual
gallery LRU; fork publication waits on existing AppCell releases and uses the complete update
transaction. Focused owner/image/LRU/candidate/native cases pass 22/22. Final affected runtime/
public/gallery, components and independently locked consumers pass 1,361/1,361, 2,964/2,964
and 16/16, with four existing interactive Windows cases skipped. Native automatic two-window
submission, passive no-redraw, explicit refused-candidate retry and image/native retirement
retain their distinct scopes. Locked checks, correctness Clippy, formatting, existing scans and
actual Windows Rust 1.92.0 framework/consumer all-target/all-feature checks pass. Dependency
versions and the independent lockfile remain unchanged. Remaining fixed-target reconciliation,
numeric budgets, canonical docking, Nara GPU/View and full R11/R12 stay open; no default changes.

The following [connected-display successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-connected-displays-and-window-association)
is verified locally at Open-GPUI `e5ad73a24a69274378264f792f4efb4d6d892ac6`, without publication.
Official App/Window observation, refresh intervals and launch initialization reuse existing
topology/facts/native ingress owners. The record retains the official reciprocal-overflow and
fork deferred-close corrections, startup fixture changes and required Core bitflags declaration.
Complete runtime/public/gallery, components and independent consumers pass 1,388/1,388,
2,964/2,964 and 18/18; four existing interactive Windows cases remain skipped. Locked checks,
correctness Clippy, formatting, existing scans and actual Windows Rust 1.92.0 checks pass.
Production Core/WGPU/Web compile for wasm32-unknown-unknown; unchanged wait-timeout prevents
Web test-support compilation. Native foreign platforms, browser/physical display changes,
numeric costs, telemetry/hot reload, remaining closure differences, canonical docking and Nara
GPU/View integration stay open. This does not complete R11/R12 or change the execution default.

The following [unified-profiler foundation](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-unified-profiler-foundation-and-accepted-frame-measurements)
is verified locally at Open-GPUI `3e5a907775e9f22d0a092a0f9f27c6f1696ade0e`, with qualification
record `a536b1d93380cc9247a7cc9576ac6b81cdb65489`, without publication. Official profiler
feature gating, safe thread-owner collection and WindowProfiler replace the fork's separate
input tracker. Candidate/input/reset/unwind integration and inherited trace/action/task ranking
corrections retain distinct source and regression evidence. Full runtime/public/gallery and
UI/component/docking/Canvas suites pass 1,426/1,426 and 2,966/2,966, with four preexisting
interactive Windows skips in the former. Disabled Core 1,095 and independent enabled/disabled
20/18 retain their scope, with final targeted feature checks. Actual native Submitted/release/
reconstruction and full-size Canvas benchmark smoke execute; no numeric overhead claim follows.
Locked checks, correctness Clippy, formatting, existing scans and actual Windows Rust 1.92.0
framework/Canvas/consumer checks pass. Production wasm compiles with profiling on and off;
browser/test-support and foreign native-platform qualification remain open. Journal/hang/monitor,
frame-signal, refresh-aware measurement, power/cadence and upstream benchmark/debug-overlay
consumers remain queued. Fixed-target parity, numeric costs,
hot reload, canonical docking, Nara GPU/View and R11/R12 stay open; no default changes.

The following [lifecycle observation successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-6-system-power-and-presentation-visibility)
is qualified locally at Open-GPUI `8b283cb151b070d7d5851a531a86e6d6d67196cb`, with record
`8554f9e765b4fe3369480fcba2d0580ec008bd18`, without publication. Official App subscriptions
and Window/Context presentation visibility replace the singleton wake path. Mandatory backend
methods and public test-context controls preserve ordinary framework use without Nara-private
dependencies. Fork ingress/SetWindowPos adaptations retain exact ordering, retirement and native
Submitted evidence. Runtime/public/gallery/native 1,446, components/docking/Canvas 2,966,
disabled Core/public 1,113 and independent enabled/disabled 28/26 pass; four existing interactive
Windows cases are skipped in the runtime cohort. Locked checks, correctness Clippy, formatting
and actual Windows Rust 1.92.0 framework/Canvas/consumer checks pass. Production wasm and
foreign-backend source review remain distinct from browser/native foreign-platform execution.
Synthetic power messages do not qualify physical suspend. Numeric costs, later journal/monitor/
signal/cadence owners, full fixed-target refresh, docking and Nara GPU/View integration remain
open; this prerequisite changes no default and does not close R11/R12.

The following [foreground journal/monitor successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-7-foreground-journal-and-hang-monitor)
is qualified locally at Open-GPUI `8493e9eb1cad7c3336fa99b4b8749c5be1d6c7c5`, with record
`84407f27ffacac0f2f5945c61220eb7567e7ac54`, without publication. The official measurement-v2
checkpoint through `d284cbbc87` supplies bounded publication, independent collectors,
lifecycle/suspend-aware boundaries and the opt-in background monitor. Fork candidate/unwind,
demand, task-count and native/Web admission corrections retain source distinctions and real
red/green controls. Complete runtime/public/gallery/native 1,531, components/docking/Canvas
2,966, disabled Core/public 1,114 and independent enabled/disabled 32/26 pass, with four
preexisting interactive Windows skips in the runtime cohort. Native exact Submitted, journal
cursors, reconstruction and monitor Flush/callback release execute. Locked checks, correctness
Clippy, formatting and actual Windows Rust 1.92.0 framework/Canvas/consumer checks pass. Stable
wasm on/off and optional existing nightly shared-memory compilation retain separate scope from
browser and native foreign-platform execution. Publisher-tail drain, accepted-backlog retirement,
physical suspend, numeric costs, final-target frame-signal/refresh-v3/cadence and complete
benchmark/report/debug-overlay comparison remain open. Full refresh, docking, hot reload, Nara
GPU/View and R11/R12 remain incomplete; no default changes.

The following [platform-signal/completed-skip successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-7-platform-frame-signals-and-completed-skips)
is qualified locally at Open-GPUI `5811543bebe10657b9f99b42bfadb214e7dad999`, with record
`c7fb5ccd8a81ee8221fb90711b02649516b4c4fa`, without publication. Official captured source/time,
raw diagnostics and nonempty completed skips retain pending-demand semantics; fork ingress and
exact native registration/retirement remain explicit. Final Core/Windows library/public 1,482,
components/docking/Canvas 2,966, disabled Core/public 1,122 and independent enabled/disabled
38/29 pass. Four existing native interactive skips and an earlier gallery fixture skip retain their
scope. Earlier WGPU/gallery contributes 228 passes; initial clipboard failure, unchanged rerun,
interrupted final run and later complete Windows success remain recorded. Locked checks,
correctness Clippy, formatting, actual Windows Rust 1.92.0 and stable/nightly production wasm
configurations pass; native foreign-platform, browser and numeric-cost scope remain unqualified.
The inherited official demand-driven platform-loop prerequisite `eb354c8d50` and X11 Expose/
recovery have the following scoped qualification before refresh-aware v3 and power/cadence;
they are not rejected fork differences.

The following [demand-driven platform-loop successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-7-demand-driven-platform-frame-loops)
is qualified locally at Open-GPUI `b6506b48efca022c13256ef17d5ecfa26787b2ae`, with record
`9901ed0bcc6272a647bccfb73016f3f22206a013`, without publication. Official weak wakeups and
schedule/wake/park use existing framework/platform owners; the old completion-driven hook is
removed. Fork post-borrow maintenance, exact candidate/Submitted authority, unwind recovery and
retirement have real consumers. Focused Core/public 62, actual production-state 27 and affected
native five pass. The full runtime cohort passes 1,748 of 1,749, with one unchanged native image
timeout passing its unchanged isolated rerun; its cause remains undiagnosed and five preexisting
skips remain explicit. Components/docking/Canvas 2,966, disabled Core/public 1,134 and independent
enabled/disabled 40/31 pass without skips. Locked checks, correctness Clippy, formatting, actual
Windows Rust 1.92.0 and stable/nightly production wasm configurations pass. Native foreign-platform,
browser execution and numerical costs remain unqualified. Complete refresh-aware v3 is planned
next, then power/cadence and later MetalFrameSource. No default or execution ADR changes; full
refresh, docking, hot reload, Nara GPU/View and R11/R12 remain incomplete.

The following [refresh-aware measurement-v3 successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-7-refresh-aware-frame-and-hang-measurement-v3)
is qualified locally at Open-GPUI `411895475e9e51a9f06954d5d676333d597b991f`, with record
`110f9c88084227eb526515167c204af3555f22a8`, without publication. Official per-refresh frame
work, view self-time and v3 detection/monitor rules use existing owners. Exact candidate/Submitted
custody and four reproduced inherited sealer/active-use fixes retain public, property and native
evidence; chronological input controls add no work or boundary owner. Focused Core/public 208,
native two and complete framework/public/Gallery/state/native 1,809 pass, with five preexisting
skips in the complete cohort. Components/docking/Canvas 2,966, disabled Core/public 1,134 and
independent enabled/disabled 46/32 pass without skips. Locked checks, correctness Clippy,
formatting, actual Windows Rust 1.92.0 and stable/nightly production wasm configurations pass.
Native foreign-platform/browser execution and numeric costs remain separate. Generic View/
ViewElement is a named queued official-interface gap, not an intentional entity-only design.
Power/cadence and MetalFrameSource follow next. No default or execution ADR changes; complete
refresh, publisher-tail/backlog retirement, docking, hot reload, Nara GPU/View and R11/R12 remain open.

The following [power/cadence successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-7-power-modes-and-stable-frame-cadence)
is implemented locally at Open-GPUI `2ceb09aa4590388b8e2d73ac1406caddc5b6c470`, with
diagnostic successor `1a6d90bdaac5256c36064be9824459de74ea91bf` and record
`a89a7200c80fc40eb073dbad37789f17153ec7b8`, without publication. Official power
observations, longest configurable limits, stable cadence and macOS
Metal/CV sources use existing owners. Public consumers remain independent of Nara. Focused
Core/public 76, Windows native 14 and portable production-state three pass. Native execution
reproduces and corrects an inherited fork retirement-retry Rc leak, retaining exact weak-owner
release. The complete framework run passes 1,849/1,850 before an IME fixture correction; final
Windows selection passes 220/221 and the final image/activation recheck passes 3/3. Recorded
capture/activation failures remain unattributed; diagnostic changes retain all original submission,
pixel and terminal conditions. Components/docking/Canvas 2,966, independent enabled/disabled 51/36
and disabled Core/public 1,162 pass. Locked checks, correctness Clippy, formatting, actual Windows
Rust 1.92.0 and stable/nightly production wasm pass. Foreign native execution, physical timing/
energy costs and a clean complete Windows gate remain open. No default or execution ADR changes;
complete refresh, generic View, report/hot reload, docking, Nara GPU/View and R11/R12 stay open.

The [generic View/component successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-7-generic-views-and-reusable-component-interfaces)
is implemented locally at Open-GPUI `652122a4e118ff2d213ae0d9392c71f49c44553b`, starting at
`a89a7200c80fc40eb073dbad37789f17153ec7b8`, with qualification record
`18e5f46e87f919049628cd9fd1be782c0fbaf49a`. Complete official embedding,
typed/erased/weak conversion, stateless scopes, macro and entity-cache interfaces replace the
inherited Component/direct AnyView Element owners. Shared phases preserve fork admission,
subscriptions and unwind. Initial owners 117, public five, actual native four and independently
locked enabled/disabled 53/38 pass. The complete framework/public/Gallery/native cohort passes
1,856/1,856 with five existing skips; components/docking/Canvas 2,966/2,966; profiler-disabled
Core/public 1,167/1,167; whole-workspace all-target, correctness Clippy, formatting, static scans
and affected Rust 1.92 checks pass. A final diagnostic caller/macro-entry correction has separate
enabled/disabled public 5/5, native 1/1 and affected Rust 1.92/Clippy/fmt evidence; earlier complete
cohorts keep their pre-correction scope. Synthetic accessibility/diagnostic publication, source-mapped
framework slices in product commits, report/benchmark/hot reload, canonical docking, numerical
costs and Nara GPU/View remain queued. No execution ADR/default or R11/R12 status changes.

The [measured-query/List successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-7-measured-layout-queries-and-list-scrolling)
is implemented locally at Open-GPUI `e58b8b7687aadc91408657b4e550a1b7d723c0a9`, after utility
`787ab55a8f68daab1dc48e2c80d12f4109028bdb`, starting at
`18e5f46e87f919049628cd9fd1be782c0fbaf49a`, with record
`aef505a3ba9c1d2e6b4fcf079f2298aa1e239235`. Complete official List,
TextLayout, Rems, os-release and shell/path/key framework slices use existing owners. Admitted
autoscroll and private revisions retain actual atlas/source rejection and exact recovery evidence.
Three List red controls precede focused 54, complete Core/public 1,390, utility 115, WGPU 89 and
affected native five passing results. Public software query/paint and Window rem-size/layout
consumers remain independent of Nara; ordinary typed closures have signature-only scope.
Complete components/docking/Canvas 2,966, disabled Core/public 1,181 and independent enabled/
disabled 56/41 pass without skips. Locked checks, correctness Clippy, actual Rust 1.92, production
wasm, formatting and existing static scans pass. Native five retains affected-regression scope;
no complete Windows/foreign-native/browser claim follows. Seven threads close. No default or execution
ADR changes; complete refresh, benchmark/accessibility/hot reload, canonical docking, numeric
budgets, Nara GPU/View and full R11/R12 remain open.

The [synthetic-accessibility/explicit-inspection successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-7-synthetic-accessibility-and-explicit-published-tree-inspection)
is implemented locally at Open-GPUI `751e563eb89eeb373dac41cf734bfe372b81d0e4`, starting at
`aef505a3ba9c1d2e6b4fcf079f2298aa1e239235`, with record
`156d0b8c1835108223bcf9fdd1f7d7c39a396013`. Official custom-element/
Div synthetic children and complete raw inspection retain the existing publication, geometry,
rollback and lifecycle owners. ADR0023 separates explicit developer copy from automatic redacted
artifacts. Final Core/public 1,412, components/docking/Canvas 2,966, WGPU/affected native 96,
disabled Core/public 1,203 and Gallery 13 pass. Independently locked both-mode compilation and
new type-only 2/2 checks pass. The six-case non-debug serializer and seven native synthetic
activation/Submitted/release trajectories retain separate scope. Locked checks, Clippy, actual
Windows Rust 1.92, production wasm, formatting and static scans pass. Eight threads close.
Other official focus/keyboard/tab-group/property/configuration mechanisms remain queued; matching
API names are not equivalence. No Nara execution ADR/default or R11/R12 status changes.

The [composite-focus/keyboard successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-8-composite-focus-and-keyboard-continuity)
is implemented locally at Open-GPUI `53decdc64a2d182fad1fbf00ababcd2b55a906e7`, after
`156d0b8c1835108223bcf9fdd1f7d7c39a396013`, with qualification record
`37ceac021fec7efb5091334bc60877ba36ab75d6`. Official active-descendant, keyboard continuity,
Tab-group and missing-node mechanisms reuse existing final-tree, actual-focus and accepted
publication owners. Concrete rollback/refused-arm failures precede their corrections. Focused
104, disabled affected 111, complete Core/public/WGPU/Windows 1,770 and components/docking/Canvas
2,966 pass; four interactive Windows cases remain skipped. Native production callback/Submitted/
release evidence remains distinct from OS/AT consumption. Independent type-only targets pass
4/4 in each mode; Gallery 13, locked checks, affected Clippy, actual Windows Rust 1.92 and stable
production wasm, formatting and existing static scans pass. Seven threads close.
Remaining property/configuration/initial-tree mechanisms, benchmark/report, hot reload, canonical
docking, numerical budgets and
Nara GPU/View remain open. No Nara execution ADR/default or R11/R12 status changes.

The [property/configuration/initial-tree successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-8-accessibility-properties-configuration-and-initial-trees)
is implemented locally at Open-GPUI `d9608b1a3174f17df270d6b92894539ec32654a8`, after
`37ceac021fec7efb5091334bc60877ba36ab75d6`, with qualification
`1670c373e34751ac520237b7d04a480b06ab7b4a`. Official property/configuration and synchronous
initial-root mechanisms reuse existing owners. The actual early-admission-revocation regression
is corrected while retaining exact queued WindowClosed settlement. Complete Core/public/WGPU/
Windows 1,787, components/docking/Canvas 2,966, disabled affected 182, independent type-only
6/6 per mode and Gallery 13 pass. Four existing interactive skips and an earlier undiagnosed
extra mouse transition retain their recorded scope. Locked checks, affected Clippy, actual
Windows Rust 1.92, stable production wasm, formatting and static gates pass. Seven threads close;
no remote publication follows. Initial reply, accepted tree, native Submitted and real AT remain
distinct evidence. Next comes the complete benchmark harness/report owner through existing
queue, dispatcher, text, platform, context and measurement modules, with actual public consumers.
That source mapping does not qualify execution. Full refresh, hot reload, numerical budgets,
canonical docking and Nara GPU/View remain open; no Nara execution ADR/default or R11/R12 changes.

The [threaded-execution/queue-closure prerequisite](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-8-threaded-execution-and-queue-closure)
is implemented locally at Open-GPUI `018fbdc5f708d91f0ae16a7fb43ac04280ebb680`, after
`1670c373e34751ac520237b7d04a480b06ab7b4a`, with qualification
`916c7cb2d2fffcaa95199598655b5cef97fb9bda`. Official concrete
worker/main/timer and closure mechanisms reuse existing platform, queue, executor and profiler
owners. An executed blocked-receiver failure precedes its correction; reviewed focused and
public runtime controls pass 182/182. Existing fork admission, timing and refusal contracts
remain intact. Complete Core/public/WGPU/Windows 1,810, components/docking/Canvas 2,966,
disabled affected 35 and independent enabled/disabled 62/47 pass; four interactive skips remain.
Locked checks, affected Clippy, actual Windows Rust 1.92, stable/nightly production wasm,
formatting and static gates pass, with five threads closed. Linux active compilation/runtime
remains unqualified. This is the benchmark/report owner's prerequisite, without qualifying its
feature split, context, text/render injection, reports or numerical budgets. Full refresh, hot reload,
canonical docking and Nara GPU/View remain open; no Nara execution ADR/default or R11/R12 changes.

The [complete benchmark/context/measurement successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-8-complete-benchmark-context-and-measurements)
is implemented locally at Open-GPUI `5f3a99fad9284c17a8eaac508ab8b4f47018be44`, against
the unchanged fixed target, with qualification `7c7a1411009a07afbcbd70340256adf55161491b`.
Complete official threaded contexts, independent measurements, native text reuse,
seeded/parameterized macros and optional fallible no-readback rendering
replace inherited virtual/forwarding paths. Actual focused consumers pass 60/60, independent
benchmark execution completes nine cases, Canvas completes five smoke cases, and complete
runtime/public/Gallery/native/metrics/macros passes 2,040/2,040 with five documented skips.
Components pass 2,964 plus two separately gated Docking controls, preserving the prior 2,966-case
set; disabled affected 112 and independent enabled/disabled 62/47 pass. Locked checks,
correctness Clippy, actual Windows Rust 1.92, stable/nightly production wasm, formatting and
existing static gates pass. Inherited corrections and fork ingress/candidate/terminal-quit
adaptations are separately evidenced. All ten threads are closed;
no remote publication or Nara runtime change follows. Dev-profile functional execution is not
an optimized numerical budget or foreign platform/device-loss qualification. Official additive
test support and actual stateful hot reload remain complete successors. Full refresh, canonical
docking and Nara GPU/View stay open; no execution ADR/default or R11/R12/R11f changes.

The [additive test-support/leak-tracking successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-8-additive-test-support-and-explicit-leak-tracking)
is implemented at Open-GPUI `c6ffb32bd1499c9e8fb46783a3d1a4de626707a0`, from
`7c7a1411009a07afbcbd70340256adf55161491b`, with qualification
`da1b1fdecbd5bb57a17e8898650cf5592958085b`, preserving official
test facilities, per-executor runtime identity and independently selected handle instrumentation
through existing owners. Actual inherited home-directory and release-diagnostic failures are
corrected with original lifetime requirements retained. Util 116, focused Core/scheduler/public
130, the complete independent feature/environment matrix and full native View with tracking
off/on pass. The framework 1,918-case union, complete Windows 224, components 2,966 and ordinary
enabled/disabled 62/47 pass. The initial native extra-event source remains unproven; separately
committed fixture convergence preserves the exact assertions. Locked checks, correctness Clippy,
actual Windows Rust 1.92, production native/wasm, feature graphs, formatting and static gates pass.
All seven implementation/review threads are closed. No engine execution owner, performance
result or Nara runtime/default change follows. Conditional hot reload follows in the separately
qualified successor below; full refresh, canonical docking, Nara GPU/View and R11/R12/R11f remain open.

The [conditional native hot-reload successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-8-conditional-native-hot-reload-and-exact-terminal-settlement)
is implemented at Open-GPUI `ad70ac9230892ca4e4d47840b93215596b8c60e0`, after
`da1b1fdecbd5bb57a17e8898650cf5592958085b`, with qualification
`480430f02aecc3bdcf78bed8126ef6b871974770`. The complete official conditional native
transport and App/View patch boundaries retain existing owners. A real independently locked
panel edit/build/apply preserves mutated typed/erased/component state, identities and actual
DX11 replacement pixels, followed by retirement/shutdown/quit/exit zero. Its single 3,419.7 ms
observation is not a latency budget. The external upstream dx x64 backport and fork terminal
correction `28eebf0e8dcd0c8a71f1064bf9f66052b5d3a55c` retain separate provenance. Five
native failure controls pass an unchanged final rerun; the prior extra-refresh source remains
unproven. Affected lifecycle/Windows 310/313 and selected rerun 2/3 leave foreground restoration
unqualified while an external game owns foreground. Disabled owner/public 68, independent
enabled/disabled 62/47, locked checks, correctness Clippy, actual Windows Rust 1.92, non-debug
exclusion, production wasm, four formatting and existing static gates pass. All thirteen threads
close. This is framework body-patch evidence; arbitrary state-layout migration and the full
Nara Editor reload/recovery workflow remain unqualified. Full refresh, canonical docking,
numerical budgets, Nara GPU/View and R11/R12/R11f stay open; no execution ADR/default changes.

The [shared atlas/renderer successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-8-shared-atlas-ownership-and-retained-renderer-bindings)
is implemented locally at Open-GPUI `67e1360709ca48be74bf986f116b9c004ff5bd59`, from
`480430f02aecc3bdcf78bed8126ef6b871974770`, with November 9 qualification
`c3d31566ffd6cc55c6f2b98e5e33810812e27262`.
Official shared key ownership, shared headless state and WGPU preupload/separate binding/cache/
render-core mechanisms retain concrete fork leases and complete-frame refusal. Scene recording
is separately committed at `186deca5a46f9e5ed48614b3e631a051b305e96a`. A reproduced local
prepared-receipt region overwrite has an owning lease correction; focused 20 and complete WGPU
library/public 110 pass; strengthened Drop/Queue controls pass 21, components pass 2,966 and
independent enabled/disabled consumers pass 63/48. Locked compilation, correctness Clippy,
actual Windows Rust 1.92, stable/nightly production wasm, formatting and static gates pass.
The preceding full framework/public/Gallery 1,934/1,935 retains the existing foreground failure
and four interactive skips. Metal's thirteen new tests remain unexecuted on macOS. This changes
no Nara runtime/default ADR. Remaining native/platform qualification, measured costs,
engine adoption and full R11/R12/R11f remain open.

The [image dependency-policy successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-9-image-dependency-feature-policy)
is implemented at Open-GPUI `b4355a36824f0709f1b73f7480ec88a884a489bc`, with qualification
`aeeef7bcd377bd99cc527fcb718d7a3298b4633e`. The complete official retained-format policy
excludes default AVIF encoding; ordinary applications restore it through their own dependency.
Resolved three-platform graphs, final Core/public/native image 25/25 and independent
encoding off/on 3/3 and 4/4 pass. Locked compilation, correctness Clippy, actual Windows Rust
1.92 framework/ordinary-consumer checks, production wasm, formatting and existing static gates
pass. The separately refreshed smoke lock passes its own locked production wasm check.
Decoder/guessing limits, existing native foreground restoration, foreign execution and numeric
costs remain explicit. Remaining renderer/API/platform closure, canonical docking, engine
adoption and full R11/R12/R11f stay open; no Nara runtime/default ADR changes.

The [WGPU dependency/shader/WebGL successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-9-wgpu-dependency-shaders-and-webgl)
is implemented at Open-GPUI `38d3041acab90cc01cbc9984a8b738a8b2c3a194`, with qualification
`7cd732eda7f8c5c80e0d08b737504f720f6fa6f5`. Official pinned dependency/shader/backend
mechanisms and the fork's exact clip/transform/lease/refusal adaptations pass complete
WGPU/public 123, native Vulkan/renderer 3, components 2,966 and independent enabled/disabled
65/48. Actual Edge WebGPU/GL/fallback passes PNG/text/resize and original interaction controls;
forced failures remain explicit. Locked checks, native/wasm correctness Clippy, actual Windows
Rust 1.93 framework/three-consumer checks, production/shared-memory wasm compilation,
four-target graphs, formatting and static gates pass. Nara GPU source identity, actual
Host/View adoption, remaining framework/platform qualification, physical loss, DPI/numeric
costs, canonical docking and full R11/R12/R11f remain open.

The [frame-time debug-overlay successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-9-frame-time-debug-overlay-and-remaining-core-apis)
is implemented at Open-GPUI `73472ca230bac117f8cb417d0cd6b0c1364c980e`, with qualification
`968f836f48a097ef49e692c62f5d1734c657bb75`. Complete official bitmap/mode/history recipes
and all six original regressions reuse existing candidate/clip and accepted-draw owners.
Separate fork resource correction `d928c5459a8ed174d1a6835abd096b3e7a996b5a` qualifies exact
headless quiescence and public shutdown reentry. Final focused 20, framework/public 1,970,
actual native 5, components 2,966, disabled Core/public 1,350, ordinary enabled/disabled 67/48
and additive consumers 4 pass. Recorded native pixel failure precedes strengthened fresh-frame
observation without weakened assertions. Locked checks, correctness Clippy, actual Windows
Rust 1.93, production/shared-memory wasm, formatting and static gates pass. All six threads
close. Historical foreground variability, foreign overlay execution and numeric costs remain
explicit. Source-confirmed animation/clock, spring and notification successors are queued;
this is not complete residual parity or engine adoption. Nara runtime/default ADRs are unchanged;
canonical docking, actual GPU/Host/View, Editor/game-runtime selection and R11/R12/R11f remain open.

The November 9 [maintenance-boundary clarification](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-9-generic-animation-and-framework-maintenance-boundary)
keeps the current continuation responsible for fixed-target refresh and modular engine-integration
assessment before an independent Codex session maintains Open-GPUI from actual Nara feedback.
Immutable consumed revisions, framework regressions and complete engine-consumer qualification
connect the repositories; product document/ECS/history and GPU authority stay in Nara.
This records delivery ownership, not a new runtime/default decision or completed handoff.
The Spring successor's shared-solver direction is source analysis awaiting its owning Open-GPUI
ADR reconciliation and implementation.

The [generic-animation qualification](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-9-generic-animation-and-framework-maintenance-boundary)
records Open-GPUI implementation `4ed461f2b591d6e44d84abc742094740fbd607af` and evidence
`af682e37f36f8fc5de575ef485c1d247509a9d22`. Official non-Spring scheduling/image controls
preserve existing owners with a reproduced fork candidate-timer correction. Focused 26, final
image 3, framework/public/Gallery 2,138 with seven declared skips, actual native 1, components
2,966, disabled Core/public animation 1,310 and independent enabled/disabled 68/52 pass. Locked
checks, correctness Clippy, actual Windows Rust 1.93, production/shared-memory wasm, formatting
and static gates pass; all six lanes close. Complete Spring, notifications, final residual
reconciliation, native foreign execution and numeric budgets remain open. Concrete device/error,
encoding/submission, native renderer and retained-image gaps await the real Nara integration
consumer. This changes no Nara runtime/default ADR and does not close R11/R12/R11f or the later
framework-maintenance handoff.

The [Spring/motion qualification](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-9-stateful-spring-and-shared-domain-sampling)
records Open-GPUI `ecbbfa361ae4489b2de4121c5f0ca8b2a1d7aebb` and qualification
`b14967342f2665c1c23d1c6a41d24108cf0fc62d`. Complete official typed targets,
playback and Element-ID state share one pure solver with existing domain motion; duplicate
physics is removed. App reduced motion is an adapter floor, with source preferences unchanged.
Reproduced numeric/translated-position and refused-candidate failures justify bounded fork
corrections. Complete framework/components pass 2,171/2,974 at their earlier checkpoint;
final motion 94, affected public/domain 43, actual native Spring 1, disabled Core/public 1,340
and independently locked Spring 6/6 in each profiler selection pass. Broader ordinary consumers
pass 74/58 at their recorded checkpoint. Locked checks, expanded correctness Clippy, actual
Windows Rust 1.93, production/shared-memory wasm, doctests, formatting and static gates pass;
all thirteen lanes close. Source-confirmed notifications and five further Core successors
remain queued. Full refresh, numeric budgets, foreign execution, canonical docking, actual Nara
GPU/Host/View and complete Editor/game-runtime choice remain open. This changes no Nara
runtime/default ADR and does not close R11/R12/R11f or framework-maintenance handoff.

The [system-notification qualification](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-9-system-notifications-and-framework-response-ownership)
records Open-GPUI implementation `26639a9715aff09ffbc61b92af45014728f02f94` and evidence
`7d2c071b9550b93654a7ac2cc19a76f49a5b7a31`. Complete official Core/native/example behavior
uses existing ordered App updates, lazy registration and Windows native retirement. Corrected
Core/public 12, actual Windows notification controls 11 including Submitted pixels/retirement,
complete framework/public/Gallery 2,194 with eight existing skips, ordinary enabled 77 and
disabled notification 3, and disabled Core/public notification 12 pass. Locked checks, expanded
correctness Clippy, actual Windows Rust 1.93 framework/consumer checks, production/shared-memory/
independent-smoke wasm, formatting and existing static gates pass. All eight current lanes close
without remaining scoped P1/P2. Synthetic activation does not qualify actual OS delivery;
foreign native bodies remain source-only. Sendable foreground/Web Fetch and other queued
mechanisms, complete refresh, numeric budgets, canonical docking and actual Nara GPU/Host/View
remain open before maintenance handoff. This updates evidence only, with no Nara runtime/default
ADR change or R11/R12/R11f completion.

The subsequent [Send/Fetch/queue qualification](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-9-send-foreground-streaming-fetch-and-browser-queue-ownership)
records Open-GPUI implementation `ff4b3f4b95d48410c7c1d3820e2b2a98444aacab`, qualification
`f6f3d98d156cd2ada88253ff4e4606a0ee50664d` and separate queue prerequisite `72676731163`.
Complete official mechanisms retain existing dispatch/admission/lifetime owners. Actual HTTP,
8/16-worker queue and stable WebGL/WebGPU/Auto smoke pass. The original runtime/public aggregate
remains 2,233/2,235 with two topology-related creation failures and nine skips; unchanged failed
cases and both native Send workflows pass 4/4. Complete ordinary enabled/disabled consumers
pass 83/67, disabled Core/public 1,351 and components/docking/Canvas 2,974 pass. Locked checks,
correctness Clippy, actual Windows Rust 1.93 framework/three-consumer checks, production/shared-
memory wasm and final static/format gates pass after recorded resource interruptions. All ten
threads and owned fixture processes close. Full fixed-target refresh, actual Nara integration,
maintenance handoff and R11/R12/R11f remain open; no runtime/default ADR changes follow.

The [randomized-tree successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-randomized-tree-and-standalone-workload-qualification)
is implemented at `b42295d0e535e4105470e80873a6ad7832e81863`, with qualification
`770463485ac9aaf4150475fa84fe90c33fbd4507`, after `f6f3d98d156`. The complete official model, sixteen
mutations, six regressions and seventeen platform tree groups expose a fork wake feedback loop;
its reviewed custody correction is independently committed at `1756e8f79aa`. Corrected focused
181, complete framework/public/Gallery 2,014, components 2,974, dev/stacker and release/non-stacker
platform 115 each, independently locked benchmark 64, final native tree/animation 2, complete
Windows 242, ordinary consumers 91/67, additive consumers 6 and disabled Core/public 1,383 pass.
Locked checks, Clippy, actual Windows Rust 1.93, production wasm, feature graphs and static/format
gates pass. Original failed runs remain recorded. All eighteen lanes close; async clipboard/Web
paste and remaining fixed-target dispositions still precede the real engine seam. This changes no Nara
runtime/default, engine-integration selection or full R11/R12/R11f status.

The [asynchronous clipboard successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-asynchronous-clipboard-and-live-input-delivery)
implements the official Task/error/full-item hooks and actual browser text/image paste through
existing frame/input/lifetime owners at `5209f4e82c7cef3c095807fcf13cd16fe860b5c6`, with
qualification `7c6b74021cdebec7e03dd558240789797bfad344`. Focused 34, Windows clipboard/pending-input 2
and complete browser gates pass. Complete cohorts pass framework/public/Gallery 2,028, components 2,985,
Windows 243, ordinary consumers 96/69 and disabled Core/public clipboard 1,379. Locked checks,
Clippy, actual Windows Rust 1.93, production wasm and final static/format gates pass. Nine lanes close.
The desktop successor below separates its native scopes; the real Nara integration remains open.
No runtime/default or R11/R12/R11f completion follows.

The [desktop clipboard successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-desktop-clipboard-ownership-and-platform-scope)
separates qualified Windows `a7e47b311aa` from source-prepared Mac `fe5bb458f2e`, recorded at
`41bb30ab4f9`. Windows 244, public clipboard 7 and independent clipboard 5 pass, together with
Windows compiler-floor/static checks. Actual Mac compilation and twelve tests remain open after
the foreign-host media build failure. The credential successor, remaining fixed-target closure
and real Nara seam still precede maintenance handoff; no runtime/default or R11 completion follows.

The [credential-contract successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-credential-contracts-and-maintenance-boundary)
is implemented at `3f943ea373f4da87a4c8b0f7d5a84e7101874824`, qualified at
`6613f6b0075c7df818518687f06128c7b19b1619`, after separate clipboard fixture correction
`510cfc5613d`. Official Mac add-only/typed errors, Windows background/native copying and the
exact Linux dependency patch retain their existing owners. Focused Windows 10, complete Windows
255, public enabled/disabled 2 each and ordinary independent 2 pass, with actual storage,
Submitted status/readback and exact task/window/text/platform retirement. Locked/compiler/
Clippy/wasm/static checks pass; earlier clipboard failures remain recorded. Apple native and
Linux Secret Service gates remain open. Eight lanes close. The bounded public audit adds five
explicit residual mechanisms, beginning with runtime identity/secondary keybindings. Fixed
refresh and the first real Nara integration still precede a separate maintenance session;
Nara adopts each later framework revision through its own affected consumer qualification.
No runtime/default, extra credential/GPU owner or R11/R12/R11f completion follows.

The [runtime-host/keybinding successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-runtime-host-identity-and-keybinding-consumers)
adopts official `389c22e1c747` at `70b993c9660772b157546f46c4a65c0ebd4375e6`, qualified at
`7ead5105e56b29255ece592e62fd5f2250542131`. Actual host identity reaches parsing, real mapper
loading, labels, dispatch and command/component/DevTools consumers; owned sampler facts and
inherited debug-label corrections retain distinct responsibilities. Final framework/public/
Windows 2,130, command/components/DevTools/Gallery 1,230, other UI consumers 2,077, ordinary
all/profiler/off 102/88/72 and disabled Core/public 1,393 pass. Actual native timed-prefix
retirement and complete browser identity/cache/input/restoration execute. Windows compiler-floor,
Clippy/wasm/static checks pass, with exact skips and pixel limits in the record. Thirteen lanes
close. Restart arguments and other remaining fixed-target dispositions precede the real engine
GPU/Host/View consumer and separate maintenance handoff. Foreign-native execution, numeric
budgets and full R11/R12/R11f remain open; no Nara runtime/default or additional owner follows.

The [restart successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-restart-arguments-and-terminal-native-exit)
records Open-GPUI implementation `175a82b47f10b0bc2bb4b5920d54e1be5d70f977` and qualification
`0b7024f86af0beed8fde92887eb1c1387997085c`. Complete official native argument forwarding
uses existing platform owners; the inherited environment bug and fork queued-quit completion
have separate corrections and actual worker/recorder evidence. Core/public 1,765, utility 127,
ordinary all/off 105/75, disabled selected 59 and native follow-up 10 pass. Original aggregate
failures 2,160/2,161 and 268/269 on unchanged clipboard/keyboard controls remain unresolved;
follow-ups do not make either aggregate green. Locked Windows compiler, Clippy, wasm, fmt and
static gates pass; nine lanes close. Remaining fixed-target mechanisms and the real Nara
GPU/Host/View consumer still precede separate maintenance. This is scoped framework evidence,
with no new engine dependency, default or R11/R12/R11f completion.

The [scoped activity successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-scoped-activity-and-native-resource-ownership)
records implementation `2459aebecc9`, qualification `633b3fced1f` and separate inherited Linux
input migration `467dc18a169`, after Open-GPUI `0b7024f86af`. Official
guard/idle-sleep/App Nap semantics use existing task/platform owners, with distinct Windows
native request/reason and ordinary-process evidence. Complete ordinary all/off 108/78 and
focused 31 pass; the failed full aggregate retains its unchanged topology refusal. Linux
protocol/activity 11, final headless 1, Core/input 22 and public 9 pass, and both windowing
features compile. The missing PipeWire screen-capture prerequisite remains separately failed.
Disabled Core/public 24, Windows compiler-floor/Clippy, production wasm and format/static gates
pass; shared Apple topology, Mac native execution and actual power-policy effects are not
implied. No Nara owner, dependency, default or R11/R12/R11f completion follows.

The [native appearance successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-native-appearance-and-remaining-windowing-lifecycle)
records implementation `0b0ea541665` and qualification `07851e88984`. Official optional Mac
override/non-Mac no-op and the retained native getter use existing owners. Windows focused/
disabled 4 each, callback/query 23, corrected App-owner 43, independent all/off 3/2 and actual
Linux 4 pass; locked/compiler-floor/host Clippy/wasm/fmt/static gates pass. Apple compilation
still stops earlier at inherited media bindings, with GUI probe/titlebar/system-following
gates open. Complete live windowing is prepared, not implemented or shared-renderer support.
Four lanes close; fixed-target closure and the real Nara consumer still precede maintenance
handoff, without a new Nara owner, dependency, UI default or R11/R12/R11f completion.

The [startup/live-windowing qualification](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-live-windowing-and-framework-maintenance-handoff)
records implementation `c1d8204f2da` and evidence `e7b2d848f55`, following separate fixture
correction `1c33677716c`. Official persistent-runtime/mode/environment and platform policy paths
retain exact native retirement, recovery generations and public independent-executor custody.
Core/public all/off 126 each, final Windows 283, Linux library 162, configured software-native
4, components/Canvas/docking 2,990 and independent all/off 115/83 pass. Locked/compiler-floor/
Clippy/wasm/fmt/static checks pass. The failed aggregate's original clipboard trigger, default
WSLg pthread-exit SIGSEGV and actual Mac compilation/execution remain separate limits. Sixteen
lanes close. Complete fixed-target reconciliation and a real Nara GPU/Host/View consumer still
precede the separate framework-maintenance session. Later engine feedback must keep framework
public regressions and affected Nara acceptance together. Live windowing adds no shared renderer,
external pump, new Nara owner, dependency, default or R11/R12/R11f completion.

The [native-quit and verification successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-targeted-upstream-verification-and-native-quit-completion)
records Open-GPUI `10f14f08be8`: official bool completion/Windows Restart Manager uses existing
ordered shutdown and exact native retirement. Core 7, corrected Windows 6 and independent
all-target compilation pass; initial Windows 4/6 and corrections remain recorded. Bounded review
finds no outstanding P1/P2 and all three lanes close. Remaining sync trusts unchanged official
coverage and qualifies affected fork seams. Foreign termination, shared Apple topology, final
fixed-target closure and the real Nara GPU/Host/View consumer remain separate. Nara has only
evidence updates; no dependency, owner, UI default or R11/R12/R11f completion changes.

The [shared Apple successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-shared-apple-framework-and-actual-native-type-checking)
records Open-GPUI `8bd39cef78d`: official shared rendering/dispatch/text/services have one Apple
owner; Mac retains native lifecycle/input. Old compatibility and duplicate owners are removed
while exact fork frame/atlas/font contracts remain. Affected host/public compilation, portable
2/2, actual Apple/Mac production/all-target foreign correctness Clippy and the independent Apple
Keychain API pass, with failures/corrections and unexecuted native/iOS scope retained. All three
lanes close without reviewed P1/P2. Final fixed-target dispositions and the real Nara consumer
still precede maintenance handoff; no engine ownership/default or R11/R12/R11f gate closes.

The [cached-identity and residual successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-cached-identity-and-targeted-residual-reconciliation)
records Open-GPUI `1a14b2076ed`: official prefix-hashed IDs, borrow/log diagnostics and list
precision retain existing fork candidate/deferred/borrow/scroll custody. Final Core/public 32,
logging 1, independent all-target compilation, affected correctness Clippy and format/static
gates pass. All six lanes close without outstanding reviewed P1/P2. Verification trusts unchanged
official coverage and targets fork seams. The expanded source inventory retains complete layout,
utility/dependency and backend/render successors before the real engine consumer and maintenance
handoff. Nara has only evidence updates; no dependency/default, full refresh or R11/R12/R11f
gate closes, and no new native execution or measured speedup is claimed.

The [official element layout/drawing successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-official-element-layout-and-drawing-successor)
records Open-GPUI `ae10ff73c3e`: complete image geometry, SVG data/parsed/exact raster,
ContainerQuery, attached Anchored and provisional List anchors retain current fork clip/transform/
atlas/admitted-scroll custody. Final focused Core/public 90, actual WGPU 3, existing native Windows
1 and ordinary independent 1 pass; affected all-target compilation/correctness Clippy and format/
static gates pass. Three fixture failures/corrections and native/consumer limits stay explicit.
Scoped reviews leave no P1/P2 and all five lanes close. Remaining framework units and the real
engine consumer still precede maintenance handoff. Nara has only evidence updates; no dependency,
UI default, full refresh, numerical-cost or R11/R12/R11f gate changes.

The [official gestures and targeted-verification successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-official-gestures-and-targeted-fork-verification)
records Open-GPUI `1b0aee8b400`, qualified at `d1de3e380078`: official gestures/scroll/padding/
hover retain current fork clock, terminal input and committed geometry owners. Focused Core 33,
portable Web 4, independent API 6 and one actual WebGL trajectory pass, alongside affected
compilation/correctness Clippy, locked wasm and format/static gates. Six lanes close without
reviewed P1/P2. Verification trusts unchanged official coverage and targets fork seams; browser
DOM observations, Core output and physical-device limits remain distinct. Remaining framework
units and the real Nara GPU/Host/View consumer precede separate maintenance. Nara has only
evidence updates; no dependency/default, full refresh or R11/R12/R11f gate changes.

The [native file-drag/source-session successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-native-file-drag-and-source-session-custody)
records Open-GPUI `fc6c9dc4403`, qualified at `e314339a3ad`: official cursor, exit/lazy payload
and Mac/Wayland source mechanisms retain existing fork geometry, candidate and exact ingress
owners. Core 18, Linux URI 5 and ordinary API 3 pass; affected host/Mac compilation/correctness
Clippy, Wayland production compilation and format/static gates pass. Five lanes close without
reviewed P1/P2. Native cross-application execution, physical input and numerical budgets remain
unqualified. Remaining framework units and the real Nara consumer precede separate maintenance;
Nara has only evidence updates, with no dependency/default or R11/R12/R11f gate change.

The [process/dedicated/string successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-process-reaping-and-dedicated-execution)
records Open-GPUI `5b1810abbcc`, qualified at `b04321bac86`: official shared process reaping,
DedicatedExecutor/nonblocking delivery, native/wasm API gates and SharedString helpers retain
current fork owners. One reproduced official queued-sender cycle receives bounded weak dispatch.
Focused 35/35, overlapping dedicated follow-up 12/12 and independent public 3/3 pass; actual
dependency identity, affected Darwin/wasm source compilation, correctness Clippy and format/
static gates pass. Five lanes close without reviewed P1/P2. Actual Darwin/browser execution,
numerical budgets, remaining framework owners and the real Nara consumer remain open before
separate maintenance. Nara has only evidence updates, with no dependency/default or full
R11/R12/R11f gate change. Trust unchanged official coverage and verify actual fork seams.

The [official HTTP/download/archive successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-official-http-and-release-download-owners)
records Open-GPUI `8223d68d89b`, qualified at `0eb736ea91b`: official reqwest request-local
redirects replace the fork cache, while portable HTTP and independent release Tasks retain
the official dependency direction. Release 10/10, final native transport 9/9 and ordinary
public 4/4 pass; targeted fixes resolve actual dependency-health findings without new ignores.
Affected correctness Clippy, wasm production and format/static gates pass. Five unit lanes
close without reviewed P1/P2; a sixth only prepares unqualified path sources. Continue actual
path/utility migration and remaining target owners before the real Nara consumer and separate
maintenance. TLS fallback/foreign execution, download Task-drop cancellation and numeric
budgets remain unqualified. No Nara dependency/default or full R11/R12/R11f gate changes.

The [canonical path/utility successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-canonical-portable-paths-and-utility-migration)
records Open-GPUI `a4e6fd03642`, qualified at `e7b541398ec`: official Apache path types replace
duplicate utility implementations through actual utility/native-dialog/public consumers. UNC/
position parsing, lenient globs and extension sorting follow the complete target. Focused
89/89, independent public 5/5 and downstream real-home 1/1 pass; affected compilation/Clippy,
actual package Rust 1.93/wasm and format/static gates pass. Three lanes close without P1/P2.
No external package version/source changes follow. Remaining framework owners and the real
Nara consumer precede separate maintenance; foreign filesystem/browser execution and numeric
costs remain unqualified. No Nara dependency/default or full R11/R12/R11f gate changes.

The [utility composition/development-asset successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-utility-composition-and-development-assets)
records Open-GPUI `8e0517f5650`, qualified at `ba4f699d828`: complete official shell, JSON,
assets and lightweight helpers replace duplicate owners through real public consumers.
Focused 47/47, ordinary dev/embedded 4/4 each, release 1/1 and Windows restart 11/11 pass.
Concrete dependency-interface/advisory corrections pass root health and five independent
audits without ignores; affected compilation/Clippy, core-util wasm and format/static gates
pass. Four lanes close without P1/P2. Installed LLVM 22.1.6 resolves the initial higher-util
missing-clang failure, and the complete patched utility/compression wasm check passes; foreign
shell/file-limit execution, numeric costs, remaining framework owners and the real Nara
consumer stay open before maintenance handoff. No dependency/default or full R11/R12/R11f
gate changes follow.

The [source-over/native child composition successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-source-over-alpha-and-native-child-composition)
records Open-GPUI `37d211ea030`, qualified at `0b3582e5acc`: complete official DirectX/WGPU
alpha settings and below-child DirectComposition startup/recreation retain fork rendering
owners. Affected compilation, existing actual native/WGPU consumers 2/2 and format/static
gates pass; one bounded reviewer closes without P1/P2. Desktop child/WebView, independent
translucent-destination alpha, real device-loss visual proof, numerical costs and the real
Nara GPU/Host/View consumer remain unqualified. Continue remaining framework owners before
maintenance handoff. No dependency/default or full R11/R12/R11f gate changes follow.

The [Windows dialog/host safety successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-11-windows-dialog-threads-and-native-host-safety)
records Open-GPUI `34e6b6a963d`, qualified at `c32e1ae9498`: complete official native dialog,
registered accelerator, stationary-input and optional composition mechanisms preserve existing
retirement/pointer/panic owners. Affected 66/67 plus corrected recovery 1/1, actual HWND/STA
worker seams, affected compilation/Clippy, corrected independent all-feature compilation and
format/static gates pass. The consumer allocator edge matches root/official Windows 0.62.2
without package/source changes. Four scoped lanes close without unresolved P1/P2. Live modal
focus/overwrite/timer cancellation, numeric costs, remaining framework owners and the actual
Nara GPU/Host/View consumer stay open before maintenance handoff. No dependency/default or
full fixed-target/R11/R12/R11f gate changes follow.

The [native state/attention successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-11-native-window-state-and-attention)
records Open-GPUI `9b671ed5148`, qualified at `62b51cf0e48`: complete official restoration,
attention and urgency use existing post-borrow and exact native lifetime owners. Owned
archives and teardown refusal add no separate placement, portable layout or persistence
authority. Corrected Core/public 25/25, actual Windows lifetime/no-op 5/5, independent
default/runtime 1/1 and 2/2, affected host/consumer compilation, host correctness Clippy,
Mac all-target foreign correctness Clippy, Linux X11/Wayland and Web production compilation
pass. Four scoped lanes close without remaining P1/P2; original test/binding/toolchain
failures and their corrections remain recorded. Actual native archive/Space/attention,
the earlier WSLg exit failure, numerical costs, remaining framework owners and the real
Nara GPU/Host/View consumer stay open before maintenance handoff. No dependency/default or
full fixed-target/R11/R12/R11f gate changes follow.

The [native control/popup successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-11-native-window-controls-and-popup-retirement)
records Open-GPUI `77b56a6af1b`, qualified at `038688f1810`: complete official regions,
layer-shell controls, simple fullscreen and native popup roles retain current command,
creation and presentation/native retirement owners. Reproduced premature-terminal retention,
reserved creation and quiesced dismissal defects are corrected through existing admission/FIFO
boundaries. Final Core/public 124/124, disabled close 6/6, Linux focused 26/26, Windows native
3/3, independent default/runtime 2/2 and 4/4, actual expanded nested-Wayland popup 1/1 and
existing X11/Wayland windowing 2/2 pass. Host/Linux correctness Clippy, independent all-feature
compilation, headless compilation and format/static gates pass; earlier Mac/Web compile
checkpoints retain their separate scope. Nine lanes close without P1/P2 and the exact owned
compositor is stopped. Native Apple effects, grabs, actual compositor dismissal, input-region/
layer behavior, numerical costs, remaining framework owners and the real Nara GPU/Host/View
consumer remain open. No dependency/default or full fixed-target/R11/R12/R11f gate changes follow.

The [Web text/viewport successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-11-web-text-sessions-and-viewport)
records Open-GPUI `7a79a113733`, qualified at `37b409123d6`: complete official text and
viewport owners preserve accepted sessions, input leases and post-borrow command ordering.
Final focused 16/16, affected Core/public/Windows 53/53, independent default/runtime 1/1 and
2/2 and actual text/clipboard/identity/touch browser workflows pass, alongside affected
compilation/Clippy and format/static gates. Nine lanes close without reviewed P1/P2; original
failures remain recorded. OS IME/mobile/safe-area hardware, Apple execution, numeric costs,
remaining framework owners and the real Nara GPU/Host/View consumer remain open. No engine
dependency/default or full fixed-target/R11/R12/R11f gate changes follow.

The [official Canvas fallback successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-11-official-canvas-font-fallback)
records Open-GPUI `f2526df594a`, qualified at `6ac5ffddb98`: complete official font fallback
and selected-face APIs reuse TextSystem/atlas. Reproduced family/import defects are corrected;
old tests follow official browser activity. Cosmic 52/52, final Web 47/47, independent 1/1,
complete real WebGL/WebGPU/Auto smoke and affected Clippy/wasm/format/static gates pass.
Five lanes close without reviewed P1/P2. OS IME/mobile/coverage elsewhere, contextual CJK,
numeric costs, remaining framework owners and the actual Nara GPU/Host/View consumer remain
open. No engine dependency/default or full fixed-target/R11/R12/R11f gate changes follow.

The [Linux IME and background-effects successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-11-linux-ime-background-effects-and-xkb)
records Open-GPUI `cf4bd2c6db1`, qualified at `4ca8269dd55`. The official Wayland/XKB/X11
mechanisms retain existing input and surface owners. Affected 48/48 and final 4/4 pass;
Core observes effective background without granting live alpha writes. X11 readback/submission,
Wayland fallback/retirement and separate backend builds are qualified. Real compositor effects,
KWin/Fcitx, remaining fixed-target units and the actual Nara consumer stay open.

The [Mac AppKit and capture successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-11-macos-native-input-appkit-and-screen-capture)
records Open-GPUI `0c1118e2f1a`, qualified at `60b7e61d9b5`. The official typed Mac mechanisms
remove the Cocoa shim and Media package while preserving input, display and retirement owners.
Apple-target checks and ordinary capture-to-surface compilation pass. Actual Apple execution,
renderer/logging/API dispositions and the real Nara GPU/Host/View consumer stay open.

The [renderer batching successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-11-renderer-batching-and-hollow-borders)
records Open-GPUI `36362708133`. Windows WARP and WGPU pixel evidence and ordinary renderer
consumer checks qualify this import. Metal remains compile-only. Logging, remaining API/dispatch
dispositions, numerical costs and actual Nara GPU/Host/View acceptance stay open; there is no new
Nara dependency or default decision.

The [standalone logging and Window successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-11-standalone-logging-win32-dispatch-and-window-interfaces)
records Open-GPUI `9e7b9c3a36f`, `9d9602bb46d` and `655e9610c77`: official Apache logging/tracing and
Win32 workers/timers retain application globals and the existing foreground/native owners.
Logger/SumTree 27/27, independent ordinary hosts 2/2 and 3/3 and actual Rust 1.93/wasm checks
pass. Window/Inspector follows admitted focus, geometry and unwind recovery; affected
Core/public/Windows 53/53 and Inspector Gallery 2/2 pass. The Wayland follow-up
`c0a36ddce01` restores the official inactive-window gate and passes affected cache/native
13/13. The remaining InputPreference event, FocusMap drop queue,
focus-lost ancestor query, final fixed-target dispositions and real Nara
GPU/Host/View consumer still precede maintenance handoff. No Nara dependency, UI default or
full R11/R12/R11f gate changes follow.

The [input/focus and hover successor](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-11-input-priority-focus-lifetime-and-hover-readmission)
records Open-GPUI `571484691ea` for official InputPreference, final-drop FocusMap and nearest
ancestor recovery, and `8ff031e95f2` for fork Hidden/Inert hover readmission. Final affected
113/113 with and without profiler, native input/retirement 3/3, explicit native profiler 1/1,
ordinary consumers 1/1 and 2/2 and correctness Clippy pass. Final source disposition, real Nara
GPU/Host/View, numerical budgets and maintenance handoff remain separate; Nara gains no new
dependency, owner or default and full R11/R12/R11f remain open.

The November 10 [modular GPU/Host source map](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-10-modular-gpu-and-host-source-map)
identifies existing Nara device scopes, immutable output leases, actual composite receipts and
producer-local retirement. GPUI's current native renderer still owns device/surface/submission,
lacks public external-target recording and skips its surface primitive on WGPU; its normal
Windows text/renderer path is coupled to DirectX devices. The two WGPU sources also have distinct
Rust identities. The plan now names these actual prerequisites and compares reusable framework
recording with a Nara Platform adapter, retaining one GPU/submission/loop arrangement. No source
map, embedded App lifetime, atlas test or CPU Host control qualifies a real engine View. Both
framework closure and that consumer remain required before maintenance handoff. No new Accepted
decision, dependency change, toolkit default or R11/R12/R11f completion follows.

The November 11 [layered integration study](../../research/2026-11-11-open-gpui-engine-integration-layers.md)
reconciles standalone framework/platform/renderer and engine-product responsibilities. Open-GPUI's
source-adoption dispositions close at audited framework `424a3a0f7c2`, recorded by local documentation
commit `e7debc21222`; this is not complete API/platform qualification. The owner's subsequent
direction selects externally hosted WGPU applications as a framework capability to build.
Existing GPUI UI/layout/text/components remain reusable while concrete native D3D11 coupling may
change. A non-Nara application must first prove production frame/input driving, external texture
composition and actual submission/close handling; Nara then consumes the same interfaces.
The current scene recorder is an implementation prerequisite with separate GPU evidence, not
that complete workflow. Recorder `0259c88f9deb` passes the final real two-scene/one-submit GPU
consumer with capability refusals and exact retained atlas release. The framework records 117
existing WGPU controls at the affected refactor checkpoint, production wasm and actual Rust 1.93,
correctness Clippy and static gates. No headless fixture is a production host-driver proof.
The separate game-UI review selects a named HUD/modal/Retry comparison after that driver,
preserving authored semantics and one live interaction executor while leaving execution reuse open.
Nara already has a finite typed graph/program/Host under ADR0094;
full ADR0121 remains Proposed. Editor/game UI selection, extension/reload acceptance and
numerical costs remain open. This documentation changes no GPU dependency or frontend default.

The [retained-image/native-renderer successor](../../research/2026-11-11-open-gpui-engine-integration-layers.md#implemented-image-and-native-renderer-prerequisites)
is locally implemented at Open-GPUI `a5d7dea101c` and `f0a51dd041b`. Existing atlas/paint and
native Window/retirement owners support retained external GPU images and supplied synchronous
Win32 graphics/text; the ordinary non-Nara application has no test/bench-support runtime.
Changed WGPU/public 125, affected native 18 and ordinary independent consumer 1 pass; the
normal application submits a changed image and quiesces its original prefix before exit.
Clippy, formatting and static gates pass. A separately reproduced pre-existing accessibility
fixture failure remains recorded, without a complete native aggregate claim. ADR0107 clarifies
that independent product authoring models permit a shared neutral toolkit. Source-confirmed
Base behavior/appearance separation does not provide external runtime hosting. The production
host-driven Window/loop, delayed Scene/input lifecycle, real Nara Host/View, authored HUD
comparison and full R11/R12/R11f remain open; no frontend or dependency default switches.
The subsequent browser-custody correction `7c5c0cd810f` passes production wasm compilation
and Clippy plus final native retention 6/6, without new browser execution qualification.

The [synchronous external-Win32 checkpoint](../../research/2026-11-11-open-gpui-engine-integration-layers.md#implemented-synchronous-external-win32-host)
supersedes the missing synchronous HWND/loop prerequisite at Open-GPUI `6ad133d3662` and
`911e628c0e9`. App's factory retains original creation/rollback authority; Win32 binds existing
native services to the application's hidden top-level HWND and loop. Exact detachment follows
GPU quiescence without claiming OS destruction or consuming host userdata/quit ownership.
Final Core/public 27/27, affected native 22/22, ordinary independent consumers 1/1 and 2/2,
both real-GPU host smoke modes, correctness Clippy and static checks pass. The original
framework record retains failures and reviewed authority/IME/COM corrections. Delayed Scene
and last-submitted interaction, production logical roots, real Nara Host/View and authored-game
comparison remain open. Base-style skin/behavior reuse and neutral `ui-core`/`motion` do not
independently qualify those runtime requirements. No dependency/frontend default or full
R11/R12/R11f/maintenance handoff changes.

The [deferred Scene successor](../../research/2026-11-11-open-gpui-engine-integration-layers.md#implemented-deferred-scene-submission)
at Open-GPUI `797372013ab` and `69d0992f2a0` implements one pending candidate and submitted interaction/publication custody
through Window, App ingress and native renderer owners. Hidden initial submission uses the same
creation/reveal transaction. Focused Core/public 57, affected shared controls 626 and native
regressions 22 pass; the ordinary real-GPU deferred host verifies original A/B producer pixels,
input, target invalidation, refusal/recovery and close, while both synchronous host controls pass.
This is a framework prerequisite, not actual Nara Host/View or game UI acceptance. Production
logical roots, dependency reconciliation, two-skin control behavior, authored HUD and measured
costs remain open; full R11/R12/R11f and maintenance handoff are not complete.

The [hosted logical-surface successor](../../research/2026-11-11-open-gpui-engine-integration-layers.md#implemented-hosted-logical-surfaces)
reuses the original App/Window without an extra native window and captures its own platform
capabilities. Four ordinary Windows WGPU trajectories pass with actual offscreen output,
independent root input, exact deferred custody and both logical-first/native-first shutdown.
The framework record owns final CPU/independent checks and revision evidence. Actual Nara
GPU-source/Host/View, control skins, authored HUD and measured costs remain open; this changes
no dependency, production default or full foundation completion claim.
Core `2910b52424e` and consumer `8809f12929e` are qualified at `5925a34f920`: affected
CPU/public 45/45, disabled capture 1/1, independent interfaces 5/5 and native lifecycle 14/14
pass, along with correctness Clippy, production wasm, formatting and document checks.

The subsequent [renderer-adapter ownership clarification](../../research/2026-11-11-open-gpui-engine-integration-layers.md#selected-nara-renderer-adapter)
selects a Nara-side `HostedSurfaceRenderer` over the existing engine Host, initially reusing the
optional framework WGPU Scene recorder. Nara retains its original device, target, submission,
composite and retirement authority. A custom Scene backend remains possible through the
framework contract. This documents the next implementation route; no Nara renderer adapter,
GPU-source reconciliation or additional runtime qualification is claimed by that decision alone.

The following [actual Host/Scene consumer](../../research/2026-11-11-open-gpui-engine-integration-layers.md#implemented-nara-owned-host-and-scene-consumer)
implements the selected Nara renderer adapter in an independent workspace. It shares the fixed
WGPU source, original engine device/target/encoder and actual submit/present; exact composite
and GPUI accepted-frame publication admit Editor picking. Generic framework support is committed
at Open-GPUI `7bfe8022f68169879464296e0d6d8ad09d89c7e3`. Prepared source construction/editing,
Undo, submitted input, 4096-byte draft presentation and pending close run on actual DX12 and
Vulkan. Nara renderer prerequisites are local `3095fa5b291ed895188f8c4fc4d20007c6081bcd`.
The actual public adapter/consumer is local `8d0849da1bdee7d18f07a99ef89462a4a5c07b6e`.
Original per-window-prefix observation keeps local recordings alive through all terminal
paths. Recovery from an acquired frame that did not present now reconfigures the existing surface
before another acquire, closing the Vulkan signaled-semaphore failure without presenting failed
drawing. Complete engine/default/frontend/game, processed-content/color-alpha, physical input/DPI, device loss,
performance and maintenance-handoff gates remain open; final run evidence belongs to the plan.

The November 12 [Source/Cooked Image color successor](../../research/2026-11-11-open-gpui-engine-integration-layers.md#implemented-source-and-cooked-image-color-composition)
extends that same native composition with actual imported and cooked PNG captures. Both managed
Runtime owners and temporary backings retire before the original inputs replay through the one
Host. The adapter reads exact output transfer/alpha descriptors and explicitly selects GPUI's
fork-owned linear-sRGB recorder; the official numeric-sRGB default stays intact. A real old-mode
137-versus-64 midtone failure and the separate dark-Path red/fix establish the changed contracts.
Stock associated clears/Opaque writes and retained descriptors pass 7/7 per selected backend.
The generic framework recorder is local Open-GPUI `909f510d18bd0d1e537faf73eb57947cee47f927`.
The Nara stock-alpha/descriptor prerequisite is local `08e65be65540942668d140f43b1ca902aee76438`.
Complete native source/cooked/editor workflows pass on DX12 and Vulkan, including original input,
Undo, paint failures, geometry and close controls. ADR0092 owns the SDR/accumulated-alpha
refinement. The plan records exact runs and limits; this does not close device-loss/busy-producer,
physical input, HDR/display management, numerical budgets, frontend/game adoption or handoff.

The November 12 [state/invalidation critique](../../research/2026-11-11-open-gpui-engine-integration-layers.md#gpui-critique-and-state-invalidation-research)
and [compositor comparison](../../research/2026-11-11-open-gpui-engine-integration-layers.md#compositor-and-cache-research-direction)
are source-grounded research, not a new numeric baseline. They distinguish existing Entity/cached
View reuse from incremental layout, upload reuse, raster caching and thread transport. R11a adds
one complete shared/local-state consumer and separate work/lifetime observations before choosing
a new reactive or cache mechanism. Nara-owned rendering does not qualify local UI invalidation;
the old Dear/kernel measurements do not qualify GPUI. Current rendering/input/resource owners
remain authoritative. No Signal runtime, raster cache, persistent layout or thread is implemented
by this documentation update; broader R11a/e budgets and framework/engine adoption stay open.

The first current-framework state-locality consumer now has a real public 5/5 CPU fixture and a
Nara child 1/1 per selected DX12/Vulkan backend. It uses the existing entity query, cached View,
panel-local draft/input owners and original prepared Editor mutation/Undo path; its caller render counts are diagnostic
only. It does not qualify complete incremental layout, upload/GPU reuse, latency budgets,
virtualization or full Editor replacement. The foundation plan records actual scope and results.

The [variable-height collection successor](../../research/2026-11-11-open-gpui-engine-integration-layers.md#implemented-variable-height-source-collections)
replaces the finite consumer's eager rows with the existing ListState and complete paginated
source queries. Public state/collection controls pass 10/10; the same real 10,001-record native
trajectory passes 1/1 on each selected DX12/Vulkan backend, including filtering, variable heights,
keyed scroll and Winit scale-aware wheel conversion. It records full query/filter/reconciliation
work separately from bounded row callbacks. Logical metadata, allocation/retention/latency/GPU
costs and complete Editor migration remain open; no new reactive/layout/cache owner follows.

The subsequent [hosted cost observation](../../benchmarks/2026-11-04-ui-work-baseline.md#hosted-gpui-source-and-frame-cost-probe)
at Nara `c11e131b5dc705ca73c68221e4716c7fa966805d` uses the same public profiler and original
Host/source workflow, with five eight-warmup/64-sample
cases. Optimized DX12 and Vulkan each pass the probe plus unchanged complete native smoke, 2/2.
The measured local edit rebuilds both changed Inspectors without Hierarchy/row reconstruction;
broad refresh additionally rebuilds Hierarchy rows. The dominant combined source/Editor-drive
interval includes preview production and model publication, so the next owning measurement
splits that path. No new Signal, persistent-layout or raster-cache implementation follows.
The report separates elapsed CPU-side phases from unmeasured GPU/upload/scanout and full
Editor/game budgets; R11a/e and foundation/default/handoff acceptance remain open.
The separate optimized allocation-enabled DX12 pair also passes 2/2. Calling-thread request
churn and process requested-live gauges retain separate scopes; they do not qualify total memory
or leaks. No production framework/default mechanism changes.

The [source-drive attribution and indexed hierarchy successor](../../benchmarks/2026-11-04-ui-work-baseline.md#original-source-and-editor-drive-attribution)
at `9715832a0b7` and `f1330617bc1` locates the dominant interval in Scene materialization and
replaces the original hierarchy
candidate/Vec replay with sparse indexed sequential validation and one final touched-owner
projection. ADR0100 records final-value publication and complete intermediate lifecycle admission;
ADRs0100/0101 retain exact order, no-flush custody, non-linked lifetime and completion invalidation.
Root membership indexes use full Entity identity. Large replacement, invalid tails, frontier,
recycling/storage and source-epoch tests keep unmanaged state and exact identity retirement.
Optimized DX12/Vulkan smoke/probe pairs and separate DX12 allocation pair each pass 2/2 with
unchanged source/render counts. Both backend edit controls meet the predeclared local
materialization P95 below 150 ms; remaining source compilation/materialization and large
allocation churn stay measured follow-ups. No GPUI, Editor/game default or full-budget claim
follows; the complete Goal and R11/R12 remain open.
Affected Runtime UI/Render pass 176/176 and independent public Scene preparation/spawn/identity
consumers pass 21/21; core hierarchy/Scene/Transform and corrected large-source coverage remain
identified separately in the cost record. No independent-consumer lock change is required.

The [deeper Scene attribution](../../benchmarks/2026-11-04-ui-work-baseline.md#scene-compilation-and-publication-attribution)
at `a55043b3b32` adds optional standard compiler/binding/publication spans, with the engine
subscriber-free and no package-version changes. Optimized DX12/Vulkan smoke/probe pairs pass
2/2 each; all seventeen intervals and five-case source/render totals are checked. The measured
structural closure is approximately 32.7 ms P50; complete preview compilation/materialization
remain about 80/85 ms. Feature-off Scene and independent-consumer all-target checks pass.
Local reuse of a stable unpublished structure is the next selected compiler change; final
World/lifecycle admission and UI/default/framework ownership remain unchanged.

The [stable-structure replacement](../../benchmarks/2026-11-04-ui-work-baseline.md#stable-structural-closure-qualification)
at `969c6f52db2` subsequently passes Scene library/public 1178/1178, independent Scene 21/21 and
optimized DX12/Vulkan smoke/probe pairs 2/2 each. It retains the original compiler's stable unpublished
structure after complete successful slot selection without adding any scope; all final
entity/working-limit and later reference-demand admission remains. Working usage reflects
removed actual scratch work rather than obsolete charges. All four selected compilation P95
cells meet the predeclared below-80-ms target with identical source/render counts and exact
retirement. This changes no UI runtime, default frontend, renderer or receiving-World lifecycle
authority. Remaining approximately 64/85 ms compilation/materialization P50 and full budgets
stay open.

The [25-span materialization attribution](../../benchmarks/2026-11-04-ui-work-baseline.md#materialization-owner-attribution)
at `9d1e667c3da` passes original optimized DX12/Vulkan smoke/probe pairs 2/2 each. Complete
repeated call counts accompany per-sample inclusive sums. Value admission/decoding is about
11.7 ms P50 and adoption about 1.9 ms; no new batch identity API is selected. The next bounded
change reuses the existing compiled document's sealed immutable topology and ID-sorted namespace
while retaining receiving definitions/values/references/assets/codecs, Native projection checks
and target-World/lifecycle admission. Raw source/claims keep complete topology proof. No new
public certificate, retained index, UI runtime/default or framework owner follows.

The [compiled receiving replacement](../../benchmarks/2026-11-04-ui-work-baseline.md#compiled-topology-receiving-reuse)
at `9b1376f75b5` reuses the sealed topology and searches all ID-sorted entity records while keeping
Native Visibility edge/witness rules and all receiving value/definition/asset/codec and World checks.
Full Scene library/public 1180/1180, independent Scene 21/21 and optimized DX12/Vulkan
smoke/probe pairs 2/2 each pass. All four decode P95 cells meet the declared below-5-ms target
at below 0.7 ms, with unchanged complete source/render/call vectors and native retirement.
Materialization/entire-owner P50 is approximately 75/167–168 ms. Higher whole-owner local tails
and unchanged compiler tails remain explicit in the report; no stable end-to-end budget follows.
The new controls cover Authoring-to-Native projection refusal, exact child diagnostic order,
empty-entity local references and receiving limits before codecs. No UI/default ownership or
full Goal/R11/R12 completion follows.

The [typed graph consumer](../../research/2026-11-11-open-gpui-engine-integration-layers.md#typed-graph-extension-consumer)
continues R11c through the independent GPUI engine workspace. The bounded ValueGraph owns typed
nodes/links, validation, atomic edits, content history and JSON; the original public Session owns
exact revisions, Save/Reopen and close. Two panel entities keep separate drafts, preedit/conflict,
connections, focus and pan/zoom while using public canvas/path and ordinary controls. Submitted
geometry and its captured operation basis publish through the original Window transaction;
automation cannot infer displayed identity from domain completion or a submission counter.
This requires no production Open-GPUI change, Signal runtime, new GPU owner or Scene epoch
contract. Domain/public Host `f954835` passes 11/11; native integration `0d12d38` passes original
Scene, graph and work trajectories 3/3 on each of DX12 and Vulkan, with exact submitted input,
fractional-scale connection pixels and original retirement. Correctness Clippy, feature-off
all-target compilation, format and document checks pass; existing warnings remain. Two panels
leave independent engine Views, late asynchronous compilation, docking,
budgets, full R11c and frontend/game selection open.

The [independent graph-value Views successor](../../research/2026-11-11-open-gpui-engine-integration-layers.md#independent-view-qualification)
adds exact document/revision/request admission at `34a92d7` and original Host native composition
at `c069b74`. Its two finite Render streams share one GPUI logical surface/window submission,
with distinct producers, outputs and exact presented input mappings. Relocation, old-route
refusal, one producer's physically completed prefix while the other continues, passive retained
pixels, late edit/Reopen/close results, capacity recovery and normal exit with a held Task are
qualified. Domain/public Host passes 28/28; five complete native trajectories pass 5/5 on each
of DX12 and Vulkan. Final correctness Clippy, feature-off compilation, format and document checks
pass with existing warnings. This consumes existing ADR0108/0111/0118 boundaries rather than
changing Surface Slot aggregation or adding another GPU/task/input owner. Continuous graph
canvas gestures, general docking, full Shader Graph, budgets and Editor/game selection remain
open; no full Goal/R11/R12 completion follows.

The [continuous graph canvas successor](../../research/2026-11-11-open-gpui-engine-integration-layers.md#continuous-canvas-qualification)
adds pure local gestures at `3903912` and original GPUI capture/scroll integration at `77dfa4c`.
Node previews commit one exact-basis Move on matching release; local pan/anchored zoom do not
mutate the document. Source/focus/Escape/retirement cancellation preserves history, with exact
submitted transform/preview observation and original default header focus. Pure/domain/public
Host passes 46/46; the six affected native trajectories pass 6/6 per backend, and final strengthened
gesture assertions pass 1/1 per backend. Actual visible pan, moved-preview close, curve pixels
and original producer retirement are covered on DX12 and Vulkan. Correctness Clippy, feature-off
checks, format and document checks pass with existing warnings. No production framework,
runtime/render or default-selection change occurs. Full graph/Timeline, docking, Editor/game
execution, physical input and numeric budgets retain separate acceptance.

The November 12 [Jellyflow adoption audit](../../research/2026-11-12-jellyflow-node-editor-audit.md)
supports independently maintained node-editing mechanisms with Nara consumer feedback. It does
not select the current authoritative Jellyflow runtime as a Nara document owner. The existing
native adapter directly executes graph commands, so external immutable domain facts and exact
intent/completion require a real integration seam before replacing the finite graph control.
Core/runtime/layout/egui baseline passes 711/711; an isolated compile against maintained
Open-GPUI reports 42 adapter API errors. Four public regressions reproduce integrity, marquee,
history and layout defects; native-publication/lifetime findings remain source-only. The study
proposes a complete Dialogue Graph consumer. One domain source/history, original Session
admission and Render/Host/preview owners
remain required. No new dependency/default, completed general node editor or full R11/R12 claim
follows.

The subsequent November 12 [maintained Jellyflow consumer](../../research/2026-11-11-open-gpui-engine-integration-layers.md#maintained-jellyflow-consumer-qualification)
completes the compilation-and-first-integration prerequisite. Open-GPUI is published on `main`;
both projects select exact Git code baseline `f75c10b2`. Jellyflow's adapter and independent
native product migrate at `2deeeba`/`8a7c2df`, now on local Jellyflow `main`; its separate
integrity draft remains unmerged. The optional Nara fixture at `99c2b26` hosts two Views over one standalone
Jellyflow graph/history through the original Render/Host/GPU owners. Adapter tests pass 519/519,
existing Nara domain/public Host tests 46/46, and the selected actual Windows DX12/Vulkan native
cohorts each pass 3/3. Pixel/input, overlay exclusion, exact completed drag/Undo, independent
navigation and window-aware retirement are covered. The [Nara handoff](../../plans/2026-11-12-nara-ui-integration-handoff.md)
records separate engine and framework development paths. Nara Session/domain/persistence
integration, remaining graph defects, general docking/Shader Graph and Editor/game selection
retain their prior owners and acceptance; no full R11/R12/Goal completion follows.

The [Dialogue Graph successor](../../plans/2026-11-12-dialogue-graph-integration-plan.md)
implements its domain/public Session and immutable-facts prerequisite at `ddcc880`. A new
extension document owns atomic node/choice/link edits, one history, bounded versioned source
and one-transition preview. Nonserialized incarnations separate restored source identity from
native lifetime; the original Workspace still owns exact-basis admission and Reopen. The
Jellyflow capture is immutable and adds no writable runtime/history. Release nextest passes
75/75, including 29 new tests and 46 existing controls. All-target correctness Clippy,
feature-off compilation, format and separate domain/facts/public reviews pass in this scope.
Jellyflow's external-document intent/completion interface and two native Dialogue Views with
ordinary property editing remain unimplemented. Earlier standalone native results do not
qualify them or complete R11c.

The subsequent [private read-side extraction](../../plans/2026-11-12-dialogue-graph-integration-plan.md#private-read-side-extraction)
at local Jellyflow `d4125cc` separates graph facts/indices/query implementation from history
and mutation hooks without a new public API. Existing Runtime/public tests pass 599/599,
adapter tests 519/519, and Runtime all-target Clippy, format and independent review pass.
At that checkpoint, `ViewSession` still used its incumbent source owner. The following
[public immutable Source prerequisite](../../plans/2026-11-12-dialogue-graph-integration-plan.md#public-immutable-external-source)
at local Jellyflow `046a660` separates read capability from the original writer. Nara's shared immutable facts and live
incarnations now feed two ordinary Views with independent state/layout and one original
Session document/history. Exact read versions, equal-value Noop publication, coalesced Undo,
Reopen reset and pre-admission feedback custody retain their owning contracts. Actual
same-token layout recovery is covered. Runtime/public release tests pass 620/620,
retained/egui adapter controls 560/560 and Nara's combined cohort 79/79. Correctness Clippy,
feature-off compilation, format, document checks and separate source reviews pass in scope.
Asynchronous external authored completion, canonical ID adoption,
native intrinsic size and the native Dialogue consumer remain open; the existing unmerged
graph-integrity draft retains its separate scope.

The subsequent [domain-outcome prerequisite](../../plans/2026-11-12-dialogue-graph-integration-plan.md#domain-outcomes-and-atomic-canvas-operations)
adds an associated accepted-command `Outcome` to the existing extension document interface.
The Workspace carries it with original command identity and accepted revision; it does not infer
allocated IDs from a later snapshot. Unit-returning owners explicitly use `Outcome = ()`.
Dialogue creation/choice results contain actual allocated identities, and wire creation,
two-endpoint reconnection and mixed removal each use one validated candidate/history entry.
Tooling passes 179/179, the independent domain/graph/Session cohort 94/94 and independent panel
consumers 46/46 in their stated scopes. Readable historical outcomes survive real Reopen/close;
they do not grant a new native frame or external interaction completion. Separate source reviews
find no concrete P1/P2. Affected root persistence, slot-invariant and existing DX12 extension
document controls pass 45/45, including one actual native trajectory. Correctness Clippy,
behavior-language trial compilation, feature-off compilation, format and document checks pass
within the unit's recorded scope; existing unrelated warnings remain.
External typed transport, canonical native ID adoption and the complete Dialogue UI remain open.

The following [external request successor](../../plans/2026-11-12-dialogue-graph-integration-plan.md#exact-external-canvas-requests-and-publication-receipts)
implements renderer-neutral typed mutation binding and exact asynchronous settlement through
the existing Jellyflow interaction session and lease at local Jellyflow `4bcd0f2`.
Runtime/public controls pass 647/647, existing adapter controls 560/560 and the independent
Nara domain/graph/Session/file cohort 110/110. The independent Nara consumer retains
original document basis, domain result and post-command source, pairing them with a sealed
publication. It adopts actual canonical IDs and complete creation-port mappings without
another writable graph or history. Creation during reconnection is one atomic domain operation.
Pending work survives its own publication, canonical projection uses current owners and a
delayed A result reconciles B before returning. Reset, close and unknown outcomes preserve
their distinct authority. The linked plan records public Session and source/interaction
verification; native journal transport, View-local intrinsic sizing and the complete two-View
Dialogue UI remain open. Full R11/R12 and the foundation Goal remain active.

The [native journal successor](../../plans/2026-11-12-dialogue-graph-integration-plan.md#implemented-native-external-transport)
at local Jellyflow `3a430b9` implements external delivery through the original adapter journal
and interaction ticket.
Its handler runs after exact original feedback and outside the Host borrow; one-shot completion
retains the original domain result through source retirement and never authorizes replay.
Pure waiting stops interaction ticks. Runtime/public controls pass 647/647; the affected
Open-GPUI/egui adapter cohort passes 583/583, including nine public TestApp/Visual trajectories.
Independent journal and Host reviews closed the corrected association, panic, retirement and
scheduling findings. The linked plan retains exact runs and test-clock limitations. View-local
canonical size and the complete native Dialogue product remain open; no new external Dialogue
GPU, physical-input or numeric-budget claim follows.
The independently locked Nara domain/graph/Session/file cohort passes 110/110; existing Scene,
captured graph gesture and standalone Jellyflow/two-View native controls pass 3/3 each on DX12
and Vulkan after rebuilding the release consumer.

The November 13 [View geometry successor](../../plans/2026-11-12-dialogue-graph-integration-plan.md#implemented-view-local-intrinsic-geometry)
at local Jellyflow `74f7f54` replaces automatic size commands with complete accepted canonical
geometry for both source kinds. Explicit authored resize and persistence stay with the original
document. Measured extents, semantic zoom, real ports/routes, remount and offscreen Fit use that
View's original geometry. Runtime/layout pass 715/715, adapters 585/585, the independent Nara
consumer 110/110 and existing DX12/Vulkan controls 3/3 each. That separate Workflow sample
checkpoint was 22/25 with the same three parent-reproduced popup/port failures; its full gate
and native Dialogue remained open. The linked plan records exact runs, reviews and scope.
No full R11/R12 or foundation completion follows.

The [Workflow repair](../../plans/2026-11-12-dialogue-graph-integration-plan.md#qualified-workflow-repair)
at local Jellyflow `0a9453a` subsequently closes those three parent-reproduced sample failures
with stable widget ancestry, accepted graph modal admission and an activated pointer-capture
fixture. Complete Workflow and model-provider consumers pass 25/25, focused owner tests
24/24 and complete adapter
controls 588/588. The rebuilt existing standalone Jellyflow/two-View GPU consumer passes
1/1 on each of DX12 and Vulkan. The complete external Dialogue desktop workflow and R11c
remain open.

The finite custom-draw and canvas consumers above do not establish a general component
state/effects lifecycle, a complete reusable component library or all complex-editor behavior.
Public semantic panel custody is now separate
from the explicitly selected Dear reference; the bounded image attachment below has its own
consumer scope, while the complete successor interface remains open. Finite Rect/Text/Mesh
paint and button events alone do not close it.
R11f adds source-built Editor edit/build/reconstruct,
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

The first [R11a work-observation prerequisite](../../benchmarks/2026-11-04-ui-work-baseline.md)
adds detached cumulative text/compiler/planner observations and executable description/dependency
checks. Debug/release CPU probes distinguish frame reuse from repeated description work and keep a
full-source startup allowance refusal separate from read-only 10,000/100,000-row projection costs.
Three release native repetitions now keep the exact static Inspector and advancing Game in the
same 128 current window frames, with actual geometry, original-device health and terminal-loss
refusal controls. Separate current-thread requested-allocation probes quantify temporary churn
without changing production allocators. Full process/native retention, upload/GPU timestamps,
OS input/display latency and fixed budgets still keep R11a open. The pinned Zed/Open-GPUI study
refines state/invalidation and collection consumers without selecting their ownership/dependencies.
That baseline selects no cache optimization. The styled successor below adds bounded compatible
text-layout reuse; the complete independent Editor and R11b-f remain incomplete.

The [native frontend capability/primary GPU prerequisite](../../benchmarks/2026-11-04-ui-work-baseline.md#primary-window-gpu-markers)
adds declaration through the existing Host before acquisition, sharing the original closed
request, late refusal and recovery rules. The independent adapter proves actual timestamp query
readback and unchanged output pixels. Three release repetitions also preserve the complete
static-Inspector/advancing-Game native oracles while observing primary-paint markers. These
diagnostics retain original resources through physical drain and keep whole-GPU, upload,
input/display latency and retention gaps open. They do not qualify public component/panel hosting
or the independent self-drawn Editor.

The [Nara issued-work observation](../../benchmarks/2026-11-04-ui-work-baseline.md#upload-and-copy-observation-boundaries)
extends the existing transaction snapshot with helper Queue requests, mapped initialization and
graph Preserve copies. Later failure/cleanup never refunds reached work, and external native
callbacks, arbitrary Feature commands and Dear stay outside its scope. Independent program
consumers preserve their original pixel/buffer and retirement oracles alongside exact counts.
Inspector change pressure separately exposes repeated compile/text work for color/palette and
environment changes without claiming actual font substitution or an incremental-cache win.

The [repeated native lifetime probe](../../benchmarks/2026-11-04-ui-work-baseline.md#repeated-native-lifetime-process-memory)
uses that same complete workload across healthy and actual device-loss teardown. It keeps
whole-process private commit, shared/private Working Set and lifetime peaks distinct from Host
payload accounting. Exact original cleanup, fresh Host identity, weak Rust-window release and
platform-window destruction pass all 48 lifetimes in three release processes. Whole-process
residuals vary despite constant logical Host payloads; short observations do not establish
UI-exclusive memory, a leak bound, long-running stability or R11a's remaining budgets.

The [local font reconfiguration prerequisite](../../guides/editable-text.md#local-editing)
adds explicit text/field font adoption under the original size/limits, retaining local edit and
focus ownership. Independent public CPU consumption exercises real fallback/order changes,
compiled semantics and new geometry. Backend offscreen readback retains old/new glyph packets
on one device/queue, including after their text engines are released. These are separate pieces
of evidence: the public field consumer uses local focus admission, and the GPU fixture consumes
a text snapshot. They do not qualify the native Editor's all-panel font transaction, presented
input/IME handoff, a general typography service or an incremental performance improvement.

The [independent presented-surface prerequisite](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#independent-presented-surface-prerequisite)
adds a finite public presenter composing the original planner, journal and outcome-permit
custody. Its independently built Nara frontend needs no Dear or game UI adapter and publishes
input only after an exact designated native composite and original one-shot acceptance. The
same scope repairs original WGPU packet-allocation and Submitted backend-generation checks;
target/device faults withdraw accepted history without taking retirement-generation ownership.
All 175 kernel/presentation tests, 396 WGPU library tests, six independent native cases and three
dependency checks pass. This is not
the whole Editor panel attachment, neutral presentation dependency split or independent default
shell. The later dependency prerequisite below qualifies the neutral split separately; public
attachment and the independent default shell remain open under R11b.

The [neutral presentation dependency prerequisite](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#neutral-presentation-consumption-prerequisite)
separates CPU values from explicit Runtime integration inside the seven existing Asset, Identity,
Diagnostic, Image, Render, Window and UI Render owners. Original nominal resource identities,
shared pixels/import leases and one backend-outcome custody implementation serve both paths.
The independently locked component library consumes code-first Image capture and presentation
without App/ECS, filesystem/tasks, game UI, Editor or a concrete native backend; serialization
does not add those dependencies. Its original native consumer separately selects WGPU/Winit and
keeps the complete 19-presentation/18-journal-commit trajectory plus four failure controls.
Shared all-feature 1,165/1,165, pure/serialization 317/317, default game UI 124/124, independent
library 30/30, product/closure 14/14 and actual native surface 2/2 pass in their separate scopes.
The linked plan records overlapping cohorts, exact feature/build coverage and limitations. The
Image assertion covers packet resources and actual presentation, without a new pixel readback.
Public panel attachment, a complete independent Editor, native OS input and performance remain
open; this is dependency and lifetime qualification rather than full R11b acceptance.

The [toolkit-independent panel custody prerequisite](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#toolkit-independent-panel-custody-prerequisite)
adds typed `EditorPanelHost<P>` state, scoped original-owner source access and exact action
observations, sharing the reference frontend's extracted submission implementation. Workspace
records transient hide/close/target transitions even when final layout fields match. An
independently locked consumer disables `imgui-reference`, edits a non-active source, preserves
history and complete source records, and passes its separate CPU and Windows/DX12 workflows.
The native path checks actual composites, retired mutation refusal, late-output disposal,
focused-successor token rejection and reducer/panel/GPU cleanup. The linked plan owns exact
counts, review corrections and qualifications. The default reference remains available; native
capabilities follow the bounded attachment below. Broader dependency minimization, the complete
independent Editor, OS input/accessibility, hot reload and performance acceptance remain open.

The [image-backed native panel attachment](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#image-backed-native-panel-attachment)
adds exact semantic instance/state lifetimes and complete-window image admission over the
original WGPU outcome. One original semantic Host keeps cancellation and subsequent input in
one chronological candidate. Finite shared keyboard/capture/modal, scoped IME/candidate area,
clipboard/cursor and explicit refresh services need no shared widget tree. Independently built
Nara and software-painted frontends preserve source/history/persistence and retirement through
these ports. The plan records exact CPU/native results, full two-image adoption, keyboard-only
focus, foreign-owner and no-output controls, review corrections and limits. Input/clipboard
are injected and image readback is not a final swapchain oracle. Named-toolkit compatibility,
OS candidate delivery, direct native drawing/overlays/detach/accessibility, full Editor,
extension reconstruction and measured performance remain separate work under R11b-f.

The [independent drawn source workspace](../../plans/2026-11-05-independent-drawn-source-workspace-plan.md)
adds a separately selected production entry and Shell over those same owners. Ordinary source
and specialized UI Hierarchy controls move out of the Dear adapter and serve both compositions;
the successor owns no second mutation/history/identity/GPU authority. Source UI capability uses
the existing source templates and complete original UI projection, with a separate deletion
refusal for degraded documents. Its completeness predicate follows the existing projection cache.
The final reference/default Editor and selected public integrations pass 472/472; the
no-reference library passes 220/220, with the Shell separately passing 19/19. Those
overlapping CPU cohorts include Shell exact requests/results, shared maintenance wakes and
pointer Cancel across composition blur at normal/compact heights. The Windows/DX12 production
trajectory passes source structure/history/persistence, all four UI recipes, complete 4,096-byte
native text, source refresh/refusal, projection/instance retirement and explicit graphics
reconstruction. Original graphics remain owned until physical close completes; diagnostic
reporting alone cannot install a replacement. Executable startup, independent public-entry
consumption and affected reference results are recorded in the unit. The complete replacement
matrix, default switch and cost budgets remain open.

The [style/layout study](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-5-style-layout-and-component-library-readiness)
pins GPUI, upstream ImGui and Godot separately from Nara qualification. The
[styled workspace](../../plans/2026-11-05-styled-source-workspace-plan.md) adds explicit regular
font provenance, validated size/line-height, atomic local typography adoption and immutable
palette/density/text-scale recipes. Field geometry owns actual baselines, padding, draw origin,
Point, selection and IME together. Measured groups and anchored menus retain explicit overflow,
dismissal and focus ownership. One geometry-keyed field layout and a bounded compatible plain-text
cache preserve reuse without claiming incremental layout, retained GPU updates or heap budgets.
The complete original Inspector and its recursive, reference, component, recovery and source-tool
drafts now live outside the Dear adapter and serve both compositions. Actual Windows/DX12 output
captures drove disabled-state and enlarged-text corrections. Exact shared, independent, CPU/native
results, review corrections and cost observations belong to the unit. This remains the first
styled source-workspace slice; a broad polished library, persistent preferences, font-family/style
selection, platform accessibility, docking, the remaining Editor tools and R11a/e budgets stay open.

The [presented UI operations unit](../../plans/2026-11-05-presented-ui-operations-plan.md) adds
opaque original-image observations, non-preparing semantic inspection and borrowing original
submission IDs. Physical input keeps native arbitration; opt-in typed provider requests use the
same callback and reducers without a universal control schema. Nara publishes actions only for
enabled, non-inert controls with a live behavior owner. The complete drawn Inspector/Shell field,
history and Save/Reopen trajectory passes on Windows/DX12, followed by all original source/UI/
retirement checks. Independent Nara and foreign frontend native consumers pass original and
observed-operation trajectories, including old output/focus refusal, modal outside clicks,
wrong-button capture and empty-scope IME acknowledgment. Their unit records exact runs, original
counts, corrections and injected-event/readback limits. Graph operations, animating-View request
waits, agent transport, OS accessibility and broader R11d/R11f/performance qualification stay open.
The same source comparison refines the numeric recipe/support inventory and corrects pressed
appearance during drag-out, with 218 shared kernel/widget regressions. This adds no new palette,
font-loading service or visual-catalogue approval.

The [keyed local-state prerequisite](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#keyed-local-state-composition-prerequisite)
adds exact parent/child scoping, typed store/incarnation handles, explicit model/schema
compatibility and transactional membership. A separately authored row composite consumes this
without a widget registry, document owner or private state map. Repeated composition preserves
compatible local text state; omitted rows follow the fixture's explicit discard policy. Error,
abandonment and caught initializer unwind preserve earlier membership. Native focus/IME, pending
domain results and external-work cancellation stay with their original owners. This is no
general component/custom-layout interface, virtualization implementation, automatic invalidation,
hot-reload codec or performance claim; broader R11b acceptance remains open.
The linked prerequisite records 271 shared regressions, 11 focused custody cases (nine overlap),
13 independent neutral consumer cases and two dependency checks. Its two final native tests
include a complete 11-presentation/ten-commit trajectory and separate original error/panic child
controls. Input delivery is injected; current Editor controllers and their draft policy remain
unchanged by this slice.

The [custom child-layout prerequisite](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#custom-child-measurement-and-layout-prerequisite)
adds immutable user-authored layout descriptions with scoped child measurement, exact placement
and explicit hiding. The compiler privately replaces its Taffy-tree dispatch while preserving
one text session, final frame projections and inert root fallback. Candidate-wide work limits
charge dispatches, callbacks and child requests. Min-content and prepared first baselines belong
to the text owner; public subtree baselines exclude synthetic box fallbacks. The actual
independent keyed-field library consumes the interface and continues through native presentation
and input. Snapshot/description owners retain callback code, while frames and last-good visuals
retain resolved facts only. Custom paths/drawing, canvas gestures, scoped effects/services,
virtualization, foreign panel attachment, code reload and the complete independent Editor remain
open. This prerequisite is no incremental-cache or performance qualification.
Its final shared/default-game suite passes 419/419; the enabled-serialization game library passes
125/125, the WGPU library 396/396, Editor/public integrations 406/406 and selected Windows/DX12
Editor trajectories 33/33. The independent component library passes 15/15, followed by both
native surface tests through the imported measured layout. The linked plan preserves overlapping
suite counts, ignored scope, review fixes and failed fixture assumptions separately.

The [same-stream geometry prerequisite](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#same-stream-custom-geometry-prerequisite)
adds captured local triangle/color facts, content-box clipping and independent source/output
limits. Neutral packets retain exact projected vertices and charge their aligned allocations;
the original UI Feature preserves quad/mesh/quad order and original native custody. A separately
authored Lyon component combines a cubic connection with ordinary node/port controls. Its
tessellator remains outside the kernel. The linked plan owns executed shared, independent and
native evidence; this is no full graph editor, canvas gesture model, geometry-cache performance
result, foreign frontend attachment or complete R11b/c acceptance.
The corrected shared suite passes 450/450, all-feature WGPU 403/403, Editor/public 406/406 and
selected Windows/DX12 Editor 33/33. The independent component library passes 19/19; its full
native trajectory and failure controls pass 2/2, while four typed-Feature GPU cases separately
cover linear/sRGB pixels, exact metadata admission and partial-allocation recovery. These
overlapping suites and the original failed fixture assumption retain their limits in the plan.

The [presented-pointer prerequisite](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#presented-pointer-samples-and-viewport-capture-prerequisite)
adds finite event-time samples, declared motion/scroll/buttons and explicit viewport capture
continuity. Exact matching presentation may renew an opted-in viewport while descendants move;
owner/ancestor, Surface, modal and explicit invalidation rules still cancel its capture. The
external curve library consumes admitted batches to update bounded local pan/zoom, with no
second journal, document history or input owner. Existing adapters migrate directly, preserving
their supported input meanings. General logical keys, dynamic propagation, Runtime wheel-source
facts, cross-window capture and complete complex-editor/Host workflows remain open. The linked
plan records actual tests, review fixes and native input-delivery limits. Shared 469/469,
Editor/public 406/406 and selected native Editor 33/33 pass. Independent component 24/24 and
native 2/2 pass; the complete native trajectory observes 19 presentations/18 journal commits,
and its separate failure test exercises four error/panic child modes. These overlapping suites
qualify this pointer prerequisite, not measured reuse or complete R11b/c.

The [source Hierarchy reuse unit](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#source-hierarchy-matched-reuse-unit)
adds private exact-frame projection/filter reuse and explicit local paint invalidation, preserving
original action/queue revalidation. Matched 128/1,024 admitted and 10,000/100,000 read-only warm
probes meet their selected work/allocation and 25-microsecond P99 targets; quantized timing and
current-thread allocation limits remain in the report. Editor/public 410/410 and selected native
5/5 pass. New model publication still refreshes conservatively, including an advancing Game;
cross-frame retained dependency reuse and the complete R11a/e budgets remain open.

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
That predecessor established neither production automatic parallel support nor a frozen R4c phase interface.
The [November 15 explicit executor successor](../../plans/2026-11-03-joint-motion-execution-plan.md#explicit-production-executor-successor)
adds an immutable code-first automatic policy over the original Bevy executors. Default Serial
survives feature unification; native Parallel requires an explicit feature and an existing
Host-initialized nonempty process-global ComputeTaskPool. All automatic schedules use the chosen
policy, while custom schedules and Runtime/Task/solver/pool lifetimes retain their original
owners. App configuration identity now differs from plugin-plan identity. App default/enabled
cohorts pass 182/183; independent motion default/unified/Parallel passes 30/31/36. Actual
DX12 hardware captures/history pass under Serial and Parallel, and the joint Material/history
software trajectory passes under Parallel with exact retained-output and cleanup checks.
Implementation `40e8e81` and independent consumer `6d45b7b` are qualified locally. The discovered
game inventory correction `81916c9` permits empty default Material Schema/storage but refuses
actual unrepresented Material assets/components; the complete ordinary game cohort passes
111/111 with two original native/build tests unselected. Root Host/public93, desktop all-target
compile, owning correctness Clippy, format and stable Wasm production checks also pass.
The owning plan records product compatibility, final verification and bounded platform scope;
full R4c/R10, arbitrary parallel effects/prediction and performance budgets remain open.
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

The November 16 [actual game audio workflow](../../plans/2026-11-03-joint-motion-execution-plan.md#concrete-game-audio-and-native-retirement-successor)
at `edc9ea2` adds an optional game-owned CPAL0.18.2/WASAPI composition under ADR0095. Exact
pickup/travel commits, Scene-instance cancellation and independent device time use existing
game/Runtime/native owners; the original close participant waits for stream/callback/worker
retirement. The ordinary desktop game compares complete original-WAV callback samples on
DX12/Vulkan hardware and retains Pause/Step, failed travel, Retry, Save/Restore and fresh
generation controls. Focused CPU45, all-feature ordinary233, minimal shipping119, oracle4,
final DX12/native4 and Vulkan1 pass in their separate scopes, with correctness Clippy,
formatting and existing independent-consumer compilation. The linked plan owns exact source,
review corrections and platform/cooperation limits. Default/shipping recipes acquire no audio
device, and unsupported native reconstruction remains refused. This qualifies the concrete
Windows consumer, not human audibility, an engine-wide AudioBackend/default, streaming,
hot-switch/cooked audio, foreign execution, numeric budgets or full R10/R12/Foundation.

The November 16 [source-owned Sound Bank and audible delivery successor](../../plans/2026-11-03-joint-motion-execution-plan.md#source-owned-sound-bank-and-source-free-audible-player)
at `b292ed9`, with CPU custody correction `bd5941e`, uses one game-owned primary Product, tracked
WAV dependencies and a source-independent canonical PCM codec through the existing asset-domain/
cooker owners. The native mixer retains the original
immutable collection and byte reservation. Plain CPU collection escape no longer reserves an
unrelated Scene-generation slot; actual Scene/Image/Material generation custody and the original
two-generation ceiling remain intact. Prepared travel refuses changed bank meaning before commit.
Actual import observations, rather than fixed WAV filenames, fence cooker inputs through publication.
Complete audible delivery passes 1/1 after real release build, installation relocation and physical
removal of copied source/cache. The independent original-WAV oracle agrees with 208,512 stereo
callback samples at zero maximum error; another Player performs physical pickup/travel/WM_CLOSE.
Original native retirement and corrupt-content/catalog refusal retain their exact receipts.
Final ordinary sample, minimal shipping and independent asset cohorts pass 269/269, 141/141 and
95/95 respectively. Original headless/silent delivery and actual Rust rebuild/adoption each pass
1/1. Owning correctness Clippy, formatting,
independent diagnostic consumer compilation and exact shipping dependency graphs pass. The linked
plan records exact runs, review corrections and bounded Windows/Rust1.95 scope.
This implements concrete cooked game audio without selecting an engine-wide backend or qualifying
arbitrary native reconstruction, streaming/hot-switch, human audibility, hardware delivery/latency,
other platforms, performance budgets or complete R10/R12/Foundation.

The [November 18 audible development successor](../../plans/2026-11-03-joint-motion-execution-plan.md#audible-development-with-explicit-effect-reset)
at `6da2745` connects the existing game-owned CPAL composition and development Host with explicit
effect reset. Probes/candidates discard sound; fresh adopters restore complete canonical gameplay
without historical playback. The sole adopted output waits for confirmed original stream/callback
and process retirement. Completed game-close errors remain observable without permanently blocking
an independently healthy output, preserving the existing failure/completion distinction. Strict
normal probe qualification is unchanged. The final real Rust-edit adoption trajectory and
failed-close control pass 2/2. Separate selections pass seven actual WASAPI/PCM cases, 222 affected
CPU cases, four minimal-feature cases, one original silent continuous adoption and the ordinary
35-record JSON CLI workflow in their
separate scopes. The native normal trajectory reports hardware Vulkan. Owning all-feature/all-target
correctness Clippy, root/game formatting and document checks pass. The linked plan records initial
overcoupling, its correction, exact runs and Windows/Rust1.95 limits. This implements the concrete
reset workflow under ADR0076, not general audio-state migration, hardware latency/human audibility,
a new domain backend, foreign execution or full R10/R12/Foundation.

The [November 15 transformed-collision successor](../../plans/2026-11-04-authored-collision-derivation-plan.md#fixed-spatial-inputs-and-transformed-collision-successor)
adds opt-in fixed-input publication to the original Hierarchy/Transform owners. Prepare's current
deferred sources complete before Simulate without replacing Historical, completed Fixed or
Presented cuts. Captured local/parent facts and full affine globals feed the existing concrete
Rapier owner. Fixed local boxes preserve shear, nonuniform scale and mirrors as validated native
convex shapes; dynamic root/unit-scale rigid poses write back before normal fixed completion.
Live writeback revalidation refuses late reparenting before solving. The unreleased fixture
replaces its component ID with canonical `fixture.motion.LocalBox`, without an old reader.
Original provider receipts and the same ready snapshot install through the existing Registry
owner's public direct constructor; Scene materialization waits for Startup after Finish.
Shared Hierarchy/Transform86, complete motion default40/Parallel-enabled46, Registry15 and public
Scene admission4 pass. Actual DX12 hardware history and software joint Material/history each
pass 1/1 through the new input role, preserving exact old outputs and original cleanup. The
owning plan records failed fixture setups, corrected review findings, final checks and bounded
Windows/Rust1.95 scope. Opt-in adds a separate source scan/cache. Parented/scaled dynamic poses,
arbitrary physical rollback, a selected local-affine schema, official backend selection,
numeric performance budgets and full R4c/R10/Foundation remain open.
Local implementations are `79cd09e` (Registry constructor), `d934b24` (spatial input) and
`c5b75ba` (concrete collision consumer); the owning plan retains their breaking API/data changes.

The [visible authored-collision continuation](../../plans/2026-11-04-authored-collision-derivation-plan.md#visible-runtime-native-qualification)
combines that same domain with public Scene editing and ordinary Sprite/stock capture. The domain
now exports its original canonical Schema provider instead of each consumer claiming another
owner, and uses the actual admitted fixed duration rather than Rapier's default step. A real
8ms free-fall RED precedes the correction; changed/zero-clock refusal preserves the whole native
state and repaired original input. Final default and Parallel-enabled motion pass 42/42 and
48/48. The continuous-stream qualification at `1532cdf`/`5197e64` retains nine original-Runtime
publications on one producer, plus independent old-input replay and fresh-Runtime streams.
Parallel/software3, Parallel/hardware2 and Serial/hardware2 pass; each full trajectory reports
eleven publications, exactly three producers and zero edge alternatives. Independent source/pose
pixels distinguish actual interpolation from current-only, preserve edit/Undo/Redo/disk Reopen
and retain old inputs/outputs after original Runtime/producer retirement. Default/Parallel
ordinary controls pass 9/9 each, final owning correctness Clippy and formatting pass, and the
owning plan records the final post-simplification software2/CPU9 repeats. The existing software
CI job includes the new native controls; no remote CI execution or publication is claimed.
These offscreen Windows/Rust1.95 results select no official Physics backend, native display
support, foreign execution or numeric performance budget. Parented/scaled dynamic poses and
full R4c/R10/Foundation remain open.

The [actual-game swept character continuation](../../plans/2026-11-04-authored-collision-derivation-plan.md#swept-character-movement-through-the-actual-game)
adds an optional directly consumed `nara_rapier2d` package. Immutable native static scenes, affine
boxes and stateless Rapier top-down sweep/slide are shared with the original motion fixture's fixed
geometry. The actual game owns durable `CollisionBox`, static wall roles, desired movement, final
TRS and reconstruction eligibility. Five visible walls per room replace ordinary boundary-only
movement, with distinct interior geometry in Meadow and Cavern. Construction/capture admission
retains its checks; ordinary movement does not rebuild geometry or rescan the Scene. Implementations
`d76aced`/`86eef47` pass owner16/16, original motion42/42 and full ordinary game326/326. Actual native
reconstruction passes with ten retained-input pixel replays. Relocated DX12/RTX4090 shipping play
passes physical contact/slide in both rooms, Save/Restore and twenty more travels, retaining a separate
software first-presentation control. Real body/private-structure rebuild adoption passes through
the original Host; owning correctness Clippy, format and document checks also pass.
No dynamic native-history serialization, default Physics backend, numerical speed claim or full
R10/R12/Foundation completion follows; Editor/UI/Dialogue product work stays paused.

The joint discontinuity unit adds semantic discontinuation to the existing CPU history lineage.
Original clones cannot be refreshed through late admission, first preparation, target adaptation
or retained candidate submission. Ordinary per-slot reset stays distinct. The independent two-View consumer
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
That predecessor does not establish seat/analog/replay, full editable text, target-build or
packaged delivery.

The November 13 [seat/gamepad/replay successor](../../plans/2026-11-13-seat-gamepad-and-input-replay-plan.md)
now separately qualifies the normalized seat/analog/replay portion under amended ADRs0041/0057/0102.
`ActionMaps` owns bounded independent seat policy; typed button/axis aggregates retain original
members through context/map cancellation. Opaque original gamepad connections extend the existing
finite Input log and terminal credits, while immutable replay ingress stays in the original
gameplay queue. A public default-configuration regression replaces accidental pristine authority
with explicit caller configuration. Final owning245 and independently locked15 pass after that
correction; affected Root82 and Game View6 retain startup/refund, pause and terminal-tail controls.
The ordinary two-player consumer serializes and loads raw operations, including Pause/Step/Resume,
and complete canonical mixed-source semantic batches. Fresh-runtime per-tick and continued-state
comparison survives physical rebind. Stop with held input/pending commands, late-token refusal
and finite recording bounds have executed controls. Existing actual acquisition/state owners
remain responsible for failed preparation and final refunds. This evidence does not qualify a
physical controller backend, universal recording/rollback or all R10 repeatability requirements.

The November 18 [actual-game authored Input successor](../../plans/2026-11-13-seat-gamepad-and-input-replay-plan.md#authored-game-input-through-source-and-player-delivery)
uses those source/processed owners through the game's complete recipe and cooker. Physical
definitions leave gameplay Rust; the existing command adapter retains stable semantic direction
and action meaning. Original unpublished installation keeps a read-only immutable selection for
capture/restore comparison, including a regression against earlier Startup map replacement.
Exact keyboard continuation uses actual installed keys and original platform reconciliation;
ordinary multi-binding play is not restricted by that narrower reconstruction policy. Final
Input/Winit297, corrected game/cook31, ordinary game347 and minimal-feature22 pass. Two actual
default/rebound-arrow Windows/DX12 reconstruction trajectories retain original pixels and cleanup.
External source-free packages preserve edited W-to-I and physically exercised D-to-J mappings;
actual headless and continuous desktop Rust-edit/adoption each pass. Original-selection `98969c4`,
native-key `7cbdd9f` and the linked unit own exact results and final-check scope. No physical
controller or broad R10 qualification follows.

The November 19 [native Gilrs successor](../../plans/2026-11-13-seat-gamepad-and-input-replay-plan.md#native-controller-implementation-and-qualification)
at `b607a64` and `6795753` adds the separate optional `nara_gilrs` package and an actual
source-game `--gamepad` selection.
The concrete Windows WGI context has bounded original-thread polling and managed close; nominal
inventory tokens assign existing window/seat routes. Original Input retains connection, member
and terminal ownership. Its read-only endpoint observation detects loss/refocus without retained
log authority. Typed retirement backpressure allows one-slot handoffs to wait for original
consumption, preserving hard zero/all-live refusal. The final 150 owning/public/bridge controls
and one genuine native initialization/close pass; the latter finds no hardware controllers.
The complete 207-case game cohort, default/arrow native reconstruction and controller-enabled first-present control
retain their linked execution scope. The last path also qualifies the separate `841ea03`
Windows EOF-device correction with 36 original process-owner and three independent public controls.
Authored controller Button alternatives preserve keyboard continuation eligibility, while actual
controller holds still refuse exact keyboard capture. The final five controller cases and
source native entry pass after the Input correction. Correctness Clippy, formatting and
document checks pass; both headless and desktop shipping dependency graphs exclude the
optional native backend. The selected foreign backend destructors lack synchronous retirement,
so acquisition refuses there before native context creation.
Physical hardware gameplay, analog game movement, shipping controller selection and full
R10/R12/Foundation remain open; paused Editor/UI/Dialogue work is not resumed.

The [explicit controller Player successor](../../plans/2026-11-13-seat-gamepad-and-input-replay-plan.md#controller-player-implementation-and-qualification)
at `4b862a9`/`a332a2f` shares game-owned placement/controller policy through source, cooker and
actual generated Players. The default remains no native controller; silent and audible desktop choices can each
select Gilrs explicitly without a new engine runtime kind. Original catalog/build owners verify
the actual selected recipes, profiles and compiler facts. CPU 270 and both actual controller
desktop/audio source-free deliveries pass. They retain relocation, unavailable source/cache,
physical keyboard gameplay, full pixel/Save/Restore/continued-travel and exact WASAPI sample
oracles. Observed Gilrs inventory is zero; this is native service composition/retirement evidence,
not physical controller gameplay or a latency budget. Existing profile names and content/save
formats remain unchanged. Original keyboard/audio and headless delivery pass; default silent
desktop passes on an isolated unchanged rerun after a recorded foreground interruption.
The twelve minimal-feature controls, correctness Clippy, formatting, dependency and document checks pass. Full
R7/R10/R12/Foundation remains active; physical controller interaction remains unqualified.

The [opt-in source-game round flow](../../plans/2026-10-20-persistent-two-level-game-plan.md#opt-in-playable-round-flow)
at `c2cf3dd`/`ffa7303` uses private typed gameplay policy over the original Input, time, command
and Scene owners.
Title, new round, pause/continue, completion, return to title and exit have seven passing public
source-game trajectories within the final 236-test release cohort; minimal feature controls pass
13/13. They retain the original fixed
acknowledgement while draining pre-pause ordinary intent, distinguish Host pause from zero game
scale, and reset only after actual new Scene reconstruction. Default exploration and checkpoint/
Resume wire formats remain unchanged; exact reconstruction rejects the unrepresented flow.
The optional existing-UI HUD/native profile passes actual Windows DX12 physical keyboard acceptance
on RTX 4090. It joins original native/UI submissions, positive text paint and glyph facts, and
completes original HWND, Runtime, pump and containing-process cleanup. The standalone ordinary
entry also passes; initial foreground refusal runs and corrections remain in the owner record.
General state hooks and scoped lifetime automation remain OQ-034; full Foundation remains active.

The [cooked round successor](../../plans/2026-10-20-persistent-two-level-game-plan.md#cooked-round-implementation-and-qualification)
extends the same round through the original Cook, independent Player expectation and native Runtime
owners. Runtime, controllers and flow are explicit game selections. The admitted Input definition
and its menu derivation retain exact handles. READY carries the original Title submission and the
current HUD even when a real Start precedes Resume confirmation. Menu state, Sprite animation and
the HUD agree before extraction. Both relocated Windows DX12 packages pass the full physical round
and original cleanup, including actual WASAPI callbacks and retirement for audio. The linked record
owns the precise results. General state hooks, flow-aware exact reconstruction, physical controller
play, PCM verification for this round and other targets remain open.

The [actual-game recording successor](../../plans/2026-11-13-seat-gamepad-and-input-replay-plan.md#actual-game-recording-and-replay-successor)
at `da96f1e` adds a source/headless exact-step game tool through the original command, Session and Scene
owners. It captures successful consumption, requires the original generation/tick acknowledgement,
then confirms Scene activation and game reconstruction before publishing a row. Fresh playback
uses the original immutable Replay-only ingress. The game/development protocol shares one finite
semantic vocabulary, and actual CLI processes exchange a bounded recording file. The owning
unit records focused21 and affected186 passing cases, actual separate-process CLI recording and
playback, correctness Clippy, minimal cooked-player compilation and formatting/document checks.
It retains ordinary versus quarantined-startup cleanup limits. This
does not qualify cooked/native replay, SaveSlot/audio effects, arbitrary live recording or
cross-platform determinism, and adds no validation to ordinary gameplay frames.

The [authored Prefab instantiation successor](../../plans/2026-10-20-persistent-two-level-game-plan.md#authored-marker-instantiation-successor)
at `6fd0135` and `895bb3c` qualifies explicit placement from the current Host's retained Immediate
Scene dependency under ADR 0117. The Identity and Scene owners retire the original level and additional authored
occurrences in one product replacement. The game uses this for marker placement, Retry, travel,
checkpoint reconstruction and exact source resume. Its [execution evidence](../../plans/2026-10-20-persistent-two-level-game-plan.md#authored-marker-execution-evidence)
covers four authored workflows, two relocated source-free Players and two Windows/DX12 source
reconstruction paths. Activation lifetime remains tied to the Host's content custody. This
unit leaves independently retained activation resources, streaming and hardware audio open.

The [textured Prefab successor](../../plans/2026-10-20-persistent-two-level-game-plan.md#textured-prefab-runtime-successor)
at `52a929d` uses a required Image edge from the retained marker Scene. Both levels keep their five direct
dependencies, and each marker occurrence receives the same resident badge handle for its level.
The sample records geometry from the original Scene installation for Retry and exact state
capture. Its [execution evidence](../../plans/2026-10-20-persistent-two-level-game-plan.md#textured-prefab-execution-evidence)
records 77 passing release regressions and five Windows paths for source-free package loading,
Restore, WASAPI and DX12 readback. Resource residency and retirement stay with the containing
Scene and retained render inputs.

The [Deferred additive region](../../plans/2026-10-20-persistent-two-level-game-plan.md#deferred-additive-region-execution-evidence)
at `81d1b6b` adds one Image-only companion to the original source and cooked Host under ADR 0117.
It keeps the primary Scene and clock active during preparation, then activates and unloads by exact
occurrence. The room can share a primary Image and release its separate relic Image while retained
render inputs remain valid. Host/source 94, sample 240 and all three Windows cooked Player paths
pass. The broader active-set, cross-region travel and native service lifetime design remains
Proposed in ADR 0089.

The [concurrent regions successor](../../plans/2026-10-20-persistent-two-level-game-plan.md#concurrent-regions-implementation-and-qualification)
at `a3f6293` extends that residency owner to independent Side Room and Lookout occurrences. It shares original
Image and catalog publications and releases each after its last live Scene user. Primary travel
captures the group revision and retires live additions through the original Scene transaction.
Native retirement fences region changes while an exact attempt is pending. Public retirement
selections now borrow receipts for preparation; the original Scene owner retains copies after
membership admission. Source recipes can select their decoded-generation ceiling. The 99 Host/public,
53 Scene, 22 source/cooked and 276 affected gameplay cases pass. Actual headless delivery and the final
Windows hardware DX12/WASAPI Players pass. The owning record distinguishes interrupted foreground
attempts and the cooker-cleanup correction at `9bbea40` from the final complete physical workflow.
General scene-scoped native services, destination sets and full ADR 0089 remain open.

The [Material-backed region successor](../../plans/2026-10-20-persistent-two-level-game-plan.md#material-region-implementation-and-qualification)
at `780ce6b` and `640024a` adds exact shared Material ownership, per-occurrence use and last-live-user release to that same
Host. It preserves original Shader/default/texture semantics, including floating-point bits,
and refuses a conflicting live definition. Source and cooked rooms share the original slot and
version. Host/public 107 and affected game/cook 305 pass. A strengthened hardware DX12 oracle
checks graded relic pixels and their disappearance, then replays four original packets after
actual travel and Runtime close. Headless, physical desktop and WASAPI packages run source-free.
The record distinguishes the corrected shipping-only trait import from final native evidence.
No runtime per-frame definition audit or universal additive-service contract is introduced.

The [TileSet-backed region successor](../../plans/2026-10-20-persistent-two-level-game-plan.md#tileset-region-implementation-and-qualification)
at `759daae` and `c32d2ad` adds explicit shared declaration admission to the original `TileSetAssets` transaction. The Host
pools only owned receipts, records each region's use and retires the last live user's declaration.
The original atlas rebuild barrier and current Image capture remain authoritative. Both authored
rooms draw eight-cell Tilemaps from one TileSet without republishing its handle/version. Tilemap
119, Host/public 107 and affected game/cook 313 pass. The hardware DX12 pixel trajectory checks
both floor patterns and disappearance, then replays four complete original captures after travel
and Runtime close. All three actual source-free headless/desktop/WASAPI delivery cases pass.
The record retains the processed whitelist correction and identity-versus-residency fixture failure.
These changes qualify the concrete Image/Material/TileSet region workflow; general additive
native services, destination sets and full ADR 0089 remain open.

The [source/processed admission successor](../../plans/2026-10-20-persistent-two-level-game-plan.md#source-agreement-and-processed-material-kind-successor)
uses original source delivery for the manifest-selected Schema and carries admitted member
observations into the existing cooker fence. The completed processed plan must match that Schema.
The original deep validator checks each decoded Material texture against its selected Image
codec role; a present Scene Product with a matching edge rejects before World/native creation.
Content/builder owners 81, Cook/source 31 plus one desktop/audio preparation control and the
independent Material 4 pass, including
source-free gameplay, absent default bundle, selected-source replacement and independently opened
wrong-kind content. The actual nested-Cargo package trajectory also passes 1/1 with relocated
source-free execution, checkpoint Restore, damaged-content refusal and physical native-process
cleanup. This closes the scoped F2/F5 implementation paths; package qualification and limits live in the
[review receiving record](../../research/2026-11-18-engine-architecture-review-reconciliation.md#source-agreement-and-processed-material-correction).
The shared Cook successor below now implements the common compiled entry. The request-owned
generated-input successor replaces the former shared-source bootstrap write.

The [Material texture custody successor](../../plans/2026-11-14-durable-material-and-shader-plan.md#material-texture-capture-implementation-and-qualification)
at `163dffd` and `10ee433` freezes each complete Material/Image binding during original Extract
and shares it across Sprite/Tilemap cells, Views and batches. Queue consumes these exact textures.
Managed and code-first admission remain distinct existing Image paths.
Owning CPU 57 and public Material 9 pass; hardware DX12 4 and software DX12 3 prove exact A/B
pixels, authored omission/recovery, same-Host replay after World retirement and post-submit panic
cleanup. Both independent production and corrected testing review have no remaining P1/P2.
The record distinguishes live GPU references from immutable content, local CI configuration from
remote execution, and offscreen Windows qualification from full R9b or performance claims.

The [request-owned generated-input successor](../../plans/2026-10-31-cooked-player-delivery-plan.md#generated-input-implementation-and-qualification)
at `ebc50da` places immutable generated Rust source on the original Cargo request. Trusted build
scripts write only Cargo's `OUT_DIR`, and absence retains ordinary Player refusal despite stale
generated output.
The original target lease, process custody and content/code fences remain authoritative. Package
binding requires separate generated name/digest provenance; canonical unpublished v1 fields and
current APIs replace their physical-source forms without compatibility fallback. Owning 17,
independent actual A/B/A plus Palette 2 and affected Two-Level 36 pass. The strengthened parent-env
control and all three actual source-free headless/desktop/WASAPI package trajectories pass.
The failed physical attempts, diagnosis, final Windows Rust 1.95.0 native evidence and
reserved-variable/configuration constraint live in that owner plan. Shared cooking has its
separate completed consumer evidence below.

The [shared compiled Cook entry](../../plans/2026-10-31-cooked-player-delivery-plan.md#shared-cook-implementation-and-qualification)
at `c3698d3` / `c2d09ca` uses the original source loader's sequential strict snapshots, bound
Scene/Prefab results and stock assets. Explicit game build facets retain complete groups and
optional input observations.
The original builder remains the encoding/deep-validation authority; Cargo and publication retain
source/code selection and physical cleanup. Two-Level removes its duplicate preparation, while
the real generated starter adds Cook/Player and preserves distinct source Export. Owning/public
86 and sample 38 pass, as do the actual independent generated three-Scene/three-Image game after
relocation and source removal, its exceptional child-cleanup control, and all three Two-Level
headless/desktop/WASAPI packages. Windows native qualification does not establish atomic or
hermetic source snapshots, foreign execution, numeric budgets, server stripping or full R7/R12.
Original generated-project compilation and custom-schema/source Export pass; the stale public
data-component test consumer is corrected at `abc0a95` and its ten workflows pass. Root/sample
correctness Clippy, affected Editor compilation and final format/document checks pass.

The [task-result/replay publication correction](../../research/2026-11-18-engine-architecture-review-reconciliation.md#runtime-result-and-replay-publication-corrections)
at `e67bdca` / `0f979aa` closes review F9/F11 within their original owners. `OrderedTaskResults`
now rejects a different completion owner before insertion, returns the original handle untouched
and releases its binding when empty; no retained token field or per-cutoff owner scan is needed. Task/public 113,
source-build 6 and Host scope 13 pass. The replay CLI publishes a complete synchronized sibling
temporary with `ExpectedTarget::Missing`, preserving foreign targets and original failed-cleanup
custody. Its results distinguish absence, publication and uncertainty with separate durability
facts. Actual CLI/publication 19 pass, including a real fresh-process replay and invalid native
Unicode output refusal before publication. Parent-sync/crash recovery and foreign execution
retain their explicit limits. Affected save/replay/checkpoint controls pass 31 with original
canonicalization, last-good, source-drift and cleanup behavior. The correction adds no replay
store or replacement task/FS owner. Task/replay correctness Clippy, format and document checks
pass; the linked record retains exact execution scope and review corrections.

The [independent author workflow](../../research/2026-11-18-engine-architecture-review-reconciliation.md#author-workflow-implementation-and-qualification)
at `752fbd8` / `af7886c` uses ordinary plugin, creation, schema, content, Cargo and Player owners
without adding a game API. Scoped CLI help and the generated first-system example precede the
complete sample. Two
same-named real external games retain their distinct creation identities and generated
Cook/Player/build-script entries, add gameplay while keeping the Image codec, finish/reset source
play and preserve all observed authored bytes. Both relocated cooked baselines run after
corresponding source/cache removal; pack substitution and equal-length catalog mutation refuse
before gameplay. These are scoped Windows Rust 1.95.0 results, with baseline versus win/reset,
remaining local compiler-input declarations and foreign/clean-machine limits explicit. The
guide's manifest/source-tree overwrite instructions are removed. The complete two-project run,
all 29 selected ordinary public tests and five final help subprocess tests pass, as do correctness
Clippy and format/document checks. Recipe and compiler diagnostic projection and paused
interactive authoring remain separate receiving actions.

The [recipe and compiler diagnostic successor](../../research/2026-11-18-engine-architecture-review-reconciliation.md#diagnostic-implementation-and-qualification)
at `01c25ef` adds finite known recipe classifications and admitted plugin facts to the original protocol,
with opaque fallback that never invokes arbitrary formatters. Generated and collection-game
entry points propagate one fallible recipe through tools and products. Cargo retains a bounded
primary location; original workspace-relative interpretation precedes selected-project display,
including sibling withholding and existing truncation accounting. The 97 owner tests, two actual
independent consumers, repeated complete author workflow, 35 ordinary public tests and eight
render-enabled recipe tests pass, alongside affected correctness Clippy and formatting/document
checks. The linked record owns exact qualification and Windows-only execution limits. This does
not complete R1/R7, missing-sidecar location projection, interactive authoring or full Foundation.

The [live-command and author-inventory correction](../../research/2026-11-18-engine-architecture-review-reconciliation.md#command-and-inventory-implementation-and-qualification)
at `a6e95bd` / `f43f98d` keeps one game-owned movement decoder for live input and recording,
with original Capture, acknowledgement, phase gates and recoverable game errors. Complete command
drafts move into the same action map; three game callers use the constructor and the obsolete whole-command setter
is removed. One existing component inventory supplies the collection sample's schemas and native
registration without changing its committed fingerprint or predecessor contract. All 60 command
tests, two bindings tests without serde, 189 source-game/replay tests, five collection-game tests
and 42 in-process source-free/region tests pass, with affected Clippy and Reference desktop-caller
compilation. The linked record retains
the fixture/target-alias corrections and Windows Rust 1.95.0 limits; no native UI, full publication
matrix or measured speedup is inferred. Full R7/R10/R12 and paused UI remain separate.

The [missing-native-metadata location successor](../../research/2026-11-18-engine-architecture-review-reconciliation.md#missing-metadata-implementation-and-qualification)
at `2b1cd2c` adds one producer-owned project-relative sidecar path to the existing content failure and
compiled-tool projection. Local aliases remain distinct from project coordinates; public
callers inspect the same optional path as the CLI. Original failure classification, omission
of other fields, bounded response and capability ownership remain unchanged. Execution qualification
passes all 52 selected protocol/content regressions and one actual generated-project CLI/public
caller recovery workflow on Windows with Rust 1.95.0. The linked record retains exact scope and
fixture corrections. Affected Clippy, formatting and document checks pass; no per-frame audit,
new filesystem authority or completed R7/R12/Foundation claim follows.

The normalized seat/replay unit's final API review also removes the ordinary consumer's hidden Input scheduling and raw UI
completion calls. Public `InputSet` documents existing ordering; `route_pending_ui_input`
synchronously lets a custom policy return decisions while the original owner finalizes each
retained entry. Native presentation authority remains unchanged. Final affected426 and the
supported-only independent15 pass, including callback-unwind prefix and original release custody.

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

The November 13 [dependent-refresh prerequisite](../../plans/2026-10-31-local-artifact-build-plan.md#ignored-tracked-read-failure-qualification)
retains the first failed tracked read in the original finite build executor. A domain that
ignores that failure cannot publish a closed recipe with the failed input omitted; real optional
absence keeps its existing meaning. The independent TextBundle regression exercises exact
budget refusal, no publication callback and corrected-input recovery; its final durable-build
suite passes 20/20 on Windows x64 / Rust 1.95.0. That prerequisite has the following live
source-build successor; it did not itself implement refresh.

The [live typed refresh successor](../../plans/2026-10-31-local-artifact-build-plan.md#dependent-refresh-implementation-and-review)
connects the original source resolver and Tasks to typed `Assets<BuiltSourceAsset<P>>` through
an explicitly compiled `SourceAssetBuildPlugin<D>`. Canonical raw paths include actual absence
and metadata selection; current adoption atomically replaces the successful set, while failed
execution keeps separate retry observations and last-good. Global admission refusal cannot
truncate an existing recovery set. Requests retain original generation-resource identity;
Image uses the same currentness guard without losing its recovery protocol. Source/sidecar
raw origin survives notification coalescing, and deletion revokes known publication even if
cleanup cannot be queued. Final Asset/Watch/Image tests pass 366/366 (one existing measurement
skip), and affected root build/content tests pass 111/111 on Windows x64 / Rust 1.95.0. The
independent TextBundle workflow proves actual automatic updates, A-to-B replacement, missing
input recovery, warm reuse and unchanged escaped Runtime bases; its final complete asset-domain
package passes 95/95. Workspace/all-feature, strict independent consumer and correctness Clippy
checks pass, with pure/serde Asset, minimal runtime-core and portable Wasm compile evidence.
Public guide and final qualification retain their actual scope; processed-input scheduling, immutable Scene/base
replacement and the remainder of R6 remain separate.

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

The [Two-Level Player integration](../../plans/2026-10-20-persistent-two-level-game-plan.md#cooked-player-native-qualification)
at `bb02fed` extends the same cooked-content/Cargo/package/Runtime owners to the actual game. The fixed
recipe keeps launch scenario/save-path data in the original first-tick gameplay queue, not in
an altered catalog fingerprint. Original `ProjectLocation`, explicit shipping Products, actual
Schema and two Image imports remain distinct. Ordinary selected CPU controls pass 135/135;
the actual Windows release delivery passes 1/1 through generated code, relocation/source-cache
removal and six sealed native processes. Travel, checkpoint Save/fresh Restore, exact resident
Images and Scene receipts, Deferred corruption preservation, catalog refusal and normal original
process/containment/readers/output cleanup pass. Correctness Clippy and formatting retain their
recorded selection. No engine owner or format changes; foreign, desktop processed, server and
processed exact-reconstruction scope remain open.

The qualified [desktop game successor](../../plans/2026-10-20-persistent-two-level-game-plan.md#desktop-player-native-qualification)
reuses those owners with explicit headless/desktop cooker placement and a separate fixed desktop
recipe at `4267672`, following native cleanup corrections `1fa730d`/`b92fd2e`. The entry uses original paused
DesktopRun presentation, first-tick launch and Resume;
bounded optional observations do not cap ordinary play. Current sample ordinary155/155 and
process35/35 controls pass. The process receipt separately records native termination attempts;
`b92fd2e` adds a fixed natural-drain allowance before residual-tree termination. Empty containment
and `force_killed=false` cannot be mistaken for a naturally exited tree.
Actual source-free package/keyboard/checkpoint/fresh-process/native-Close acceptance passes1/1:
two RTX4090/DX12 ordinary sessions,20 further travels, software DX12 first-present, three finite
scripted controls and invalid initial-path refusal. Actual launch application is observed from
the original gameplay owner rather than inferred from ingress or generic action errors.
The original Runtime, pump and windows retire with no native termination, while CWD stays empty,
the three-file installation stays immutable and stored checkpoint bytes stay unchanged. The linked
record retains exact rendered-receipt/CPU-content scope; pixel output, numeric costs, foreign
execution and full Foundation remain open.

The [game-owned shipping feature closure](../../plans/2026-10-20-persistent-two-level-game-plan.md#development-tools-and-cooked-runtime-feature-closure)
at `20d47a2` removes unconditional development Host/Cargo/watcher selection from this same game.
The actual target normal/build dependency graph admits only the explicit headless or desktop
Player features, and production delivery checks it before generated compilation. Default source
tools and exact development requested/resolved admission retain their existing runtime/process
owners. Final minimal119, all-feature188 and cook-only7 ordinary controls pass in their separate
selections; actual source-free headless/desktop packages and headless/continuous-desktop rebuilt
adoption pass4/4. Existing source/cache deletion, physical keyboard progression, fresh Restore,
hardware/software presentation and no-intervention cleanup assertions remain. Correctness
Clippy, selected compilation, formatting and document checks pass. Root/domain Rust, dependency
versions/sources and locks do not change. This game dependency correction does not complete
root source/schema stripping, server delivery, foreign execution, numeric budgets or Foundation.

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

The November 14 [material declaration/Shader successor](../../plans/2026-11-14-durable-material-and-shader-plan.md)
at `899d64b` replaces static Palette definitions with one typed/dynamic field declaration,
actual Encase layout and Naga interface admission, original asset-baseline publication and
immutable exact bindings. Sprite/Tilemap and native stock drawing consume all declared Image versions; value/texture
content does not specialize pipelines. The final combined material/derive/Sprite/native suite
passes 498/498 with 65 explicit skips; independent Palette and renamed-root material consumers
each pass 1/1. The selected native group passes 13/13, with its production Host reporting
Vulkan/RTX 4090. The same five Host trajectories also pass on DX12. These overlapping scopes include
nonzero mixed layouts, integer/texture controls, same-epoch compatible B/rejected C/late A,
whole-program budgets, partial-OOM original-prefix recovery and bounded Ready/Rejected cache
history without restricting a single 66-variant working set. The linked plan retains the actual
failed fixtures, independent review corrections and remaining product scope. Complete persistent
Schema/source/Runtime material delivery, graph-shared material resources, full R9b/R12 and the
foundation Goal remain open. These scoped results do not admit the full Proposed0121.

The persistent successor at `2535b46`, with independent consumers at `d091151`, has integrated
product qualification: an imported material Product,
one authored carrier, complete typed source
dependency discovery, candidate-local validation and original Scene resource publication.
Its final owning CPU/combination selection passes 241/241 in
`75e72673-2dcf-45d7-8a22-e8bcdeb2c2f1`. Snapshot
clones retain original source/Runtime charges; managed extraction checks actual Product and
slot authority. The final independently locked cohort passes 8/8 in
`cdc577e3-638e-40bc-912a-5af6cce82272`, executing prepared edits, Undo/Redo, exact
Save/Reopen/Check, source material residency and source-free processed running. Complete typed
parameter maps reuse the existing safe authoring shapes, and the first-party native carrier
joins the original Editor/default composition without executing arbitrary recipe plugins.
Actual source Export passes 1/1 in `868062fd-b075-4f56-9f0a-ef5a48026fc0` and the separately
compiled cooked Player passes 1/1 in `aa341ef5-71a2-4d17-9b01-cfe0dd6346af` after relocation
and source/cache deletion. Six current authoring bundles add the actual provider with all old
owner records preserved. The affected native material/cache cohort passes 15/15 in
`f8cf8622-7eb0-41bc-b11d-105f7febbe8e`, and the production Host DX12 cohort passes 5/5 in
`9badbda0-1909-45bd-9e01-0bb765ed2d5a`. Reference composition passes 4/4, both sample Check
entries succeed, and final workspace/correctness/strict consumer Clippy/format/document checks
pass in the same linked plan. Automatic live Material/Shader preview, numerical budgets and full
R9b remain open.

The graph-shared input successor at `13a0927`, with independent consumers at `e9d8755`, adds
explicit exact material/semantic-slot bindings to program textures, complete sampled-access
preflight, original native realization and local builder refusal/retry. Selected graph textures
bypass Image upload/cache accounting and retain
their original program/submission storage. The final affected owning/public-sidecar selection
passes 324/324 in `96cd0d92-dfb1-4dcf-b8f6-112f088a35b2`; the independent source/stock/material
cohort passes 21/21 in `9dc4cc7d-0e17-404f-b5ce-af704c1f6230` on Vulkan, and its four Material
native trajectories pass DX12 in `069b1603-8d48-478c-8283-bd09ae3e5108`. A targeted original storage
observation survives actual pending submission after all external source leases release, then
expires on its real callback. CPU consumers use the original public sealed-batch take, also
adopted by Editor Game View, without selecting native-window presentation. Same-Host device
reconstruction, automatic source dependency updates, live Editor hydration, foreign execution
and numerical budgets remain outside this qualification. Feature-matrix/workspace build,
correctness/strict independent Clippy, format and document checks pass in the linked plan.
The full R9b/Foundation Goal stays open.

The [Scene-owned source-update successor](../../plans/2026-11-14-durable-material-and-shader-plan.md#scene-owned-source-updates)
at `c03b940`, with independent consumers at `55b0ae4`, adds semantic source ingress to the existing
`ProjectScenes` surface. Original successful closure facts and the latest complete valid
automatic-failure read cut drive one coalesced successor;
missing newly referenced WGSL can recover after a creation notice. Ready content stays frozen,
gameplay overlay and publication remain explicit, and terminal slots require exact acknowledgement.
Source interest captures the original Runtime generation and materialized startup instance without
early current-Scene publication. Actual ordinary/managed commit advances its closure, including
publication preceding degraded reconciliation. No generic asset UUID, second publisher, watcher or
scheduler is introduced. The owning/affected controls pass 145/145 in
`ba5bb798-cfd1-4936-a313-c57f92bbd684`; the full independent Material package passes 10/10 in
`da08a271-6156-4ddb-93bf-1abdd81fdd77`, including eleven automatic requests with eight commits,
two refusals and one cancellation while preserving real asset state, decoded pixels and gameplay
progress. The independent managed-domain controls pass 16/16 in
`ebdb3fa5-15cf-4c77-92cb-130a3ad95eb6`, retaining original native custody assertions plus
source-interest checks before publication and during degraded reconciliation/retry. Final workspace
build, owning/independent correctness Clippy, format and document checks pass. The adjacent public
Input consumer migration at `a51ad6e` changes no production Input behavior. The linked plan records
review corrections and verification scope. Native watcher
composition/close, successful optional fallback availability, Editor preview, foreign execution,
numerical budgets and full R9b/R12 remain open.

The [native Scene source-watching successor](../../plans/2026-11-14-durable-material-and-shader-plan.md#native-scene-source-watching)
at `f948da9`, with the shared watcher close owner at `bf8bda1` and independent game at `159da63`,
implements the selected physical route without a second asset publisher. It matches the explicit
native root to the original loader, watches selected directories plus uncovered ancestors and
establishes fresh coverage before full reconciliation. The maintained notify Windows/Linux patch
retains and joins the original worker; registry custody survives setup failure, Drop, rescan and
Stop. Read-only observers can outlive the World without retaining a native backend after close.
The owning/affected controls pass 112/112 in `cd789eef-abc3-435a-b9a4-f7b183027a98`; the full
independent Material package passes 11/11 in `e30abfbf-4111-43b6-9d9a-c777bc3b4712`. Its actual
file edits produce eight explicit commits and three refusals, preserving complete material/image
facts and gameplay progress, with original workers joined before World destruction. Windows live
descendant watches deny ancestor moves; separate gap controls retire/join them before real
replacement and explicit recovery. Linux library/test and root production cross-compilation pass,
but Linux execution remains unqualified. Workspace, owning/independent correctness Clippy, format
and document gates pass. Windows undifferentiated native events currently require full rescan;
native filtering costs, notification-to-GPU presentation, optional-fallback interest and full
R9b/R12/Foundation completion remain open. Editor/component work remains paused.

The [live Source-to-GPU successor](../../plans/2026-11-14-durable-material-and-shader-plan.md#live-native-source-updates-through-external-gpu-execution)
at `0602b5d` adds an ordinary caller-driven `HeadlessSession` consumer of the original sealed
stock batch. Its external WGPU Host executes and reads pixels between live Runtime frames. File-backed
defaults reserve native-window Publish consumption for projects requesting desktop capability;
compiled desktop support cannot steal an external consumer's batch. The complete independent
Material package passes 14/14. Actual Vulkan and DX12 each pass 1/1 in
`3f66fcc3-00c3-483a-846e-17758f7c1403` and `ebade381-d85b-4155-9364-0c35535c5c6b`, covering
eight explicit commits, three refusals with new last-good frames and two first submissions of
retained CPU inputs after Shader/Image successors. Old CPU inputs release their original Source
budget before a third revision; native pipeline/Image caches and 14 published outputs retain
their separate native custody. Outputs survive Stop and producer retirement, all 11 watcher
workers join, and final GPU retained bytes/candidate bytes/cleanup faults are zero. These are
Windows/Rust 1.95.0 live offscreen results, not OS-window scanout, foreign-native execution or
numeric production budgets. Full R9b/R12/Foundation and paused Editor work remain open.
Affected root composition/configuration passes 73/73, with final independent controls 14/14;
owning/independent correctness Clippy, format and document checks pass. The linked unit owns
the failed attempts, corrections and exact final scope.

The [ordinary desktop Source successor](../../plans/2026-11-14-durable-material-and-shader-plan.md#live-source-updates-through-ordinary-desktop-presentation)
at `177684e` qualifies the physical-file workflow through public main-thread DesktopRun,
preserving the original window Publish consumer and runner-local GPU owner. Sealed copied
stock/Scene facts join 12 representative states to exact present and completion receipts.
Vulkan/RTX 4090, DX12/RTX 4090 and DX12 software rendering each complete eight commits and
three refusals, with clean original telemetry, native target/provider/surface retirement,
all 11 watcher joins before World destruction and one physical-pump recovery. Three actual
fixture callback failure controls verify that same retirement before resuming the first panic;
production panic policy is unchanged. Ordinary independent controls pass 14/14, the original
offscreen Vulkan/DX12 trajectories each 1/1, all-target correctness Clippy and format/document
checks pass. Linux cross-compilation covers the ordinary example only; native Linux execution,
window pixels/scanout, source-edit latency and numerical production budgets remain unqualified.
No new renderer, GPU/World owner, batch publisher or native loop is introduced. Full R9b/R12/
Foundation remains open and paused Editor/Dialogue changes are preserved.

The [captured View and Shader time successor](../../plans/2026-11-14-durable-material-and-shader-plan.md#captured-view-and-shader-time-successor)
is implemented at `ac6828a` and `26af57b`, with the independent consumer at `9212074`.
The Extract owner freezes the existing App real/virtual/fixed/render clocks, settings, validated
spatial stamp and actual interpolation decision with each ordinary View; target adaptation and
delayed preparation retain that sample, while manually supplied views advertise unavailable clock
data. The stock Material
interface admits the engine-owned 224-byte `NaraView` at group 2 binding 0, outside authored
parameters; WGPU uses bounded per-preparation byte-sharing and the original allocation/submission
custody, with time excluded from pipeline variants. Owning Material/Render/Transform controls pass
179/179 in `fa56fe66-85fc-4be6-88e0-a32d85f96d29`, the corrected all-feature owning cohort passes
558/558 with 67 skips in `c423765c-ff30-4d49-8a99-cdc742456eb6`, and the independent ordinary
consumer passes 1/1 on Vulkan RTX 4090 (`05d558a5-e4d3-485c-a0ce-44cf0c0a5cb3`), DX12 RTX 4090
(`6ff5a302-73b7-435c-9c28-b85ed6ba2e99`) and DX12 Microsoft Basic Render Driver fallback
(`bb70dc1d-8ac6-425b-86a6-11584d4fe038`). Each consumer run captures/publishes seven frames,
retains one pipeline, replays old View/time data after a real Camera change and releases its
original World. These results do not qualify arbitrary View effects, full temporal compatibility,
scanout or numerical production budgets; full R9b/R12/Foundation remains open.

The [joint Material and history successor](../../plans/2026-11-14-durable-material-and-shader-plan.md#joint-material-and-history-discontinuity-successor)
at `693149b` combines ordinary Material View/time, real Animation/Rapier motion and the existing
temporal graph in one independently locked public consumer. ADR0094's original lineage cut
rejects late raw A, first preparation of admitted A and submission of prepared A as
`StaleHistory`; only the affected View's predecessor slots reset. The concrete effect selects
Camera cuts, solver corrections, parameter adoption and same-ABI Shader edits as incompatible;
invalid Shader edits retain last-good and continuous history. No new public taxonomy or History
owner is introduced. An independent full-pixel and float-history oracle checks four cuts,
fourteen temporal publications, four late non-temporal stock outputs, exact fixed tick 4 at
`(1.25, 2)` and native pipeline count `7 -> 8`. Outputs remain readable after original World
destruction and both drawing-producer retirements; final retained bytes, candidate metadata
and cleanup faults are zero. The affected DX12 selection passes 9/9; final joint runs pass
on DX12/Vulkan RTX 4090 and the combined Material/history selection passes 2/2 on DX12 software.
The updated ordinary history window separately passes its three-presentation/retirement
trajectory. Correctness Clippy, independent formatting and configuration/document checks pass.
These are overlapping scoped results, not full platform, TAA/reprojection, scanout or performance
qualification. Editor/Dialogue remains paused; full R9/R12/Foundation remains open.

The [depth and spatial-unit successor](../../plans/2026-11-14-durable-material-and-shader-plan.md#depth-and-spatial-unit-successor)
implements ADR0094's bounded depth extension at `199a39d`, with an independently locked public
consumer at `d7e7c54`. `TextureInterpretation` separates Color transfer/alpha from Depth; whole
`Depth32Float` uses finite `[0, 1]` clear or exact predecessor copy, with existing semantic ports,
unique writers, initialization, budgets, immutable outputs/imports and original native custody.
Policy final images and current color-only consumers refuse depth. The consumer draws captured
meter/centimeter geometry in both orders, samples original/preserved depth, changes a 64-pixel
preserved region without touching original attachments and reads retained output after producer
retirement. Upload-after-preparation failure retains actual work and its original producer
prefix, physically cleans it and retries the same input. Full-pixel/depth oracles pass on DX12
and Vulkan RTX 4090; explicit DX12 software also passes with the original three native color/
buffer controls. Owning default 442/442, selected native 21/21, standalone CPU 11/11, affected
History/Material 9/9, typed-port doctests 2/2, six direct caller compilations, correctness Clippy
and format/document checks qualify their recorded overlapping scopes. A stale 20,000-batch
fixture premise is replaced by an actual cost-derived crossing of the unchanged 64 MiB ceiling;
no production budget is relaxed. This selects no global Camera3d, handedness, reverse-Z, stencil,
MSAA, foreign platform or performance contract. Full R8/R9/R12/Foundation remains open.

The [presented drawing/picking successor](../../plans/2026-11-14-durable-material-and-shader-plan.md#presented-drawing-and-picking-successor)
at `cf22a5b`, with independent consumer `a7a5fc8`, refines ADR0094/0111 with an optional immutable
`WorldInputMapping2d`. Original policy admission evaluates it once; stock declares identity and
custom policies default to unavailable. Runtime and embedded Game completion join the captured
inverse to existing Window/Canvas/View/camera
facts only after presentation. A pending successor retains the old mapping; displayed unmapped
output revokes it. The independently locked ordinary consumer exercises parented/mirrored
cameras, coordinate-preserving and horizontal effects, exact post-Host `WorldPointer`, retained
CPU input replay and immutable typed outputs after original native/World retirement. DX12
hardware passes 3/3 including the injected observer panic; Vulkan hardware passes actual mixed
DPI and DX12 software passes one display. The current 150%/125% crossing settles original
client extents `384x256 -> 320x213 -> 384x256`, with native and captured geometry agreement.
The linked qualification records owner/bridge checks, separate OS title-bar dragging, exact
prefix/resource cleanup and review corrections. The render/WGPU libraries pass 515/515,
original native guard controls 4/4 and affected Game bridge controls 20/20; correctness Clippy,
format and document checks pass with their recorded overlapping scope. Replay pixels are not
front-buffer/scanout readback. Nonlinear effects, UI hit retargeting, foreign native execution and numerical budgets
retain separate scope. Editor/Dialogue remains paused; full R8/R9/R12/Foundation stays active.

The [measured geometry successor](../../plans/2026-11-14-durable-material-and-shader-plan.md#measured-sprite-and-tile-geometry-successor)
at `10cd109` supplies an independently locked domain-module Runtime/stock-policy/Host consumer.
The private Material value-sharing correction `c796594`, measured at `94fc219`, preserves
original generation, value equality and resource lifetime while reducing per-cell clone work.
Six Vulkan/RTX 4090 release processes retain 13,824 sample rows, full pixels, old-input first
realization after World retirement, retained outputs and zero final Host cleanup. The
[report](../../benchmarks/2026-11-15-public-geometry.md) records 84.740--91.960% lower large bound
managed-drive/capture P50 against the fastest matching baseline, at most 0.0124 ms small-scene
P50 regression and identical thirteen sampled native/resource/count fields. Existing
Material/Sprite 136, selected Host/stock Material 21, independent CPU 2 and asserted
Vulkan-hardware/DX12-software pixel controls pass in separate overlapping scopes. Independent
review, owning correctness Clippy and strict consumer Clippy pass. The private legacy cost
collector is removed with historical reproduction kept separate. This qualifies the simpler
immutable-value correction under ADR0096; a retained geometry cache, total memory budget,
stable production tail, foreign execution and full R9c/R8/R9/R12 remain open.

The following private borrowed-queue experiment at `6c88c87` is rejected against three fresh
`814899e` baseline processes. Only 13/18 large bound comparisons meet its 10% P50 gate, despite
unchanged thirteen sampled count fields and passing full pixels, late original-input replay,
retained outputs and final cleanup in all six processes. The
[same cost report](../../benchmarks/2026-11-15-public-geometry.md#visible-queue-material-capture-follow-up)
preserves all raw samples, distributions and digests. The original owned queue and earlier
immutable-value correction are retained; new offscreen/zero-View diagnostic and exact multi-View
Shader/Image capture controls remain. This negative outcome adds no new render owner or cache.
Final selected Material/Sprite 138, independent CPU 2 and actual DX12 software full-pixel control
pass in their separate scopes; correctness Clippy, format and document/data checks pass.

The November 16 [bounded tile extraction trial](../../benchmarks/2026-11-15-public-geometry.md#bounded-tile-extraction-reuse-trial)
is also rejected under ADR0096. All six public measurement processes retain complete pixels,
old-input/output lifetime and identical native work fields, but zero of 24 large static/camera
comparisons meets the preselected 15% P50 gate. The private cache and dedicated nominal cell
revision are removed; Tilemap and extraction production sources match the pre-trial base.
Eight source-refresh/current-owner tests and an independent warmed-input-after-World-retirement
pixel control remain at `9b24741`. Final owners pass 158/158, independent CPU/Vulkan hardware
4/4 and DX12 software native 2/2, with correctness/strict Clippy and formatting. The linked report retains
candidate source, all raw digests and independent data review. No geometry-cache adoption or
general rendering budget follows; R9c/R12/Foundation remain open.

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

The November 14 [retained Stop correction](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#retained-stop-control-observation)
at `0e784ec` implements ADR0076/0084's operation-result lifetime separately from physical close
authority. The original Stop record survives ordinary-history eviction and preserves its initial failed
attempt when a later RetryClose succeeds. Each retry has a distinct ticket; current Pending
observations remain available, ordinary completed results can expire and foreign generations
remain unavailable. The owning public regression passes 2/2 after reproducing the original
failure, and the complete App suite passes 180/180 with one existing disposable-child skip.
The root Runtime/renamed-dependency targets pass 68/68, and the independently locked two-Runtime
consumer passes 1/1, retaining the survivor's original systems and real participant close/drop
counts. The linked plan records exact commands, results, review and Windows CPU limits. This
does not close full R4c/R12 or add a lifecycle, observation-cache or physical-retirement owner.

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

The November 13 [managed startup pause correction](../../plans/2026-11-03-joint-motion-execution-plan.md#managed-startup-pause-correction-november-13)
restores ADR0039/0084's existing Startup time-policy contract: promotion preserves the final pause
setting and exposes matching Runtime state, with faults retaining priority. Public configured and
Startup pause cases retain real-time maintenance, exact Step and Resume; Startup can also resume
an initially paused configuration. The release-mode App library/public cohort passes 164/164
with one explicit skip on Windows. This does not qualify gameplay-state transitions, desktop
background policy or complete R4c/R10.

The November 18 [managed time correction](../../research/2026-11-18-engine-architecture-review-reconciliation.md#managed-time-correction-unit)
repairs external-review F1 at `f51618a` and F8 at `50f451c`. App reports missing resources at its
actual post-schedule consumption points through the existing first-fault/Stop contract; resources
removed after their final use do not trigger a blanket audit. Its frame status now publishes the
settled simulation-pause fact before Extract, and Transform consumes that fact independently of
requested control settings. Exact Step followed immediately by elapsed Resume presents the
current pose and preserves the remainder. Six resource-loss cases and the `2.5` versus `3.0`
presentation difference are reproduced before repair. Final App/Transform/public-time 128 and
camera/captured-packet 7 pass, along with original-owner correctness Clippy, root library and
format checks. This is Windows CPU/runtime evidence; no new native timing or complete R10/R12
claim follows. Other findings from the two reviews retain their recorded open/paused scopes.

The November 13 [Headless exact-entry successor](../../plans/2026-11-03-joint-motion-execution-plan.md#headless-exact-run-entry-maintenance-november-13)
separates once-per-run product preparation from individual fixed transactions under the same
ADR0039/0084 Runtime owner. Entry always drives one paused maintenance frame, including an
initially paused Runtime, and can restore simulation readiness there. Entry Exit stops before
any fixed transaction through the original result/retirement path. Normal frame-mode driving
retains its original policy. The final affected cohort passes 71/71, comprising 50 Host tests
and all 21 selected public startup tests;
the independent processed Host cohort passes 3/3, including exact original Scene/source/asset
custody and shutdown observations after removing input files. Both retain Windows CPU scope;
the linked plan distinguishes the RED evidence, independent review and remaining scope. This
adds no scheduler, public phase interface, gameplay-state machine or native qualification.

The November 13 [retained Headless prerequisite](../../plans/2026-11-13-rust-state-reconstruction-plan.md#current-evidence-and-limits)
adds a public `HeadlessSession` over the original private product Host. Source and processed
products retain their original content, Runtime generation, command queue and physical retirement
between caller drives. Paused capture requires a completed full frame; each exact tick is followed
by paused TaskUpdate/Last maintenance. Ordinary control/command refusal preserves the incumbent,
post-admission fault retains its output, and AppExit prevents further execution and input.
The affected Host/public boot cohort passes 111/111 and an independently locked renamed consumer
passes 12/12 on Windows CPU, including failed-candidate isolation, execution fault and
pending/completed-error close.
The linked plan records initial review defects and fixes, exact verification revisions and limits.
This prerequisite does not complete R12 or the foundation rebaseline.

The same day's [game reconstruction successor](../../plans/2026-11-13-rust-state-reconstruction-plan.md#game-reconstruction-and-actual-rust-builds)
adds bounded game-owned semantic state, original admitted source-content stamps, fresh-runtime
reconstruction through the original gameplay/Scene owners and a finite game-owned process consumer.
The affected Engine content/Scene cohort passes 60/60, the ordinary sample passes 65/65, and an
explicitly selected actual-build test passes 1/1 after real movement-body and private-state-structure
edits. It compares exact restored state and subsequent
movement/pickups/markers/travel, retains the original executable/process across failures and
rejects incompatible or stale operations before candidate acceptance. The linked plan records
review corrections, actual run IDs and Windows source/headless limits. Supervised desktop adoption,
unsupported native services, general replay and measured latency/memory remain open; Editor UI
and physical mixed-DPI work stay paused. Complete R12 and the foundation rebaseline remain active.

The [shared native-process prerequisite](../../plans/2026-11-13-rust-state-reconstruction-plan.md#shared-native-process-custody-successor)
moves the Editor's existing platform/output/log/wake implementation into concrete
`project_tooling::process` execution. Supervised Play retains generation/READY/lifecycle ownership;
the game retains its state format and finite protocol. Both now use the original contained native
process and physical cleanup owner. Windows qualification passes 31 root process cases, 3
independently locked public-consumer cases, 77 original Editor supervised cases and the real A/B/C
game rebuild/reconstruction trajectory. The linked record separates helper counts, exact runs,
review corrections and platform scope. This replaces direct-child supervision in the game Host;
it does not extend desktop v1's post-start protocol, adopt a Runtime candidate, resume paused UI,
qualify Linux execution or complete R12.

The [game-owned development Host successor](../../plans/2026-11-13-rust-state-reconstruction-plan.md#development-host-qualification)
implements source/headless candidate preparation, complete canonical probes, fresh-adopter state
restoration and a nominal-ticket route exchange. Final adoption revalidates source and both full
states; predecessor cleanup remains independent after commit, with the new active game usable
while another rebuild refuses. Final ordinary sample qualification passes 78/78; actual Rust body
and private-structure adoption and the original A/B/C regression each pass 1/1. The ordinary JSON
CLI passes three actual process trajectories, including malformed selection with a retained
candidate and EOF cleanup, with exact native handles observed exited and original clean game/
containment/readers/output reports. Independent reviews close the per-child cleanup budget,
capture-stage, reaped-PID and unknown-field corrections; actual CLI execution also corrects a
Windows logical/native inventory-path mismatch. The linked plan records runs, final checks and
Windows Rust 1.95 scope. Desktop adoption, unsupported services, hard compiler interruption,
performance budgets, Linux execution and the complete R12/foundation outcome remain open.

The November 14 [desktop polling prerequisite](../../plans/2026-11-13-rust-state-reconstruction-plan.md#desktop-polling-and-control-qualification)
distinguishes exact fixed ticks from full App frames and keeps one original desktop aggregate
between caller polls. Managed controls, typed gameplay submission, healthy paused read-only
capture, hidden startup/reveal and physical Close share the existing Runtime, native pump and
presentation owners. Core qualification passes 168/168, all-feature Winit CPU 62/62, selected
original native Winit 5/5 and root desktop/public controls 23/23. An ordinary desktop game child
also passes 1/1 through actual gameplay, managed controls, original native retirement and
independent GPU replay pixels; the final default source/headless sample regression passes 78/78.
The linked record separates finite visual evidence, canonical capture and independent review
corrections.
Complete desktop canonical reconstruction, held-input transfer, rebuilt-game adoption, foreign
execution and measured budgets remain open. Source study also identifies the required time
continuation and native pause-sampling correction; no phase is silently cleared. Editor UI and
physical mixed-DPI work stay paused.

The [scheduling continuation successor](../../plans/2026-11-13-rust-state-reconstruction-plan.md#scheduling-continuation-qualification)
adds portable actual time policy and pending duration through the original managed time owner,
preserving local history and deriving interpolation. Headless/desktop delegates require original
paused maintenance after application. Native `drive_elapsed` accounts for past paused waits in
RealTime without virtual catch-up; explicit supplied-delta behavior remains. Game resume and
continuity protocol version two require the time value. App passes 178/178, ordinary game 97/97,
all-feature Winit CPU 67/67 and selected native Winit 5/5; an actual desktop game/GPU replay passes
1/1 through the new public time operation. Independent review corrects authority callbacks that
report a fault while returning success, with before/after application regressions. The linked record
retains actual rebuild evidence and platform limits. Complete desktop reconstruction, input/service eligibility,
adoption, foreign execution, costs and full R12 remain open.

The November 14 [keyboard continuation successor](../../plans/2026-11-13-rust-state-reconstruction-plan.md#keyboard-continuation-and-native-activation-qualification)
restores finite declared Button holds through the original Input owner with fresh local members,
silent held projections and separate admission/terminal provenance. Exact steps reuse the original
Input-to-command preparation; a late-input mapping regression passes after staging shares the
resolver's frame guard. Native isolation, real focus and Windows physical A/D/W/S reconciliation
remain separate from logical restoration. Seven selected native trajectories and the actual
desktop game/GPU consumer pass 7/7 and 1/1 respectively. Independent review also closes original
endpoint focus restoration and ordinary Platform-release reactivation deadlocks. The linked
record retains final CPU/public checks and the narrower physical/GPU evidence. Complete desktop
canonical capture, service eligibility, rebuilt-game adoption and full R12 remain open; UI and
physical mixed-DPI remain paused.

The follow-up [desktop capture boundary](../../plans/2026-11-13-rust-state-reconstruction-plan.md#desktop-service-inventory-and-placement)
now routes the same game-owned canonical capture through a healthy paused `DesktopRun`. The
explicit service inventory admits the known fresh-Runtime window/render/WGPU/image/sprite/input
services and names the qualification-only sealed-frame observer separately; unknown custom
services remain refused. The actual desktop/GPU consumer captures the full state and compares the
visual trajectory's represented semantic fields. The capture retains the authoritative configuration
fingerprint; the visual oracle does not independently check it or complete inactive-level state.
Final run `20c77f33-efb2-499b-ba9e-1ebb4d2a815a` passes 1/1 after
the first RED run correctly exposed the unclassified evidence observer. This advances desktop
canonical capture only; restored desktop candidate presentation, route adoption and predecessor
retirement remain open.

The following [desktop reconstruction/production-adoption unit](../../plans/2026-11-13-rust-state-reconstruction-plan.md#desktop-reconstruction-and-production-adoption-qualification)
at `4080306`/`65c0f75` connects actual declared Input membership, complete canonical state, fresh desktop Scene/Runtime
reconstruction and original sealed/presented state in the game-owned `SourceDesktop` development
Host. Probe disposal, a fresh adopter, final source/dual-state/presentation checks and one route
exchange preserve the original owners. Native activation and predecessor retirement have separate
postcommit outcomes; `Closing` retains the original child for explicit retry. Complete affected
Input/Gameplay/Winit CPU 346/346, root exact-input 6/6 and reusable native 7/7 pass. Sequential
desktop reconstruction with independent GPU replay and actual Rust body/private-structure
desktop adoption each pass 1/1. Ordinary game/CLI CPU 101/101, desktop-feature CPU 74/74, original
headless actual-build/adoption 2/2 and actual JSON CLI 4/4 pass. The actual ordinary-recipe control closes the previous visual
oracle's configuration coverage gap; the linked record keeps hardware/backend, receipt/replay,
source refusal, lifecycle and cost limits explicit. The game protocol is version three; supervised
Editor Play remains separate. Continuous Editor Play, unsupported service/state migration,
cross-platform execution, numeric budgets and complete R12 remain open. UI and mixed-DPI remain
paused.

The following [continuous desktop development successor](../../plans/2026-11-13-rust-state-reconstruction-plan.md#continuous-development-qualification)
at `a1490b9`/`ec57133` connects that concrete game Host to original continuous native execution. Read-only execution
status distinguishes managed Runtime state from game phase, complete capture and presentation.
Original Pause/Resume tickets and explicit input activation retain their existing owners;
paused-only request refusals preserve the Running incumbent. The actual A160/B80/private-structure
C native trajectory passes 1/1, including matched input, stale candidate refusal after resumed
play, exact restored presentation, Pending-predecessor continuous gameplay and Running Close.
The ordinary JSON CLI's actual original/adopted execution and cleanup also pass. Protocol four
does not extend supervised Editor Play. Final ordinary/desktop CPU 103/103 and 76/76, original
actual headless builds 2/2 and original paused desktop adoption 1/1 pass. The linked qualification keeps CPU/compatibility,
hardware/backend and cost scopes explicit. UI, full R12 and broader measured product completion
remain open.

The November 14 [development/runtime cost attribution](../../benchmarks/2026-11-14-development-runtime-costs.md)
at `f1c2256`/`f79c7da` adds opt-in fixed Host phase aggregates without a new lifecycle or tracing
owner. All 18 complete native no-edit/body/private-structure rebuild workflows pass; retained
correct-running medians are 14.7706/23.7901/25.4273 seconds. Six matched ordinary/witness runs
retain 120 exact frames each with original receipt, telemetry and physical cleanup. Quantized
whole-process CPU ranges overlap and do not establish a witness regression. Final ordinary/
desktop CPU 107/107 and 80/80, original headless actual builds/adoption 2/2 and continuous native
adoption 1/1 pass. The linked report retains raw data/digests, inclusive-phase endpoints,
independent review, actual platform scope and remaining numeric/Editor limits. No performance
change or full R12 completion is selected from these observations.

The subsequent [paused-control attribution and correction](../../benchmarks/2026-11-14-development-runtime-costs.md#paused-control-turnaround-successor)
at `d11a8c0`/`ebbc1e6` preserves that complete endpoint while avoiding an extra native poll when
the original healthy complete paused frame already permits capture. Matched control observations
retain identical state/status and fresh original presentation, with capture/presentation medians
135.344/461.789 ms before and 60.965/268.541 ms after. All eighteen full rebuild attempts and
both original continuous/paused native adoption trajectories pass; final all-feature/default
ordinary tests pass 113/113 and 110/110. Six original Cargo checkpoint intervals retain source,
tool, lease and artifact verification. Independent review, correctness Clippy and formatting pass.
The linked record preserves correlated-sample, shared-source, compiler/cache/load and native
scope limits; neither full-workflow tails nor a shipping CPU budget is established. Full R12 and
Editor integration remain open.

The subsequent [declaration-only finite build qualification](../../benchmarks/2026-11-14-development-runtime-costs.md#declaration-only-cargo-preparation-successor)
at `215ef54` separates target declarations from full resolved metadata using the same pure
bounded selectors and artifact decoder. The finite adapter uses `--no-deps`, while full graph
facts/Editor remain strict. Optional invocation metadata features distinguish unobserved from
resolved-empty sets. All eighteen complete game workflows pass; metadata/recheck medians fall
to 0.15–0.17 seconds from 1.3–1.5 seconds. Compiler variation prevents a general full-workflow
speedup claim. Root facts/cleanup 44/44, independent API 3/3, actual native/foreign builds 2/2,
Editor build facts 11/11 and affected UI CPU 124/124 pass. The inherited restored-input adapter
omission is corrected at `eec5f00`. Correctness Clippy, formatting and bounded independent
reviews pass; source/tool/lease revalidation, failure custody and physical cleanup retain their
original owners. The linked raw data keeps actual source/tool/platform and cost limits; full
R12 and paused Editor/UI work remain open.

The [original child startup qualification](../../benchmarks/2026-11-14-development-runtime-costs.md#original-child-startup-attribution)
at `ed4cd86` preserves the original process/namespace/executable/native/READY owners and adds
four opt-in elapsed phases. Twelve actual independent startup processes and one excluded
post-spawn failure pass. Headless/desktop startup medians are 145.984/1595.177 ms; most desktop
time lies within initialized READY wait, not the separately measured native creation. Original
continuous/paused adoption 2/2, desktop CPU 83/83 and default-disabled ordinary tests 110/110
pass. Correctness Clippy, formatting, latest consumer compilation and independent reviews pass;
the report keeps the test-only P3 correction, original raw-source digest and unexecuted injected
panic scope explicit. No startup optimization, CPU/cold-start/tail budget or full R12 completion
is selected; original presentation and physical retirement retain separate qualification.

The [original DesktopRun startup observations](../../benchmarks/2026-11-14-development-runtime-costs.md#original-desktop-startup-operations)
at `11c1a67` add five default-disabled public spans to the same action and original presentation
receipt.
Unfinished native-poll accumulation stays private; the public Copy/Eq/Debug snapshot contains
completed values only. Six release/DX12 comparisons preserve exact frames, canonical witnesses,
neutral state, telemetry sealing and physical-loop recovery. Later microsecond first polls defer
substantial initialization: accumulated native calls through first presentation take 622–1043 ms.
These elapsed values overlap other observations and are not GPU time or cold-start/tail budgets.
Ordinary game/child options remain disabled; no protocol, Runtime, native or render owner changes.
Root/public controls 28/28, original continuous desktop adoption 1/1, minimal feature checking,
correctness Clippy, formatting and document/data checks pass; source review closes all P1/P2.
Further owning-operation attribution and full R12 remain open.

The [first-presentation investigation](../../benchmarks/2026-11-14-development-runtime-costs.md#first-presentation-and-original-gpu-calls)
at `42b4eef`/`44725e0` retains the actual first transaction separately from running warmup/samples.
Six temporary source-local probe runs locate Adapter request as the largest measured call at 524–740 ms;
the exact probe/consumer delta and all raw observations are retained after probes are removed.
Stock pipelines were already keyed by actual use. Two bounded corrections defer material layout
until a real extension and use one Surface capabilities snapshot, preserving original format/
mode/color policy, caches and device/target ownership. Actual ordinary/custom allocation failure,
retry, texture/parameter pixels, mixed reuse, clear and device replacement pass in the affected
24/24 renderer cohort. Final ordinary six-run and complete continuous/paused desktop adoption
2/2, minimal feature checking, correctness Clippy, formatting and document/data checks pass;
independent review finds no P1/P2. No dominant-latency improvement, global cache or complete
R12 is claimed.

The November 16 [explicit paused native cadence](../../benchmarks/2026-11-14-development-runtime-costs.md#explicit-paused-native-cadence)
at `ef67473`/`8b8954c` adds original-session Poll/Wait changes independently of Runtime pause.
Pending demand, presentation maintenance, exact stepping and long-wait elapsed-time settlement
retain their owners. The concrete source-desktop child selects Wait only for its known paused inventory,
explicitly drives asynchronous Scene reconstruction, and resumes ordinary Poll gameplay.
Six paired Windows/RTX4090/DX12 runs reduce aggregate idle App frames and actual presentations
from sixty to zero with unchanged complete canonical bytes and zero simulation/logical ticks.
Owner CPU 87/87, sample desktop/cost CPU 149/149, actual native window/maintenance 2/2 and
complete real desktop body/structure rebuild/adoption/gameplay 2/2 pass. The continuous child
reports Vulkan independently of the DX12 cost consumer. Reviews close the reconstruction
demand omission and outdated structural-edit fixture; the report preserves the failed attempt.
ADRs0039/0076 and the current guides own the explicit policy and workload limits. Continued
observer polling and quantized overlapping CPU values preclude a stable CPU or power claim;
general task wakes, foreign native execution and full R12 remain open.

The [observer-deadline and submitted-work successor](../../benchmarks/2026-11-14-development-runtime-costs.md#observer-deadlines-and-submitted-work-maintenance)
at `47490b1`/`bf92633` removes implicit observer polling while retaining pending original GPU-prefix
maintenance independently of observer/telemetry configuration. Callback-induced final-owner releases receive
another original cleanup pass before settled submission work is reported. Five finite consumers
declare their absolute deadlines; original verification, seal/veto and native retirement remain.
Root CPU 27/27, affected GPU-owner CPU 45/45, actual DX12 17/17 and software DX12 4/4 pass.
Six complete native v4 runs reduce periodic-Wait callbacks/native polls from 1474/737 to 12/6
with unchanged canonical state, zero paused App/presentation/simulation work, explicit controls
and full cleanup; 100 ms requests arrive at 100.0873–100.6636 ms with 1 s caller wait caps.
Independent consumers compile and bounded reviews report no remaining P1/P2. This is original
submitted-work/deadline evidence, not arbitrary cache/task wakes, a stable CPU/power budget,
foreign execution or completion of full Foundation/R12. ADR0078 and the guides own its scope.

The [private request-notification successor](../../plans/2026-11-13-rust-state-reconstruction-plan.md#native-request-hints-for-the-development-child)
retains original file-packet/native process custody and introduces only bounded request hints,
joinable game-private watching and original maintenance-only session wakes. Protocol-v5
observations commit after envelope/reply/operation validation and remain outside canonical
state and cleanup authority. Winit CPU 95/95, actual native wake/cadence 4/4, original handoff/
maintenance 8/8 and sample ordinary 170/170 pass. Twelve fresh original children pass matched
500 ms comparisons: headless inbox checks 103-to-nine (91.2621%), desktop 594-to-twelve
(97.9798%); all 150 selected requests stay within 100 ms and original canonical state,
App-frame progression, exact Step, presentation and physical cleanup remain valid. The first
headless gate failure is retained in the [cost report](../../benchmarks/2026-11-14-development-runtime-costs.md#native-request-notifications-and-reconciliation).
ADRs0039/0076 and current guides own the wake/observation contract. Original 250 ms
reconciliation, vendor internal wakes, 10 ms parent response polling, foreign execution and
full R12 remain separate limits; this is not an engine-wide message bus or CPU/power guarantee.
Actual continuous-desktop, paused-desktop and headless production Host regressions pass 3/3,
including real body/private-structure builds, fresh-adopter semantics, separate predecessor
retirement and continued gameplay through original native owners.
Implementation `686a5d9`/`021814c` also passes independent headless 166/166, final affected
child 16/16, locked all-feature checking, correctness Clippy and formatting. The linked report
and metadata retain exact source scope/digests; ten scoped threads close without reported P1/P2.

The November 16 [physical Task-return successor](../../plans/2026-11-13-rust-state-reconstruction-plan.md#physical-task-return-and-game-capture)
adds the original non-driving `TaskPools::return_state()` snapshot and replaces the reference
game's diagnostic-count capture gate. Root observation holds its three original queue guards
through one cut; activities reuse original pending-return accounting and preserve sibling
isolation. Cancelled queued capture disposal and sticky cleanup failure remain ineligible even
after terminal/handle loss and zero diagnostics. Two actual RED regressions precede the owning
Task 109/109 and desktop/cost game 172/172 GREEN results, including exact release/reconstruction,
continued play and failed-but-physically-complete Close. Child settling uses this fact within
existing deadlines without extra App frames solely for disposal. A rejected notification-count
regression and the corrected wait-before-probe comparison remain in the [existing report](../../benchmarks/2026-11-14-development-runtime-costs.md#physical-task-return-and-request-check-ordering):
105-to-nine headless checks and 595-to-ten desktop checks, with all 150 selected requests within
100 ms and original canonical/control/cleanup facts intact. This refines ADRs0008/0052 and the
concrete ADR0076 game workflow; it grants no admission freeze, worker-join proof, arbitrary game
checkpoint or native rollback. Full Foundation/R12 and foreign execution remain open.

The November 16 [active gameplay baseline](../../benchmarks/2026-11-14-development-runtime-costs.md#active-gameplay-workload-baseline)
uses the existing Travel script and original native Runtime/presentation/retirement owners.
Its measuring consumer and controls are committed at `6c9471a`.
Three complete trajectories retain 447 exact initialized presentations and completion matches,
real movement/pickup/marker/scene changes and final game facts. Actual native acceptance plus
registered-observer panic cleanup passes 2/2; final game library/metric controls pass 102/102.
Correctness Clippy, formatting and evidence checks pass. The linked report keeps the first
initialized frame's broad approximately 0.6-second span separate from later frame classes,
with descriptive quantiles and whole-process points. This adds a measuring consumer rather than
a production owner or engine optimization. Larger workloads, first-frame attribution, reliable
tail/resource budgets, foreign execution and full R12 remain open.

The [first-frame attribution successor](../../plans/2026-11-13-rust-state-reconstruction-plan.md#first-initialized-frame-cost-attribution)
extends that same consumer with fixed original WGPU Runtime operation wall spans, Desktop startup
snapshots and complete entry-to-first-presentation observations. Collection is explicit/default-off,
per-transaction and independent of submission/presentation authority. CPU controls and independent
game library/metrics pass 19/19 and 130/130; actual three-run Travel/panic cleanup and affected
native Host controls pass 2/2 and 8/8. The exact 483-frame
[report](../../benchmarks/2026-11-14-development-runtime-costs.md#first-initialized-frame-attribution)
places 94.91–95.89% of the first initialized frame's wall span in original namespace preparation,
without asserting a single-call driver attribution or a performance improvement. Minimal native
compilation, owning/consumer correctness Clippy and formatting pass. Original lifetimes and all
gameplay/cleanup checks remain; further device-operation attribution, larger workloads, budgets,
foreign execution and full Foundation/R12 remain open.

The owner implementation and full consumer above are committed at `46f798c` and `e88ec46`.

The [original namespace acquisition successor](../../plans/2026-11-13-rust-state-reconstruction-plan.md#original-namespace-acquisition-costs)
borrows fixed nested observations through the original Host/device operations. The actual complete
479-frame native report attributes 450.75–502.32 ms to Adapter requests and 52.54–62.00 ms to
Device requests. First-namespace request ownership and all subsequent Ready-frame absences are
checked separately from aggregate counts. CPU/consumer controls pass 27/27 and 132/132, full
Travel/panic cleanup 2/2 and original native Device/Host controls 11/11. Owning compilation,
correctness Clippy, formatting and source/report/executable provenance pass. The
[report](../../benchmarks/2026-11-14-development-runtime-costs.md#original-namespace-acquisition-results)
separates official wgpu candidate probing from Nara policy; it selects no new filtering, cache or
GPU owner and claims no speedup. Further native sampling, larger workloads, reliable budgets,
foreign execution and full Foundation/R12 remain open.

The namespace owner implementation is `4a2584d`; its complete consumer and evidence are `e29e51c`.

The [authored Tile workload successor](../../plans/2026-11-13-rust-state-reconstruction-plan.md#authored-tile-workloads-through-native-travel)
at `2b98a0c`/`9f0e5dc` qualifies copied 64/4K/16K source terrain through nine complete native Travel
lifetimes, preserving original members, input/game facts and five Scene-instance replacements.
It closes two admission mismatches through existing owners: complete-value/depth reference
over-reservation and generic reference limits narrower than the receiving compiled-value policy.
Reflect supplies the shared conversion and plan-visit admission, with default file/value/Host-memory
policies unchanged. The 16K representation uses root-bound Prefab halves under original wire limits.
Shared owner/content controls pass 1,049/1,049, sample controls 138/138 and hardware DX12 workflow/
panic cleanup 4/4. Twenty-seven original-input GPU replays retain/reread earlier images after later
publication and producer retirement, and complete original prefix/Host cleanup. Source copies are
removed; original source Weak/budget lifetimes have separate focused coverage. The
[report](../../benchmarks/2026-11-14-development-runtime-costs.md#authored-tile-workloads-through-native-travel)
keeps native waits, resource points, source-layout confounds and process-memory limits explicit.
Correctness Clippy, formatting, provenance and independent review pass. Further attribution,
numeric runtime budgets, foreign execution and full Foundation/R9/R12 remain open; UI stays paused.

The same Tile consumer initially added opt-in span attribution at `a58c630`. The thread-local
subscriber omitted IO workers, so the historical ranges below are partial and are rejected as
complete preparation attribution. The binary-local correction below requires actual worker
entries, six original preparations/publications and no outstanding spans after physical retirement.
The initial implementation records existing `nara::scene::work` boundaries without changing
default runtime behavior: 138/138 feature-qualified metric controls and 4/4 native workflows pass,
but the 109-entry reports do not qualify worker coverage. `scene_decode` is compiled-component
preflight/native codec validation, not source file reading or JSON/RON parsing.

The [corrected worker observation and per-map extraction](../../benchmarks/2026-11-14-development-runtime-costs.md#corrected-original-worker-coverage-and-per-map-extraction)
at `a91e68c`/`f25312c` qualifies 139/139 game/metric/thread controls, 56/56 complete Sprite controls
and 4/4 hardware DX12 workflows. Every complete run retains 149 original span entries, including eight worker
expansions/flat admissions and six owner preparations/publications, with no unfinished or retained
spans at physical retirement. Old-span exits cannot charge a later run; failed reports retain
original observation and cleanup facts. Nine Tile lifetimes retain 1,655 initialized presentations
and 27 original-input GPU replays. A private context reuses only current per-map material/image,
tile axes and the last index/UV result, preserving complete extraction, per-cell diagnostics,
authorization, cleared-dirty refresh and retained inputs. Nested ranges are not CPU or a controlled
speedup; source I/O/parsing, extraction timing, reliable runtime budgets, foreign execution and
full Foundation/R9/R12 remain open. UI/Dialogue work stays paused.

The [source-work split and validated value-cost reuse](../../plans/2026-11-13-rust-state-reconstruction-plan.md#validated-value-cost-reuse)
at `a7c7a27`/`4aaa3d8` separates actual content-read and Scene/Prefab candidate-decode envelopes
from compiled preflight and compiler subphases. Nine expansions retain the game's checkpoint
validation preparations; six publications include resident-source Retry. Private costs are
refreshed after every semantic value mutation and discarded with recomposed candidates, while
complete reference scans/visits and explicit metadata accounting remain. Root/public content
controls pass 81/81, full Scene/Reflect libraries 981/981, public value-bearing conversion with
reference-triggered recomposition 5/5 and game/metric/span/native controls 143/143. Nine Tile
lifetimes retain 27 GPU replays, and the balanced old/new comparison adds twelve 16K lifetimes
and 36 replays. Exact source/executable/report hashes and finite timing limits are in the
[report](../../benchmarks/2026-11-14-development-runtime-costs.md#validated-value-cost-qualification).
The local output envelope drops from a 38.455 ms wall median to 0.145 ms, but whole expansion/
process do not show consistent improvement. Source/candidate decoding, numeric budgets, foreign
execution and full Foundation/R9/R12 remain open; paused UI/Dialogue work stays excluded.

The [source payload successor](../../plans/2026-11-13-rust-state-reconstruction-plan.md#source-payload-decode-successor)
at `e40f2a5`/`920cd9a` separates wire/header/payload/finish observations and removes repeated
untagged buffering for known component List/Map payloads. Scalars, value-first, serialized form,
JSON/RON and parent-buffered semantics retain their original paths. Direct nested RON Map/Seq
representations are corrected after independent review. Full Reflect/Scene controls pass
1,010/1,010, strengthened wire 29/29, Scene format/conversion 36/36, public Reflect formats and
references 20/20, root/public limits 81/81 and complete game/metric/span/native controls 143/143.
Original byte/shape/header-contract/payload/admission order, source freshness and physical cleanup
remain. Nine Tile lifetimes retain 27 GPU replays; twelve paired 16K lifetimes add 36. The
[exact measured report](../../benchmarks/2026-11-14-development-runtime-costs.md#streamed-container-qualification-and-comparison)
records payload wall medians of 3,463.926/1,197.454 ms and whole-process walls of
43.919/34.897/34.488/42.650 seconds in before/after/after/before order. This is finite Windows/
hardware DX12 workload evidence, not CPU/GPU duration, production tail or physical-memory bounds.
Feature checks, correctness Clippy, formatting and source/executable/report integrity pass;
full Foundation/R9/R12 and foreign execution remain open, and paused UI/Dialogue stays excluded.

The [bounded materialization-cost successor](../../plans/2026-11-13-rust-state-reconstruction-plan.md#component-materialization-and-bounded-cost-return)
at `e9c6f5d` reuses complete cost returned by the original Reflect raw walker and default
preflight within the original Scene candidate. No-insertion preserves its no-scan move, Native/
admitted wrappers retain unit success and exact errors, and source/depth/semantic/reference/work
admission remains. Full Reflect/Scene/Material libraries pass 1,094/1,094, public Reflect/Scene
26/26 and 36/36, root receiving limits 81/81 and game/metric/span/native controls 143/143.
Nine Tile lifetimes retain 27 GPU replays; twelve paired 16K lifetimes add 36. The
[report](../../benchmarks/2026-11-14-development-runtime-costs.md#bounded-cost-return-qualification-and-comparison)
records lower local materialization wall medians but both changed process groups are slower;
there is no demonstrated end-to-end speedup or production/CPU/scanout/memory budget. Feature-free
tests/checks, scoped correctness Clippy, formatting and source/executable/report integrity pass.
No new cache, witness or budget owner is introduced. Full Foundation/R9/R12 and foreign
execution remain open; paused UI/Dialogue work stays excluded.

The [reference-scan successor](../../plans/2026-11-13-rust-state-reconstruction-plan.md#reference-planning-without-duplicate-payload-scans)
at `d5acd0d` reuses complete counts from the original first raw walk within each collector call.
Full Schema interpretation remains; only reference-free payloads whose checked payload and result
counts fit the original limits omit the final repeated scan. Non-null AssetRef results are included,
while EntityRef-bearing, overflowing and over-limit cases preserve the original walker and errors.
All-feature owner libraries pass 1,102/1,102, public Reflect/Scene controls 26/26 and 36/36,
root receiving controls 81/81 and game/metric/span/native controls 143/143. The
[report](../../benchmarks/2026-11-14-development-runtime-costs.md#reference-planning-without-duplicate-payload-scans)
retains complete native/pixel/failure evidence and a balanced frozen-executable comparison:
reference planning and materialization wall medians decrease, and both changed process groups
are faster in this finite observation. It does not establish general performance, CPU/scanout,
memory or production-tail budgets. No cache, public witness, larger limit or new owner is added;
full Foundation/R9/R12 and foreign execution remain open, with UI/Dialogue paused.

The [current-version migration successor](../../plans/2026-11-13-rust-state-reconstruction-plan.md#current-version-migration-cost-reuse)
at `81a9d59` preserves bounded raw cost only when the concrete authority moves the same value
without advancing its version. Actual migration output still receives complete admission before
defaults; final Schema/reference/cost and original work/retained limits remain. Complete Scene
and owned-storage controls pass 783/783, public consumers 45/45 and game/metric/span/native controls
143/143. The [report](../../benchmarks/2026-11-14-development-runtime-costs.md#current-version-migration-cost-reuse)
retains exact native/pixel/failure evidence and a balanced frozen-program comparison: the migration
wall envelope is almost eliminated, but no meaningful end-to-end speedup is established. The
[shared-payload successor](../../plans/2026-11-13-rust-state-reconstruction-plan.md#component-payload-sharing-through-existing-retained-owners)
at `bcc481f` implements component-local immutable storage with deep public source Clone, private compiler
sharing, isolated actual mutations and full independent payload/backing charges. Reflect/Scene
libraries pass 1,037/1,037, selected public Scene consumers 104/104, Tooling/UI 304/304 and root
content/custody/receiving controls 91/91. Complete independent consumers pass 262/262, with the
original timing case skipped; final game/metric/span/native acceptance passes 143/143 without skips.
The balanced comparison retains complete native/pixel/cleanup evidence but moves most copying
into required defaults and establishes no stable end-to-end speedup. No cache, budget owner or
larger capacity is introduced.

Independent acceptance also exposes the separate missing custom Gameplay producer path during
exact Step. Its [App-owned intent correction](../../plans/2026-11-13-rust-state-reconstruction-plan.md#custom-gameplay-producers-during-exact-step)
at `c8f7c7d` registers each producer once in the original App graph, preserving final Finish admission, selected
automatic execution and managed faults. Owner/public scheduling controls pass 250/250, complete
independent replay passes and the native cohort above remains green. The guide documents the new
helper ordering boundary. The superseded module-local schedule is removed before commit; final
review closes its P1 without remaining P1/P2. Full Foundation/R9/R10/R12 and foreign execution stay
open, with UI/Dialogue paused.

The [large Tilemap payload study](../../research/2026-11-17-large-tilemap-payload-ownership.md)
compares pinned official packed inline/chunk data with Nara's generic effective-value ownership.
A frozen borrowed-view or typed payload remains a research candidate, requiring complete codec,
receiving/reference/cook consumption rather than hidden flattening. The separate
[isolated migration/default correction](../../plans/2026-11-13-rust-state-reconstruction-plan.md#preserve-isolated-migration-storage-during-defaults)
at `e2c2a38` preserves unique migration storage through the existing default interpreter. Reflect/Scene release
libraries pass 1,038/1,038, selected public Scene consumers 62/62 and public effective-value/error
controls 18/18 without skips; owning correctness Clippy, feature-free production and formatting/
document checks pass. The first copy of shared current-version values remains. No new native
measurement, value representation, budget or asset-lifecycle owner is selected by that bounded
correction. The representation decision now compares actual persistent configuration/cells
granularity before imposing a common borrowed read interface.
Its [test-only persistent-record probe](../../plans/2026-11-13-rust-state-reconstruction-plan.md#bounded-persistent-tilemap-record-probe)
at `7088b9e` passes 5/5 release cases without skips, retaining all 16K cells across direct and root-bound
sources, shared defaults, actual edit isolation, receiving limits and independent cooked values.
Both stock and split retain the original default-wire refusal and use the same explicit finite
wire allowance for complete positive cook/readmission. No production limits or representation
change. Native pairing, TileSet membership, source upgrades and the complete game/GPU/cost
comparison remain before selection; this does not close full Foundation/R9/R10/R12.

The subsequent [frozen effective-read implementation](../../plans/2026-11-13-rust-state-reconstruction-plan.md#frozen-effective-reads-through-the-existing-component)
at `6ceb534` retains that single source/native Tilemap after comparing pairing, migration and packed-value
costs. Reflect freezes selected defaults over private admitted backing; actual native/derived
codecs, complete validation, Scene preparation and cook read `ComponentValueRef` directly.
ADRs0006/0045/0117 and the component guide record the changed read/callback contract. Initial
Reflect owner/public qualification passes 265/265. Complete affected owner/domain libraries
pass within the recorded 1,608-case CPU scope after fixture corrections; root custody/content
controls pass 139/139. Actual game/metric/span/native controls pass 143/143 and the corrected
16K probe 5/5, including 27 original-input GPU replays and both panic cleanup controls. The
[balanced comparison](../../benchmarks/2026-11-14-development-runtime-costs.md#frozen-effective-reads)
adds twelve 16K lifetimes and 36 replays but establishes no stable end-to-end speedup. Complete
independent consumers pass 262/262 with one manual timing case skipped; selected public
Reflect/Scene integrations pass 134/134 and 179/179, ordinary reference-game controls 44/44 and
the final reference-copy owner cohort 285/285. Language-consumer compilation, owning/root/sample
correctness Clippy, feature-free libraries and formatting pass. Source wire, domain schemas, full
per-owner accounting and original publication/retirement authorities remain. No capacity,
physical-memory, numeric-budget, complete R12 or Foundation qualification follows.

The subsequent [compiler validation-locality unit](../../plans/2026-11-13-rust-state-reconstruction-plan.md#validation-locality-after-the-frozen-read-comparison)
at `9fc9322` separates field semantics from full payload-cost admission after default materialization. Scene
reuses its unchanged effective value's original cost; standalone dynamic-value and receiving
validation keep their paths. ADRs0045/0049 distinguish reusable facts from new data and current
World/resource checks. The [source investigation](../../research/2026-11-17-large-tilemap-payload-ownership.md#5-validation-at-owning-boundaries)
traces actual pinned Godot/Bevy loading/construction rather than assuming every layer needs a
complete generic budget scan. Reflect controls pass 266/266, Scene/Tooling 983/983, existing selected
public Scene integrations 76/76 and new regression cases 4/4 after a population-fixture correction.
Release game/library/metric/span/probe/native qualification passes 148/148 with original GPU
replays and both panic cleanup controls. The independent Scene consumer passes 157/157, with one
manual timing case skipped. The [paired comparison](../../benchmarks/2026-11-14-development-runtime-costs.md#compiler-validation-locality)
retains twelve additional 16K lifetimes and 36 replays: semantic-validation wall median drops
83.977/0.815 ms, complete materialization 394.190/310.312 ms and expansion 1,056.126/940.849 ms.
This establishes reduced work in those finite cumulative wall envelopes, not stable process,
CPU/GPU, physical-memory or numerical production-tail performance. Source-compiled receiving
reuse, other traversal costs and full Foundation/R9/R10/R12 qualification remain open.

The [receiving-field locality successor](../../plans/2026-11-13-rust-state-reconstruction-plan.md#receiving-field-validation-locality)
at `f5c34ff` reuses the same explicit semantics with the original actual-work/scratch callback after complete
receiving checks. It removes default planning and frozen-value cost repetition within that
operation; standalone/default-selecting entry behavior and current receiving World/resource
checks remain. The owner run passes 1,239/1,241 with two obsolete internal-work expectations;
both corrected controls pass without weakening allocation/refusal or exact/one-short behavior.
Public Reflect controls pass 11/11, selected Scene workflows 74/74 and independently locked Scene
consumers 157/157 with one manual timing case skipped. Original release game/native qualification
passes 148/148. The [paired 16K comparison](../../benchmarks/2026-11-14-development-runtime-costs.md#receiving-field-validation-locality)
completes twelve further lifetimes/36 replays, observing cumulative flat-admission wall medians
490.235/383.589 ms, native decoding/preflight 932.016/776.535 ms and unbound preparation
358.099/299.969 ms before/after. Overlapping wall envelopes do not establish exclusive CPU,
GPU, physical-memory, frame-tail or statistical whole-process performance. Review finds no P1/P2;
broader exact-definition reuse and full Foundation/R9/R10/R12 remain open.

The [single Native-value admission unit](../../plans/2026-11-13-rust-state-reconstruction-plan.md#single-native-value-admission)
at `ab19dc9` returns the cost already computed by the Native depth/reference walker and consumes it under
Scene's remaining aggregate allowance. Accepted values avoid a separate cost scan. Failed Native
values preserve complete cost classification/charges; caller refusal returns immediately. Full
owner libraries pass 1,244/1,244, public Reflect/Scene controls 14/14 and 74/74, and original release
game/native qualification 148/148. Independent Scene consumers pass 157/157 with one manual timing
case skipped. Independent review finds no P1/P2. Its
[paired 16K comparison](../../benchmarks/2026-11-14-development-runtime-costs.md#single-native-value-admission)
completes twelve further lifetimes and 36 replays, recording cumulative flat-admission wall medians
384.536/334.978 ms, Native decoding/preflight 788.297/711.898 ms and unbound preparation
303.411/280.802 ms. These overlapping envelopes do not establish exclusive CPU/GPU, physical-memory,
frame-tail or general whole-process performance. Exact-definition/immutable fact reuse, source
decoding and full Foundation/R9/R10/R12 qualification remain open.

The [immutable compiled-value measurement unit](../../plans/2026-11-13-rust-state-reconstruction-plan.md#immutable-compiled-value-measurements)
at `9583ad7` retains cost and maximum depth from the existing Native walk with each original immutable
component. Receiving aggregate admission, stored-data size checks and root retained fees reuse
these facts; stricter/exhausted receiving policies preserve original diagnostics. Changed data
and independent cooked claims obtain fresh measurements, while schema/default/numeric,
undeclared-reference, resource, namespace and World/publication checks retain their own owners.
There is no new certificate/cache/lifecycle mechanism. Owner libraries pass 1,246/1,246, public
controls 87/87, Root content/retention/startup 162/162, original release game/native qualification
148/148 and independent Scene consumers 157/157 with one manual timing case skipped. Review
finds no P1/P2. The [paired 16K comparison](../../benchmarks/2026-11-14-development-runtime-costs.md#immutable-compiled-value-measurements)
adds twelve lifetimes and 36 replays, observing cumulative Native decoding/preflight wall medians
746.259/590.254 ms and unbound preparation 295.399/230.450 ms before/after. Overlapping envelopes,
before-process drift and correlated lifetimes do not establish exclusive CPU/GPU, physical-memory,
general process or frame-tail budgets. Broader definition-wide reuse and full Foundation/R9/R10/R12
remain open. The owning plan also requires comparison with corresponding Godot/Bevy source paths
before adding validation; historical counters alone do not justify scanning unchanged data again.

The [same-operation Native codec unit](../../plans/2026-11-13-rust-state-reconstruction-plan.md#same-operation-native-codec-admission)
at `6128594` dispatches original bound codecs after Scene's complete receiving check without repeating its
default/Native/field admission. Standalone public preflight stays complete. An actual change to
shared decode limits routes later values through ordinary preflight; domain/assets, native
binding, lifecycle, World and publication owners remain. No new witness/cache/skip mode is
introduced. Final owner/public controls pass 1,353/1,353 and original release game/native
qualification 148/148. Independent Scene consumers pass 157/157 with one original manual timing
case skipped; owning checks and scoped review corrections are complete. The
[balanced 16K comparison](../../benchmarks/2026-11-14-development-runtime-costs.md#same-operation-native-codec-admission)
adds twelve lifetimes and 36 replays, observing cumulative Native decoding/preflight wall medians
600.139/474.344 ms and unbound preparation 240.167/189.013 ms before/after. These overlapping
finite envelopes do not establish exclusive CPU/GPU, physical-memory, general process or
numerical frame-tail performance. Broader cross-preparation reuse, source/reference costs,
foreign execution and full Foundation/R9/R10/R12 remain open.

The [known-tag scalar wire unit](../../plans/2026-11-13-rust-state-reconstruction-plan.md#known-tag-scalar-payload-decoding)
at `3ecb373` uses scalar callbacks directly when the tag is already known, preserving final numeric/string
conversion and the original unknown/value-before-tag/Null/EntityRef fallback. Wire format,
file gates, limits and receiving/publication owners remain. Differential tests and review expose
and close accidental RON character-to-string acceptance from serde's default visitor forwarding.
Owner/public controls pass 1,195/1,195, including all 32 wire controls; original release game/native
qualification passes 148/148. Independent Scene consumers pass 157/157 with one original manual
timing case skipped. The
[paired 16K report](../../benchmarks/2026-11-14-development-runtime-costs.md#known-tag-scalar-payload-decoding)
adds twelve lifetimes and 36 replays, observing payload wall medians 713.247/297.126 ms. Lower
structure/expansion overlaps nested decoding, unchanged wire preflight increases and complete
process ordering is inconsistent. This establishes the selected payload result, not exclusive
CPU/GPU, physical-memory, general process or numerical frame-tail performance. Broader
preflight/header work, reference costs, foreign execution and full Foundation/R9/R10/R12 remain open.

The [borrowed preflight-key unit](../../plans/2026-11-13-rust-state-reconstruction-plan.md#borrowed-keys-in-structural-preflight)
at `9bd3a54` keeps serde input-lifetime keys borrowed in the original per-map duplicate index, copies transient
keys after admission and moves owned strings. Original decoded equality, duplicate rejection,
limits, error order and file gates remain; no new owner, parser or interner is introduced.
Core format/allocation controls pass 13/13, owner/public controls 1,148/1,148 and original release
game/native qualification 148/148. Independent Scene consumers pass 157/157 with one original
manual timing case skipped. Production review finds no P1/P2. The
[balanced 16K report](../../benchmarks/2026-11-14-development-runtime-costs.md#borrowed-keys-in-structural-preflight)
adds twelve lifetimes and 36 replays, observing structural-preflight wall medians 338.446/206.105
ms before/after with nearly equal source payload decode. Nested structure/expansion overlaps
the changed work and full process ordering is inconsistent. This is scoped allocation/workload
evidence, not exclusive CPU/GPU, physical-memory or numerical frame-tail qualification. Header
fusion, remaining source/reference work, foreign execution and full Foundation/R9/R10/R12 remain open.

The [mature-engine module comparison](../../benchmarks/2026-11-17-runtime-module-comparison.md)
adds ordinary public 64/4K/16K consumers and nine final local processes with 72 accepted case
records and exact data checks. Bevy typed Scene, official World RON, Godot generic/packed metadata
and native cell construction retain their different prior work and storage guarantees. Nara's
roughly 37 ms JSON decode, 18 ms compile and 8 ms prepare/commit at 16K establish optimization
orientation, not a whole-engine ratio. Provisional source-to-native/post-decode targets remain
proposals; no validator, source format, production dependency or owner changes. Header fusion
is deferred behind larger post-decode/representation attribution. Full Foundation/R9/R10/R12
and numerical runtime-tail budgets stay open.

The [joint effective-reference interpretation](../../plans/2026-11-13-rust-state-reconstruction-plan.md#joint-effective-reference-interpretation)
at `6295ccb` implements one shared Reflect interpretation for Scene flat/source-backed receiving admission,
keeping existing asset product/resolver checks before entity-plan finalization and namespace
closure. Complete single-kind visitors, hidden-reference refusal and actual work/allocation
admission remain. Owner/public tests pass 1,148/1,148 and original release game/native acceptance
148/148; independent production/test reviews close without P1/P2. The
[four-process comparison](../../benchmarks/2026-11-14-development-runtime-costs.md#joint-effective-reference-interpretation)
retains twelve 16K lifetimes/36 GPU replays and observes lower flat/unbound envelopes, without
crediting unchanged stages or claiming memory/frame-tail/general speedup. Independent Scene
consumers pass 157/157 with one original manual timing skip; correctness Clippy, feature-free
compilation and formatting pass. ADR0006/0049 are refined without a new
cache, source authority, receiving owner or production dependency. Full Foundation stays active.

The [container-cursor successor](../../plans/2026-11-13-rust-state-reconstruction-plan.md#reference-paths-from-active-container-cursors)
at `26c12dc` keeps complete raw reference/limit checks while replacing per-value path/Exit events with active
Map/List positions and exact on-demand path materialization. Reference controls pass 29/29,
full owner/public 1,154/1,154 and original release game/native 148/148. Production/test reviews
find no P1/P2. The [paired game report](../../benchmarks/2026-11-14-development-runtime-costs.md#reference-paths-from-active-container-cursors)
retains twelve 16K lifetimes/36 replays and observes lower planner/flat/unbound envelopes without
combining nested costs or claiming physical-memory, GPU or frame-tail gains. Source metric reuse
remains a separate, deferred contract; no cache, producer or new validation authority is added.
Independent Scene consumers pass 157/157 with one original manual timing skip; correctness
Clippy, feature-free compilation and formatting pass. Full Foundation is open.

The [value-budget cursor unit](../../plans/2026-11-13-rust-state-reconstruction-plan.md#value-admission-through-active-container-cursors)
at `240a453` retains complete source/Native cost-depth and reference policy while removing
per-child Value events and iterator reinsertion. Actual scratch admission and frozen retained
storage remain; scalar/empty roots need no traversal allocation. The corrected Reflect library
passes 283/283, other owner/public cases 884/884 with unchanged production, original game/native
148/148 and independent Scene consumers 157/157 with one manual timing skip. The
[paired report](../../benchmarks/2026-11-14-development-runtime-costs.md#value-admission-through-active-container-cursors)
observes lower original/Native admission envelopes without claiming an unbound-preparation,
physical-memory, GPU or frame-tail improvement. Source/program/report hashes, correctness Clippy,
feature-free compilation and formatting pass. No new validation owner, cache, source format or
public receiving policy is introduced. Full Foundation remains open.

The [unbound attribution unit](../../plans/2026-11-13-rust-state-reconstruction-plan.md#unbound-preparation-cost-attribution)
at `aeeb79f` distinguishes receiving admission, all-component codec preflight, World preparation
and candidate materialization through five feature-gated original-owner scopes. The collector
separates nine diagnostic preflights from six actual unbound calls using registered ancestry,
including explicit-parent and grandparent controls. Scene/public tests pass 64/64, release
game/native 148/148 and final observer 1/1. A further complete three-lifetime 16K process and nine
GPU replays attribute most of the roughly 82 ms envelope to receiving/codec work, without a
speed-improvement claim. Correctness Clippy, feature-free compilation, formatting and source/
report checks pass. No new validation, lifecycle or observation owner is introduced; full
Foundation and remaining runtime representation/Native-construction work stay open.

The [direct-native Tilemap experiment](../../plans/2026-11-13-rust-state-reconstruction-plan.md#direct-native-tilemap-construction)
is measured and rejected: actual unbound component preparation is cheaper, diagnostic preparation
is more expensive and combined decode/process observations establish no complete improvement.
The original production algorithm is restored; independent public codec controls at `8e8a1f6`
pass with the full 110/110 Tilemap library. A restored release process completes three original
16K lifetimes/nine GPU replays and cleanup. Candidate success results, exact archived patch and
program/source/report identities remain explicitly separate in the
[rejected report](../../benchmarks/2026-11-14-development-runtime-costs.md#direct-native-tilemap-construction-experiment).
No API, format, policy, dependency or new owner is adopted. Future typed/domain construction
work and full Foundation remain open.

The [membership-index comparison](../../plans/2026-11-13-rust-state-reconstruction-plan.md#native-tile-coordinate-membership-index)
defers the performance choice and restores the original production BTreeSet. The two-line HashSet
candidate preserves owner/render 118/118, independent TileSet 14/14 and original game 148/148,
but unchanged parsing/admission stage drift prevents reliable cost attribution across two full
comparisons. Exact source/program/raw identities and selected complete 24-lifetime/72-replay facts
remain in the [finite report](../../benchmarks/2026-11-14-development-runtime-costs.md#native-tile-coordinate-membership-index).
The rebuilt unchanged program completes another three 16K lifetimes/nine replays and cleanup.
No runtime, validation, dependency, profiler or public interface change is adopted. Larger
domain/diagnostic preparation work and full Foundation remain open.

The [codec-owned diagnostic successor](../../plans/2026-11-13-rust-state-reconstruction-plan.md#codec-owned-diagnostic-validation)
at `1b4a4ad` adds an optional validator to the original frozen native binding. Check/load
diagnosis can avoid candidate construction while preserving full receiving/domain/context
semantics and original fallback lifetimes; actual preparation executes only original preflight.
Tilemap shares its configuration and complete cell parser without building a diagnostic typed
Vec/native tree. Public controls pass 13/13, affected Scene consumers 20/20, independently
locked consumers 33/33 and original release game/native controls 148/148. Full owning libraries
record 1,199 successes plus one corrected fixture passing separately; a final valid-provider
control also passes. Correctness Clippy, feature-free compilation, formatting and source/report
checks pass. The [same-observer report](../../benchmarks/2026-11-14-development-runtime-costs.md#codec-owned-diagnostic-validation)
retains twelve original lifetimes/36 GPU replays and cleanup, but multiple unchanged stages
also slow down: no elapsed improvement or causal regression is established. This is qualified
behavioral separation, not achieved performance targets; full Foundation remains open.

The November 13 [two-level marker recovery](../../plans/2026-10-20-persistent-two-level-game-plan.md#november-13-marker-reconstruction-recovery)
qualifies two concrete ADR0027 recovery cases through complete managed HeadlessRun frames:
repairing damaged dynamic representation and replacing a legally retired owned marker. Real
Degraded Retry preserves the original committed Scene source/receipt and logical progress,
recovers exact runtime identity/components/root membership, then permits ordinary travel and
retirement without removing an unrelated entity. The release-mode sample cohort passes 32/32
with no skips on Windows. Partial repair effects remain possible on failure; native physics/audio
reconstruction, general rollback, arbitrary authored-component recovery and other platforms are
not qualified.

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

The November 16
[source-owned recursive child successor](../authoring-read-projection-design.md#source-owned-recursive-child-discovery-successor)
qualifies the bounded direct-source operation. Scene/Prefab borrows Record declarations,
root-relative tombstones, Map keys and KeyedList identities through the existing Reflect
property walker. Tooling admits
complete target/row/cursor metadata before output copies and retains exact basis/owner fencing.
Available declaration/identity rows do not replace Present/Defaulted/Missing reads. The new
independent semantic Host workflow is separate from the retained direct-member regression;
neither requires a native window. Shared owners pass 1839/1839, the independent semantic Host
passes 8/8 and the retained independent prepared-authoring target passes 10/10. The linked
qualification retains run IDs, actual review corrections and final focused/static checks.
Production and independent consumer revisions are `7f45d87` and `0737909`.
Full R2, Prefab-overlay discovery and locality/performance comparisons remain open.

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

The November 19 [authoring correctness repairs](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#november-19-save-identity-and-direct-reference-closure)
bind stable Scene and Prefab Save to the open document's sidecar identity at the original file
switch. They also validate both representations of direct local references against the final
source candidate before committing a deletion. The linked record owns the actual verification
and remaining audit items. ADRs [0038](0038-scene-prefab-authoring-identity-and-provenance.md) and
[0090](0090-unavailable-schema-and-lossless-authoring.md) own the respective contracts.

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
| [0002](0002-use-bevy-ecs-as-ecs-substrate.md) | accepted | implemented | `nara_ecs` | [crates/nara_ecs/Cargo.toml](../../../crates/nara_ecs/Cargo.toml); [crates/nara_ecs/src/lib.rs](../../../crates/nara_ecs/src/lib.rs); [internal support](../../../crates/nara_ecs/src/__private.rs) | [authoring-surface contract](../../../crates/nara_ecs/tests/authoring_surface.rs); [renamed ECS fixture](../../../tests/fixtures/derive-dependencies/renamed-ecs/src/lib.rs); [renamed root fixture](../../../tests/fixtures/derive-dependencies/renamed-root/src/lib.rs); [advanced observer consumer](../../../tests/fixtures/derive-dependencies/observer_contract.rs); [renamed schedule extension](../../../tests/fixtures/schedule-extension/renamed-root/src/lib.rs); [U7 derive evidence](../../knowledge/engineering/verification/2026-10/2026-10-22T190137Z-u7-facade-aware-bundle-and-systemparam-extraction-f14b220201744f71a90a123e476b1769.md) | Ordinary exports and the six maintained derives preserve original Bevy runtime types independently of prelude growth. Complete version-coupled substrate remains under `bevy_ecs`; hidden lifecycle-free support remains under `__private`. Both renamed fixtures now also execute original custom Event/EntityEvent observers through a caller-local `bevy_ecs` import, preserving payloads, target isolation and retirement. The complete four-case parent harness passes; this adds no derive wrapper or automatic path-resolution guarantee for other Bevy macros. | Upgrades must review exact released expansions and execute both renamed fixtures, including caller-configured advanced observers. Broader Bevy compatibility is not promised. | 2026-11-14 @ advanced observer consumers; prior U7 evidence retained |
| [0003](0003-own-app-plugin-and-schedule-lifecycle.md) | accepted | implemented | `nara_app` and schedule-set-owning domains | [crates/nara_app/src/lib.rs](../../../crates/nara_app/src/lib.rs); [execution selection](../../../crates/nara_app/src/execution.rs); [crates/nara_app/src/runtime.rs](../../../crates/nara_app/src/runtime.rs) | [schedule registry](../../../crates/nara_app/tests/schedule_registry.rs); [profile consumer](../../../crates/nara_app/tests/execution_profile.rs); [joint qualification](../../plans/2026-11-03-joint-motion-execution-plan.md#explicit-production-executor-successor) | Graphs initialize fallibly after Finish and before Ready/Startup. Default Serial and explicitly admitted native Parallel use original Bevy executors across all automatic schedules; controlled deferred settings survive installation. Custom schedules keep explicit policy/driving. Configuration and plugin-plan identities are distinct. Runtime retains NonSend, faults and physical close without destroying the process-global compute pool. | Full R4c/R10 consumer, arbitrary effects and cost requirements retain their owning plan scope. | 2026-11-15 @ explicit production executor qualification; prior U6/U7 evidence retained |
| [0004](0004-use-bevy-reflect-backed-component-metadata.md) | superseded | superseded | [ADR 0119](0119-schema-first-semantic-metadata-authority.md) | - | - | Original reflection-authority decision is historical; Schema-first authority is now owned by ADR 0119. Existing native behavior was not requalified by this documentation change. | - | 2026-10-23 @ decision supersession only; earlier evidence retained in history |
| [0005](0005-dimension-aware-runtime-with-2d-first-authoring.md) | accepted | partial | 2D/3D spatial and render domains | [crates/nara_transform/src/lib.rs](../../../crates/nara_transform/src/lib.rs); [crates/nara_render/src/lib.rs](../../../crates/nara_render/src/lib.rs) | [crates/nara_transform/src/lib.rs](../../../crates/nara_transform/src/lib.rs); [crates/nara_sprite_render/src/tests.rs](../../../crates/nara_sprite_render/src/tests.rs) | Completed inherited 2D affine snapshots and hierarchy propagation are implemented. No concrete 3D hierarchy/render consumer has proved the additive 3D boundary. Future persistent local-affine and 3D representations remain Proposed, not accepted implementation debt. | A concrete 3D hierarchy/render workflow validates the separate dimension boundary and selects any required schema/source migration under ADR 0097. | 2026-10-23 @ spatial decision-scope refinement; prior U4 evidence retained |
| [0006](0006-scene-and-prefab-data-model.md) | accepted | implemented | `nara_scene`, `nara_reflect`, persistent component providers and product composition | [document](../../../crates/nara_scene/src/document.rs); [shared preflight](../../../crates/nara_scene/src/validation.rs); [data carrier](../../../crates/nara_scene/src/data_components.rs); [staged publication](../../../crates/nara_scene/src/spawn/publication.rs); [asset reservations](../../../crates/nara_scene/src/asset_reservations.rs); [live export](../../../crates/nara_scene/src/export.rs); [recipe](../../../src/product.rs) | [runtime-family evidence](../../knowledge/engineering/verification/2026-10/2026-10-27T125421Z-production-scene-native-data-runtime-and-managed-authority-qualification-294f613511bb48e49fed958c05a1a469.md); [product selection evidence](../../knowledge/engineering/verification/2026-10/2026-10-27T141025Z-mixed-scene-product-selection-and-content-to-runtime-admission-qualification-7c90588821aa4e8da374958ef2e436e4.md); [public product consumer](../../../tests/product_data_components.rs); [current ownership/export qualification](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#scene-staging-asset-reservations-and-live-export) | Explicit native/data storage, exact managed selection, atomic publication and guarded member access remain. Unbound preparation, allocated staged replacement and private asset reservations name distinct custody. Live export covers all registered Scene instances, flattens Prefabs and admits complete native/data eligibility before encoders. Registry defaults use exact residual node/byte limits, including zero. Owning 1070/1070, independent 53/53, product 24/24, real headless reconstruction/adoption 2/2 and current continuous Windows/Vulkan adoption 1/1 pass. Product Check/content/startup/replacement retain exact metadata witnesses. | Actual language-owner adoption remains in the Scene plan/Proposed 0120; pure data is not executable readiness. Trusted encoder allocation is outside logical value limits. The first historical B exit remains unexplained despite a passing complete repeat; full R12 and foreign execution remain open. | 2026-11-14 @ `88798b0` and complete native repeat `7d3718dd-2228-433e-94b5-d7b7f41ca6ed`; original family scope retained |
| [0007](0007-asset-identity-and-import-pipeline.md) | accepted | implemented | `nara_asset` and concrete asset/content owners | [Product server](../../../crates/nara_asset/src/server.rs); [storage](../../../crates/nara_asset/src/storage.rs); [selected codecs](../../../src/runtime_content/codecs.rs); [base](../../../src/runtime_content.rs) | [Host/source-facet qualification](../../knowledge/engineering/verification/2026-10/2026-10-29T152520Z-first-party-processed-runtime-hosts-and-explicit-source-facets-qualification-c2d2a14cd90243f0a200a287f1ce4d20.md); [binding predecessor](../../knowledge/engineering/verification/2026-10/2026-10-29T060803Z-processed-runtime-scene-and-complete-product-binding-qualification-cd202f5ff4514802a3c5f57a32b43b7a.md) | Checked process-unique complete Product/type bindings remain separate from content and residency. Explicit source facets no longer accompany native storage. The original plan freezes codec definitions/limits and the captured base retains finite Scene data dependencies. Passive unpublished baselines, actual value/state association and pre-admitted catalogue copies preserve failure/cancellation without publishing claims. Raw mint/retype stays private; detached render identities grant no residency. | Source locator correction and complete input/target/package delivery remain ordered work. Actual native-family publication retains its concrete owner. | 2026-10-29 @ `44470ed`; predecessor identity scopes retained |
| [0008](0008-runtime-concurrency-and-task-pools.md) | accepted | implemented | `nara_app`, `nara_tasks`, task-consuming domains | [crates/nara_app/src/lib.rs](../../../crates/nara_app/src/lib.rs); [crates/nara_tasks/src/runtime.rs](../../../crates/nara_tasks/src/runtime.rs) | [crates/nara_tasks/src/tests/execution.rs](../../../crates/nara_tasks/src/tests/execution.rs); [crates/nara_tasks/src/tests/close.rs](../../../crates/nara_tasks/src/tests/close.rs); [U2 return evidence](../../knowledge/engineering/verification/2026-10/2026-10-22T181737Z-u2-task-return-and-physical-running-accounting-ce30f14820ee4350931de3355bb0c955.md) | - | - | 2026-10-23 @ U2 return evidence; prior integration evidence retained |
| [0009](0009-diagnostics-errors-and-logging.md) | accepted | partial | `nara_diagnostic`, diagnostic producers | [crates/nara_diagnostic/src/report.rs](../../../crates/nara_diagnostic/src/report.rs); [crates/nara_diagnostic/src/field.rs](../../../crates/nara_diagnostic/src/field.rs) | [crates/nara_diagnostic/src/contract_tests.rs](../../../crates/nara_diagnostic/src/contract_tests.rs); [crates/nara_project/src/tests.rs](../../../crates/nara_project/src/tests.rs) | Current scene, asset, project, and tooling reports use static summaries and classified fields; remaining CLI/editor/AI consumption and runtime producer bridge convergence are incomplete. | A concrete workflow or safety defect identifies a missing diagnostics consumer or producer bridge. | 2026-07-10 @ 6a70847 |
| [0010](0010-plugin-lifecycle-dependencies-and-failure.md) | accepted | partial | `nara_app`, product start ownership | [plugin lifecycle](../../../crates/nara_app/src/plugin.rs); [resolution](../../../crates/nara_app/src/plugin/resolve.rs); [domain facts](../../../crates/nara_app/src/plugin/domain_error.rs); [product lowering](../../../src/project_host/runtime_diagnostics/plugin.rs) | [N05 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19T170951Z-n05-safe-plugin-failure-facts-and-invocation-context-d01f7391b84348a8827a0daa760dd1a4.md); [App domain tests](../../../crates/nara_app/tests/plugin_domain_errors.rs); [recipe runtime tests](../../../tests/project_runtime_boot.rs); [Editor Play tests](../../../tests/workspace_play_runtime.rs) | Static declarations, pure closure, repeatable preparation/closed commit, explicit shutdown obligations, sealed Apps and sticky mutation guards remain implemented. Checked plugin-domain code/summary replaces factory-only unchecked codes and also serves hooks. Direct App addition/seal and retained product owners preserve actual invocation and primary/shutdown facts, including retryable first preflight. Product lowering preserves safe identity; Editor publishes shutdown failures once while physical close is pending. Focused owner/product tests and the all-features/all-targets workspace check pass. Final desktop/native integration and future admitted replacement contracts remain incomplete in this broader decision scope. | Desktop/native lifecycle parity or a concrete replacement workflow supplies the missing evidence without weakening closed commit or retained-owner cleanup. | 2026-10-20 @ `40bcdc6` and N05 evidence; earlier `5ddbf18` retained |
| [0011](0011-component-schema-ids-and-migrations.md) | accepted | implemented | `nara_reflect`, schema-owning domains | [crates/nara_reflect/src/schema.rs](../../../crates/nara_reflect/src/schema.rs); [crates/nara_reflect/src/migration.rs](../../../crates/nara_reflect/src/migration.rs) | [crates/nara_reflect/tests/registry_contract.rs](../../../crates/nara_reflect/tests/registry_contract.rs); [crates/nara_reflect/tests/catalog_format.rs](../../../crates/nara_reflect/tests/catalog_format.rs); [property lineage](../../../crates/nara_reflect/tests/property_lineage.rs); [property resolver](../../../crates/nara_reflect/tests/property_addressing.rs); [R5a verification](../../knowledge/engineering/verification/2026-10/2026-10-24T024954Z-r5a-final-owner-diagnostics-and-workspace-qualification-f58a0ab130cb47a8bc2829e32f1d0a86.md) | Root identities, native bindings, freeze and migrations retain their existing evidence. R5a qualifies sibling-scoped Record member IDs, root-relative declaration-path tombstones, explicit keyed-list selectors and exact current-Schema/version resolution. Reflect/Scene passes 532 tests, and four independent Scene/Prefab consumer tests cover migration, reorder, atomic editing, history and persistence. This is Schema-relative addressing, not complete source-document/entity/occurrence identity. | A new accepted Schema shape or identity-evolution requirement reopens this contract. Complete source/document/occurrence addressing is a separate owner obligation, not a missing part of the Schema-relative selector. | 2026-10-24 @ R5a verification; earlier f91e2e0 evidence retained |
| [0012](0012-render-crate-boundaries.md) | accepted | implemented | Render authoring, extraction, and backend crates | [crates/nara_render/src/lib.rs](../../../crates/nara_render/src/lib.rs); [crates/nara_sprite_render/src/extract.rs](../../../crates/nara_sprite_render/src/extract.rs) | [crates/nara_render/src/lib.rs](../../../crates/nara_render/src/lib.rs); [crates/nara_sprite_render/src/tests.rs](../../../crates/nara_sprite_render/src/tests.rs) | - | - | 2026-07-13 @ e276402 |
| [0013](0013-platform-window-and-runner-boundaries.md) | accepted | implemented | `nara_app`, `nara_window`, platform adapters | [crates/nara_app/src/lib.rs](../../../crates/nara_app/src/lib.rs); [native mode lowering](../../../crates/nara_winit/src/window_mode.rs); [bounded Winit patch](../../../vendor/winit/NARA-PATCH.md) | [runner tests](../../../crates/nara_winit/src/tests.rs); [actual Windows mode tests](../../../crates/nara_winit/src/window_mode/native_tests.rs); [mixed-DPI qualification](../../verification/native-editor-qualification.md#mixed-dpi-window-controls); [Runtime GPU/picking](../../plans/2026-11-14-durable-material-and-shader-plan.md#independent-native-drawing-and-picking-qualification) | Startup applies windowed/borderless/exclusive mode with exact selection or explicit pre-creation refusal. The bounded Windows 11 repair retains the original owner, suggested DPI position and explicit size requests. Fresh actual 125%/150% geometry and OS title-bar controls pass 2/2; earlier adapter 64/64 and other native 17/17 retain their scope. A separate ordinary Runtime qualifies destination GPU presentation and captured picking on DX12/Vulkan, with retained-input pixel replays. | GPU presentation during the injected title-bar gesture, maximized/snap/keyboard DPI movement, human-hand observation, dynamic mode switching and foreign native execution remain separately unqualified. | 2026-11-15 @ mixed-DPI repeat and separate Runtime GPU/picking; earlier mode scope retained |
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
| [0029](0029-animation-strategy.md) | accepted | partial | `nara_sprite_animation`; Two-Level Game exploration plugin; existing Asset, Sprite, Image, Scene and Runtime owners | [immutable clips and codecs](../../../crates/nara_sprite_animation/README.md); [game selection](../../../samples/two-level-game/src/animation.rs); [projection](../../../samples/two-level-game/src/gameplay/mod.rs) | [edited-resource workflows](../../../samples/two-level-game/tests/authored_animation.rs); [source/native/cooked qualification](../../plans/2026-11-03-joint-motion-execution-plan.md#authored-sprite-clips-through-runtime-and-delivery) | Immutable named clips replace the production hardcoded frame selector. Unequal durations, exact boundaries, loop/one-shot completion and source/Runtime admission share constructors; sampling uses caller time without allocation or repeated validation. The actual game retains original fixed/paused/restored projection, Image/sampler and collection lifetimes. Module28, complete ordinary game330, actual native reconstruction with ten retained pixel replays, relocated source-free desktop delivery, real Rust-edit adoption and thirteen minimal-feature controls pass. Catalog7 and native binding3 require bundle refresh/recook; no new clock or resume field is introduced. | Independent playback cursors, events, persistent field targets, blending, universal controllers and skeletal animation remain unselected. No numerical performance or foreign-platform qualification follows. | 2026-11-18 @ authored Sprite clips through Runtime/delivery; module `93701c3`, ADR 0095 ownership retained |
| [0030](0030-audio-strategy.md) | superseded | superseded | Historical audio abstraction | - | - | Replaced by ADR 0095. | - | 2026-07-20 @ ADR 0095 |
| [0031](0031-text-and-font-strategy.md) | accepted | partial | `nara_text`, UI compiler/widgets/planner and WGPU | [font engine](../../../crates/nara_text/src/engine.rs); [editing](../../../crates/nara_text/src/editing.rs) | [text guide](../../guides/editable-text.md); [styled unit](../../plans/2026-11-05-styled-source-workspace-plan.md); [independent styled row](../../../tests/fixtures/ui-module-consumer/tests/styled_property_row.rs) | Finite ordered faces, bounded local editing and immutable snapshots retain their earlier qualification. Validated typography, actual fallback baselines, atomic full-history adoption, explicit Regular face provenance and shared field geometry now serve the complete Inspector in both compositions. Source, glyph, raster and final output limits remain original-owner contracts. | Native OS IME/accessibility, live all-panel font-family switching, project fonts, variable axes, localization and world text remain separate; the unit records injected-input and actual-platform limits. | 2026-11-05 scoped typography and styled source workspace; earlier U12/native/export scopes retained |
| [0032](0032-render-backend-integration-boundary.md) | accepted | partial | platform/render adapters | [crates/nara_window/src/backend.rs](../../../crates/nara_window/src/backend.rs); [crates/nara_render_wgpu/src/host.rs](../../../crates/nara_render_wgpu/src/host.rs) | [tests/window_surface_retirement.rs](../../../tests/window_surface_retirement.rs); [crates/nara_render_wgpu/src/host/imgui/tests.rs](../../../crates/nara_render_wgpu/src/host/imgui/tests.rs) | Safe owning surfaces, scoped main-thread retirement, resize, generations and at-most-once acquire/submit/present are implemented. Resource-complete packets avoid live World/asset reads. ADR 0117 admits bounded serial views/offscreen sequencing; ADR 0118 adds concurrent Editor views, detach/reopen and device recovery on Windows/DX12. Arbitrary target coordination and native Linux/macOS release qualification remain outside the evidence. | Measure packet/workload pressure before widening coordination; qualify each additional native platform with its actual lifecycle and recovery trajectory. | 2026-10-17 @ foundation `ca3d803`, selected Editor `f4ffdd18` |
| [0033](0033-asset-import-and-render-resource-preparation-seam.md) | accepted | partial | `nara_asset`, `nara_image`, `nara_tilemap`, content residency and render backends | [Image content](../../../crates/nara_image/src/asset.rs); [TileSet codec](../../../crates/nara_tilemap/src/runtime.rs); [publication](../../../crates/nara_tilemap/src/content/publication.rs); [Host residency](../../../src/project_host/runtime/content_residency.rs) | [Host/residency qualification](../../knowledge/engineering/verification/2026-10/2026-10-29T152520Z-first-party-processed-runtime-hosts-and-explicit-source-facets-qualification-c2d2a14cd90243f0a200a287f1ce4d20.md); [TileSet/native predecessor](../../knowledge/engineering/verification/2026-10/2026-10-29T114236Z-processed-tileset-ownership-and-exact-scene-resource-baselines-qualification-e5b31550eacc4fd1a3d4b6168fb4df52.md); [retained-output predecessor](../../knowledge/engineering/verification/2026-10/2026-10-29T094316Z-actual-runtime-decoded-output-charging-and-shared-pixel-retention-qualification-14f023431b4c462dbeb1051f1739662e.md) | Optional Product/provenance stays distinct from actual Image content. Source/processed startup and travel share stock Image/TileSet stores, exact association/content baselines, actual Required dependencies, atlas/cell checks and one publication tail. Native pixels retain the original decoded-output and metadata owners through retirement. Source execution is explicit. Earlier DX12 atlas readback remains its predecessor scope; the successor adds actual stock extraction/preparation and the selected native Editor regressions. | Third-party native publication, durable CAS/import-cache, streaming and complete input/target/package delivery remain open. CPU decode alone grants no native residency. | 2026-10-29 @ `44470ed`; earlier Image/output/TileSet scopes retained |
| [0034](0034-editor-play-mode-world-boundary.md) | accepted | implemented | `nara_tooling`, concrete Editor Host | [crates/nara_tooling/src/play.rs](../../../crates/nara_tooling/src/play.rs); [src/project_host/runtime/editor.rs](../../../src/project_host/runtime/editor.rs) | [R5c Play-source qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T210921Z-trusted-play-source-continuity-and-source-local-close-qualification-61897d1b17744e2aaff6fc6efb1594a7.md); [R5c source/preview qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T180456Z-common-source-admission-and-retained-scene-preview-qualification-45ffac37614a4edaa792f08d650f292e.md); [tests/scene_play_mode.rs](../../../tests/scene_play_mode.rs); [tests/workspace_play_runtime.rs](../../../tests/workspace_play_runtime.rs) | Trusted Play retains exact document/incarnation/revision through every real non-Empty phase, including start reservation, Game activation, cancellation and physical retirement/retry. Current revision observes that source rather than active navigation. The dedicated source-local Close/public/native qualification passes without widening Active-only execution. | Unrelated document Close preserves Play; associated Close/Exit still waits for actual return. The six Windows/DX12 trajectories do not qualify Linux/macOS, monitor scanout or the remaining projection/history redesign. | 2026-10-24 @ Play-source `ebc0d48`; earlier scoped evidence retained |
| [0035](0035-project-manifest-and-runtime-settings-authority.md) | accepted | partial | `nara_project`, facade composition and concrete source/Runtime Hosts | [manifest](../../../crates/nara_project/src/manifest.rs); [effective settings](../../../crates/nara_project/src/effective.rs); [Runtime configuration](../../../crates/nara_project/src/runtime_configuration.rs); [composition](../../../src/project_host/composition.rs) | [Source authority](../../knowledge/engineering/verification/2026-10/2026-10-24T065424Z-project-source-authority-carrier-and-creation-identity-qualification-6cdfe4a580b14b8da78e659010497632.md); [Input source](../../knowledge/engineering/verification/2026-10/2026-10-30T083131Z-immutable-input-definitions-and-global-source-workflow-qualification-f8db4628f79943ac94a90593cef6c736.md); [processed Input](../../knowledge/engineering/verification/2026-10/2026-10-30T100534Z-processed-startup-input-ownership-and-prebuilt-source-free-physical-process-qualification-35c2432f89d04982b10d2be3fa0f9d12.md) | Bounded manifest/profile admission, source identity and shared source-independent Runtime composition retain their linked qualification. Source projection preserves an unfulfilled Input path; strict Runtime projection refuses it. Processed configuration separately admits an optional complete primary Product, round-trips exact values and cannot select code. Actual catalog/recipe admission requires membership, Input plugin and selected typed decoder before startup. Real source Check/Export and prebuilt processed execution have separate evidence. | Remaining semantic capability/default and file-backed specialized-plugin configuration choices retain their concrete consumer triggers. Target-build/package selection and complete Editor/frontend delivery remain separate; no path-to-Product guess or source-authority fallback is admitted. | 2026-10-30 @ explicit source obligation and processed Product configuration; prior source authority scope retained |
| [0036](0036-event-message-and-resource-queue-lifetime.md) | accepted | partial | runtime domains | [source changes](../../../crates/nara_asset/src/source_changes.rs); [reload requests](../../../crates/nara_asset/src/reload/requests.rs); [Input owner](../../../crates/nara_input/src/timeline/owner.rs) | [source-change tests](../../../crates/nara_asset/src/source_changes/tests.rs); [request tests](../../../crates/nara_asset/src/reload/tests_requests.rs); [bounded Input qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T140508Z-bounded-input-ownership-and-total-ui-presentation-qualification-b97643044e4a4f92b892a3539d2ad415.md) | Gameplay, diagnostic, reload and watcher queues retain their recorded bounds and cleanup, including full-rescan recovery. Input now owns finite topology/member/log storage and terminal credits. Projection/observation/action copies keep original records charged until actual completion, including late next-frame copies. Independent mutation queues are removed. | Other producers still need concrete cleanup/diagnostic/replay declarations. The Input slice does not qualify arbitrary queues or all replay formats. | 2026-10-30 @ bounded Input successor; `c54d366` scopes retained |
| [0037](0037-asset-load-request-cache-and-lifetime-policy.md) | accepted | partial | `nara_asset`, concrete asset domains | [crates/nara_asset/src/state.rs](../../../crates/nara_asset/src/state.rs); [crates/nara_asset/src/storage.rs](../../../crates/nara_asset/src/storage.rs); [crates/nara_asset/src/reload/resolution.rs](../../../crates/nara_asset/src/reload/resolution.rs); [crates/nara_image/src/import/publication.rs](../../../crates/nara_image/src/import/publication.rs) | [crates/nara_asset/src/reload/tests_requests.rs](../../../crates/nara_asset/src/reload/tests_requests.rs); [crates/nara_asset/src/reload/tests_resolution.rs](../../../crates/nara_asset/src/reload/tests_resolution.rs); [contract evidence](../../knowledge/engineering/verification/2026-10/2026-10-18T164837Z-component-plugin-and-asset-contract-refactor-b4c2497eff5d4bdb90cebe1de995ceb3.md); [R0 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19T021717Z-image-lifecycle-contract-repairs-95901fe367a149bc90ec922ad0384a73.md); [U5 asset qualification](../../knowledge/engineering/verification/2026-10/2026-10-22T193928Z-u5-asset-association-desired-recipe-and-runtime-owned-watch-rescan-e081ec487db94262b392fc8b7d7ff3ef.md) | Managed value/state association precedes baseline capture; common admission rejects stale success, failure, removal, resource replacement and empty-slot ABA. Image retains desired metadata independently from attempts and last-good pixels, binding retained recipes to publication/slot/metadata observation. Watcher scope loss retains runtime-owned rescan with weak Host receipts. Final combined 329-test cohort passes, including retained relative removal/metadata order across rejection and coalescing; independent Image/Watcher rereviews have no remaining blocker. Identity, concrete residency ownership and retention policy remain separate; no mixed cache modes or generic lease/eviction mechanism are reserved. Broader acquisition/dependency workflows remain unqualified. | Existing public content/native consumers retain their recorded scope. A complete acquisition/dependency/retirement workflow identifies missing concrete owners and snapshot requirements; shared execution additionally requires measured duplication. | 2026-10-23 @ identity/residency/policy scope refinement; U5 evidence retained |
| [0038](0038-scene-prefab-authoring-identity-and-provenance.md) | accepted | partial | `nara_scene`, `nara_tooling`, and projection-aware consumers | [compiled.rs](../../../crates/nara_scene/src/compiled.rs); [prefab.rs](../../../crates/nara_scene/src/prefab.rs); [provenance.rs](../../../crates/nara_scene/src/provenance.rs) | [compiled production qualification](../../knowledge/engineering/verification/2026-11/2026-11-02T071856Z-closed-compiled-scene-production-and-independent-delivery-qualification-91cc04f45ba249f881cec4f9f78bd3d1.md); [root/placement qualification](../../knowledge/engineering/verification/2026-11/2026-11-02T121917Z-explicit-root-binding-and-source-placement-production-qualification-0eada24ff287416bbfe94ccf511d2380.md); [complete extraction consumer](../../knowledge/engineering/verification/2026-11/2026-11-03T115127Z-complete-public-prefab-extraction-and-native-workflow-qualification-c440d6b31843411897951e9ba2221a2e.md) | Canonical source/occurrence identities, closed flat compiled values and provenance, source-owned placements and explicit root binding have qualified production owners. Complete conversion preserves declared references and hidden defaults through exact history and Save/Reopen. Extraction adds typed routed roots, complete correspondence, saved-generation guards, selective publication and CLI-only recovery; independent inheritance/Revert/conversion, strict Check, actual Play and native controls qualify the selected complete unit. | Broader write-back, additional platforms and complete R5 remain open. Keep source/occurrence lowering under ADR0102; this unit does not accept all of ADR0083. Public child-receipt projection and idle native incidence retain explicit evidence limits. | 2026-11-03 @ complete extraction consumer; topology `88fd728`; earlier scoped evidence retained |
| [0039](0039-main-loop-time-pause-and-runtime-state.md) | accepted | partial | `nara_app`, `nara_input` | [crates/nara_app/src/lib.rs](../../../crates/nara_app/src/lib.rs); [crates/nara_input/src/timeline.rs](../../../crates/nara_input/src/timeline.rs) | [crates/nara_app/src/lib.rs](../../../crates/nara_app/src/lib.rs); [crates/nara_input/tests/timeline_runtime.rs](../../../crates/nara_input/tests/timeline_runtime.rs) | Real/virtual/fixed/render clocks, pause, exact Step, managed Winit driving, cleanup, App exit and truthful close are implemented. The normalized timeline routes UI pause-safely, retains action edges and resumes only unconsumed actions. Typed gameplay-state transition queues/stages with OnExit/transition/OnEnter and broader background/redraw policy remain incomplete. | OQ-034 requires a real boot/menu/gameplay/pause/game-over topology before selecting the minimum state and scoped-lifetime surface; a measured desktop background-policy failure separately admits the remaining runner policy. | 2026-08-05 @ `c54d366` |
| [0040](0040-render-resource-lifetime-and-submitter-ownership.md) | accepted | partial | render domains and WGPU Host | [output leases](../../../crates/nara_render_wgpu/src/output.rs); [original prefixes](../../../crates/nara_render_wgpu/src/submission.rs); [Host](../../../crates/nara_render_wgpu/src/host.rs) | [immutable-output qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T121248Z-immutable-render-outputs-owning-producers-and-original-native-prefix-qualification-1b4de7f4305740698abba87b589f9459.md); [history and cleanup qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T210034Z-captured-view-histories-and-original-submission-cleanup-production-qualification-5c061376803446c0bff1a95495ca55c7.md) | Exact outputs, shared byte custody, producer-local retirement and every original prefix retain their separate responsibilities. Stock windows and images share the finite program owner. Captured history publication, late flat cleanup isolation and stable fault/abandonment observation are qualified within the linked scope; callbacks never publish success. | Complete cross-stream/custom composition and measured retention work; browser-local async execution and external replacement authority remain separate. | 2026-11-01 @ captured-history and submitted-cleanup successor; prior evidence retained |
| [0041](0041-input-routing-actions-text-focus-and-accessibility.md) | accepted | partial | Input definition/state, finite ingress, optional native collector, source/Runtime owners, game adapters and UI | [definitions](../../../crates/nara_input/src/definition.rs); [original installation](../../../crates/nara_input/src/state.rs); [native Gilrs](../../../crates/nara_gilrs/README.md); [game input](../../../samples/two-level-game/src/gameplay/input.rs); [native continuation](../../../crates/nara_winit/src/keyboard_continuation.rs); [Runtime custody](../../../src/runtime_content/input.rs) | [Seat/replay predecessor](../../plans/2026-11-13-seat-gamepad-and-input-replay-plan.md); [authored game/delivery](../../plans/2026-11-13-seat-gamepad-and-input-replay-plan.md#authored-game-input-through-source-and-player-delivery); [native controller qualification](../../plans/2026-11-13-seat-gamepad-and-input-replay-plan.md#native-controller-implementation-and-qualification); [explicit Player selection](../../plans/2026-11-13-seat-gamepad-and-input-replay-plan.md#controller-player-implementation-and-qualification) | Original seat/axis/replay and reconstruction scopes remain. Optional Windows WGI uses original connections/terminals, read-only loss observation and typed retirement backpressure. Input/public/bridge150 and game207 retain their recorded scope. The shared source/cooked configuration adds explicit silent/audio desktop controller delivery; CPU270 and both actual source-free combinations pass with inventory0, original keyboard/pixel/Save/Restore and WASAPI cleanup. Controller declarations preserve keyboard capture eligibility while actual held controller members still refuse. | Physical controller button/stick/hotplug gameplay, foreign backend retirement, broader shipping targets, live rebinding UI, arbitrary multi-binding reconstruction, complete text/IME and universal rollback remain separate. No current-generation lookup recovers an old originless event. | 2026-11-19 @ explicit optional native controller delivery; prior `98969c4`/`7cbdd9f` scopes retained |
| [0042](0042-runtime-service-and-backend-boundary.md) | accepted | partial | Native/threaded domain owners and runtime lifecycle | [crates/nara_tasks/src/runtime.rs](../../../crates/nara_tasks/src/runtime.rs); [crates/nara_window/src/backend.rs](../../../crates/nara_window/src/backend.rs) | [crates/nara_tasks/src/tests/execution.rs](../../../crates/nara_tasks/src/tests/execution.rs); [crates/nara_tasks/src/tests/close.rs](../../../crates/nara_tasks/src/tests/close.rs) | Typed cross-thread integration, native ownership, target lifecycle, finite truthful close, terminal evidence, and retained incomplete task cleanup are implemented for their concrete owners. A universal generation-scoped service session, common queue/ack/poison model, and second native service are no longer completion requirements. Browser/device recovery and future long-lived owners still need domain-specific lifecycle evidence. | Each future native/threaded owner proves only the stages, affinity, queue/batch, and synchronous or retryable close semantics its implementation actually needs. A shared session Interface requires a separate evidence-backed ADR. | 2026-07-20 @ ADR 0095 refinement |
| [0043](0043-scene-prefab-and-patch-document-migration-policy.md) | accepted | implemented | Scene, Reflect and explicit project-upgrade owners | [Scene format](../../../crates/nara_scene/src/format.rs); [Reflect migration](../../../crates/nara_reflect/src/migration.rs); [authored upgrade](../../../src/project_host/runtime/editor/authored_upgrade.rs) | [format contract](../../../crates/nara_scene/tests/format_contract.rs); [Scene tests](../../../crates/nara_scene/src/tests.rs); [CP1/S2 qualification](../../knowledge/engineering/verification/2026-11/2026-11-01T203621Z-reviewed-authored-schema-upgrades-and-fresh-process-recovery-qualification-cf7c8ecab10c4e21b4cfcff4906805de.md) | Reviewed authored upgrades transform complete declarations and establish explicit saved/history checkpoints; ordinary Open does not execute project conversion. | CP2 extraction and occurrence remapping have a separate representation and qualification boundary. | 2026-11-02; earlier format evidence retained |
| [0044](0044-root-facade-and-prelude-layering-policy.md) | accepted | implemented | Root `nara` facade and `nara_ecs` authoring surface | [src/lib.rs](../../../src/lib.rs); [crates/nara_ecs/src/lib.rs](../../../crates/nara_ecs/src/lib.rs) | [tests/product_capabilities.rs](../../../tests/product_capabilities.rs); [authoring-surface contract](../../../crates/nara_ecs/tests/authoring_surface.rs); [renamed root fixture](../../../tests/fixtures/derive-dependencies/renamed-root/src/lib.rs); [renamed schedule extension](../../../tests/fixtures/schedule-extension/renamed-root/src/lib.rs) | Root and ECS preludes remain explicit selections; complete substrate imports are opt-in and hidden transaction helpers are not ordinary extension APIs. Root-only renamed derives, custom scheduling and complete workspace targets compile and execute against the original Bevy types. | Upgrades must review exact released expansions and execute both renamed fixtures; broader Bevy compatibility is not promised. | 2026-10-23 @ explicit authoring/full-dependency/internal boundary and renamed-consumer verification; prior facade evidence retained |
| [0045](0045-component-schema-capability-metadata.md) | accepted | implemented | `nara_reflect`, `nara_scene`, `nara_tooling` | [numeric intervals](../../../crates/nara_reflect/src/numeric_interval.rs); [effective.rs](../../../crates/nara_reflect/src/effective.rs); [validation.rs](../../../crates/nara_scene/src/validation.rs); [authoring_decode.rs](../../../crates/nara_scene/src/authoring_decode.rs) | [S3 verification](../../knowledge/engineering/verification/2026-10/2026-10-19T162634Z-authoring-s3-component-constraints-diagnostics-and-inventory-64aacf18a3af432584d306af27d32eb3.md); [interval tests](../../../crates/nara_reflect/tests/numeric_intervals.rs); [U10 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u10-closed-vec2.md); [U11 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u11-recursive-inspector.md) | Recursive shapes, bounded defaults, separate reference-work budgets, raw-preserving projection and source-local reference closure remain implemented. S3 adds typed signed/unsigned/finite-float intervals, F32-materialized comparison, schema admission and canonical fingerprints; absent interval metadata preserves previous identity. Shared validation covers values, defaults/templates and native preflight. Independent component workflows and native rejection/correction pass. Vec2 and recursive Inspector evidence retain their earlier scope. | Cross-field and arbitrary native invariants remain owner-specific; a new shape or aggregate effective-work policy needs a concrete consumer. | 2026-10-20 @ S3 verification; U10 `b0648b6` and U11 `7bec88f` retained |
| [0046](0046-plugin-metadata-and-default-plugin-groups.md) | accepted | partial | `nara_app`, `nara_reflect` and product/plugin recipes | [crates/nara_app/src/plugin.rs](../../../crates/nara_app/src/plugin.rs); [crates/nara_app/src/plugin/definition.rs](../../../crates/nara_app/src/plugin/definition.rs); [crates/nara_reflect/src/plugin.rs](../../../crates/nara_reflect/src/plugin.rs) | [crates/nara_app/tests/plugin_composition.rs](../../../crates/nara_app/tests/plugin_composition.rs); [tests/plugin_composition.rs](../../../tests/plugin_composition.rs); [contract evidence](../../knowledge/engineering/verification/2026-10/2026-10-18T164837Z-component-plugin-and-asset-contract-refactor-b4c2497eff5d4bdb90cebe1de995ceb3.md) | Static declarations, configuration identity, groups, closure, schema selection and provenance are implemented. Before direct-App schema authority publication, actual provider receipts must equal the complete installed declarations' provider union, including multiple installation calls. ProductRecipe/SchemaContribution delegation, standalone registries and asset-free/staged codecs remain supported; diagnostic PluginId is not durable ownership. Package distribution, file-backed settings and missing-package recovery remain unproven. | OQ-045 proves the remaining external package lifecycle and file-backed/missing-package workflows without introducing a universal package or provider registry. Cross-provider replacement remains outside this ADR unless separately accepted. | 2026-10-18 @ contract evidence linked above; earlier `56264181873249c18d5477ce2170c8b1` |
| [0047](0047-editor-workspace-and-scene-document-state.md) | accepted | partial | Tooling, Scene authoring and Editor persistence owners | [workspace](../../../crates/nara_tooling/src/workspace.rs); [owned Save](../../../src/project_host/runtime/editor/persistence/background.rs); [source readmission](../../../src/project_host/persistence/reopen.rs); [runtime custody](../../../src/project_host/runtime/editor/persistence/retirement.rs) | [R5c Play-source qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T210921Z-trusted-play-source-continuity-and-source-local-close-qualification-61897d1b17744e2aaff6fc6efb1594a7.md); [R5c additive Open qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T202313Z-additive-scene-and-prefab-open-qualification-d0b1fc894dd2419fa1cf43c9e39eb79d.md); [R5c multi-source refresh qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T192721Z-multi-source-prefab-refresh-obligation-qualification-47f6266123094e7bb2ea046f42131843.md); [R5c source/preview qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T180456Z-common-source-admission-and-retained-scene-preview-qualification-45ffac37614a4edaa792f08d650f292e.md); [ordinary structure qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T051819Z-ordinary-structural-authoring-and-captured-source-reopen-qualification-f9b317e0ceda49a6b37f9000c61af8d7.md); [Host lifetime qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T030524Z-editor-host-lifetime-cleanup-custody-qualification-12c6b025d6c74ed1bf6900c8b0413877.md); [Owned Reopen](../../knowledge/engineering/verification/2026-10/2026-10-20T022302Z-owned-editor-reopen-preparation-and-publication-qualification-557ea95db99b4b8a87829870db1d6b13.md); [Owned Save](../../knowledge/engineering/verification/2026-10/2026-10-20T013307Z-owned-editor-save-custody-native-observation-and-measurements-9828019f2f934061a1773899ce356ffb.md); [R5c projection qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T225800Z-disposable-scene-projection-epochs-and-stable-native-preview-qualification-2478d897135549daacbc8d1fbc90b9d9.md); [direct-member and panel query qualification](../../knowledge/engineering/verification/2026-10/2026-10-26T004155Z-bounded-direct-member-discovery-and-panel-query-workflow-7003450c49204d8eba92ba90db765dc0.md); [source-owned creation qualification](../../knowledge/engineering/verification/2026-10/2026-10-24T203944Z-source-owned-entity-creation-and-generated-product-qualification-c5fd5454223046e38fc1761338ddffb0.md); [source conversion and lookup qualification](../../knowledge/engineering/verification/2026-10/2026-10-27T041718Z-complete-source-owned-prefab-conversion-and-current-stable-lookup-qualification-cbce190af0c4488b97895e68905573a6.md); [Editor conversion qualification](../../knowledge/engineering/verification/2026-10/2026-10-27T064545Z-complete-editor-prefab-conversion-and-source-admission-qualification-bc6d938e4ea5421fa42d0028230a11dc.md); [CP1/S2 qualification](../../knowledge/engineering/verification/2026-11/2026-11-01T203621Z-reviewed-authored-schema-upgrades-and-fresh-process-recovery-qualification-cf7c8ecab10c4e21b4cfcff4906805de.md) | Save captures checkpoint/basis and acknowledges only that revision; later edits remain dirty. Retained original file Hosts survive failed cleanup, abandonment and runtime replacement. Reopen keeps exact revision/source/schema admission and owner-native affinity. Exact observed or authoring-validated uncertain source bytes now restore captured topology after later unsaved moves; external bytes retain exact unavailable preservation. The 20-test persistence cohort and ordinary public/native workflows pass. R2 adds exact non-active direct-entity/component/root pages, bounded selected-root reads, one-level direct-member pages, owner-bound prepared publication, retained transaction-local construction, selection-independent Session/panel query forwarding and a shared close-admission fence. R5c retains typed file lanes by document and admission-time Hosts through Save/Reopen, uncertainty and exact cleanup; public same-kind Open is additive and retains distinct file Hosts. Common source admission now checks current retained targets across Scene/Prefab/extension lanes. Explicit Scene selection owns the retained preview binding; failed publication restores exact prior navigation without rebinding preview. Canonical Prefab-domain observations and source-keyed refresh obligations survive Close; complete successor validation preserves last-good on failure. Exact Play-source continuity now keeps unrelated Close source-local through cancellation, Game handoff and physical retirement/retry. Requested and accepted preview source/revision/omissions remain distinct across Scene epochs. Clear preserves shared authoring custody; physically held Image import and Save continue through repeated publications. Ordinary creation now uses the source result, with no Workspace ordinal, candidate-ID helper, success increment or Reopen reset; failed creation preserves selection and saved Reopen retains the actual stored ID. Authoring source validation now retains used stable-query observations and resolves a fresh bounded catalogue, detecting new metadata ambiguity. Source conversion tokens have separate qualification; contextual Editor custody remains open. The integrated Host now retains one reviewed conversion operation with actual file/working-copy participants, fresh post-review physical validation, correlated terminal results and physical cancellation/Close custody. Complete-set Schema upgrade adoption binds exact captured persistence predecessors, source/history checkpoints and lane observations before final Ready. | Public two-Scene/two-Prefab non-active editing, Save/Reopen and exact Close now pass; External merge and broader source mutation remain unqualified; Play-source continuity has its separate public/native evidence. Independent extension lanes are qualified separately by the 2026-10-22 workflow integration. No second saved document/manifest or relaxed owner publication is introduced. | 2026-11-02; `bf8e2d6`, `6da9980` and earlier scoped evidence retained |
| [0048](0048-runtime-diagnostics-and-observability-bus.md) | accepted | partial | `nara_diagnostic`, composition | [crates/nara_diagnostic/src/runtime.rs](../../../crates/nara_diagnostic/src/runtime.rs); [crates/nara_diagnostic/src/plugin.rs](../../../crates/nara_diagnostic/src/plugin.rs) | [crates/nara_diagnostic/src/contract_tests.rs](../../../crates/nara_diagnostic/src/contract_tests.rs); [crates/nara_asset_watch/src/tests.rs](../../../crates/nara_asset_watch/src/tests.rs); [scoped Host capture](../../../tests/fixtures/module-consumer/editor-panel-consumer/examples/diagnostic_capture/capture.rs) | The bounded privacy, dedupe, retention, pressure, and headless plugin core is present, and watcher loss bridges to structured diagnostics and pressure. A decision-local Host-composed tracing Layer now qualifies actual editor source-import/prepared-edit lifetimes and original desktop frame completion without installing a global subscriber or changing product custody; it is separate from the runtime bus. Asset, task, window, render, project, and general editor runtime producer bridges remain absent. ADR 0099 makes offline product evidence decision-local rather than part of this runtime bus contract. | A workflow requires a named runtime producer bridge; the scoped capture remains a concrete consumer rather than a reusable runtime collector. Offline product observations remain decision-local under ADR 0099. | 2026-11-14 @ Host-composed execution capture |
| [0049](0049-untrusted-project-input-and-parse-budget-policy.md) | accepted | partial | persistent format and concrete external-reader owners | [crates/nara_core/src/limits.rs](../../../crates/nara_core/src/limits.rs); [crates/nara_core/src/serde_shape.rs](../../../crates/nara_core/src/serde_shape.rs) | [crates/nara_reflect/tests/catalog_format.rs](../../../crates/nara_reflect/tests/catalog_format.rs); [crates/nara_scene/tests/format_contract.rs](../../../crates/nara_scene/tests/format_contract.rs) | Scene, prefab, patch, schema-catalog, project-manifest, canonical asset metadata, the audited static non-interlaced PNG path, and the aggregate authorized startup closure enforce their implemented byte, shape, domain, and publication budgets. Other codecs and persistent formats, time/journal budgets, and complete ADR 0048 bridges remain incomplete. ADR 0099 removed the unused generic offline evidence ingester. | Each newly admitted external reader or format proves its own byte, shape, path, and publication budget; no reader-independent evidence framework is required. | 2026-08-01 @ ADR 0099 refinement |
| [0050](0050-asset-root-symlink-junction-and-package-trust-policy.md) | accepted | partial | filesystem/platform adapters and file-backed domains | [crates/nara_fs/src/capability.rs](../../../crates/nara_fs/src/capability.rs); [crates/nara_fs/src/platform/windows.rs](../../../crates/nara_fs/src/platform/windows.rs) | [U8 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u8-unix-leaf-admission.md); [filesystem contract](../../../crates/nara_fs/tests/filesystem_contract.rs) | Manifest/content closure and bounded source indexing use host-issued root identity. Unix leaf opens use O_NONBLOCK plus existing no-follow controls, then reject non-regular types on the same handle. Linux FIFO/regular/symlink cases pass in both trust modes using original source via a temporary standalone manifest; this is not root locked-workspace Linux qualification. Windows locked crate coverage also passes. Remaining watcher/cache migrations and broader Host trust bindings remain incomplete. | Rename/persistence workflows admit remaining callers without duplicate platform authority; root locked Linux and other Unix qualification remain separate. | 2026-10-19 @ `eff95c2`; earlier `f341255` retained |
| [0051](0051-persistent-file-envelope-migration-and-golden-fixtures.md) | accepted | partial | persistent format owners | [Core envelope](../../../crates/nara_core/src/format.rs); [Scene format](../../../crates/nara_scene/src/format.rs); [Runtime Image framing](../../../crates/nara_image/src/runtime.rs) | [Core contract](../../../crates/nara_core/tests/format_contract.rs); [Scene contract](../../../crates/nara_scene/tests/format_contract.rs); [Image qualification](../../knowledge/engineering/verification/2026-10/2026-10-29T090832Z-anonymous-and-processed-image-content-with-retained-native-publication-qualification-3e529af056d34a5caa2c12924c0c1014.md) | Canonical source/Schema/asset metadata and processed Scene/catalog kinds retain their owning evidence. Runtime Image v1 uses the same kind/version/engine/generator admission in bounded metadata plus exact raw RGBA; a nonempty binary golden and malformed/unknown/duplicate/positional/length cases pass. No compatibility reader is introduced. | Other remaining formats retain their own concrete workflow triggers. Binary content need not expand into JSON arrays or Base64; source metadata and Runtime pixels retain separate format owners. | 2026-10-29 @ compact Runtime Image qualification; prior envelope scopes retained |
| [0052](0052-task-backpressure-cancellation-and-long-running-diagnostics.md) | accepted | implemented | `nara_tasks`, task-consuming domains | [task runtime](../../../crates/nara_tasks/src/runtime.rs); [asset integration](../../../crates/nara_asset/src/reload.rs) | [cancellation and measurement evidence](../../knowledge/engineering/verification/2026-10/2026-10-19T182252Z-editor-responsiveness-measurements-and-standalone-task-cancellation-0e2521d0010a4d94940d83fa865f3d0c.md); [execution](../../../crates/nara_tasks/src/tests/execution.rs); [close](../../../crates/nara_tasks/src/tests/close.rs); [U2 return evidence](../../knowledge/engineering/verification/2026-10/2026-10-22T181737Z-u2-task-return-and-physical-running-accounting-ce30f14820ee4350931de3355bb0c955.md) | Per-task return observes executor-owned capture and late-result disposal independently of terminal/output lifetime; caught unwinds report CleanupFailed. Physical running count/age remains until final worker-owned disposal. Scene/Browser/Open replace local flags with this observation. Tasks 51 and affected root 24 tests pass, with gated reaper observation separately passing. Caller-thread rejected/coalesced capture or external terminal-result disposal can still block; no nonblocking destructor guarantee is claimed. | G9/O2: an actual measured disposal-latency consumer can justify an owner-policy change; a future Future consumer needs its own execution/cancellation contract. | 2026-10-23 @ U2 return evidence; prior execution evidence retained |
| [0053](0053-visibility-culling-and-tilemap-render-cache.md) | superseded | superseded | Historical render scaling topology | - | - | Replaced by ADR 0096. | - | 2026-07-20 @ ADR 0096 |
| [0054](0054-gpu-upload-budget-and-buffer-allocation-policy.md) | accepted | partial | render backend | `crates/nara_render_wgpu/src/budget.rs`; `crates/nara_render_wgpu/src/backend.rs` | [U4 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u4-instance-budget.md) | Finite cumulative instance reservations cover each Runtime frame and Editor Host transaction, with typed rejection and RAII rollback/commit. The 122-test WGPU cohort, mixed Sprite/UI readback, two native Editor targets and old-completion/new-scope regression pass. Broader upload statistics, reusable staging and measured resource-class overload policies remain open. | A reproducible workload exceeds a measured threshold, justifying the next private mechanism and resource-class-specific overload policy under ADR 0096. | 2026-10-19 @ `964d25c`; initial `0b078a0` |
| [0055](0055-feature-matrix-boundary-checks-and-compatibility-fixtures.md) | accepted | partial | repository CI | [Cargo.toml](../../../Cargo.toml); [.github/workflows/ci.yml](../../../.github/workflows/ci.yml) | [tests/product_capabilities.rs](../../../tests/product_capabilities.rs); [tests/ci_policy.rs](../../../tests/ci_policy.rs) | Local dependency/feature checks and locked root/reference-game/module-consumer workflows compile all targets and run supported complete suites without a custom documentation parser. The recorded six-cell Windows/Linux hosted matrix passed at f098876. Candidate policy retains product/security semantics; invalid refs/repeated attempts fail explicitly. Broader profiles remain unqualified. | Broader capability profiles remain evidence-triggered. | 2026-08-04 @ authority simplification |
| [0056](0056-headless-runtime-and-dedicated-server-readiness.md) | accepted | partial | facade/runtime composition | [src/product.rs](../../../src/product.rs); [examples/headless_server.rs](../../../examples/headless_server.rs) | [src/lib.rs](../../../src/lib.rs); [crates/nara_gameplay/src/lib.rs](../../../crates/nara_gameplay/src/lib.rs) | Bounded threaded work, preserve-debt time, headless diagnostics/pressure, tick-aware commands, compiled ceilings, server isolation and pre-mutation closure are implemented. Sticky faults and Host-owned startup/publication/retirement drive a deterministic movement/combat/wave and stable CLI. Metrics bridges, hosted artifact proof, networking and dedicated-server delivery remain incomplete in this recorded scope. | A delivery or desktop-parity consumer proves the missing scope without reopening lifecycle ownership. | 2026-07-19 @ db511a7 |
| [0057](0057-authoritative-fixed-tick-and-command-ingress.md) | accepted | implemented | `nara_gameplay` | [command values](../../../crates/nara_gameplay/src/command.rs); [queue](../../../crates/nara_gameplay/src/queue.rs); [seat mappings](../../../crates/nara_gameplay/src/action.rs) | [U10 execution evidence](../../knowledge/engineering/verification/2026-10/2026-10-22T193136Z-u10-exact-gameplay-batch-success-and-reporter-identity-380c336814b2450abe14023d43cf6253.md); [seat/replay unit](../../plans/2026-11-13-seat-gamepad-and-input-replay-plan.md); [public Runtime](../../../tests/runtime_instance.rs) | Exact generation/tick/reporter completion continues to gate Capture/Ack and quarantine failure. Static bindings include seat. A fresh immutable Replay-only queue refuses competing streams and local source mutation; the finite consumer imports complete canonical recorded order with original provenance retained as data. Owning245 and independent15 pass with per-tick fresh-runtime state, Stop and budget controls. | Crash transactions, universal persistent replay/rollback and panic containment remain outside this contract. Replay ingress does not sandbox trusted native effects. | 2026-11-13 @ seat/replay unit; U10 execution evidence retained |
| [0058](0058-stable-runtime-identity-and-entity-references.md) | accepted | partial | `nara_identity`, `nara_scene`, identity consumers | [domain.rs](../../../crates/nara_identity/src/domain.rs); [export.rs](../../../crates/nara_scene/src/export.rs); [epoch proof](../../../crates/nara_identity/src/domain/scene_projection_epoch.rs); [projection owner](../../../crates/nara_scene/src/spawn/projection.rs) | [identity tests](../../../crates/nara_identity/src/tests.rs); [durable_references.rs](../../../crates/nara_reflect/tests/durable_references.rs); [durable_identity.rs](../../../tests/fixtures/module-consumer/tests/durable_identity.rs); [U6/U8 evidence](../../knowledge/engineering/verification/2026-10/2026-10-22T191516Z-u6-typed-codec-and-u8-identity-consumer-qualification-0c76d56bc4b040118283f1024fe2b788.md); [R5c projection qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T225800Z-disposable-scene-projection-epochs-and-stable-native-preview-qualification-2478d897135549daacbc8d1fbc90b9d9.md); [public instance-end qualification](../../knowledge/engineering/verification/2026-10/2026-10-24T031111Z-public-exact-scene-instance-end-and-source-context-design-review-a91caf241959454f8674b9b04eabc141.md); [source/occurrence qualification](../../knowledge/engineering/verification/2026-10/2026-10-24T114901Z-required-scene-sources-and-active-occurrence-ownership-qualification-e249da9a1f404677a72e52457ac990c4.md) | Durable logical records and checked runtime incarnations are separate. Unloaded may reactivate without historical growth; Deleted is terminal. A separately compiled nested explicit/default and required/optional consumer preserves durable values across source publication, spawn, Prefab and patches, while live operations reject unloaded/deleted or old-incarnation targets. Export remains strict and distinguishes residency outcomes. First-target adoption admits Add/Insert observers before marker mutation. Exact retirement prepares through shared domain reads; ordinary World functions own borrowing and short index publication. Identity/Scene 311 and external durable/manual 10 tests pass. Complete-domain disposable Scene epochs prove all predecessor history, logical records and live markers before preparation and final admission. Foreign responsibility rejects; fresh domains retain N+1 claims without continuing-domain eviction or Deleted readmission. The public exact Scene-instance end operation now covers empty and individually emptied activations, stale/foreign receipt refusal and lifecycle admission through the same authoring retirement path. Scene serde287, the independent parse/spawn/end consumer and three rustdoc tests pass; Scene now owns active source/occurrence association in the same exact publication/end cut. Empty instances retain occupancy, limits reject without eviction, and final registration revalidates the exact domain and allocator baseline. | L02/A03 and E05 are corrected and independently reviewed. Independent history budgets and future restore/streaming proofs retain their existing scope. | 2026-10-24 @ active occurrence qualification; public instance-end qualification; projection `207bbcc`; earlier scoped evidence retained |
| [0068](0068-global-resource-budgets-metrics-and-diagnostic-privacy.md) | accepted | partial | `nara_core`, `nara_diagnostic`, runtime producers | [limits](../../../crates/nara_core/src/limits.rs); [diagnostic identities](../../../crates/nara_diagnostic/src/identity.rs); [App source-fact validation](../../../crates/nara_app/src/failure_metadata.rs); [plugin lowering](../../../src/project_host/runtime_diagnostics/plugin.rs); [scoped Host capture](../../../tests/fixtures/module-consumer/editor-panel-consumer/examples/diagnostic_capture/capture.rs) | [N05 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19T170951Z-n05-safe-plugin-failure-facts-and-invocation-context-d01f7391b84348a8827a0daa760dd1a4.md); [diagnostic contracts](../../../crates/nara_diagnostic/src/contract_tests.rs); [recipe privacy tests](../../../tests/project_runtime_boot.rs); [capture tests](../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/diagnostic_capture.rs) | Shared scalar units, bounded runtime privacy and pressure primitives, asset ingress/request ceilings, sticky bounded observation recovery and image-domain RAII admission metrics remain present. N05 shares checked static source-fact validation between App plugin errors and RuntimeFaultDetail without a diagnostic dependency. Product bridges preserve domain code/summary and bounded invocation context while legacy dynamic text, private paths and forged owner claims remain absent from DiagnosticReport Debug/JSON. The decision-local Host capture now enforces finite Vec payload bounds, exact static metadata/outcome/numeric whitelists, explicit drop/fault/incomplete reports and post-cleanup export. It does not add a global budget or make a hard real-time claim. The U9 oracle stays decision-local. Image faults and other named producers still lack complete ADR 0048 bridges. | A named producer needs a runtime bridge; each diagnostic capture needs its concrete product consumer and limits. Product observations remain decision-local under ADR 0099. | 2026-11-14 @ Host-composed execution capture |
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
| [0092](0092-sdr-color-space-alpha-and-output-encoding.md) | accepted | partial | `nara_core`, `nara_image`, UI/material/render domains, render backends | [core](../../../crates/nara_core/src/lib.rs); [Image](../../../crates/nara_image/src/asset.rs); [stock policy](../../../crates/nara_render_wgpu/src/stock_policy.rs); [UI authoring color](../../../crates/nara_ui_kernel/src/style.rs) | [Image tests](../../../crates/nara_image/src/tests.rs); [stock alpha](../../../crates/nara_render_wgpu/src/host/program_tests/alpha.rs); [content/color qualification](../../research/2026-11-11-open-gpui-engine-integration-layers.md#implemented-source-and-cooked-image-color-composition); [UI byte conversions](../../../crates/nara_ui_kernel/tests/color.rs) | Linear authored values, straight sources, accumulated premultiplied clears/Blend/Opaque and exact output descriptors have scoped owning GPU evidence. Actual Source/Cooked GPUI composition selects explicit linear-sRGB recording into the engine's sRGB target and passes midtone/alpha/overlay/clip/retirement on DX12 and Vulkan. | Complete authoring/codec validation, unsupported-surface behavior, full palette/primitive/precision breadth, physical display/input and HDR/gamut/scene-to-display policies remain separate. This SDR prerequisite neither proves Dear gamma parity nor closes R11 or maintenance handoff. | 2026-11-12 selected SDR/alpha/native consumer; original audit retained |
| [0093](0093-rust-authoring-hot-iteration-and-optional-scripting-adapters.md) | accepted | partial | Rust build/runtime hosts, Editor Host and optional adapter packages | [crates/nara_asset/src/reload.rs](../../../crates/nara_asset/src/reload.rs); [crates/nara_tooling/src/play.rs](../../../crates/nara_tooling/src/play.rs) | [crates/nara_asset/src/lib.rs](../../../crates/nara_asset/src/lib.rs); [tests/scene_play_mode.rs](../../../tests/scene_play_mode.rs) | Domain data reload and rebuild-oriented Play foundations exist. Editor extension edit/build/reconstruct and owner/UI recovery are unimplemented R11f targets, separate from game/Play restoration. Broader change classification, executable activation and optional hot-patch scope retain their own evidence. | Qualify changed Editor code, dirty-content/history/draft compatibility, safe refusal/last-good recovery and correct-presentation latency. Named optional-language/installable-tool comparisons and Rust hot-patching retain their separate consumer/lifetime/target requirements. | 2026-11-04 Editor iteration design refinement; earlier bounded evidence retained |
| [0094](0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md) | accepted | partial | `nara_render`, native Feature packages and WGPU Host | [programs](../../../crates/nara_render_wgpu/src/program/mod.rs); [policy bindings](../../../crates/nara_render_wgpu/src/policy.rs); [owned candidates](../../../crates/nara_render_wgpu/src/candidate.rs); [history](../../../crates/nara_render_wgpu/src/history.rs); [Host](../../../crates/nara_render_wgpu/src/host.rs) | [independent program](../../../tests/fixtures/render-program-consumer/README.md); [independent policy/history](../../../tests/fixtures/render-policy-consumer/README.md); [typed-policy qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T192756Z-original-typed-render-policy-capture-and-complete-consumer-qualification-df7f9377f51142b78060b53617c800a1.md); [history qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T210034Z-captured-view-histories-and-original-submission-cleanup-production-qualification-5c061376803446c0bff1a95495ca55c7.md) | Typed texture/buffer programs, original policy captures, alternate scalar rendering and stock Runtime/Editor/image consumers share one Host. Move-owned candidates pin complete history bases, retain actual Feature owners and publish coherent slots/watermarks. Independent four-chain numerics, a Runtime window, reset/refusal/loss and cleanup evidence qualify the selected history transaction. Central payloads and combined facades are removed. | Complete source/processed interleaving, independently paced two-window/image and Nara-drawn composition remain required before full0121 acceptance. General scheduling, external native imports and dynamic unload remain separate. | 2026-11-01 @ captured View/history successor; earlier scoped evidence retained |
| [0095](0095-plugin-owned-specialized-domains-and-project-configuration.md) | accepted | partial | Specialized plugins, root product composition, schema-owning packages | [plugin composition](../../../crates/nara_app/src/plugin.rs); [concrete collision owner](../../../tests/fixtures/motion-execution/physics/src/authored.rs); [static Rapier package](../../../crates/nara_rapier2d/README.md); [game-owned collision](../../../samples/two-level-game/src/gameplay/collision.rs) | [plugin controls](../../../crates/nara_app/tests/plugin_composition.rs); [transformed collision qualification](../../plans/2026-11-04-authored-collision-derivation-plan.md#fixed-spatial-inputs-and-transformed-collision-successor); [actual-game qualification](../../plans/2026-11-04-authored-collision-derivation-plan.md#source-and-native-character-qualification) | Plugin-owned ECS/systems, conflicts and owner-local schema registration remain implemented. ProductRecipe and typed schema contributions share repeatable direct/file-backed configuration. The independent dynamic Rapier owner retains exact bindings, original fixed-input geometry, writeback and source/history qualification. A directly consumed immutable static-query package now shares affine geometry and native sweep/slide with the actual game. Owner16, reused motion42, ordinary game326, original native reconstruction and relocated physical desktop delivery pass. Game time, TRS and persistence stay with the game; movement does not rescan the Scene. No official Physics backend is selected. | OQ-045 retains independently versioned package lifecycle, file-backed settings and degraded-authoring work. General dynamics, moving platforms, native-history restoration, foreign execution and numeric cost budgets require their selected workflows; no second solver is required without a portable cross-provider Interface. | 2026-11-18 @ static character Runtime/delivery qualification; prior recipe/dynamic evidence retained |
| [0096](0096-evidence-gated-render-scaling-and-upload-policy.md) | accepted | partial | `nara_render`, render domains, `nara_render_wgpu` | [crates/nara_render/src/lib.rs](../../../crates/nara_render/src/lib.rs); [crates/nara_render_wgpu/src/backend.rs](../../../crates/nara_render_wgpu/src/backend.rs) | [U2/U3 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u2-u3-recovery-and-device-admission.md); [U4 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u4-instance-budget.md) | Closed render ownership, device invalidation, Sprite/UI encoding and resource-complete image/glyph packets are implemented. Live-device texture admission distinguishes content rejection from retryable allocation failure; cumulative instance reservations cover Runtime/Editor scope demand. Target-plan primitive/resource/atlas/retention budgets remain. Measured scaling thresholds and generalized chunking/ring/staging mechanisms remain unimplemented. | A reproducible measured workload, rather than a general scaling roadmap, justifies the next private mechanism. | 2026-10-19 @ `ebbe005`, `964d25c`; scoped U3/U4 evidence |
| [0097](0097-future-capable-2d-3d-spatial-transform-model.md) | accepted | partial | `nara_transform`, scene/schema, render, import, tooling, and physics domains | [crates/nara_transform/src/lib.rs](../../../crates/nara_transform/src/lib.rs); [crates/nara_transform/src/propagation.rs](../../../crates/nara_transform/src/propagation.rs) | [crates/nara_transform/src/lib.rs](../../../crates/nara_transform/src/lib.rs); [transformed collision qualification](../../plans/2026-11-04-authored-collision-derivation-plan.md#fixed-spatial-inputs-and-transformed-collision-successor); [U4 spatial qualification](../../knowledge/engineering/verification/2026-10/2026-10-22T194451Z-u4-immutable-spatial-cuts-and-exact-hierarchy-publication-4023754775ba445a887c9056715584e0.md) | Authored 2D TRS and phase spatial cuts replace Global Components; direct non-spatial parents terminate inheritance. U4 covers finite publication, exact World association, source edits and interpolation. The bounded Rapier consumer qualifies full-affine fixed boxes and root/unit-scale dynamic writeback under Serial/Parallel, with actual contacts and source/history persistence. Future local-affine/3D fields and KeepWorld normalization remain [Proposed candidates](../spatial-affine-representation-design.md); parented/scaled dynamic poses remain refused. | Preserve the supported 2D snapshot contract. A real non-TRS import or world-preserving edit compares representation, precision, Inspector and undo; a focused 3D consumer admits its fields. Explicit source/schema migration is allowed. | 2026-11-15 @ bounded transformed collision; prior U4 evidence retained |
| [0098](0098-schema-owner-lineage-and-active-runtime-composition.md) | accepted | implemented | `nara_reflect`, schema-owning plugins, and product composition | [provider](../../../crates/nara_reflect/src/provider.rs); [Scene selection](../../../crates/nara_scene/src/component_bindings.rs); [recipe](../../../src/product.rs); [composition](../../../src/project_host/composition.rs) | [registry contract](../../../crates/nara_reflect/tests/registry_contract.rs); [product consumer](../../../tests/product_data_components.rs); [selection evidence](../../knowledge/engineering/verification/2026-10/2026-10-27T141025Z-mixed-scene-product-selection-and-content-to-runtime-admission-qualification-7c90588821aa4e8da374958ef2e436e4.md) | Owner-local lineage remains separate from composition. Explicit native/data selection has complete semantic identity, exact owner admissions and metadata-only content witnesses; generated recipe identity includes lineage facts. | Language execution/publication remains Proposed 0120. | 2026-10-27 @ product-selection evidence; original 9e3ae84 retained |
| [0099](0099-decision-local-product-evidence-and-publication-admission.md) | accepted | implemented | Product measurement owners and release operations | [.github/workflows/reference-game-candidate.yml](../../../.github/workflows/reference-game-candidate.yml); [tests/measurement_policy.rs](../../../tests/measurement_policy.rs) | [tests/measurement_policy.rs](../../../tests/measurement_policy.rs); [tests/ci_policy.rs](../../../tests/ci_policy.rs) | - | - | 2026-08-01 @ 74b15a3 |
| [0100](0100-runtime-structural-hierarchy-and-completed-2d-transform-projection.md) | accepted | partial | Runtime hierarchy, `nara_transform`, scene materialization, runtime UI, snapshots, render extraction, and reference-game product proof | [crates/nara_hierarchy/src/relation.rs](../../../crates/nara_hierarchy/src/relation.rs); [crates/nara_transform/src/snapshot.rs](../../../crates/nara_transform/src/snapshot.rs); [crates/nara_hierarchy/src/lib.rs](../../../crates/nara_hierarchy/src/lib.rs) | [fixed-input qualification](../../plans/2026-11-04-authored-collision-derivation-plan.md#initial-shared-qualification); [U4 spatial qualification](../../knowledge/engineering/verification/2026-10/2026-10-22T194451Z-u4-immutable-spatial-cuts-and-exact-hierarchy-publication-4023754775ba445a887c9056715584e0.md); [scale comparison](../../benchmarks/2026-10-23-low-level-spatial-cuts.md) | One non-linked runtime relation owns ordered projection. Historical/Fixed/Presented cuts retain World/runtime, full Entity, role/time and captured order. Opt-in FixedInput completes Prepare's sources before Simulate through the same validators without replacing other cuts or adding work to unselected worlds. Shared86, default/Parallel motion40/46 and separate actual hardware/software history controls pass. Captured sources are read facts; native writeback must revalidate live targets. Prior U4/U11 native/product evidence, writer sparse baselines and Scene no-flush windows retain their original scope. Visibility, linked lifetime and 3D remain unaccepted. | The extra opt-in source scan/cache has no numeric performance claim. Prepared batches retain their no-flush/no-hierarchy-mutation window. Full R4c/R10 and native Linux/macOS qualification remain open. | 2026-11-15 @ fixed-input and concrete collision qualification; prior U4/U11 evidence retained |
| [0101](0101-ordered-persistent-placement-and-runtime-projection.md) | accepted | partial | Persistent Scene/Prefab/Patch documents, runtime hierarchy, authoring tools, and UI projection | [crates/nara_scene/src/document.rs](../../../crates/nara_scene/src/document.rs); [crates/nara_scene/src/prefab.rs](../../../crates/nara_scene/src/prefab.rs); [crates/nara_hierarchy/src/writer.rs](../../../crates/nara_hierarchy/src/writer.rs) | [crates/nara_scene/src/tests.rs](../../../crates/nara_scene/src/tests.rs); [crates/nara_scene/src/product_transaction_tests.rs](../../../crates/nara_scene/src/product_transaction_tests.rs); [crates/nara_hierarchy/src/tests.rs](../../../crates/nara_hierarchy/src/tests.rs) | Canonical ordered roots/children support exact invertible document/tooling mutations. Spawn, nested Prefabs, export, replace, Retry, retirement and UI consume the same completed order and structured provenance; runtime-only hierarchy roots retire atomically with Scene replacement. Runtime writer batches now use a private sparse candidate/baseline path while preserving full private Scene validation. | Future Editor UI and other consumers must preserve the completed hierarchy and structured provenance without introducing a UI-local order or parsed identity authority. Prepared batches retain their owner-controlled no-flush/no-hierarchy-mutation requirement. | 2026-10-18 @ F11 trusted-generation evidence |
| [0102](0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md) | accepted | partial | Text, UI kernel/widgets, Runtime/Editor adapters, input and presentation | [compiler](../../../crates/nara_ui_kernel/src/compiler.rs); [field](../../../crates/nara_ui_widgets/src/text_field.rs); [UI Runtime](../../../crates/nara_ui/src/runtime.rs) | [U9 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u9-text-content-box.md); [complete-key successor](../../knowledge/engineering/verification/2026-10/2026-10-24T185535Z-complete-persistent-ui-identity-and-neutral-conformance-successor-0cf84bdfd2f54766be84f3ca206f7931.md); [typed Image qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T015832Z-canonical-source-locations-and-typed-ui-image-binding-qualification-0ce5bafdc65f4893aa0079009668e41a.md); [completion/candidate qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T140508Z-bounded-input-ownership-and-total-ui-presentation-qualification-b97643044e4a4f92b892a3539d2ad415.md); [original-result/close qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T082117Z-original-ui-result-custody-and-logical-close-qualification-6e1965c6428841ec9138088bef20f90c.md); [text guide](../../guides/editable-text.md) | Immutable frame/session/packet authority, complete keys, typed Images and total original-result/close custody retain their prior qualification. The bounded text successor adds exact finite fallback faces, local editable buffers, field-incarnation/focus tokens, shared clipped drawing/pointer/candidate geometry and committed TextInput semantics. Owning, independent and actual GPU consumers qualify this prerequisite separately from a complete Editor panel. | Complete Editor toolkit, native editable-text/IME/accessibility workflows, virtualization, render output/mirror completion and Linux/macOS remain separate. Preserve UI rejection without Scene rollback; bounded primitives do not imply whole-toolkit parity. | 2026-11-04 @ bounded text prerequisite; `8e9d840` and earlier scopes retained |
| [0103](0103-concrete-domain-owned-asset-import-execution.md) | accepted | implemented | `nara_asset`, concrete domains and project/build Hosts | [source/import](../../../crates/nara_asset/src/import.rs); [optional build facet](../../../crates/nara_asset/src/build.rs); [live typed adapter](../../../src/asset_build/source_reload.rs); [finite executing Host](../../../src/asset_build.rs); [Editor facet](../../../src/project_host/runtime/editor/source_asset_import.rs) | [original extraction](../../knowledge/engineering/verification/2026-10/2026-10-28T233934Z-source-import-contracts-below-project-hosts-with-typed-read-errors-e617bfdd9a104fb69bdf3a20936f7d57.md); [local-build qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T231610Z-tracked-local-artifact-builds-and-complete-group-publication-qualification-d3fcd08e15e04d45812ae09d7c81e944.md); [live refresh](../../plans/2026-10-31-local-artifact-build-plan.md#dependent-refresh-implementation-and-review) | Pure source/build facets stay below root/Scene/2D. Root owns actual capabilities, budgets and complete captured groups. The live typed consumer uses original Asset/Tasks/value owners, exact request issuer and value/state guards, complete raw subscriptions and separate failure retry observations. Metadata raw provenance survives coalescing; escaped decoded and Runtime content keep original custody. Concrete Image recovery remains domain-owned. No erased executor, parallel value dictionary or global budget manager is introduced. | General processed-input scheduling, immutable Scene/Runtime-base refresh, streaming and broader native/platform qualification remain separate. | 2026-11-13 @ live source-build refresh; prior source/Editor/local-build evidence retains its scope |
| [0104](0104-host-owned-editor-workspace-installation.md) | accepted | implemented | Process Hosts, `nara_tooling`, Editor sessions, and UI adapters | [crates/nara_tooling/src/workspace.rs](../../../crates/nara_tooling/src/workspace.rs); [src/project_host/runtime/editor.rs](../../../src/project_host/runtime/editor.rs) | [crates/nara_project/src/tests.rs](../../../crates/nara_project/src/tests.rs); [crates/nara_tooling/src/workspace.rs](../../../crates/nara_tooling/src/workspace.rs) | - | - | 2026-08-04 @ `92ee7f9` |
| [0105](0105-closed-first-party-render-plan.md) | accepted | partial | `nara_render`, first-party render domains, UI packet lowering, and WGPU | [crates/nara_render/src/lib.rs](../../../crates/nara_render/src/lib.rs); [crates/nara_sprite_render/src/types.rs](../../../crates/nara_sprite_render/src/types.rs); [tests/project_runtime_boot.rs](../../../tests/project_runtime_boot.rs) | [crates/nara_sprite_render/src/tests.rs](../../../crates/nara_sprite_render/src/tests.rs); [crates/nara_ui_render/src/packet_tests.rs](../../../crates/nara_ui_render/src/packet_tests.rs); [tests/project_runtime_boot.rs](../../../tests/project_runtime_boot.rs) | Sprite/Tilemap share per-view layer/sort-key order, optional foot-Y and stable ties; only adjacent compatible items batch. UI keeps overlay/painter order. Immutable resource-complete packets reach WGPU without live World/asset lookup, with exclusive targets, bounded views, offscreen sequencing and independent Editor views. Saved file-backed TileSets now reach textured static render batches with verified Image pixels. Multipart sorting groups and a general graph remain unimplemented. | Use a saved/reopened real map and measured cost to admit only necessary sorting/cache/culling work. Broader execution authority still requires a concrete consumer and focused decision. | 2026-10-18 @ [Tilemap slice](../../plans/2026-10-18-tilemap-file-backed-foundation-plan.md); 2026-10-17 @ foundation `ca3d803`, selected Editor `f4ffdd18` |
| [0106](0106-content-and-presentation-product-composition.md) | accepted | partial | Root product composition, project capabilities, reference-game Hosts, and presentation adapters | [Cargo.toml](../../../Cargo.toml); [src/product.rs](../../../src/product.rs) | [crates/nara_project/src/tests.rs](../../../crates/nara_project/src/tests.rs); [tests/product_capabilities.rs](../../../tests/product_capabilities.rs) | 2D/UI content and presentation are separate capabilities. Explicit WGPU/Winit contributions preserve base-plugin behavior under unrelated compile features; ProductRecipe owns ordinary file-backed Host composition. | The bounded Runtime UI product evidence is published. External backend replacement, dynamic contribution discovery and generic package distribution remain outside this ADR. | 2026-08-13 @ `56264181873249c18d5477ce2170c8b1` |
| [0107](0107-shared-ui-foundation-and-independent-product-layers.md) | accepted | partial | UI Foundation, Runtime UI, Editor Domain, Editor Host and toolkit adapters | [kernel](../../../crates/nara_ui_kernel/src/compiler.rs); [semantic panels](../../../editor/src/panels/host.rs); [native attachment](../../../editor/src/native/images.rs) | [runtime adapter](../../../crates/nara_ui/src/adapter_tests.rs); [independent UI Host](../../../tests/fixtures/editor-ui-host-consumer/README.md); [measured-property unit](../../plans/2026-11-05-measured-property-composition-plan.md) | Independent layers, neutral presentation, scoped state, custom layout/geometry and public semantic/image-backed panels retain their bounded evidence. Exact-width child measurement and immutable field/viewport descriptions support reusable property/icon composition through original text/layout/input/GPU owners. The linked unit records actual independent and Editor qualification. | Complete independent Editor, broader component/effects lifecycle, typography handoff, R11a/e invalidation/collection budgets, remaining UI operations and extension reconstruction remain open. Cached-child dependency membership and native OS input/accessibility need their own evidence; toolkit convergence is optional. | 2026-11-05 allocation/property follow-up; earlier bounded evidence retained |
| [0108](0108-ui-surface-presented-session-and-atomic-interaction.md) | accepted | implemented | UI session/interaction, Input, Winit accessibility and Host Surface lifecycle | [interaction session](../../../crates/nara_ui_kernel/src/interaction/session.rs); [Surface state](../../../crates/nara_ui/src/runtime/state.rs); [paint lowering](../../../crates/nara_ui_render/src/planner/lowering.rs) | [interaction tests](../../../crates/nara_ui_kernel/tests/interaction.rs); [adapter tests](../../../crates/nara_ui/src/adapter_tests.rs); [paint boundary qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T044452Z-module-goal-alignment-and-concrete-boundary-repairs-qualification-f7676d2222234fd39b8b565dbcaf996f.md); [completion/claim qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T140508Z-bounded-input-ownership-and-total-ui-presentation-qualification-b97643044e4a4f92b892a3539d2ad415.md) | Paint baselines precede clipping and preserve half-open mapping in the original35-test scope. Prepared completion now checks Input invalidation and the live UI basis before publication; C1 consumes old-basis candidates. Original Input claims survive suspension/geometry and protect same-label replacements. Restore clears predecessor interaction only for an actual lifetime handoff. Late completion preserves current effects and backend pixel facts without false interactive publication. | These bounded lifecycle/paint scopes do not qualify full editable text, OS IME delivery, native accessibility parity, font fallback or monitor scanout. | 2026-10-30 @ total completion/endpoint claims; `40d3f87` / `8582427` scopes retained |
| [0109](0109-root-local-ui-compilation-and-presentation-closure.md) | accepted | partial | UI authoring/compiler, text limits, UI presentation planner and WGPU | [compiler](../../../crates/nara_ui_kernel/src/compiler.rs); [scoped layout](../../../crates/nara_ui_kernel/src/layout.rs) | [custom-layout tests](../../../crates/nara_ui_kernel/tests/custom_layout.rs); [exact-width tests](../../../crates/nara_ui_kernel/tests/allocated_measurement.rs); [property unit](../../plans/2026-11-05-measured-property-composition-plan.md) | Root failure/presentation closure and scoped measurement remain authoritative. Plain-text/layout-width retention keeps independent final-output admission. Exact-width exploratory measurement preserves known-dimension placement, root poison and work charging; immutable field/viewport identity preserves compatible frame reuse. | Full R11a/R11e budgets, cross-frame incremental layout/paint, retained GPU updates and heap-byte measurements remain open. Last-good visuals and bounded request caches do not qualify them. | 2026-11-05 exact allocation and bounded reuse; earlier root/presentation scope retained |
| [0110](0110-first-party-project-workflow-and-source-carrying-export.md) | accepted | implemented | Root project workflow, `nara_project`, `nara_fs`, project content and Runtime Hosts | [starter](../../../src/project_workflow/starter.rs); [selected delivery owner](../../../src/project_content/delivery.rs); [export](../../../src/project_tooling/export.rs); [shared Cook](../../../src/project_tooling/project_cook.rs) | [creation](../../../tests/project_workflow_creation.rs); [delivery](../../../tests/project_content_delivery.rs); [real compiled CLI](../../../tests/project_workflow_cli.rs); [export source matching](../../../src/project_workflow/export/source_tests.rs); [compiled domain execution](../../verification/2026-10-21-compiled-asset-domain-export.md); [source authority verification](../../knowledge/engineering/verification/2026-10/2026-10-24T065424Z-project-source-authority-carrier-and-creation-identity-qualification-6cdfe4a580b14b8da78e659010497632.md); [generated-game Cook](../../../tests/project_cook.rs) | Capability-owned creation, exact selected Scene closures, finite declared delivery, presentation-free headless execution, desktop startup verification, Cargo artifact selection, source-carrying Export and startup repair are implemented. The compiled CLI builds an external domain into both release targets, checks two real product observations and rejects missing auxiliary content before gameplay. Startup executable agreements remain distinct. Canonical protocol 1 and its [structured-failure qualification](../../knowledge/engineering/verification/2026-10/2026-10-25T100841Z-compiled-domain-failure-projection-and-truthful-publication-observations-8b995496a82b411184e95f6f462b48f8.md) retain their scoped evidence. Creation preserves prepared source universes and startup metadata, including published tamper retention. Shared compiled Cook now consumes original sequential source snapshots and game build facets; the starter publishes Cook/Player and a build script. The actual generated three-Scene/three-Image game and all three source-free Two-Level package placements pass through the original assembly, Cargo and publication owners. | Full backups, automatic rebinding, hermetic/reproducible build closure and foreign-target execution remain separate. Source Export observations do not establish full native presentation/input. Current-level activation, residency retirement and game saves follow the persistent-game plan; finite cooked evidence does not complete R7/R12. | 2026-11-19 @ shared compiled Cook and generated source-free game; earlier scoped evidence retained |
| [0111](0111-editor-scene-view-execution-and-picking-authority.md) | accepted | implemented | Native Editor Host, `nara_render`, and `nara_render_wgpu` | [render packets](../../../crates/nara_render/src/lib.rs); [native execution](../../../crates/nara_render_wgpu/src/host.rs) | [immutable-output qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T121248Z-immutable-render-outputs-owning-producers-and-original-native-prefix-qualification-1b4de7f4305740698abba87b589f9459.md); [earlier projection qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T225800Z-disposable-scene-projection-epochs-and-stable-native-preview-qualification-2478d897135549daacbc8d1fbc90b9d9.md) | Configuration, exact physical outputs, display occurrences and revision-bound picking are separate production owners. Actual retained A1/A2 pixels, registrations, Scene Reimport, no-window public use, producer-local close, original input through loss and native Scene/Canvas source/mapping authority pass. Stable configuration no longer aliases future pixels. | Graph/Feature and broader transfer/color/history support remain under the current R8/R9 unit and Proposed0121. Native evidence is Windows/DX12; browser and other platform execution are not implied. | 2026-10-31 @ immutable-output qualification |
| [0112](0112-editor-game-view-runtime-presentation-and-input-authority.md) | accepted | implemented | Native Editor Host, executable Runtime owner, Runtime UI presentation, `nara_render`, and `nara_render_wgpu` | [Game bridge](../../../src/project_host/runtime/editor/game_view.rs); [native consumer](../../../editor/src/platform/imgui.rs) | [designated-display qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T121248Z-immutable-render-outputs-owning-producers-and-original-native-prefix-qualification-1b4de7f4305740698abba87b589f9459.md); [Runtime tests](../../../tests/workspace_play_runtime.rs) | Borrowed final target activation preserves coordinated unpublished Runtime start. Actual M-only presentation preserves D's original UI permit; same-window occurrences, later D, partial window results, close/loss and compatible input capture/fixed Step pass without replay. Output leases carry no duplicate UI permission. | Broader remote/transport placement, graph/Feature integration, platform accessibility and live edit retain their separate decisions and consumers. | 2026-10-31 @ designated-display qualification |
| [0113](0113-editor-ui-prefab-authoring-and-canvas-authority.md) | accepted | implemented | `nara_scene` Prefab authoring, `nara_tooling` workspace, project Host persistence and content refresh, effective project settings, the selected Dear ImGui authoring frontend, and private Editor Canvas | [crates/nara_scene/src/authoring.rs](../../../crates/nara_scene/src/authoring.rs); [crates/nara_scene/src/patch.rs](../../../crates/nara_scene/src/patch.rs) | [R5c additive Open qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T202313Z-additive-scene-and-prefab-open-qualification-d0b1fc894dd2419fa1cf43c9e39eb79d.md); [R5c multi-source refresh qualification](../../knowledge/engineering/verification/2026-10/2026-10-23T192721Z-multi-source-prefab-refresh-obligation-qualification-47f6266123094e7bb2ea046f42131843.md); [crates/nara_scene/src/tests.rs](../../../crates/nara_scene/src/tests.rs); [editor/src/frontend/imgui/interaction_tests.rs](../../../editor/src/frontend/imgui/interaction_tests.rs) | Ordered Prefab structure, finite schema/brush catalogs, exact image choices, source-local preflight and atomic allocator/history/selection publication are implemented. Stale picks, gesture cancellation, focus and Canvas retirement are covered; real-source Save/Reopen, Runtime restart and successor presentation pass. R5c replaces singleton refresh requests with source-keyed obligations, canonical configured-root observations and budgeted complete-set validation. Retry and retirement recovery retain every obligation after Close; 850 library, 39 integration, 31 consumer and eight selected actual native cases pass. ADR 0118 owns the current frontend/window migration; toolkit convergence, responsive authoring and live Runtime mutation remain separate. | Additive anchored Prefab Open, two-source exact Save/refresh and the separately scoped Play-source retirement continuity now pass; see ADRs0034/0118 for that continuity evidence. Responsive authoring, another complete toolkit consumer or live Runtime mutation needs separate evidence; ADR 0118 owns current frontend/windows. | 2026-11-03 status reconciliation; 2026-10-24 @ R5c additive Open; earlier Prefab/Canvas evidence retained |
| [0114](0114-standalone-generic-editor-authoring-and-execution-boundary.md) | accepted | partial | Private Editor, project Host, project/reflect/Scene/tooling and workflow owners | [generic Editor](../../../src/project_host/runtime/editor.rs); [source Browser](../../../crates/nara_tooling/src/project_browser/state.rs); [ordinary structural consumer](../../../editor/tests/support/ordinary_starter_structure.rs) | [ordinary structure qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T051819Z-ordinary-structural-authoring-and-captured-source-reopen-qualification-f9b317e0ceda49a6b37f9000c61af8d7.md); [strict Check tests](../../../tests/project_workflow_check.rs); [external Play consumer](../../../editor/tests/supervised_play_clean_room.rs); [CP1/S2 qualification](../../knowledge/engineering/verification/2026-11/2026-11-01T203621Z-reviewed-authored-schema-upgrades-and-fresh-process-recovery-qualification-cf7c8ecab10c4e21b4cfcff4906805de.md) | Generic Open, bounded data-only schema admission, degraded preservation, Recovery and strict Runtime separation remain. Existing Browser/Image import and source publication evidence retains its scope. Ordinary Scene/Prefab structure now works without game Rust loaded; a generated desktop starter performs create/arrange/remove/history/Save/Reopen, strict public Check and actual supervised Play/Stop/Exit. Compiled Editor authoring uses retained source Native/Data authority independently of Runtime bindings; the Root native-only source fallback is removed. Windows widget and external application evidence are separate. | Native Linux/macOS, broader source mutation/Prefab closure, shader/animation authoring, watchers or dynamic extension hosting needs a selected workflow. The source-local editing proof does not establish full production maturity. | 2026-11-02; `e673693`, selected Editor `f4ffdd18` and original `ec64e4c` evidence retained |
| [0115](0115-editor-image-import-metadata-and-authoring-publication.md) | accepted | implemented | `nara_asset`, `nara_image`, project Host image authoring, UI-neutral tooling, and private Editor adapters | [crates/nara_image/src/import.rs](../../../crates/nara_image/src/import.rs); [src/project_host/runtime/editor/image_import/transaction.rs](../../../src/project_host/runtime/editor/image_import/transaction.rs) | [crates/nara_image/tests/image_settings.rs](../../../crates/nara_image/tests/image_settings.rs); [editor/src/platform/imgui/scene_image_tests.rs](../../../editor/src/platform/imgui/scene_image_tests.rs); [R0 evidence](../../knowledge/engineering/verification/2026-10/2026-10-19T021717Z-image-lifecycle-contract-repairs-95901fe367a149bc90ec922ad0384a73.md); [Image custody qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T035244Z-editor-image-metadata-publication-cleanup-custody-qualification-c097c716d6f3480692d800285ec53544.md) | Canonical metadata, receipt-bound publication, retained-pixel leases, bounded hydration and R0 identity revocation remain implemented. Metadata writers register before mutation; runtime custody retains failed cleanup through terminal failure, close, session Drop and post-publication unwind. Successful publication retires the writer while pending decode retains exact fresh-read authority and binding. Focused engine/public 116, Editor/application 186 and Windows/DX12 native 2 tests pass; workspace check and independent integration reviews pass. | - | 2026-10-20 @ `bae0440`; earlier R0 evidence and `ec64e4c` |
| [0116](0116-standalone-supervised-editor-play.md) | accepted | partial | Private `nara-editor`, project Host, executable product Hosts, UI-neutral tooling and platform process adapters | [editor/src/supervised_play/owner.rs](../../../editor/src/supervised_play/owner.rs); [src/project_host/runtime/editor/external_play.rs](../../../src/project_host/runtime/editor/external_play.rs) | [src/project_host/runtime/editor/external_play.rs](../../../src/project_host/runtime/editor/external_play.rs); [editor/src/platform/imgui/external_cadence_tests.rs](../../../editor/src/platform/imgui/external_cadence_tests.rs); [storage and diagnostics evidence](../../knowledge/engineering/verification/2026-10/2026-10-20T152924Z-reflection-declaration-checks-and-linux-supervised-play-storage-corrections-efc879a2a8974c989725df994b86e5db.md) | Committed Windows evidence covers exact saved generations, child READY, lifecycle, build failure/Run Last Good, cancellation, quiet observation and physical cleanup. Restart rejects changed or unavailable saved-source closures before retirement. Bounded preparation/launch failure facts and quiet repeated-error projection pass 175 Editor library tests. Linux generation storage now uses the standard user cache: six actual-source Linux tests and ext4 publication pass; explicit tmpfs remains rejected before staging. These storage results do not qualify the complete hosted Linux Play trajectory. | Complete native Linux/macOS process/window release trajectories before platform release claims. Retain ADR 0116's exact generation, receipt and cleanup guarantees; observe successor hosted Linux clean-room/reference Play using the new bounded diagnostics. | Earlier `ca3d803`, `ab211bc`, `0b3f569`; 2026-10-20 @ diagnostics `258c1ab`, Linux cache `ce49696` |
| [0117](0117-2d-foundation-ownership-and-authoring-contracts.md) | accepted | partial | App, content/Scene, render, tooling and concrete product Hosts | [managed runtime](../../../crates/nara_app/src/runtime.rs); [Image publication](../../../crates/nara_image/src/content.rs); [TileSet publication](../../../crates/nara_tilemap/src/content.rs); [View lifecycle](../../../crates/nara_render/src/view.rs); [world canvas](../../../crates/nara_render/src/canvas.rs); [project content](../../../src/project_content.rs); [product runtime](../../../src/project_host/runtime.rs) | [workflow integration](../../knowledge/engineering/verification/2026-10/2026-10-22T130424Z-engine-workflow-refactor-final-integration-ac0a299fafd64a6dba6179531a1f5cff.md); [TileSet publication](../../knowledge/engineering/verification/2026-10/2026-10-19-tileset-publication-and-recovery.md); [canvas qualification](../../knowledge/engineering/verification/2026-10/2026-10-20T070752Z-fixed-world-canvas-and-native-ui-qualification-e7c7f1ee29e4459688d51f1ca20282d8.md); [View/history qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T210034Z-captured-view-histories-and-original-submission-cleanup-production-qualification-5c061376803446c0bff1a95495ca55c7.md) | Existing evidence retains managed Runtime, checked content/GPU publication, fixed WorldCanvas and presentation-gated input within its recorded scope. Neutral captured Views now have non-reused process lifetimes independent of Entity/order/extent, stable through filtering or missing targets and replaced on camera removal/readdition. Public packets reject duplicate IDs; original temporal View owners reclaim slots independently of shared effect declarations. | The low-level plan's phase-cut and managed mutation successors retain their own evidence. Joint motion/solver ownership, physics/audio, tile authoring/durable IDs, scaling, telemetry and native Linux/macOS expansion remain separate. | 2026-11-01 @ captured View lifetime successor; prior scoped evidence retained |
| [0118](0118-editor-workspaces-frontends-and-creation-templates.md) | accepted | partial | Editor Application, workspace, tooling, reflection, document and native Host owners | [Application](../../../editor/src/app.rs); [semantic panel Host](../../../editor/src/panels/host.rs); [native image attachment](../../../editor/src/native/images.rs); [reference Host](../../../editor/src/platform/imgui.rs); [Open preparation](../../../src/project_host/runtime/editor/open/preparation.rs) | [current R11 units](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r11b-public-panel-hosting-and-component-composition); [independent panel consumer](../../../tests/fixtures/editor-ui-host-consumer/README.md); [prepared-panel lineage](../../knowledge/engineering/verification/2026-10/2026-10-25T024527Z-prepared-panel-complete-review-and-scoped-workflow-qualification-49e27d9779054a5b8fda1142add06519.md); [multi-source lineage](../../knowledge/engineering/verification/2026-10/2026-10-23T202313Z-additive-scene-and-prefab-open-qualification-d0b1fc894dd2419fa1cf43c9e39eb79d.md) | Exact ordered submissions, non-active queries/preparation/construction, per-document persistence and source/preview custody keep their bounded qualification above and in the owning plan. Public typed panel state/result custody is independent of the optional Dear reference. Image-backed native attachment adds finite original-owner focus/capture/modal, scoped IME, clipboard/cursor/refresh and complete-window receipt admission. Independent Nara and software-painted consumers preserve full source, history, persistence and original physical retirement; exact results and limits are recorded above. Existing phase/performance evidence retains its original scope. | Complete independent Shell and remaining component/editor workflows, public UI operations, R11f code reconstruction/recovery and measured performance remain open. Native Linux/macOS, OS IME/accessibility, direct drawing/overlays/detach, installable dynamic frontends, recursive/collection discovery, inline tools and broader deletion need their own consumers. Tile tools remain deferred. | 2026-11-05 image-backed native panel attachment; earlier source/workspace qualification retained |
| [0119](0119-schema-first-semantic-metadata-authority.md) | accepted | partial | Semantic Schema and caller-owned native reflection adapters | [provider](../../../crates/nara_reflect/src/authoring.rs); [registry](../../../crates/nara_reflect/src/registry.rs) | [native contract](../../../crates/nara_reflect/tests/registry_contract.rs); [caller-owned reflection](../../../crates/nara_reflect/tests/math_reflection.rs); [non-Reflect consumer](../../../tests/fixtures/module-consumer/tests/support/nested_schema.rs); [R12 qualification](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#schema-registry-and-advanced-ecs-qualification) | Native providers use explicit Schema/codec contracts without Reflect. Builders, snapshots and provider merges no longer retain an unused Bevy TypeRegistry or reflection registration/query methods. Caller-owned Bevy reflection preserves dependent math registrations and the complete dynamic-value round trip. Affected Reflect/Scene targets pass 1042/1042, independent public consumers 23/23 and actual game rebuild/reconstruction/adoption 2/2, retaining provider failure atomicity, exact native binding and physical cleanup. Bevy reflection dependencies/reexports remain; no measured memory or build-speed saving is claimed. New domain descriptors and broader execution/publication choices retain their own scope. | Retain non-Reflect/native complete-binding and direct reflection consumers when changing their owners; qualify new metadata adapters against their concrete operations. | 2026-11-14 @ native reflection ownership separation |
| [0120](0120-schema-publications-and-execution-representations.md) | proposed | not-started | Schema, Scene materialization and concrete gameplay adapters | [public data consumer](../../../tests/fixtures/module-consumer/tests/scene_data_components.rs); [language consumer](../../../tests/fixtures/behavior-languages/README.md); [raw storage probes](../../../tests/fixtures/module-consumer/tests/behavior_storage_comparison.rs) | [representation comparison](../../research/2026-10-27-behavior-representation-comparison.md); [body/lifetime comparison](../../research/2026-10-27-behavior-code-iteration-and-lifetime.md); [public-language evidence](../../knowledge/engineering/verification/2026-10/2026-10-27T145608Z-public-scene-language-consumer-and-obsolete-representation-runtime-removal-67783a7cb8514ce9bbb7957670b4ec37.md); [finite-tool evidence](../../knowledge/engineering/verification/2026-10/2026-10-27T180941Z-production-finite-source-tools-and-exact-physical-close-observations-fc2363973364434096d350ae4a8d762c.md) | Experimental C#/Lua execution uses the adopted public Scene data family and real window-free Session editing/history/persistence. The obsolete preview/post-commit materialization/export Runtime is deleted. Finite trusted child tools now have their separate production Session/Application contract under ADRs 0047/0118. Same-World body switching and VM/collectible-context leases remain scoped execution probes; this executable publication/migration/cohort proposal is not adopted. | Integrate an executable gameplay owner through product admission, then cross-owner compatibility, in-flight work, Stop-first, catalog migration and complete physical retirement. Inline tool facets and installation remain separate R1b workflows. | 2026-10-28 @ finite-tool qualification; separately adopted ADR 0006 and bounded language evidence retained |
| [0121](0121-composable-render-programs-and-host-execution.md) | proposed | partial | Render Feature/domain adapters and local GPU Host | [independent stock consumer](../../../tests/fixtures/stock-composition-consumer/README.md); [production integration](../render-program-production-design.md) | [shared stock qualification](../../knowledge/engineering/verification/2026-11/2026-11-01T013746Z-shared-stock-composition-original-captures-and-native-scene-effects-qualification-2f250ecb33b24b5091f28a141cd6b244.md); [native-composition qualification](../../knowledge/engineering/verification/2026-10/2026-10-31T225517Z-native-composition-physical-pump-handoff-and-retained-stock-input-qualification-f0cd93c17a3c47fa905993cee07e6c5c.md); [cost method/results](../../benchmarks/2026-10-31-render-composition.md) | Finite programs, original typed captures, selected View/history, Source/Processed replay and independent native/image streams use the same Host under Accepted0094. Shared world/Canvas/UI stages now carry external blur/grade through Runtime, actual native Scene View and offscreen, with exact HUD, retained resource semantics and policy-independent picking. Optimized controls establish no stable cost winner; the complete proposal remains unqualified. | Durable material generations, broader View/time facts, the editable Nara-drawn Inspector and measured geometry retain their separate gates. Attribute reproducible production costs before interface freeze; full0121 remains Proposed. | 2026-11-01 @ shared stock-composition successor; Proposed status unchanged |
| [0122](0122-scoped-imported-product-identity-and-authored-continuity.md) | accepted | implemented | `nara_asset`, Reflect/Scene reference owners, project content and concrete metadata facets | [product identity](../../../crates/nara_asset/src/product_identity.rs); [families/codecs](../../../crates/nara_asset/src/runtime_codec.rs); [canonical reference](../../../crates/nara_reflect/src/asset_reference.rs); [prepared-product checks](../../../src/project_content/domains.rs); [compound owner](../../../tests/fixtures/module-consumer/asset-domain-consumer/src/bundle.rs) | [continuity/format qualification](../../knowledge/engineering/verification/2026-10/2026-10-29T004054Z-compound-product-continuity-and-independent-runtime-format-qualification-a2d6d4837fd44218b7cb87f67cb38063.md); [declared-reference qualification](../../knowledge/engineering/verification/2026-10/2026-10-29T023522Z-declared-product-references-and-prepared-content-target-admission-qualification-37c1b3f6eeca448f928a6961f1c7ab6b.md); [compound project and cached Editor Play](../../../tests/fixtures/module-consumer/asset-domain-consumer/tests/compound_workflow.rs); [Editor transition](../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/compound_import.rs) | Scoped IDs, authored continuity, deterministic Runtime text format and predecessor-aware Editor publication pass. The successor preserves complete Product references through existing nested/default Schema traversal, rewrite, Scene export and Prefab copy, validates actual prepared targets and rechecks edited cached Play. Primary-only consumers refuse auxiliary selection; JSON/RON records remain strict. Known-retired preservation remains relative to the admitted lineage. | Runtime catalog delivery, CAS/group publication and project-wide migration remain separate work; the scoped decision does not adopt them. | 2026-10-29 @ `c3c6d33`; declared references and prepared target qualification |
