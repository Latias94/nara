#[allow(unused_imports)]
#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;
#[path = "support/reference_project_copy.rs"]
mod reference_project_copy;
#[allow(dead_code, unused_imports)]
#[path = "support/ui_product_fixture.rs"]
mod ui_product_fixture;

use std::fs;

use nara::{
    reflect::{
        ComponentFieldId, ComponentFieldPath, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue,
    },
    scene::{PrefabDocument, ScenePatchDocument, ScenePatchOperation},
};
use project_content_fixture::try_load_project_content_from_path;
use reference_project_copy::ReferenceProjectCopy;
use ui_product_fixture::{
    CANONICAL_UI_TARGET, ProductUiCapture, SemanticCapture, capture_readability_case_at,
};

const RETRY_BUTTON: &str = "retry-button";
const EXPANDED_RETRY_BUTTON: &str = "reference-ui/retry-button";
const UI_NODE: &str = "nara.ui.UiNode";
const UI_BUTTON: &str = "nara.ui.UiButton";
const EDITED_RETRY_WIDTH: f64 = 216.0;
const EDITED_RETRY_ACTION: &str = "reference-game.retry-authored";

#[test]
fn canonical_prefab_edit_reaches_reloaded_runtime_adapter_without_rust_changes() {
    let project = ReferenceProjectCopy::new("ui-authoring-roundtrip");
    let baseline = capture_readability_case_at(project.path(), CANONICAL_UI_TARGET);
    let baseline_retry = action_node(&baseline, nara_reference_game::REFERENCE_RETRY_ACTION);
    assert_ne!(f64::from(baseline_retry.width), EDITED_RETRY_WIDTH);

    let encoded = {
        let loaded = try_load_project_content_from_path(project.path())
            .expect("isolated reference project must load");
        let prefab = loaded
            .snapshot
            .prefabs()
            .iter()
            .find(|prefab| prefab.path().as_str() == "reference-ui.prefab.json")
            .expect("reference UI prefab must be present");
        let mut document = prefab.document().instantiate();
        let patch = ScenePatchDocument::new([
            ScenePatchOperation::SetField {
                entity: scene_id(RETRY_BUTTON),
                component: ComponentTypeId::new(UI_NODE),
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("style.width.value"),
                value: ComponentValue::f64(EDITED_RETRY_WIDTH)
                    .expect("edited width must be finite"),
            },
            ScenePatchOperation::SetField {
                entity: scene_id(RETRY_BUTTON),
                component: ComponentTypeId::new(UI_BUTTON),
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("action"),
                value: ComponentValue::String(EDITED_RETRY_ACTION.to_owned()),
            },
        ]);
        let report = patch.apply_to_scene(
            &mut document,
            loaded.plan.schema_validation().registry(),
        );
        assert!(report.applied, "typed prefab patch must apply: {report:#?}");

        let edited = PrefabDocument::new(document.roots, document.entities);
        format!(
            "{}\n",
            edited
                .to_json_string()
                .expect("edited prefab must use the canonical writer")
        )
    };
    fs::write(
        project.file("prefabs/reference-ui.prefab.json"),
        encoded,
    )
    .expect("edited prefab must be writable");

    let reloaded = try_load_project_content_from_path(project.path())
        .expect("edited reference project must reload through the ordinary loader");
    let source_prefab = reloaded
        .snapshot
        .prefabs()
        .iter()
        .find(|prefab| prefab.path().as_str() == "reference-ui.prefab.json")
        .expect("edited reference UI prefab must remain present");
    assert_eq!(
        retry_width(&source_prefab.document().entities, RETRY_BUTTON),
        EDITED_RETRY_WIDTH
    );
    assert_eq!(
        retry_width(
            &reloaded.snapshot.expanded_startup_scene().entities,
            EXPANDED_RETRY_BUTTON,
        ),
        EDITED_RETRY_WIDTH
    );
    drop(reloaded);

    let edited = capture_readability_case_at(project.path(), CANONICAL_UI_TARGET);
    let edited_retry = action_node(&edited, EDITED_RETRY_ACTION);
    assert_eq!(f64::from(edited_retry.width), EDITED_RETRY_WIDTH);
    assert_eq!(edited_retry.key, baseline_retry.key);
    assert_ne!(edited_retry.actions, baseline_retry.actions);
}

fn action_node<'a>(capture: &'a ProductUiCapture, action: &str) -> &'a SemanticCapture {
    capture
        .semantic_facts
        .iter()
        .find(|fact| fact.actions.iter().any(|candidate| candidate.as_str() == action))
        .expect("expected authored action must remain semantic")
}

fn retry_width(entities: &[nara::scene::SceneEntityRecord], entity_id: &str) -> f64 {
    component_in(entities, entity_id)
        .value
        .get_path(&ComponentFieldPath::from_fields(["style", "width", "value"]))
        .expect("UiNode width path must exist")
        .as_f64()
        .expect("UiNode width must be a finite float")
}

fn component_in<'a>(
    entities: &'a [nara::scene::SceneEntityRecord],
    entity_id: &str,
) -> &'a nara::scene::SceneComponentRecord {
    entities
        .iter()
        .find(|entity| entity.id.as_str() == entity_id)
        .and_then(|entity| entity.components.get(&ComponentTypeId::new(UI_NODE)))
        .expect("target UiNode component must exist")
}

fn scene_id(value: &str) -> nara::scene::SceneEntityId {
    nara::scene::SceneEntityId::new(value).expect("reference UI entity ID must be valid")
}
