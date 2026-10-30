use std::path::Path;

use nara_asset::{
    AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, ImportedProductId,
    ProjectAssetDatabase, StableAssetId,
};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, ComponentSchemaVersion,
    ComponentTypeId, ComponentValueKind, PersistentComponentSchemaOwnerId,
};
use nara_scene::{SceneComponentRecord, SourceDocumentOrigin};

use super::*;

#[test]
fn source_product_prepares_after_admission_and_exposes_actual_path_and_original_selection() {
    let fixture = Fixture::new();
    let root = fixture.root.clone();
    fixture.with_world(move |world| {
        world.insert_resource(pools());
        let context = Arc::clone(&world.resource::<RequestOwner>().context);
        let Context::Source { loader, .. } = context.as_ref() else {
            unreachable!()
        };
        let expected = CurrentScene::get(world).unwrap().receipt().clone();
        let baseline = loader.budget_snapshot();
        let (gate, control) = TestGate::new(TestPhase::BeforeRead);
        world.resource_mut::<RequestOwner>().gate = Some(gate);
        let request = ProjectScenes::scope(world, |scenes| scenes.request(primary(2))).unwrap();
        assert!(control.wait_until_started());
        ProjectScenes::scope(world, |scenes| {
            assert_eq!(
                scenes.state(request),
                Some(ProjectSceneRequestState::Preparing)
            );
            assert!(scenes.prepared(request).is_none());
            assert!(!scenes.acknowledge(request));
        });
        assert_active_budget(&context, baseline);
        // The Product did not exist when request returned. Discovery belongs to the worker.
        write_metadata(
            &root,
            "scenes",
            "second.scene.json",
            primary(2),
            AssetSourceKind::Scene,
        );
        control.release();
        wait_for_request(world, request, ProjectSceneRequestState::Ready);
        let owner = world.resource::<RequestOwner>();
        let Some(Slot::Ready { admission, content }) = owner.slot.as_ref() else {
            panic!("the source Product must return captured source content")
        };
        assert!(api::content_matches_admission(content, admission));
        let PreparedSceneContent::Source(snapshot) = content else {
            panic!("source requests cannot fall back to processed content")
        };
        assert_eq!(snapshot.selected_scene_product(), Some(primary(2)));
        assert!(matches!(snapshot.selected_scene_source_key().origin(),
            SourceDocumentOrigin::ProjectLocation { path, .. }
                if path.as_str() == "scenes/second.scene.json"));
        assert!(snapshot.content_closure().members().iter().any(|member| {
            member.kind() == crate::project_content::ProjectContentMemberKind::AssetMetadata
                && member.path().as_str() == "scenes/second.scene.json.meta"
        }));
        ProjectScenes::scope(world, |scenes| {
            let view = scenes.prepared(request).unwrap();
            assert_eq!(
                view.selected_scene(),
                &ProjectSceneSelection::Product(primary(2))
            );
            assert_eq!(view.selected_scene_product(), Some(primary(2)));
            assert_eq!(view.selected_scene_path(), Some(&selected()));
            assert_eq!(view.scene().roots, [SceneEntityId::new("second").unwrap()]);
            assert_eq!(view.expected_instance(), &expected);
            assert!(scenes.cancel(request));
            assert!(scenes.acknowledge(request));
        });
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);

        // The view reports original selection, independently of the source's real association.
        let path_request =
            ProjectScenes::scope(world, |scenes| scenes.request(selected())).unwrap();
        wait_for_request(world, path_request, ProjectSceneRequestState::Ready);
        ProjectScenes::scope(world, |scenes| {
            let view = scenes.prepared(path_request).unwrap();
            assert_eq!(
                view.selected_scene(),
                &ProjectSceneSelection::Source(selected())
            );
            assert_eq!(view.selected_scene_path(), Some(&selected()));
            assert_eq!(view.selected_scene_product(), None);
            assert!(scenes.cancel(path_request));
            assert!(scenes.acknowledge(path_request));
        });
        assert_active_budget(&context, baseline);
    });
}

#[test]
fn source_product_dependency_membership_does_not_authorize_travel() {
    let fixture = Fixture::new();
    write_scene(&fixture.root, "hidden.scene.json", "hidden");
    write_metadata(
        &fixture.root,
        "scenes",
        "hidden.scene.json",
        primary(3),
        AssetSourceKind::Scene,
    );
    let startup = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("startup").unwrap(),
    )
    .with_component(
        ComponentTypeId::new(DOOR),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            nara_reflect::encode_asset_reference_value(&AssetRef::Product(primary(3))),
        )
        .with_owner(door_owner_id()),
    )]);
    fs::write(
        fixture.root.join("scenes/startup.scene.json"),
        startup.to_json_string().unwrap(),
    )
    .unwrap();
    let context = fixture.context_with_recipe(door_recipe());
    let Context::Source {
        candidate,
        plan,
        loader,
    } = context.as_ref()
    else {
        unreachable!()
    };
    let snapshot = loader.load(candidate, plan).unwrap();
    assert!(snapshot.content_closure().members().iter().any(|member| {
        member.kind() == crate::project_content::ProjectContentMemberKind::Scene
            && member.path().as_str() == "scenes/hidden.scene.json"
    }));
    let root = fixture.root.clone();
    with_context_world_and_startup(
        context,
        snapshot.scene_source(),
        Some(snapshot),
        move |world| {
            world.insert_resource(pools());
            let expected = CurrentScene::get(world).unwrap().receipt().clone();
            // A captured dependency still cannot activate. Declaration refusal precedes target decode.
            fs::write(root.join("scenes/hidden.scene.json"), b"{ malformed").unwrap();
            let request = ProjectScenes::scope(world, |scenes| scenes.request(primary(3))).unwrap();
            assert_eq!(
                wait_rejected(world, request),
                ProjectSceneRequestError::UndeclaredScene
            );
            ProjectScenes::scope(world, |scenes| {
                assert!(scenes.prepared(request).is_none());
                assert!(
                    scenes
                        .diagnostics(request)
                        .unwrap()
                        .iter()
                        .any(|diagnostic| {
                            diagnostic.code().as_str() == "project.content.scene-not-declared"
                        })
                );
                assert!(scenes.acknowledge(request));
            });
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
            assert_eq!(
                CurrentScene::get(world).unwrap().source().roots,
                [SceneEntityId::new("startup").unwrap()]
            );
        },
    );
}

#[test]
fn source_product_missing_auxiliary_wrong_kind_and_ambiguity_reject_in_preparation() {
    let auxiliary = AssetProductRef::new(
        primary(2).source(),
        ImportedProductId::parse_subproduct("00000000-0000-4000-8000-000000000099").unwrap(),
    )
    .unwrap();
    for (case, product) in [primary(9), auxiliary, primary(3), primary(2)]
        .into_iter()
        .enumerate()
    {
        let fixture = Fixture::new();
        let root = fixture.root.clone();
        fixture.with_world(move |world| {
            world.insert_resource(pools());
            let context = Arc::clone(&world.resource::<RequestOwner>().context);
            let Context::Source { loader, .. } = context.as_ref() else {
                unreachable!()
            };
            let baseline = loader.budget_snapshot();
            let expected = CurrentScene::get(world).unwrap().receipt().clone();
            let (gate, control) = TestGate::new(TestPhase::BeforeRead);
            world.resource_mut::<RequestOwner>().gate = Some(gate);
            let request = ProjectScenes::scope(world, |scenes| scenes.request(product)).unwrap();
            assert!(control.wait_until_started());
            assert_eq!(
                ProjectScenes::scope(world, |scenes| scenes.state(request)),
                Some(ProjectSceneRequestState::Preparing)
            );
            match case {
                0 => {} // Missing actual source association.
                1 => write_metadata(
                    &root,
                    "scenes",
                    "second.scene.json",
                    primary(2),
                    AssetSourceKind::Scene,
                ),
                2 => {
                    fs::write(
                        root.join("prefabs/other.prefab.json"),
                        nara_scene::PrefabDocument::from_ordered_roots([])
                            .to_json_string()
                            .unwrap(),
                    )
                    .unwrap();
                    write_metadata(
                        &root,
                        "prefabs",
                        "other.prefab.json",
                        primary(3),
                        AssetSourceKind::Prefab,
                    );
                }
                3 => {
                    write_metadata(
                        &root,
                        "scenes",
                        "second.scene.json",
                        primary(2),
                        AssetSourceKind::Scene,
                    );
                    write_scene(&root, "duplicate.scene.json", "duplicate");
                    write_metadata(
                        &root,
                        "scenes",
                        "duplicate.scene.json",
                        primary(2),
                        AssetSourceKind::Scene,
                    );
                }
                _ => unreachable!(),
            }
            control.release();
            assert!(matches!(
                wait_rejected(world, request),
                ProjectSceneRequestError::Content(_)
            ));
            ProjectScenes::scope(world, |scenes| {
                assert!(scenes.prepared(request).is_none());
                assert!(scenes.diagnostics(request).unwrap().has_errors());
                assert!(scenes.acknowledge(request));
            });
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
            assert_eq!(
                CurrentScene::get(world).unwrap().source().roots,
                [SceneEntityId::new("startup").unwrap()]
            );
            assert_active_budget(&context, baseline);
        });
    }
}

#[test]
fn source_product_ready_and_queued_publication_never_rediscover_or_retarget() {
    for managed in [false, true] {
        let fixture = Fixture::new();
        let root = fixture.root.clone();
        fixture.with_world(move |world| {
            world.insert_resource(pools());
            write_metadata(
                &root,
                "scenes",
                "second.scene.json",
                primary(2),
                AssetSourceKind::Scene,
            );
            let expected = CurrentScene::get(world).unwrap().receipt().clone();
            let request = ProjectScenes::scope(world, |scenes| scenes.request(primary(2))).unwrap();
            wait_for_request(world, request, ProjectSceneRequestState::Ready);
            if managed {
                begin_prepared_project_scene_travel(world, request, limits(), &[], |_| {}).unwrap();
            }
            // The disk now associates the requested Product with startup and different data.
            // Neither Ready commit nor the queued native publication may follow that new lookup.
            write_metadata(
                &root,
                "scenes",
                "second.scene.json",
                primary(4),
                AssetSourceKind::Scene,
            );
            write_metadata(
                &root,
                "scenes",
                "startup.scene.json",
                primary(2),
                AssetSourceKind::Scene,
            );
            write_scene(&root, "second.scene.json", "changed-after-capture");
            write_scene(&root, "startup.scene.json", "retargeted");
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
            if managed {
                world.resource_mut::<nara_app::RealTime>().frame += 1;
                poll_project_scene_requests(world);
            } else {
                ProjectScenes::scope(world, |scenes| {
                    let view = scenes.prepared(request).unwrap();
                    assert_eq!(view.selected_scene_product(), Some(primary(2)));
                    assert_eq!(view.selected_scene_path(), Some(&selected()));
                    assert_eq!(view.scene().roots, [SceneEntityId::new("second").unwrap()]);
                });
                let report = commit_prepared_project_scene(world, request, limits(), &[], |_| {});
                assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
                assert!(report.instance.is_some());
            }
            ProjectScenes::scope(world, |scenes| {
                assert_eq!(
                    scenes.state(request),
                    Some(ProjectSceneRequestState::Committed)
                );
                assert!(scenes.acknowledge(request));
            });
            let current = CurrentScene::get(world).unwrap();
            assert_ne!(current.receipt().instance_id(), expected.instance_id());
            assert_eq!(
                current.source().roots,
                [SceneEntityId::new("second").unwrap()]
            );
            assert_eq!(
                world
                    .resource::<ProjectAssetDatabase>()
                    .record_for_stable_id(primary(2).source())
                    .unwrap()
                    .path()
                    .as_str(),
                "scenes/second.scene.json"
            );
        });
    }
}

#[test]
fn source_product_publication_rejects_foreign_candidate_definitions_and_selection() {
    for changed_candidate in [false, true] {
        let fixture = Fixture::new();
        let root = fixture.root.clone();
        fixture.with_world(move |world| {
            world.insert_resource(pools());
            write_metadata(
                &root,
                "scenes",
                "second.scene.json",
                primary(2),
                AssetSourceKind::Scene,
            );
            let request = ProjectScenes::scope(world, |scenes| scenes.request(primary(2))).unwrap();
            wait_for_request(world, request, ProjectSceneRequestState::Ready);
            let context = Arc::clone(&world.resource::<RequestOwner>().context);
            let Context::Source {
                candidate,
                plan,
                loader,
            } = context.as_ref()
            else {
                unreachable!()
            };
            let expected = CurrentScene::get(world).unwrap().receipt().clone();
            let mut foreign_candidate = candidate.clone();
            if changed_candidate {
                let manifest_path = root.join("nara.toml");
                let manifest = fs::read_to_string(&manifest_path).unwrap();
                fs::write(
                    manifest_path,
                    manifest.replace("Scene Request", "Different Candidate"),
                )
                .unwrap();
                foreign_candidate = ingest_project_manifest(
                    &loader
                        .project_root()
                        .open_file(&RelativePath::new("nara.toml").unwrap())
                        .unwrap(),
                    None,
                )
                .unwrap();
                assert_ne!(foreign_candidate.lineage(), candidate.lineage());
            }
            let foreign_plan =
                resolve_product_recipe(&foreign_candidate, crate::ProductRecipe::new()).unwrap();
            // The live root and Ready candidate already occupy this Host's two generations.
            // A foreign preparation has its own budget owner while sharing the actual root.
            let foreign_loader = ProjectContentLoader::new(loader.project_root()).unwrap();
            let foreign = foreign_loader
                .load_scene_product(&foreign_candidate, &foreign_plan, primary(2))
                .unwrap();
            assert_eq!(foreign.selected_scene_product(), Some(primary(2)));
            assert_eq!(foreign.selected_scene_path(), &selected());
            if !changed_candidate {
                assert_eq!(
                    foreign.schema_fingerprint(),
                    plan.schema_validation().composition_fingerprint()
                );
                assert!(!foreign.shares_component_bindings(plan.schema_validation().bindings()));
            }
            {
                let mut owner = world.resource_mut::<RequestOwner>();
                let Some(Slot::Ready { admission, content }) = owner.slot.as_mut() else {
                    panic!("the original request is Ready")
                };
                let other_selection = Admission {
                    context: Arc::clone(&admission.context),
                    id: admission.id,
                    expected: admission.expected.clone(),
                    selection: primary(4).into(),
                };
                assert!(!api::content_matches_admission(content, &other_selection));
                let foreign = PreparedSceneContent::Source(foreign);
                assert!(!api::content_matches_admission(&foreign, admission));
                *content = foreign;
            }
            ProjectScenes::scope(world, |scenes| assert!(scenes.prepared(request).is_none()));
            let report = commit_prepared_project_scene(world, request, limits(), &[], |_| {
                panic!("foreign captured facts cannot configure publication")
            });
            assert!(report.instance.is_none());
            assert!(report.diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "project.scene.prepared-input-mismatch"
            }));
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
            ProjectScenes::scope(world, |scenes| assert!(scenes.acknowledge(request)));
        });
    }
}

#[test]
fn source_product_stale_context_and_current_receipt_preserve_last_good() {
    for change_context in [false, true] {
        let fixture = Fixture::new();
        let root = fixture.root.clone();
        fixture.with_world(move |world| {
            world.insert_resource(pools());
            write_metadata(
                &root,
                "scenes",
                "second.scene.json",
                primary(2),
                AssetSourceKind::Scene,
            );
            let request = ProjectScenes::scope(world, |scenes| scenes.request(primary(2))).unwrap();
            wait_for_request(world, request, ProjectSceneRequestState::Ready);
            let mut expected = CurrentScene::get(world).unwrap().receipt().clone();
            if change_context {
                let mut owner = world.resource_mut::<RequestOwner>();
                let Context::Source {
                    candidate,
                    plan,
                    loader,
                } = owner.context.as_ref()
                else {
                    unreachable!()
                };
                owner.context = Arc::new(Context::Source {
                    candidate: candidate.clone(),
                    plan: plan.clone(),
                    loader: Arc::clone(loader),
                });
            } else {
                let report = crate::startup_scene::restart_current_scene_with_product(
                    world,
                    &expected,
                    limits(),
                    &[],
                    |_| {},
                );
                assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
                let restarted = report.instance.unwrap();
                assert_ne!(restarted, expected);
                expected = restarted;
            }
            ProjectScenes::scope(world, |scenes| {
                assert_eq!(
                    scenes.state(request),
                    Some(ProjectSceneRequestState::Rejected(
                        ProjectSceneRequestError::Stale,
                    ))
                );
                assert!(scenes.prepared(request).is_none());
            });
            let report = commit_prepared_project_scene(world, request, limits(), &[], |_| {
                panic!("stale context or receipt cannot configure publication")
            });
            assert!(report.instance.is_none());
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
            assert_eq!(
                CurrentScene::get(world).unwrap().source().roots,
                [SceneEntityId::new("startup").unwrap()]
            );
            ProjectScenes::scope(world, |scenes| assert!(scenes.acknowledge(request)));
        });
    }
}

#[test]
fn source_product_cancellation_waits_for_physical_return_and_refunds_capture() {
    let fixture = Fixture::new();
    let root = fixture.root.clone();
    fixture.with_world(move |world| {
        world.insert_resource(pools());
        write_metadata(
            &root,
            "scenes",
            "second.scene.json",
            primary(2),
            AssetSourceKind::Scene,
        );
        let context = Arc::clone(&world.resource::<RequestOwner>().context);
        let Context::Source { loader, .. } = context.as_ref() else {
            unreachable!()
        };
        let baseline = loader.budget_snapshot();
        let expected = CurrentScene::get(world).unwrap().receipt().clone();
        for phase in [TestPhase::BeforeRead, TestPhase::AfterPreparation] {
            let (gate, control) = TestGate::new(phase);
            world.resource_mut::<RequestOwner>().gate = Some(gate);
            let request = ProjectScenes::scope(world, |scenes| scenes.request(primary(2))).unwrap();
            assert!(control.wait_until_started());
            ProjectScenes::scope(world, |scenes| assert!(scenes.cancel(request)));
            for _ in 0..3 {
                poll_project_scene_requests(world);
                ProjectScenes::scope(world, |scenes| {
                    assert_eq!(
                        scenes.state(request),
                        Some(ProjectSceneRequestState::Cancelling)
                    );
                    assert!(!scenes.cancel(request));
                    assert!(!scenes.acknowledge(request));
                    assert!(scenes.prepared(request).is_none());
                    assert_eq!(
                        scenes.request(primary(2)),
                        Err(ProjectSceneRequestError::Busy)
                    );
                });
            }
            if phase == TestPhase::AfterPreparation {
                assert_eq!(
                    loader.budget_snapshot().active_generations(),
                    baseline.active_generations() + 1
                );
            }
            control.release();
            wait_for_request(world, request, ProjectSceneRequestState::Cancelled);
            ProjectScenes::scope(world, |scenes| assert!(scenes.acknowledge(request)));
            assert_active_budget(&context, baseline);
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
        }
    });
}

fn primary(id: u64) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(format!("00000000-0000-4000-8000-{id:012x}")).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn write_metadata(
    root: &Path,
    directory: &str,
    path: &str,
    product: AssetProductRef,
    kind: AssetSourceKind,
) {
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(product.source(), AssetPath::new(path).unwrap(), kind),
        (),
    );
    fs::write(
        root.join(directory).join(format!("{path}.meta")),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
}

fn write_scene(root: &Path, path: &str, entity: &str) {
    let scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new(entity).unwrap(),
    )]);
    fs::write(
        root.join("scenes").join(path),
        scene.to_json_string().unwrap(),
    )
    .unwrap();
}

fn limits() -> SceneProductTransactionLimits {
    SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE)
}

fn wait_rejected(world: &mut World, request: ProjectSceneRequestId) -> ProjectSceneRequestError {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        poll_project_scene_requests(world);
        match ProjectScenes::scope(world, |scenes| scenes.state(request)) {
            Some(ProjectSceneRequestState::Rejected(error)) => return error,
            Some(ProjectSceneRequestState::Preparing) => {}
            state => panic!("source preparation must reject instead of publishing {state:?}"),
        }
        assert!(
            Instant::now() < deadline,
            "source Product preparation did not reject"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

const DOOR: &str = "test.source-scene.Door";

fn door_owner_id() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.source-scene").unwrap()
}

#[derive(Default)]
struct DoorPlugin;

impl nara_app::Plugin for DoorPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        static DECLARATION: nara_app::PluginDeclaration = nara_app::PluginDeclaration::new(
            nara_app::PluginId::new("test.source-scene"),
            nara_app::PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

fn door_recipe() -> crate::ProductRecipe {
    let schema = ComponentSchema::new(
        ComponentTypeId::new(DOOR),
        "Door",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("target"),
        "Target",
        ComponentFieldPath::empty(),
        ComponentValueKind::AssetRef,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::AssetRef)]);
    let owner = ComponentSchemaOwnerSnapshot::admit(
        door_owner_id(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    crate::ProductRecipe::new()
        .add_contribution(
            crate::SchemaContribution::<DoorPlugin>::data_for_default([owner]).unwrap(),
        )
        .unwrap()
}
