use super::*;

fn sources() -> InMemoryPrefabSourceResolver {
    let mut n = SceneEntityRecord::new(id("n"));
    n.prefab = Some(PrefabInstance::forest(source("recursive")));
    InMemoryPrefabSourceResolver::new().with_prefab(
        source("recursive"),
        key("recursive"),
        PrefabDocument::from_ordered_roots([n]),
    )
}

fn scene() -> SceneDocument {
    let mut overlay = PrefabOverlay::new(key("recursive"));
    overlay.instances.push(PrefabInstanceClaim {
        target: target(&[("n", "recursive")], "n"),
        value: None,
    });
    let mut instance = PrefabInstance::forest(source("recursive"));
    instance.overrides = Some(overlay);
    let mut host = SceneEntityRecord::new(id("host"));
    host.prefab = Some(instance);
    let outer = target(&[("host", "recursive")], "n");
    let inner = target(&[("host", "recursive"), ("n", "recursive")], "n");
    // These are actual containing-source placements, rather than overlay placements.
    // The same canonical Prefab is entered twice before the outer claim detaches n.
    SceneDocument::new(
        [
            SceneEntityPlacement::root(id("host")),
            SceneEntityPlacement::new(
                target(&[], "deep_child"),
                ScenePlacement::first(ScenePlacementParent::Entity(inner.clone())),
            ),
            SceneEntityPlacement::new(
                target(&[], "z_before"),
                ScenePlacement::before(ScenePlacementParent::Entity(outer.clone()), inner.clone()),
            ),
            SceneEntityPlacement::new(
                target(&[], "a_after"),
                ScenePlacement::after(ScenePlacementParent::Entity(outer), inner),
            ),
        ],
        [
            host,
            SceneEntityRecord::new(id("deep_child")),
            SceneEntityRecord::new(id("z_before")),
            SceneEntityRecord::new(id("a_after")),
        ],
    )
}

#[test]
fn source_placements_enter_finite_same_source_recursion_for_parent_and_sibling_order() {
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    let (report, requests) = compile_with_requests(&scene(), &sources(), &registry);
    let document = compiled(report);
    assert_eq!(requests, [source("recursive"), source("recursive")]);
    assert_eq!(document.entities().len(), 6);
    assert_eq!(document.canonical_provenance().len(), 6);
    assert_eq!(document.roots(), &[id("host")]);
    let outer = projected(&document, "host", &[("recursive", "n")]);
    let inner = projected(&document, "host", &[("recursive", "n"), ("recursive", "n")]);
    assert_ne!(
        outer, inner,
        "canonical source equality must not collapse occurrences",
    );
    assert_eq!(entity(&document, &id("host")).children(), &[outer.clone()],);
    assert_eq!(
        entity(&document, &outer).children(),
        &[id("z_before"), inner.clone(), id("a_after")],
    );
    assert_eq!(entity(&document, &inner).children(), &[id("deep_child")]);
    for local in ["z_before", "a_after", "deep_child"] {
        assert!(entity(&document, &id(local)).children().is_empty());
        assert!(
            document.canonical_provenance()[&id(local)]
                .projection_path()
                .is_none()
        );
    }
}

#[test]
fn removing_terminal_claim_refuses_the_same_source_recursion_with_routed_placements() {
    let mut scene = scene();
    scene
        .entities
        .iter_mut()
        .find(|record| record.id == id("host"))
        .unwrap()
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap()
        .instances
        .clear();
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    let (report, requests) = compile_with_requests(&scene, &sources(), &registry);
    assert!(
        report.candidate.is_none(),
        "a cycle must not publish a flat document",
    );
    assert!(report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert_eq!(
        report.failure.as_ref().map(|failure| *failure.reason()),
        Some(SceneCompilationFailureKind::OwnerAdmission),
        "{:?}",
        report.diagnostics,
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "scene.prefab-cycle"),
        "the unchanged recursion must be refused as a cycle: {:?}",
        report.diagnostics,
    );
    assert!(
        requests.len() > 2,
        "the cycle lies beyond the finite control's terminal claim",
    );
    assert!(
        requests
            .iter()
            .all(|requested| requested == &source("recursive")),
    );
}
