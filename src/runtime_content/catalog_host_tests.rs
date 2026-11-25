use std::{fs, io::Write};

use nara_asset::{AssetProductRef, ImportedProductId, StableAssetId};
use nara_core::{FormatKind, FormatVersion};
use nara_scene::{
    RuntimeSceneCandidate, RuntimeSceneLimits, SceneDocument, SourceDocumentKey,
    SourceDocumentKind, runtime_configuration::RuntimeConfiguration,
};

use super::{
    RuntimeContentBase, RuntimeContentCatalog, RuntimeContentCatalogDefinition,
    RuntimeContentCatalogLimits, RuntimeContentError, RuntimeContentExpectation,
    RuntimeContentLimits, RuntimeContentMember, RuntimeContentMemberKind, RuntimeContentTarget,
};
use crate::{
    ProductRecipe,
    fs::{ContentDigest, RelativePath},
    project_host::{
        CompositionError, RuntimeConfigurationPlanError, RuntimePlanError,
        open_explicit_product_project_root, resolve_runtime_configuration,
    },
    startup_scene::compile_test_scene,
};

#[test]
fn core_host_reads_v1_startup_input_metadata_but_refuses_to_open_the_pack() {
    let recipe = ProductRecipe::new();
    let mut configuration = RuntimeConfiguration::default();
    let plan =
        resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe.clone()).unwrap();
    let startup_scene = AssetProductRef::new(
        StableAssetId::parse_str("00000000-0000-4000-8000-000000000001").unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap();
    let source = SourceDocumentKey::stable_asset(startup_scene.source(), SourceDocumentKind::Scene);
    let bindings = plan.schema_validation().bindings();
    let scene = compile_test_scene(&SceneDocument::default(), bindings, source.clone());
    let scene = RuntimeSceneCandidate::cook(
        &scene,
        bindings,
        nara_asset::ProjectAssetDatabase::default().resolver(&nara_asset::AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap()
    .to_json_bytes()
    .unwrap();
    let input =
        include_bytes!("../../crates/nara_input/tests/fixtures/formats/v1/runtime-action-map.json");
    let input_product = AssetProductRef::new(
        StableAssetId::parse_str("919617ea-c89c-4f83-9cce-b084de435a83").unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap();
    configuration.input.action_map = Some(input_product);
    let definition = RuntimeContentCatalogDefinition {
        target: RuntimeContentTarget::new("x86_64-pc-windows-msvc", "release").unwrap(),
        configuration,
        plugin_plan_fingerprint: plan.plugin_plan().fingerprint().as_bytes(),
        schema_fingerprint: plan
            .schema_validation()
            .composition_fingerprint()
            .as_bytes(),
        runtime_codecs_fingerprint: plan.runtime_codecs_fingerprint().as_bytes(),
        startup_scene,
        scenes: vec![startup_scene],
        addressable_roots: vec![],
        members: vec![
            RuntimeContentMember::new(
                "input.runtime.json",
                ContentDigest::of_bytes(input),
                RuntimeContentMemberKind::Asset {
                    format: FormatKind::new("runtime_action_map").unwrap(),
                    version: FormatVersion::ONE,
                },
                vec![input_product],
                vec![],
            )
            .unwrap(),
            RuntimeContentMember::new(
                "startup.runtime.json",
                ContentDigest::of_bytes(&scene),
                RuntimeContentMemberKind::Scene { source },
                vec![startup_scene],
                vec![],
            )
            .unwrap(),
        ],
    };
    let catalog =
        RuntimeContentCatalog::new(definition, RuntimeContentCatalogLimits::default()).unwrap();
    let encoded = catalog.to_json_bytes().unwrap();
    assert_eq!(
        RuntimeContentCatalog::from_json_bytes(&encoded, RuntimeContentCatalogLimits::default())
            .unwrap()
            .configuration()
            .input
            .action_map,
        Some(input_product)
    );

    let root = std::env::temp_dir().join(format!("nara-r7-input-pack-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let result = (|| {
        let mut pack = fs::File::create_new(root.join("content.pack")).unwrap();
        pack.write_all(b"NARAPACK").unwrap();
        pack.write_all(&1_u32.to_le_bytes()).unwrap();
        pack.write_all(&(encoded.len() as u64).to_le_bytes())
            .unwrap();
        pack.write_all(&encoded).unwrap();
        pack.write_all(input).unwrap();
        pack.write_all(&scene).unwrap();
        drop(pack);
        let file = open_explicit_product_project_root(&root)
            .unwrap()
            .open_file(&RelativePath::new("content.pack").unwrap())
            .unwrap();
        let expected = RuntimeContentExpectation::new(
            ContentDigest::of_bytes(&encoded),
            catalog.target().clone(),
        );
        RuntimeContentBase::open_pack(file, &expected, recipe, RuntimeContentLimits::default())
    })();
    fs::remove_file(root.join("content.pack")).unwrap();
    fs::remove_dir(&root).unwrap();
    assert!(matches!(
        result,
        Err(RuntimeContentError::Composition(
            RuntimeConfigurationPlanError::Resolution(RuntimePlanError::Composition(
                CompositionError::InputOwnerRequired
            ),)
        ))
    ));
}
