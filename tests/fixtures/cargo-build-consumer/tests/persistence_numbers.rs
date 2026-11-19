//! Generated schema admission and exact authored numbers in an independent feature graph.

#![cfg(any(windows, target_os = "linux"))]
#![forbid(unsafe_code)]

use std::fs;

use engine::{
    ProductRecipe, SchemaContribution,
    app::{App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId},
    asset::AssetRef,
    fs::RelativePath,
    project_host::{ProjectContentLoader, ingest_project_manifest, resolve_product_recipe},
    project_tooling::host_fs::readable_directory,
    project_workflow::{ProjectCheckRequest, check_project},
    reflect::{
        AuthoringSchemaBundle, ComponentCapability, ComponentFieldId, ComponentFieldPath,
        ComponentFieldSchema, ComponentFloat, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueKind, PersistentComponentSchemaOwnerId,
    },
    scene::{
        PrefabDocument, PrefabDocumentCandidate, PrefabInstance, PrefabInstanceBinding,
        SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId,
        SceneEntityRecord,
    },
};

#[path = "support/workspace.rs"]
#[allow(dead_code)]
mod workspace;

const FIELD_VALUE: f64 = 0.9333333373069763;
const COMPONENT: &str = "consumer.persistence.Ratio";
const SCHEMA: &str = "schema/authoring.bundle.json";
const PREFAB: &str = "sample.prefab.json";

#[derive(Default)]
struct RatioPlugin;

impl Plugin for RatioPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.persistence"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, _: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

#[test]
fn generated_schema_authoring_defaults_and_document_values_keep_exact_f64() {
    let project = workspace::Temporary::new();
    for path in ["assets", "scenes", "prefabs", "schema"] {
        fs::create_dir(project.root.join(path)).unwrap();
    }
    fs::write(
        project.root.join("nara.toml"),
        concat!(
            "schema_version = 1\n[project]\n",
            "source_universe = '551cce45-2058-41e1-8efd-a54c26e14b0c'\n",
            "native_document_identity = 'project_location'\nname = 'Persistence numbers'\n",
            "[startup]\ndefault_scene = 'startup.scene.json'\n",
            "[authoring]\nstate = 'ready'\nschema_bundle = 'schema/authoring.bundle.json'\n",
            "[runtime]\npreset = 'local-headless'\n",
            "[capabilities]\nrequested = ['runtime-2d']\n",
        ),
    )
    .unwrap();

    let value = ComponentValue::F64(ComponentFloat::new(FIELD_VALUE).unwrap());
    let schema_owner = PersistentComponentSchemaOwnerId::try_new(COMPONENT).unwrap();
    let kind = ComponentTypeId::new(COMPONENT);
    let schema = ComponentSchema::new(kind.clone(), "Ratio", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("ratio"),
            "Ratio",
            ComponentFieldPath::from_fields(["ratio"]),
            ComponentValueKind::F64,
            value,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let owner = ComponentSchemaOwnerSnapshot::admit(
        schema_owner.clone(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let recipe = ProductRecipe::new()
        .add_contribution(SchemaContribution::<RatioPlugin>::data_for_default([owner]).unwrap())
        .unwrap();
    let root = readable_directory(&project.root).unwrap();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let admitted = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&admitted, recipe.clone()).unwrap();
    let bundle = plan.schema_validation().authoring_schema_bundle();
    let json = bundle.to_json_string().unwrap();
    fs::write(project.root.join(SCHEMA), &json).unwrap();

    let entity = |name, value| {
        SceneEntityRecord::new(SceneEntityId::new(name).unwrap()).with_component(
            kind.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([(
                    "ratio",
                    ComponentValue::F64(ComponentFloat::new(value).unwrap()),
                )]),
            )
            .with_owner(schema_owner.clone()),
        )
    };
    let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
    anchor.prefab = Some(PrefabInstance {
        binding: PrefabInstanceBinding::Forest,
        source: AssetRef::path(PREFAB).unwrap(),
        arguments: None,
        overrides: None,
    });
    let (scene_value, prefab_value) = (-FIELD_VALUE, FIELD_VALUE / 2.0);
    let scene = SceneDocument::from_ordered_roots([entity("scene", scene_value), anchor])
        .to_json_string()
        .unwrap();
    let prefab = PrefabDocument::from_ordered_roots([entity("prefab", prefab_value)])
        .to_json_string()
        .unwrap();
    fs::write(project.root.join("scenes/startup.scene.json"), &scene).unwrap();
    fs::write(project.root.join("prefabs").join(PREFAB), &prefab).unwrap();
    check_project(ProjectCheckRequest::new(root.share(), recipe.clone())).unwrap();
    assert_eq!(
        AuthoringSchemaBundle::from_json_str(&json).unwrap(),
        *bundle
    );
    assert_eq!(
        SceneDocumentCandidate::decode_json_str(&scene)
            .unwrap()
            .to_json_string()
            .unwrap(),
        scene
    );
    assert_eq!(
        PrefabDocumentCandidate::decode_json_str(&prefab)
            .unwrap()
            .to_json_string()
            .unwrap(),
        prefab
    );

    let loaded = ProjectContentLoader::new(root.share())
        .unwrap()
        .load(&admitted, &plan)
        .unwrap();
    assert_eq!(loaded.scene().to_json_string().unwrap(), scene);
    let [loaded_prefab] = loaded.prefabs() else {
        panic!("expected the one anchored Prefab dependency");
    };
    assert_eq!(loaded_prefab.document().to_json_string().unwrap(), prefab);
    let mut actual_values: Vec<_> = loaded
        .compiled_scene()
        .entities()
        .iter()
        .filter_map(|record| {
            let ratio = record.components().get(&kind)?;
            Some(
                ratio
                    .value()
                    .get("ratio")
                    .unwrap()
                    .as_f64()
                    .unwrap()
                    .to_bits(),
            )
        })
        .collect();
    actual_values.sort_unstable();
    let mut expected_values = [scene_value.to_bits(), prefab_value.to_bits()];
    expected_values.sort_unstable();
    assert_eq!(actual_values, expected_values);
    drop(loaded);

    let changed = json.replace("0.9333333373069763", "0.9333333373069764");
    assert_ne!(changed, json);
    fs::write(project.root.join(SCHEMA), changed).unwrap();
    let error = check_project(ProjectCheckRequest::new(root, recipe)).unwrap_err();
    assert!(
        error
            .content_error()
            .unwrap()
            .diagnostics()
            .iter()
            .any(|entry| {
                entry.code().as_str() == "project.runtime-agreement.selected-owner-divergent"
            }),
        "{error:?}"
    );
}
