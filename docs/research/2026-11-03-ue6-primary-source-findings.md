# Unreal Engine 6: Primary-Source Findings

Research date: 2026-11-03, Asia/Shanghai. This is a source investigation, not an adopted Nara
decision, an implementation audit, or a benchmark. Only publicly returned material was inspected;
no authenticated engine checkout, video playback, or audiovisual transcription was performed.

The central distinction is between the gameplay programming contract and the engine's native
implementation. Epic's native-code documentation and transactional-runtime work provide direct
evidence for that distinction. Replacing a gameplay framework does not establish removal of the
C++ engine. [Unreal Engine source access](https://www.unrealengine.com/ue-on-github),
[AutoRTFM technical article](https://www.unrealengine.com/tech-blog/bringing-verse-transactional-memory-semantics-to-c?lang=en).

## Provenance And Chronology

| Material | Verified date and provenance | How it is used |
| --- | --- | --- |
| State of Unreal, Chicago | Epic's event page places the event on June 16–18, 2026 and the keynote on June 17. Epic's corporate announcement is dated June 17. | Establishes the announcement's event date, independently of the later article date. [Event](https://www.unrealengine.com/events/unreal-fest-chicago-2026), [corporate announcement](https://www.epicgames.com/site/news/unreal-fest-chicago-the-road-to-unreal-engine-6). |
| The road to Unreal Engine 6 | [English](https://www.unrealengine.com/news/the-road-to-ue-6) and [Chinese](https://www.unrealengine.com/news/the-road-to-ue-6?lang=zh-CN): June 22, 2026; author Marcus Wassmer, development-team lead. | R1–R4 use English wording. Publication follows the keynote by five days. |
| Shanghai event | Two exhibitors' own announcements identify October 22–23, 2026. | Participant-primary corroboration of event dates, not an Epic transcript or the precise day of Juan Canada's remarks. [Silicon Studio](https://www.siliconstudio.co.jp/news/12635-2/), [Marvelous Designer](https://www.marvelousdesigner.com/en-ca/resources/newsroom/ad0539caad354a99a36e20880a7a0b10). |
| Supplied Bilibili clip | Public metadata identifies the verified official Unreal Engine account, MID `138827797`, a 98-second clip, and publication on November 2, 2026 at 18:00 UTC+08:00 (`pubdate=1793613600`). | An official account's summary, published after the Shanghai event; upload date is not the talk date. [Video](https://www.bilibili.com/video/BV1aT4F6QEPN/), [metadata](https://api.bilibili.com/x/web-interface/view?bvid=BV1aT4F6QEPN). |
| Recent implementation evidence | Epic's Fortnite 43.00 release notes explicitly date the release November 1, 2026. | A recent shipped-product reference, separate from roadmap claims. [43.00 notes](https://dev.epicgames.com/documentation/fortnite/43-00-fortnite-ecosystem-updates-and-release-notes). |

## Claim, Evidence, And Uncertainty

“Announced” below describes a stated direction or commitment. “Documented” describes an available
product/API description, without claiming that this investigation executed it.

| ID / claim under examination | Primary evidence and status | Uncertainty or scope limit |
| --- | --- | --- |
| R1. Architecture direction | UE5/UEFN convergence; new Verse/Scene Graph gameplay framework; interoperability and model tooling. **Announced.** [Road](https://www.unrealengine.com/news/the-road-to-ue-6). | Not release qualification. |
| R2. Execution status | Single-thread Verse calls LLVM-instrumented C++; parallelism unfinished; distributed STM prototyped. [Road](https://www.unrealengine.com/news/the-road-to-ue-6). | General distributed persistence unqualified. |
| R3. Transition | Actors/Blueprints initially retained; later deprecation conditional on maturity; converters promised. **Announced.** [Road](https://www.unrealengine.com/news/the-road-to-ue-6). | Retirement version and coverage unspecified. |
| R4. Availability | Early Access end-2027; full release 12–18 months later; `ue6-main` not an alpha. **Targets.** [Road](https://www.unrealengine.com/news/the-road-to-ue-6). | Source not inspected; delivery unverified. |
| R5. The official Shanghai clip establishes immediate removal and automatic migration | Its description forecasts no Blueprint/C++ gameplay framework and AI-assisted completion of most or all UE5-to-UE6 project-conversion work. **Official summary of intent.** [Bilibili metadata](https://api.bilibili.com/x/web-interface/view?bvid=BV1aT4F6QEPN). | No transcript obtained; version scope, exact spoken qualifications, and conversion results unverified. |
| R6. C++ engine implementation is being deleted | Epic documents complete C++ engine source access. AutoRTFM adds Verse-compatible transactions to native C++ execution. **Direct evidence of native substrate and interoperation.** [Source access](https://www.unrealengine.com/ue-on-github), [AutoRTFM](https://www.unrealengine.com/tech-blog/bringing-verse-transactional-memory-semantics-to-c?lang=en). | This does not promise arbitrary future C++ gameplay APIs or unchanged plugin compatibility. |
| R7. Every existing gameplay subsystem will simply be translated to Verse | Epic Staff member Zhi Kang Shao says GAS will be replaced rather than converted to Verse; legacy GAS remains in UE6.0 before eventual deprecation. Mover and Mass belong to the future stack. **Specific June 24, 2026 answer.** [GAS answer](https://forums.unrealengine.com/t/gameplay-ability-system-gas-future-in-unreal-engine-6/2738426/2). | Subsystem-specific statement, not a full migration matrix or evidence of a finished successor. |
| R8. Scene Graph exists only as a future announcement | Epic announced publishable UEFN Scene Graph Beta on June 7, 2025. Current documentation retains the Beta label. **Released foundation.** [Beta announcement](https://www.fortnite.com/news/publish-fortnite-islands-created-with-scene-graph-now-in-beta?lang=en-US), [current guide](https://dev.epicgames.com/documentation/fortnite/getting-started-in-scene-graph-in-fortnite). | A publishable UEFN foundation does not qualify every planned UE6 capability. |
| R9. Actors and entities are already interchangeable | Epic says they are different systems that coexist; connecting Scene Graph functionality to Creative devices requires custom Verse. **Documented coexistence.** [Getting started](https://dev.epicgames.com/documentation/fortnite/getting-started-in-scene-graph-in-fortnite). | Coexistence is not automatic semantic conversion. |
| R10. Reusing UE5 assets establishes whole-project compatibility | The existing UE/UEFN Migration Tool copies content and dependencies without validating destination compatibility; asset restrictions remain. **Documented asset transport.** [Migration guide](https://dev.epicgames.com/documentation/en-us/fortnite/migrating-assets-from-unreal-engine-to-unreal-editor-for-fortnite). | This is not a UE5-to-UE6 gameplay or Blueprint converter. |
| R11. Verse persistence already means arbitrary persistent global worlds | Current persistence uses constrained per-player `weak_map` data, including schema-compatibility checks and size limits. **Documented implementation.** [Persistence guide](https://dev.epicgames.com/documentation/fortnite/using-persistable-data-in-verse?lang=en-US). | It does not establish unrestricted distributed-world persistence. |
| R12. Convergence means every project must ship inside Fortnite | Epic's corporate announcement explicitly includes traditional platforms and developers' own ecosystems. **Announced distribution scope.** [Corporate announcement](https://www.epicgames.com/site/news/unreal-fest-chicago-the-road-to-unreal-engine-6). | Existing UEFN publishing still uses Creator Portal. [Current publishing guide](https://dev.epicgames.com/documentation/en-us/fortnite/publishing-projects-in-unreal-editor-for-fortnite). |

## What “C++ Gameplay Framework” Includes

The current UE5.8 Gameplay Framework documentation describes a coordinated family of classes:
Actors and Actor Components, Pawns/Characters, Controllers, Game Mode, Game State, Player State,
and related lifetime/managing objects. It supports authoring through C++, Blueprint, or both.
Thus, “C++ gameplay framework” plausibly refers to this author-facing class and behavior model.
That scope is an interpretation grounded in Epic's terminology, not a verified quotation from the
Shanghai speech. It cannot be expanded into an exhaustive deletion list for UObject, rendering,
physics, animation, editor extensions, or every native plugin.
[Gameplay Framework documentation](https://dev.epicgames.com/documentation/en-us/unreal-engine/gameplay-framework-in-unreal-engine).

The GAS answer provides a useful concrete boundary: adopting a different component/language model
can require a replacement design, while other technology can become actor-agnostic. The author is
not merely an unidentified community commenter: the public forum profile JSON returns
`title=Staff` and `primary_group_name=Epic_Games_Inc` for `ZhiKangShao`.
[Answer](https://forums.unrealengine.com/t/gameplay-ability-system-gas-future-in-unreal-engine-6/2738426/2),
[profile metadata](https://forums.unrealengine.com/u/ZhiKangShao.json).

Epic's March 15, 2024 article by Phil Pizlo explains the native mechanism: AutoRTFM creates
instrumented function clones and records effects for rollback. It reports deployment to some
Fortnite 28.10 servers and deliberately distinguishes single-thread rollback from eventual
parallel STM. External effects such as file writes are not automatically reversible; an
uninstrumented call aborts by default. This is historical implementation evidence, not a claim
that every limitation described in 2024 still applies in November 2026.
[AutoRTFM article](https://www.unrealengine.com/tech-blog/bringing-verse-transactional-memory-semantics-to-c?lang=en).

The distinction also appears in the public C++ API: `AutoRTFM` is documented as a Runtime module at
`/Engine/Source/Runtime/AutoRTFM/`, with allocation, commit, and abort hooks. Calling Scene Graph
“Verse native” should therefore not be read as saying the whole engine is implemented in Verse.
The latter is an inference the inspected evidence does not support.
[AutoRTFM API](https://dev.epicgames.com/documentation/unreal-engine/API/Runtime/AutoRTFM).

## Concrete Scene Graph And Migration Evidence

The entity API defines hierarchical entities, reusable editor-authored prefab classes, and
component-owned behavior. It explicitly recommends keeping logic in components so prefab
structure can change without a corresponding class-hierarchy rewrite. These APIs establish a
composition model; they do not establish a particular native ECS storage layout, automatic
parallel scheduler, or measured performance advantage.
[Entity API](https://dev.epicgames.com/documentation/en-us/fortnite/verse-api/versedotorg/scenegraph/entity).

Component lifetime has separate scene-entry, simulation-start, asynchronous simulation,
simulation-end, and scene-removal phases. `OnSimulate` is canceled before `OnEndSimulation`.
These are inspectable runtime contracts, beyond the generic label “Scene Graph.”
[Component API](https://dev.epicgames.com/documentation/en-us/fortnite/verse-api/versedotorg/scenegraph/component).

For existing content, asset reuse, gameplay translation, and persistent-state migration are
different questions. Scene Graph already exposes project assets through generated components and
prefab classes. That supports reuse of underlying content without proving equivalent Blueprint
behavior. The asset Migration Tool also has version-direction and supported-type restrictions;
successful copying is not compatibility validation.
[Asset-generated components](https://dev.epicgames.com/documentation/fortnite/getting-started-in-scene-graph-in-fortnite),
[migration restrictions](https://dev.epicgames.com/documentation/en-us/fortnite/migrating-assets-from-unreal-engine-to-unreal-editor-for-fortnite).

Persistent data has its own contract: published schemas undergo compatibility checks; classes can
gain defaulted fields under documented restrictions; a player record is limited to 256 KB.
The guide also warns that reverting an island version resets player persistence. Code generation
or asset conversion alone would not prove preservation of live save data.
[Persistence compatibility and limits](https://dev.epicgames.com/documentation/fortnite/using-persistable-data-in-verse?lang=en-US).

The most recent inspected release, UEFN 43.00, reports Scene Graph performance work, prefab
inheritance changes, and recoverable placeholders for missing Verse classes. Its health/damage
pipeline remains Experimental. The same notes identify a camera issue on play servers despite
working edit sessions. These are concrete progress and qualification limits, not a completed
general-purpose UE6 framework.
[43.00 release notes](https://dev.epicgames.com/documentation/fortnite/43-00-fortnite-ecosystem-updates-and-release-notes).

There is also a documentation-age discrepancy: the Scene Graph known-issues page says it was
updated for 42.00 on August 20, 2026 and describes missing redirector support; 43.00 subsequently
documents improved asset redirectors behind a project setting. Prefer the newer, scoped release
statement for that feature, without assuming it fixes every reference-loss case.
[Older known issues](https://dev.epicgames.com/documentation/en-us/fortnite/scene-graph-known-issues-in-fortnite),
[newer release notes](https://dev.epicgames.com/documentation/fortnite/43-00-fortnite-ecosystem-updates-and-release-notes).

## Reconciling The Two Supplied Sources

R3 and R5 remain unresolved: endpoint and transition could coexist, or plans could have changed.
The later summary establishes neither version scope nor explicit supersession of the earlier
policy. Immediate removal cannot be inferred.

Likewise, the AI migration statement is an expectation, not a measured conversion result. Its
description qualifies the amount of project-conversion work, not the proportion of UE5 projects
proven convertible. The evidence obtained contains no demonstrated coverage for arbitrary project
code, third-party plugins, Blueprint variants, or live save schemas. Those remain migration
acceptance questions. [Official description](https://api.bilibili.com/x/web-interface/view?bvid=BV1aT4F6QEPN).

## Access, Transcript, And Source Limits

- **Bilibili:** Search Service failed; ordinary proxied HTTPS returned HTML and metadata, including
  organizational verification. `view.data.subtitle.list` and `player.data.subtitle.subtitles`
  were empty; `need_login_subtitle=false`. No caption text was obtained; burned-in text/audio were
  not inspected. Other subtitle routes remain unverified.
  [View metadata](https://api.bilibili.com/x/web-interface/view?bvid=BV1aT4F6QEPN),
  [player metadata](https://api.bilibili.com/x/player/v2?aid=117545487044946&cid=43256775379).
- **Chicago recording:** Epic's verified Unreal Engine channel publishes the official recording
  dated June 17, 2026. Its chapter metadata places Marcus Wassmer's UE6 section at `1:19:03` and
  Kurtis Schmidt's Verse section at `1:28:13`. An English automatic-caption track was advertised,
  but the ordinary timed-text request returned zero bytes. No transcript or spoken quotations
  from this recording are claimed.
  [Official recording](https://www.youtube.com/watch?v=xXGSvLH9zAs),
  [Epic's announcement linking the keynote](https://www.epicgames.com/site/news/unreal-fest-chicago-the-road-to-unreal-engine-6).
- **Shanghai lecture/slides:** No full original lecture transcript or downloadable slide deck was
  obtained. An exhibitor-linked WeChat page returned a verification screen. The attempted Epic
  Shanghai event URL was inaccessible; it is not treated as a verified schedule. Older official
  Scene Graph talks are linked by Epic, but Epic warns their APIs/features have changed; they
  cannot establish the November migration policy.
  [Exhibitor announcement and WeChat link](https://www.siliconstudio.co.jp/news/12635-2/),
  [Epic's historical-talk warning](https://www.fortnite.com/news/publish-fortnite-islands-created-with-scene-graph-now-in-beta?lang=en-US).
- **Engine source:** The linked `ue6-main` URL returned HTTP 404 to the unauthenticated HTTP request;
  Search Service could not fetch it. Epic's source-access instructions require linked accounts
  and login. This is an access limitation, not evidence that the branch is absent. No commit,
  native implementation completeness, or converter implementation was inspected.
  [Linked branch](https://github.com/EpicGames/UnrealEngine/tree/ue6-main),
  [official access instructions](https://www.unrealengine.com/ue-on-github).

All dates above come from visible publication/event text or explicit Unix metadata. Retrieval
occurred on November 3, 2026; a current crawl date is not a publication or implementation date.
Search snippets were useful for discovery but were not treated as watched lectures. The
exhibitors support event logistics only; technical conclusions use Epic-owned material or
identified Epic Staff statements.

## Modest Implications For Nara

These are research inferences, not requests to change the current architecture or product scope.

1. **Evaluate authoring and execution separately.** A more accessible gameplay surface can use a
   native substrate. The evidence gives no reason by itself to abandon Nara's complete Rust path
   or make another language mandatory. [Nara Strategy](../../STRATEGY.md),
   [native transaction mechanism](https://www.unrealengine.com/tech-blog/bringing-verse-transactional-memory-semantics-to-c?lang=en).
2. **Study concrete composition and lifetime contracts.** Prefab structure, component behavior,
   and cancellation-before-cleanup are more actionable comparison points than adopting the name
   “Scene Graph.” Any Nara change still needs a local workflow and evidence.
   [Entity](https://dev.epicgames.com/documentation/en-us/fortnite/verse-api/versedotorg/scenegraph/entity),
   [component lifetime](https://dev.epicgames.com/documentation/en-us/fortnite/verse-api/versedotorg/scenegraph/component).
3. **Validate migration by the preserved result.** Distinguish copying assets, translating behavior,
   and retaining persisted state. Generated conversion needs explicit unsupported cases and
   consumer verification; compilation alone is insufficient. [Asset migration limits](https://dev.epicgames.com/documentation/en-us/fortnite/migrating-assets-from-unreal-engine-to-unreal-editor-for-fortnite),
   [persistence contract](https://dev.epicgames.com/documentation/fortnite/using-persistable-data-in-verse?lang=en-US).
4. **Keep scale claims proportional to proof.** A published API, a Beta workflow, a prototype, and
   an actual production result are different evidence levels. No inspected material warrants
   importing a distributed-world runtime or a new native ABI into Nara's current production
   proof. [Scene Graph Beta](https://www.fortnite.com/news/publish-fortnite-islands-created-with-scene-graph-now-in-beta?lang=en-US),
   [Nara scope](../../STRATEGY.md).

Unresolved questions concern release coexistence, retirement boundaries, converter coverage,
native extension contracts, and save-data handling. A full Shanghai transcript or versioned Epic
migration guide would materially improve this evidence.
