use super::*;

use nara::{diagnostic::DiagnosticValueRef, scene::PrefabInstanceClaim};

const RECURSIVE_SOURCE: &str = "prefabs/recursive.prefab.json";
const ANCHOR: &str = "recursive-mount";
const LEAF: &str = "base-root";

fn self_hop(key: &SourceDocumentKey) -> PrefabInstanceHop {
    PrefabInstanceHop {
        anchor: PrefabEntityOrigin::SourceEntity(id(ANCHOR)),
        expected_base: key.clone(),
    }
}

fn recursive_prefab(key: &SourceDocumentKey) -> PrefabDocument {
    let recursive = PrefabInstance::forest(AssetRef::path("recursive.prefab.json").unwrap());
    let mut cutoff = PrefabOverlay::new(key.clone());
    cutoff.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id(ANCHOR)),
        value: None,
    });
    cutoff.components.push(PrefabComponentIntent::Suppress {
        target: PrefabEntityTarget::source(id(LEAF)),
        component: ComponentTypeId::new(COMPONENT),
    });

    let mut outer = PrefabOverlay::new(key.clone());
    outer.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget {
            through: vec![self_hop(key)],
            entity: PrefabEntityOrigin::SourceEntity(id(ANCHOR)),
        },
        value: Some(PrefabInstance {
            overrides: Some(cutoff),
            ..recursive.clone()
        }),
    });
    let mut anchor = SceneEntityRecord::new(id(ANCHOR));
    anchor.prefab = Some(PrefabInstance {
        overrides: Some(outer),
        ..recursive
    });
    PrefabDocument::from_ordered_roots([anchor].into_iter().chain(base(INITIAL_SCORE).entities))
}

#[test]
fn a_registered_panel_rejects_a_set_that_breaks_a_finite_self_reference() {
    let project = text_project::TextProject::new();
    let key = SourceDocumentKey::project_location(
        "24ed5e36-1b37-5fa3-9c42-232066b17916".parse().unwrap(),
        AssetSourcePath::new(RECURSIVE_SOURCE).unwrap(),
        SourceDocumentKind::Prefab,
    );
    let path = project.root.join(RECURSIVE_SOURCE);
    let original = recursive_prefab(&key).to_json_string().unwrap();
    fs::write(&path, &original).unwrap();
    fs::write(
        project.root.join("schema/authoring.bundle.json"),
        schema().to_json_string().unwrap(),
    )
    .unwrap();

    let mut app = Application(EditorApplication::new(EditorLaunchOptions::new(
        &project.root,
    )));
    app.native_shell_ready();
    wait(&mut app, |current| {
        !current.project_browser().has_pending_work()
    });
    let active = frame(&app).workspace().active_document.unwrap();
    let document = open_source(&mut app, RECURSIVE_SOURCE);
    assert_ne!(document, active);
    submit(
        &mut app,
        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document: active }),
    );
    let before = frame(&app);
    let expected_basis = basis(&app, document);
    assert_eq!(before.workspace().active_document, Some(active));
    assert_eq!(
        before.workspace().document(document).unwrap().kind,
        EditorDocumentKind::Prefab
    );

    let mut panel = PanelDriver::register(&mut app);
    let inspection = start(&mut panel, &mut app, document, ANCHOR);
    expect_phase(&mut app, inspection, Phase::Ready);
    let address = panel.observe(&mut app, |context| {
        let snapshot = context.prefab_effective_observation(inspection).unwrap();
        let entity = PrefabEntityTarget {
            through: vec![self_hop(&key); 2],
            entity: PrefabEntityOrigin::SourceEntity(id(LEAF)),
        };
        let component = ComponentTypeId::new(COMPONENT);
        let property = property("score");
        let mut deeper = entity.clone();
        deeper.through.push(self_hop(&key));
        assert!(matches!(
            snapshot.target(&deeper).unwrap().component(&component),
            Err(AuthoringReadError::ComponentNotFound)
        ));
        let mut budget = ComponentPropertyReadBudget::new(
            ItemLimit::new(8).unwrap(),
            ByteLimit::new(1024).unwrap(),
        );
        let observed = snapshot
            .target(&entity)
            .unwrap()
            .component(&component)
            .unwrap()
            .property_value(&property, &mut budget)
            .unwrap();
        assert!(observed.controlling_claim().is_none());
        let AuthoringPrefabPropertyValue::Present(value) = observed.value else {
            panic!("the shallower recursive occurrence must inherit score");
        };
        assert_eq!(
            value
                .copy_value(ComponentValueMaterializationLimits::new(
                    ItemLimit::ONE,
                    ByteLimit::new(128).unwrap(),
                    DepthLimit::ONE,
                ))
                .unwrap(),
            ComponentValue::U64(INITIAL_SCORE)
        );
        EditorPrefabPropertyAddress {
            entity,
            component,
            property,
        }
    });

    assert_eq!(
        panel.dispatch(&mut app, |_| Action::PreparePropertyWrite {
            inspection,
            address,
            intent: PrefabPropertyIntent::Set(ComponentValue::U64(CHANGED_SCORE)),
            limits: PrefabDeclarationLimits::default(),
        }),
        Outcome::Accepted {
            inspection,
            phase: Phase::PreparingEdit,
        }
    );
    wait(&mut app, |current| {
        !current.prefab_inspection().phase.work_pending()
    });
    let after = frame(&app);
    let view = after.prefab_inspection();
    assert_eq!(view.phase, Phase::Terminal, "{view:?}");
    let Some(EditorPrefabInspectionResult::Failed {
        inspection: actual,
        reason: Rejection::SourceRejected,
        diagnostics,
    }) = &view.result
    else {
        panic!("the nested Set must fail candidate composition: {view:?}");
    };
    assert_eq!(*actual, inspection);
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.prefab-compilation-failed"
                && diagnostic.fields().iter().any(|field| {
                    field.key().as_str() == "failure-kind"
                        && field.value() == DiagnosticValueRef::Identifier("component")
                })
        }),
        "{diagnostics:?}"
    );
    assert_eq!(basis(&app, document), expected_basis);
    assert_eq!(after.workspace().active_document, Some(active));
    assert_eq!(
        after.workspace().document(document),
        before.workspace().document(document)
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    acknowledge(&mut panel, &mut app, inspection);
    assert!(app.close());
}
