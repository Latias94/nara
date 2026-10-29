use std::{
    fs,
    num::NonZeroU32,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use nara::{
    ProductRecipe, SchemaContribution,
    app::{
        App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId,
    },
    asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId},
    ecs::{Component, Query, Resource, World, bevy_ecs::system::SystemState},
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativePath, TrustMode},
    project::ProjectPath,
    project_host::{
        HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, ProjectContentErrorKind,
        ProjectContentLoader, ProjectDomainAssets, ProjectSceneRequestError, ProjectSceneRequestId,
        ProjectSceneRequestState, ProjectScenes, advanced::commit_prepared_project_scene,
        ingest_project_manifest, resolve_product_recipe,
    },
    project_workflow::{ProjectCheckRequest, check_project},
    reflect::{
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaVersion, ComponentTypeId,
        PersistentComponent, PersistentComponentProvider,
    },
    scene::{
        SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
        advanced::SceneProductTransactionLimits,
    },
};
use nara_asset::{
    AssetImportError, AssetImportLimits, AssetImportSource, AssetProductRef, ImportedProductId,
    RuntimeAssetCodec, SourceAssetImporter,
};
use nara_asset_domain_consumer::bundle::{TextBundle, TextBundleDomain, TextBundleSettings};

const SOURCE: &str = "cc91a0c8-e2b4-42ae-9d78-3481b9f34d65";
const FIRST: &str = "10a5c635-e71b-478d-86c4-ef933956a425";
const SECOND: &str = "f319f4c7-d809-40d9-8c97-4abb6c68b6da";
const RAW: &[u8] = br#"{"suffix":"suffix.txt","entries":[{"name":"Opening","text":"Hello"},{"name":"Ending","text":"Goodbye"}]}"#;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.bundle.Caption",
    version = 1,
    alias = "Bundle Caption",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct Caption {
    #[nara(
        id = "product",
        alias = "Product",
        capabilities(scene, inspect, edit, asset_ref)
    )]
    product: AssetRef,
}

nara::persistent_components! { mod components = [Caption]; }

const PROVIDER_ID: PluginSchemaProviderId = PluginSchemaProviderId::new("consumer.bundle.schema");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("consumer.bundle"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("consumer.bundle.native", 1),
    components::initial_catalog,
    components::register,
);

#[derive(Default)]
struct BundlePlugin;

struct RuntimeOnlyFamily;

impl nara_asset::AssetDomain for RuntimeOnlyFamily {
    type Product = TextBundle;
}

struct PanickingMembership;

impl nara_asset::AssetDomain for PanickingMembership {
    type Product = TextBundle;

    fn contains_product(_: &TextBundle, _: ImportedProductId) -> bool {
        panic!("membership query failed");
    }
}

impl SourceAssetImporter for PanickingMembership {
    type Settings = TextBundleSettings;
    const SOURCE_KIND: &'static str = TextBundleDomain::SOURCE_KIND;

    fn limits() -> AssetImportLimits {
        TextBundleDomain::limits()
    }

    fn prepare<E>(
        source: &mut AssetImportSource<'_, E>,
    ) -> Result<TextBundle, AssetImportError<E>> {
        TextBundleDomain::prepare(source)
    }
}

#[derive(Clone, Default, Debug, Resource)]
struct Travel {
    request: Option<ProjectSceneRequestId>,
    missing_product_rejected: bool,
    finished: bool,
}

impl Plugin for BundlePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration =
            PluginDeclaration::new(PluginId::new("consumer.bundle"), PluginCategory::Runtime)
                .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
                .provides_schema(&[PROVIDER_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<Travel>()?
            .add_systems(CoreStage::Update, travel)?;
        Ok(())
    }
}

fn recipe() -> ProductRecipe {
    schema_recipe()
        .add_asset_domain::<TextBundleDomain>()
        .unwrap()
}

fn schema_recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_contribution(SchemaContribution::<BundlePlugin>::for_default([PROVIDER]).unwrap())
        .unwrap()
}

fn selected_text(world: &mut World) -> String {
    let mut query =
        SystemState::<(Query<&Caption>, nara::ecs::Res<ProjectDomainAssets>)>::new(world);
    let (captions, assets) = query.get(world).unwrap();
    let caption = captions.single().unwrap();
    let reference = caption.product.as_product().unwrap();
    let bundle = assets
        .collection::<TextBundleDomain>()
        .unwrap()
        .get(&AssetRef::StableId(reference.source()))
        .unwrap();
    bundle.get(&reference).unwrap().text.clone()
}

fn travel(world: &mut World) {
    let request = world.resource::<Travel>().request;
    if let Some(request) = request {
        let state = {
            let mut access = SystemState::<ProjectScenes>::new(world);
            let scenes = access.get_mut(world).unwrap();
            scenes.state(request).unwrap()
        };
        if !world.resource::<Travel>().missing_product_rejected {
            if state == ProjectSceneRequestState::Preparing {
                return;
            }
            assert_eq!(
                state,
                ProjectSceneRequestState::Rejected(ProjectSceneRequestError::Content(
                    ProjectContentErrorKind::AssetReference
                ),)
            );
            assert_eq!(selected_text(world), "Hello world");
            let next = {
                let mut access = SystemState::<ProjectScenes>::new(world);
                let mut scenes = access.get_mut(world).unwrap();
                assert!(
                    scenes
                        .diagnostics(request)
                        .unwrap()
                        .iter()
                        .any(|diagnostic| {
                            diagnostic.code().as_str() == "project.content.asset-product-missing"
                        })
                );
                assert!(scenes.acknowledge(request));
                scenes
                    .request(ProjectPath::new("other.scene.json").unwrap())
                    .unwrap()
            };
            let mut travel = world.resource_mut::<Travel>();
            travel.missing_product_rejected = true;
            travel.request = Some(next);
        } else if state == ProjectSceneRequestState::Ready {
            {
                let mut access = SystemState::<ProjectScenes>::new(world);
                let scenes = access.get_mut(world).unwrap();
                let prepared = scenes.prepared(request).unwrap();
                let bundle = prepared
                    .domain_assets()
                    .collection::<TextBundleDomain>()
                    .unwrap()
                    .get(&AssetRef::stable_id(SOURCE).unwrap())
                    .unwrap();
                let encoded = TextBundleDomain::encode_runtime(bundle).unwrap();
                let decoded = TextBundleDomain::decode_runtime(&encoded).unwrap();
                let second = AssetProductRef::new(
                    StableAssetId::parse_str(SOURCE).unwrap(),
                    ImportedProductId::parse_subproduct(SECOND).unwrap(),
                )
                .unwrap();
                assert_eq!(decoded.get(&second).unwrap().text, "Goodbye world");
            }
            let result = commit_prepared_project_scene(
                world,
                request,
                SceneProductTransactionLimits::new(
                    nara::core::ItemLimit::ONE,
                    nara::core::ItemLimit::ONE,
                ),
                &[],
                |_| {},
            );
            assert!(!result.diagnostics.has_errors(), "{:?}", result.diagnostics);
            assert_eq!(selected_text(world), "Goodbye world");
            let mut access = SystemState::<ProjectScenes>::new(world);
            assert!(access.get_mut(world).unwrap().acknowledge(request));
            world.resource_mut::<Travel>().finished = true;
        }
    } else {
        assert_eq!(selected_text(world), "Hello world");
        let mut access = SystemState::<ProjectScenes>::new(world);
        let request = access
            .get_mut(world)
            .unwrap()
            .request(ProjectPath::new("invalid.scene.json").unwrap())
            .unwrap();
        world.resource_mut::<Travel>().request = Some(request);
    }
}

#[test]
fn compound_products_pass_strict_check_and_actual_two_scene_travel() {
    let fixture = Fixture::new();
    let delivery = check_project(ProjectCheckRequest::new(fixture.capability(), recipe())).unwrap();
    assert_eq!(delivery.scene_roots().len(), 2);
    for expected in [
        "assets/bundle.json",
        "assets/bundle.json.meta",
        "assets/suffix.txt",
    ] {
        assert!(
            delivery
                .members()
                .iter()
                .any(|member| member.path().as_str() == expected)
        );
    }
    fixture.write_scene("invalid.scene.json", "dd195b6a-449c-4e1a-bec5-24aa8d83870c");
    let manifest = fixture.root.join("nara.toml");
    fs::write(
        &manifest,
        fs::read_to_string(&manifest).unwrap().replace(
            "scenes = ['other.scene.json']",
            "scenes = ['other.scene.json', 'invalid.scene.json']",
        ),
    )
    .unwrap();
    let error =
        check_project(ProjectCheckRequest::new(fixture.capability(), recipe())).unwrap_err();
    let error = error.content_error().unwrap();
    assert_eq!(error.kind(), ProjectContentErrorKind::AssetReference);
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.content.asset-product-missing"
    }));
    let intent = HeadlessRunIntent::frames(NonZeroU32::new(2000).unwrap(), Duration::ZERO)
        .with_recipe(recipe())
        .stop_when(|state: &Travel| state.finished);
    let mut run = HeadlessRun::<Travel>::new(fixture.capability(), intent, Vec::new());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(state) => {
                assert!(state.finished);
                assert!(state.missing_product_rejected);
                break;
            }
            HeadlessRunOutcome::Failed => {
                panic!("compound travel failed: {:?}", report.diagnostics())
            }
            HeadlessRunOutcome::CleanupIncomplete => {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

#[test]
fn product_membership_panics_are_contained_and_release_candidate_reservations() {
    let fixture = Fixture::new();
    let directory = fixture.capability();
    let manifest = directory
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(
        &candidate,
        schema_recipe()
            .add_asset_domain::<PanickingMembership>()
            .unwrap(),
    )
    .unwrap();
    let loader = ProjectContentLoader::new(directory).unwrap();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::AssetDomain);
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "project.content.domain-panicked" })
    );
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn rebuild_and_source_move_preserve_products_while_stale_continuity_refuses() {
    let fixture = Fixture::new();
    let encoded = || {
        let directory = fixture.capability();
        let manifest = directory
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let plan = resolve_product_recipe(&candidate, recipe()).unwrap();
        let loader = ProjectContentLoader::new(directory).unwrap();
        let snapshot = loader.load(&candidate, &plan).unwrap();
        assert!(
            snapshot
                .domain_assets()
                .collection::<RuntimeOnlyFamily>()
                .is_none()
        );
        let collection = snapshot
            .domain_assets()
            .collection::<TextBundleDomain>()
            .unwrap();
        let bundle = collection
            .get(&AssetRef::stable_id(SOURCE).unwrap())
            .unwrap();
        let primary = AssetProductRef::new(
            StableAssetId::parse_str(SOURCE).unwrap(),
            ImportedProductId::PRIMARY,
        )
        .unwrap();
        for reference in [
            AssetRef::Path(collection.iter().next().unwrap().0.path().clone()),
            AssetRef::stable_id(SOURCE).unwrap(),
            AssetRef::Product(primary),
        ] {
            assert!(std::ptr::eq(collection.get(&reference).unwrap(), bundle));
        }
        assert!(
            collection
                .get(&AssetRef::Product(product_ref(FIRST)))
                .is_none()
        );
        TextBundleDomain::encode_runtime(bundle).unwrap()
    };
    let original_metadata = fs::read(fixture.root.join("assets/bundle.json.meta")).unwrap();
    let before = encoded();
    assert_eq!(encoded(), before);
    assert_eq!(
        fs::read(fixture.root.join("assets/bundle.json.meta")).unwrap(),
        original_metadata
    );

    fs::rename(
        fixture.root.join("assets/bundle.json"),
        fixture.root.join("assets/renamed.json"),
    )
    .unwrap();
    let settings = TextBundleSettings::initial(
        RAW,
        64,
        [
            ImportedProductId::parse_subproduct(FIRST).unwrap(),
            ImportedProductId::parse_subproduct(SECOND).unwrap(),
        ],
    )
    .unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str(SOURCE).unwrap(),
            AssetPath::new("renamed.json").unwrap(),
            AssetSourceKind::Other(TextBundleDomain::SOURCE_KIND.into()),
        ),
        settings,
    );
    fs::write(
        fixture.root.join("assets/renamed.json.meta"),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
    fs::remove_file(fixture.root.join("assets/bundle.json.meta")).unwrap();
    assert_eq!(encoded(), before);
    check_project(ProjectCheckRequest::new(fixture.capability(), recipe())).unwrap();

    fs::write(fixture.root.join("assets/renamed.json"),
        br#"{"suffix":"suffix.txt","entries":[{"name":"Ending","text":"Goodbye"},{"name":"Opening","text":"Hello"}]}"#).unwrap();
    let error =
        check_project(ProjectCheckRequest::new(fixture.capability(), recipe())).unwrap_err();
    assert_eq!(
        error.content_error().unwrap().kind(),
        ProjectContentErrorKind::AssetDomain
    );
    assert_eq!(
        TextBundleDomain::decode_runtime(&before)
            .unwrap()
            .get(
                &AssetProductRef::new(
                    StableAssetId::parse_str(SOURCE).unwrap(),
                    ImportedProductId::parse_subproduct(FIRST).unwrap()
                )
                .unwrap()
            )
            .unwrap()
            .text,
        "Hello world"
    );
}

#[test]
fn copied_source_needs_a_new_enclosing_identity_before_catalogue_admission() {
    let fixture = Fixture::new();
    let copy_id = "7ed195c9-0a78-4c4a-a90e-36245eb8ef88";
    fs::write(fixture.root.join("assets/copy.json"), RAW).unwrap();
    let write_copy_metadata = |source: &str| {
        let settings = TextBundleSettings::initial(
            RAW,
            64,
            [
                ImportedProductId::parse_subproduct(FIRST).unwrap(),
                ImportedProductId::parse_subproduct(SECOND).unwrap(),
            ],
        )
        .unwrap();
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str(source).unwrap(),
                AssetPath::new("copy.json").unwrap(),
                AssetSourceKind::Other(TextBundleDomain::SOURCE_KIND.into()),
            ),
            settings,
        );
        fs::write(
            fixture.root.join("assets/copy.json.meta"),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
    };
    write_copy_metadata(SOURCE);
    assert!(check_project(ProjectCheckRequest::new(fixture.capability(), recipe())).is_err());
    write_copy_metadata(copy_id);
    let directory = fixture.capability();
    let manifest = directory
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&candidate, recipe()).unwrap();
    let loader = ProjectContentLoader::new(directory).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    let original = snapshot
        .domain_assets()
        .collection::<TextBundleDomain>()
        .unwrap()
        .get(&AssetRef::stable_id(SOURCE).unwrap())
        .unwrap();
    let foreign = AssetProductRef::new(
        StableAssetId::parse_str(copy_id).unwrap(),
        ImportedProductId::parse_subproduct(FIRST).unwrap(),
    )
    .unwrap();
    assert!(original.get(&foreign).is_none());

    let mut copied_scene = caption_scene(FIRST);
    copied_scene.entities[0]
        .components
        .get_mut(&ComponentTypeId::new("consumer.bundle.Caption"))
        .unwrap()
        .value = Caption {
        product: AssetRef::Product(foreign),
    }
    .__encode_persistent_component()
    .unwrap();
    fs::write(
        fixture.root.join("scenes/other.scene.json"),
        copied_scene.to_json_string().unwrap(),
    )
    .unwrap();
    let copied_snapshot = loader
        .load_scene(
            &candidate,
            &plan,
            &ProjectPath::new("other.scene.json").unwrap(),
        )
        .unwrap();
    let record = &copied_snapshot.expanded_scene().entities[0].components
        [&ComponentTypeId::new("consumer.bundle.Caption")];
    let caption = Caption::__decode_persistent_component(&record.value).unwrap();
    let selected = caption.product.as_product().unwrap();
    assert_eq!(selected, foreign);
    let copied_collection = copied_snapshot
        .domain_assets()
        .collection::<TextBundleDomain>()
        .unwrap();
    let copied_bundle = copied_collection
        .get(&AssetRef::StableId(selected.source()))
        .unwrap();
    assert_eq!(copied_bundle.source(), selected.source());
    assert_eq!(
        copied_collection.iter().next().unwrap().0.path().as_str(),
        "copy.json"
    );
    assert_eq!(copied_bundle.get(&selected).unwrap().text, "Hello world");
    check_project(ProjectCheckRequest::new(fixture.capability(), recipe())).unwrap();
}

mod cached_editor_play {
    use super::*;
    use std::sync::mpsc::{self, SyncSender, TryRecvError};

    use nara::{
        ProductConfiguration,
        app::StartupStage,
        ecs::Res,
        project_host::{
            EditorProjectCloseProgress, EditorProjectSession, TrustedCompiledEditorProjectIntent,
        },
        reflect::{ComponentFieldId, ComponentValue},
        scene::{ScenePatchDocument, ScenePatchOperation},
        tooling::{
            EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentMutation,
            EditorNavigationAction, EditorPlayAction, EditorPlayFailure, EditorPlayOperation,
            EditorPlayOperationResult, EditorPlayRequestResult, EditorPlayState,
        },
    };

    #[derive(Clone, Resource)]
    struct Observations(SyncSender<(AssetRef, ProjectDomainAssets)>);

    impl ProductConfiguration for Observations {
        fn write_canonical(&self, output: &mut Vec<u8>) {
            output.extend_from_slice(b"compound-cached-editor-play-v1");
        }
    }

    struct EditorBundlePlugin(Observations);

    impl Plugin for EditorBundlePlugin {
        fn declaration() -> &'static PluginDeclaration {
            BundlePlugin::declaration()
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            // This consumer observes startup without installing the two-Scene travel driver.
            app.insert_resource(self.0.clone())?
                .add_systems(StartupStage::Runtime, observe_products)?;
            Ok(())
        }
    }

    fn observe_products(
        captions: Query<&Caption>,
        assets: Res<ProjectDomainAssets>,
        observations: Res<Observations>,
    ) {
        observations
            .0
            .try_send((
                captions.single().unwrap().product.clone(),
                (*assets).clone(),
            ))
            .unwrap();
    }

    fn dispatch(editor: &mut EditorProjectSession, action: EditorAction) -> EditorActionOutcome {
        let frame = editor.current_frame();
        let mut outcomes = editor
            .dispatch_editor_actions(EditorActionBatch::new(frame.revision(), [action]))
            .unwrap()
            .into_outcomes();
        assert_eq!(outcomes.len(), 1);
        outcomes.pop().unwrap()
    }

    fn request_play(editor: &mut EditorProjectSession) {
        let frame = editor.current_frame();
        let scene = frame.workspace().active_scene.as_ref().unwrap();
        assert_eq!(
            dispatch(
                editor,
                EditorAction::Play(EditorPlayAction::Play {
                    document: scene.document,
                    base_revision: scene.revision,
                }),
            ),
            EditorActionOutcome::Play(EditorPlayRequestResult::Accepted),
        );
    }

    fn drive_until(editor: &mut EditorProjectSession, expected: EditorPlayState) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if editor.drive_editor_frame(Duration::ZERO).play().state() == expected {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let frame = editor.current_frame();
        assert_eq!(frame.play().state(), expected, "{:?}", frame.diagnostics());
    }

    fn edit_product(editor: &mut EditorProjectSession, reference: AssetRef) {
        let frame = editor.current_frame();
        let scene = frame.workspace().active_scene.as_ref().unwrap();
        let result = dispatch(
            editor,
            EditorAction::Mutate(EditorDocumentMutation::ApplyScenePatch {
                document: scene.document,
                base_revision: scene.revision,
                patch: ScenePatchDocument::new([ScenePatchOperation::SetAssetRefField {
                    entity: SceneEntityId::new("caption").unwrap(),
                    component: ComponentTypeId::new("consumer.bundle.Caption"),
                    component_version: ComponentSchemaVersion::ONE,
                    field: ComponentFieldId::new("product"),
                    asset_ref: reference,
                }]),
            }),
        );
        assert!(
            matches!(result, EditorActionOutcome::Workspace(ref report) if report.applied),
            "{result:?}",
        );
        editor.drive_editor_frame(Duration::ZERO);
    }

    fn selected_product(editor: &EditorProjectSession) -> ComponentValue {
        let frame = editor.current_frame();
        frame
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .editor
            .inspector
            .selected_entity_view
            .as_ref()
            .unwrap()
            .components
            .iter()
            .find(|component| component.component.as_str() == "consumer.bundle.Caption")
            .unwrap()
            .fields
            .iter()
            .find(|field| field.id.as_str() == "product")
            .unwrap()
            .value
            .clone()
            .unwrap()
    }

    #[test]
    fn edited_missing_product_rejects_cached_play_without_changing_document_or_resources() {
        let (sender, observations) = mpsc::sync_channel(2);
        let recipe = ProductRecipe::new()
            .add_contribution(
                SchemaContribution::<EditorBundlePlugin>::configured(
                    Observations(sender),
                    |configuration| EditorBundlePlugin(configuration.clone()),
                    [PROVIDER],
                )
                .unwrap(),
            )
            .unwrap()
            .add_asset_domain::<TextBundleDomain>()
            .unwrap();
        let fixture = Fixture::with_recipe(recipe.clone());
        let saved_scene = fs::read(fixture.root.join("scenes/startup.scene.json")).unwrap();
        let mut editor = EditorProjectSession::open_trusted_compiled(
            fixture.capability(),
            TrustedCompiledEditorProjectIntent::new().with_recipe(recipe),
        )
        .unwrap();

        request_play(&mut editor);
        drive_until(&mut editor, EditorPlayState::Running);
        let (selection, original_assets) = observations.try_recv().unwrap();
        assert_eq!(selection, AssetRef::Product(product_ref(FIRST)));
        let original = original_assets.collection::<TextBundleDomain>().unwrap();
        let bundle = original.get(&AssetRef::stable_id(SOURCE).unwrap()).unwrap();
        let encoded = TextBundleDomain::encode_runtime(bundle).unwrap();
        assert_eq!(bundle.get(&product_ref(FIRST)).unwrap().text, "Hello world");
        assert_eq!(
            dispatch(&mut editor, EditorAction::Play(EditorPlayAction::Stop)),
            EditorActionOutcome::Play(EditorPlayRequestResult::Accepted),
        );
        drive_until(&mut editor, EditorPlayState::Empty);
        assert_eq!(
            dispatch(
                &mut editor,
                EditorAction::Play(EditorPlayAction::AcknowledgeResult),
            ),
            EditorActionOutcome::Play(EditorPlayRequestResult::Accepted),
        );

        let document = editor.current_frame().workspace().active_document.unwrap();
        assert!(matches!(dispatch(&mut editor, EditorAction::Navigate(
            EditorNavigationAction::SelectEntity {
                document,
                entity: Some(SceneEntityId::new("caption").unwrap()),
            },
        )), EditorActionOutcome::Workspace(report) if report.applied));
        editor.drive_editor_frame(Duration::ZERO);
        let missing = AssetRef::Product(product_ref("dd195b6a-449c-4e1a-bec5-24aa8d83870c"));
        edit_product(&mut editor, missing.clone());
        let frame = editor.current_frame();
        let document = frame.workspace().active_document.unwrap();
        let edited_scene = frame.workspace().scene(document).unwrap().clone();
        assert!(edited_scene.dirty);
        let edited_property = selected_product(&editor);
        assert_eq!(
            edited_property,
            nara::reflect::encode_asset_reference_value(&missing),
        );

        request_play(&mut editor);
        drive_until(&mut editor, EditorPlayState::Empty);
        let rejected = editor.current_frame();
        assert_eq!(
            rejected.play().result(),
            Some(EditorPlayOperationResult::Failed {
                operation: EditorPlayOperation::Play,
                failure: EditorPlayFailure::Preparation,
            }),
        );
        assert_eq!(rejected.play().generation(), None);
        assert!(rejected.diagnostics().iter().any(|diagnostic| {
            diagnostic.code().as_str() == "project.content.asset-product-missing"
        }));
        assert_eq!(rejected.workspace().active_document, Some(document));
        assert_eq!(rejected.workspace().scene(document), Some(&edited_scene));
        assert_eq!(selected_product(&editor), edited_property);
        assert_eq!(observations.try_recv().unwrap_err(), TryRecvError::Empty);
        assert_eq!(TextBundleDomain::encode_runtime(bundle).unwrap(), encoded);
        assert_eq!(
            fs::read(fixture.root.join("scenes/startup.scene.json")).unwrap(),
            saved_scene,
        );

        assert_eq!(
            dispatch(
                &mut editor,
                EditorAction::Play(EditorPlayAction::AcknowledgeResult),
            ),
            EditorActionOutcome::Play(EditorPlayRequestResult::Accepted),
        );
        // SECOND was not selected by the loaded startup Scene, but exists in its cached bundle.
        edit_product(&mut editor, AssetRef::Product(product_ref(SECOND)));
        request_play(&mut editor);
        drive_until(&mut editor, EditorPlayState::Running);
        let (selection, recovered_assets) = observations.try_recv().unwrap();
        assert_eq!(selection, AssetRef::Product(product_ref(SECOND)));
        let recovered = recovered_assets.collection::<TextBundleDomain>().unwrap();
        assert!(std::ptr::eq(original, recovered));
        assert_eq!(
            recovered
                .get(&AssetRef::stable_id(SOURCE).unwrap())
                .unwrap()
                .get(&product_ref(SECOND))
                .unwrap()
                .text,
            "Goodbye world",
        );

        let deadline = Instant::now() + Duration::from_secs(10);
        while !matches!(
            editor.advance_close(),
            EditorProjectCloseProgress::Complete(_)
        ) {
            assert!(
                Instant::now() < deadline,
                "Editor Session cleanup did not finish",
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

fn product_ref(product: &str) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(SOURCE).unwrap(),
        ImportedProductId::parse_subproduct(product).unwrap(),
    )
    .unwrap()
}

fn caption_scene(product: &str) -> SceneDocument {
    let caption = Caption {
        product: AssetRef::Product(product_ref(product)),
    };
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("caption").unwrap(),
    )
    .with_component(
        ComponentTypeId::new("consumer.bundle.Caption"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            caption.__encode_persistent_component().unwrap(),
        ),
    )])
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        Self::with_recipe(recipe())
    }

    fn with_recipe(recipe: ProductRecipe) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "nara-compound-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        for directory in ["assets", "prefabs", "scenes", "schema"] {
            fs::create_dir(root.join(directory)).unwrap();
        }
        fs::write(root.join("nara.toml"), "schema_version = 1\n[project]\nsource_universe = 'af3176df-4556-45a5-9b2f-334ce47a5fbf'\nnative_document_identity = 'project_location'\nname = 'Compound'\n[startup]\ndefault_scene = 'startup.scene.json'\n[content]\nscenes = ['other.scene.json']\n[authoring]\nschema_bundle = 'schema/bundle.json'\n[runtime]\npreset = 'local-headless'\n[capabilities]\nrequested = ['runtime-2d']\n").unwrap();
        for (path, product) in [("startup.scene.json", FIRST), ("other.scene.json", SECOND)] {
            fs::write(
                root.join("scenes").join(path),
                caption_scene(product).to_json_string().unwrap(),
            )
            .unwrap();
        }
        fs::write(root.join("assets/bundle.json"), RAW).unwrap();
        fs::write(root.join("assets/suffix.txt"), b" world").unwrap();
        let settings = TextBundleSettings::initial(
            RAW,
            64,
            [
                ImportedProductId::parse_subproduct(FIRST).unwrap(),
                ImportedProductId::parse_subproduct(SECOND).unwrap(),
            ],
        )
        .unwrap();
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str(SOURCE).unwrap(),
                AssetPath::new("bundle.json").unwrap(),
                AssetSourceKind::Other(TextBundleDomain::SOURCE_KIND.into()),
            ),
            settings,
        );
        fs::write(
            root.join("assets/bundle.json.meta"),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
        let fixture = Self { root };
        let directory = fixture.capability();
        let file = directory
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&file, None).unwrap();
        let plan = resolve_product_recipe(&candidate, recipe).unwrap();
        fs::write(
            fixture.root.join("schema/bundle.json"),
            plan.schema_validation()
                .authoring_schema_bundle()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fixture
    }

    fn write_scene(&self, path: &str, product: &str) {
        fs::write(
            self.root.join("scenes").join(path),
            caption_scene(product).to_json_string().unwrap(),
        )
        .unwrap();
    }

    fn capability(&self) -> DirectoryCapability {
        #[cfg(windows)]
        let file = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&self.root)
                .unwrap()
        };
        #[cfg(not(windows))]
        let file = fs::File::open(&self.root).unwrap();
        DirectoryCapability::from_host_handle(
            file,
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
        )
        .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara-compound-source-")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
