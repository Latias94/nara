use std::{convert::Infallible, fs::File, sync::atomic::Ordering};

use nara::{
    asset::{AssetRef, RuntimeAssetCodec},
    core::{FormatKind, FormatVersion},
    fs::{ContentDigest, FileCapability, TrustMode},
    material::{Material2dDomain, Material2dRuntimeCodec},
    project_workflow::cooked_content::CookedContentError,
    runtime_content::{
        RuntimeContentBase, RuntimeContentCatalog, RuntimeContentCatalogError,
        RuntimeContentCatalogLimits, RuntimeContentDependency, RuntimeContentDependencyRole,
        RuntimeContentError, RuntimeContentExpectation, RuntimeContentLimits, RuntimeContentMember,
        RuntimeContentMemberKind, RuntimeContentTarget,
    },
};

#[path = "processed_workflow/project.rs"]
mod project;
#[path = "processed_workflow/runtime.rs"]
mod runtime;

use project::{
    Fixture, assert_image, authority, catalog_definition, indices_product, material_product,
    palette_product, parameters, publish_material_payloads, publish_payloads, scene_product,
    write_catalog,
};

const MATERIAL_BINDING_MISMATCH: &str = "runtime.content.material-binding-mismatch";

#[test]
fn source_free_palette_materializes_its_real_scene_and_material_only_texture() {
    let fixture = Fixture::new();
    let prepared = fixture.assemble(true).unwrap();
    let catalog = prepared.base().catalog();
    assert_eq!(catalog.members().len(), 4);
    assert_eq!(catalog.scenes(), [scene_product()]);
    assert!(catalog.addressable_roots().is_empty());
    let required =
        |product| RuntimeContentDependency::new(product, RuntimeContentDependencyRole::Required);
    let mut scene_dependencies = [required(material_product()), required(indices_product())];
    scene_dependencies.sort_unstable_by_key(RuntimeContentDependency::product);
    assert_eq!(
        catalog
            .member_for_product(scene_product())
            .unwrap()
            .dependencies(),
        scene_dependencies,
    );
    assert_eq!(
        catalog
            .member_for_product(material_product())
            .unwrap()
            .dependencies(),
        [required(palette_product())],
    );
    for product in [indices_product(), palette_product()] {
        assert!(
            catalog
                .member_for_product(product)
                .unwrap()
                .dependencies()
                .is_empty()
        );
    }
    let expected_shader = fixture.material.generation().shader().clone();
    let expected_declaration = fixture.material.generation().declaration().clone();
    let encoded = Material2dRuntimeCodec::encode_runtime(&fixture.material).unwrap();
    let probe = fixture.probe.clone();
    probe.assert_not_started();
    drop(fixture);
    assert!(!probe.removed_root.exists());
    let base = prepared.into_base();
    let validation = base.validate_deep().unwrap();
    assert_eq!(validation.members(), 4);
    assert_eq!(validation.products(), 4);
    probe.assert_not_started();
    let acquired = base
        .acquire_asset::<Material2dDomain, Material2dRuntimeCodec>(material_product())
        .unwrap();
    assert_eq!(
        Material2dRuntimeCodec::encode_runtime(acquired.product()).unwrap(),
        encoded
    );
    assert_eq!(acquired.product().generation().shader(), &expected_shader);
    assert!(acquired.product().generation().inputs().is_empty());
    drop(acquired);
    let observation = runtime::play(base);
    assert_eq!(probe.builds.load(Ordering::SeqCst), 1);
    assert_eq!(probe.frames.load(Ordering::SeqCst), 2);
    assert!(!probe.removed_root.exists());
    assert_eq!(
        observation.asset.generation().declaration(),
        &expected_declaration
    );
    assert_eq!(observation.asset.generation().shader(), &expected_shader);
    assert_eq!(
        observation.binding.generation(),
        observation.asset.generation()
    );
    assert_eq!(observation.asset.default_parameters(), &parameters(0.0));
    assert_eq!(observation.material.parameters(), &parameters(2.0));
    assert_eq!(observation.binding.parameters(), &parameters(2.0));
    assert_image(&observation.images[0], indices_product());
    assert_image(&observation.images[1], palette_product());
}

#[test]
fn omitted_palette_member_rejects_the_actual_material_closure_before_gameplay() {
    let fixture = Fixture::new();
    let error = fixture.assemble(false).unwrap_err();
    assert!(matches!(
        *error,
        CookedContentError::Runtime(RuntimeContentError::Catalog(
            RuntimeContentCatalogError::MissingDependencyProduct { product, .. }
        )) if product == palette_product()
    ));
    fixture.probe.assert_not_started();
    let control = fixture.assemble(true).unwrap();
    assert_eq!(control.base().validate_deep().unwrap().members(), 4);
    fixture.probe.assert_not_started();
}

#[test]
fn scene_material_declaration_mismatch_rejects_finish_and_independent_pack() {
    let v1 = Fixture::new();
    let v2 = Fixture::with_declaration_version(2);
    assert_eq!(v1.material.product(), v2.material.product());
    let builder = v1.builder_with_material(&v2.material, true).unwrap();
    let CookedContentError::Runtime(error) = builder.finish().unwrap_err() else {
        panic!("finish must refuse the Scene binding to the other declaration version");
    };
    assert_material_binding_refusal(error);

    let control = v1.assemble(true).unwrap();
    let (payloads, expectation) = publish_material_payloads(control.base(), &v2.material);
    let recipe = v1.recipe.clone();
    let probe = v1.probe.clone();
    drop(control);
    drop(v1);
    let limits = RuntimeContentLimits::default();
    let captured = RuntimeContentBase::open(
        &authority(payloads.path()),
        &expectation,
        recipe.clone(),
        limits,
    )
    .unwrap();
    let path = payloads.path().join("content.pack");
    captured
        .write_pack(&mut File::create_new(&path).unwrap())
        .unwrap();
    drop(captured);
    let file =
        FileCapability::from_host_handle(File::open(path).unwrap(), TrustMode::TrustedLocal, 0)
            .unwrap();
    let pack = RuntimeContentBase::open_pack(file, &expectation, recipe, limits).unwrap();
    let acquired = pack
        .acquire_asset::<Material2dDomain, Material2dRuntimeCodec>(material_product())
        .expect("the matching Product and its Material payload must individually admit");
    assert_eq!(acquired.product().generation().declaration().version(), 2);
    drop(acquired);
    assert_material_binding_refusal(pack.validate_deep().unwrap_err());
    runtime::assert_material_binding_refused(pack);
    probe.assert_not_started();

    let compatible = v2.assemble(true).unwrap().into_base();
    assert_eq!(compatible.validate_deep().unwrap().members(), 4);
    drop(v2);
    let observation = runtime::play(compatible);
    assert_eq!(observation.material.declaration_version(), 2);
}

fn assert_material_binding_refusal(error: RuntimeContentError) {
    assert!(
        matches!(&error, RuntimeContentError::Asset(error)
            if error.diagnostic().code().as_str() == MATERIAL_BINDING_MISMATCH),
        "{error:?}",
    );
}

#[test]
fn scene_texture_override_requires_an_admitted_image_before_building_the_player() {
    let fixture = Fixture::with_non_image_override();
    let CookedContentError::Runtime(error) = *fixture.assemble(true).unwrap_err() else {
        panic!("finish must refuse the Scene override pointing to a Material Product");
    };
    assert_material_binding_refusal(error);
    fixture.probe.assert_not_started();
    let compatible = Fixture::new();
    let control = compatible.assemble(true).unwrap();
    let (payloads, expectation) = fixture.publish_scene_payload(control.base());
    let recipe = compatible.recipe.clone();
    let probe = compatible.probe.clone();
    drop(fixture);
    drop(control);
    drop(compatible);
    let limits = RuntimeContentLimits::default();
    let captured = RuntimeContentBase::open(
        &authority(payloads.path()),
        &expectation,
        recipe.clone(),
        limits,
    )
    .unwrap();
    let path = payloads.path().join("content.pack");
    captured
        .write_pack(&mut File::create_new(&path).unwrap())
        .unwrap();
    drop(captured);
    let file =
        FileCapability::from_host_handle(File::open(path).unwrap(), TrustMode::TrustedLocal, 0)
            .unwrap();
    let pack = RuntimeContentBase::open_pack(file, &expectation, recipe, limits).unwrap();
    assert_material_binding_refusal(pack.validate_deep().unwrap_err());
    runtime::assert_material_binding_refused(pack);
    probe.assert_not_started();
}

#[test]
fn scene_in_material_texture_slot_rejects_finish_and_independent_encoded_base() {
    let fixture = Fixture::new();
    let material = fixture
        .material
        .resolve_texture_references(|reference| {
            assert_eq!(*reference, AssetRef::Product(palette_product()));
            Ok::<_, Infallible>(scene_product())
        })
        .unwrap();
    let builder = fixture.builder_with_material(&material, false).unwrap();
    let CookedContentError::Runtime(error) = builder.finish().unwrap_err() else {
        panic!("the completed builder must reject the Material texture's target kind");
    };
    assert_scene_texture_refusal(error);
    fixture.probe.assert_not_started();

    let control = fixture.assemble(true).unwrap();
    let (payloads, expectation) = publish_material_payloads(control.base(), &material);
    let recipe = fixture.recipe.clone();
    let probe = fixture.probe.clone();
    drop(material);
    drop(control);
    drop(fixture);
    assert!(!probe.removed_root.exists());
    let base = RuntimeContentBase::open(
        &authority(payloads.path()),
        &expectation,
        recipe,
        RuntimeContentLimits::default(),
    )
    .expect("independently encoded payloads and matching catalog edges must admit");
    let removed_payloads = payloads.path().to_path_buf();
    drop(payloads);
    assert!(!removed_payloads.exists());
    assert_eq!(base.catalog().members().len(), 3);
    assert!(base.member(palette_product()).is_none());
    assert!(matches!(
        base.member(scene_product()).unwrap().kind(),
        RuntimeContentMemberKind::Scene { .. },
    ));
    assert_eq!(
        base.member(material_product()).unwrap().dependencies(),
        [RuntimeContentDependency::new(
            scene_product(),
            RuntimeContentDependencyRole::Required,
        )],
    );
    assert_eq!(
        base.acquire_asset::<Material2dDomain, Material2dRuntimeCodec>(material_product())
            .expect("the actual Material payload must agree with its declared edges")
            .product()
            .textures()["palette"],
        AssetRef::Product(scene_product()),
    );
    probe.assert_not_started();
    assert_scene_texture_refusal(base.validate_deep().unwrap_err());
    probe.assert_not_started();
}

fn assert_scene_texture_refusal(error: RuntimeContentError) {
    assert!(matches!(
        &error,
        RuntimeContentError::MaterialTextureKindMismatch { material, slot, target }
            if *material == material_product() && slot == "palette" && *target == scene_product(),
    ));
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains(&material_product().to_string()),
        "{diagnostic}",
    );
    assert!(diagnostic.contains("palette"), "{diagnostic}");
    assert!(
        diagnostic.contains(&scene_product().to_string()),
        "{diagnostic}",
    );
    assert!(diagnostic.contains("expected Image kind"), "{diagnostic}");
}

#[test]
fn the_same_real_payloads_reject_unselected_material_format_version_and_wrong_host_target() {
    let fixture = Fixture::new();
    let prepared = fixture.assemble(true).unwrap();
    let payloads = publish_payloads(prepared.base());
    let recipe = fixture.recipe.clone();
    let probe = fixture.probe.clone();
    let control = RuntimeContentBase::open(
        &authority(payloads.path()),
        prepared.expectation(),
        recipe.clone(),
        RuntimeContentLimits::default(),
    )
    .unwrap();
    assert_eq!(control.catalog_digest(), prepared.base().catalog_digest());
    drop(fixture);
    assert!(!probe.removed_root.exists());
    for (format, version) in [
        (
            FormatKind::new("consumer_unselected_material2d").unwrap(),
            FormatVersion::ONE,
        ),
        (
            FormatKind::new(Material2dRuntimeCodec::RUNTIME_KIND).unwrap(),
            FormatVersion::new(2).unwrap(),
        ),
    ] {
        let mut definition = catalog_definition(prepared.base());
        let member = definition
            .members
            .iter_mut()
            .find(|member| member.products().contains(&material_product()))
            .unwrap();
        *member = RuntimeContentMember::new(
            member.path(),
            member.digest(),
            RuntimeContentMemberKind::Asset { format, version },
            member.products().to_vec(),
            member.dependencies().to_vec(),
        )
        .unwrap();
        let catalog =
            RuntimeContentCatalog::new(definition, RuntimeContentCatalogLimits::default()).unwrap();
        write_catalog(&payloads, &catalog);
        let expected = RuntimeContentExpectation::new(
            ContentDigest::of_bytes(&catalog.to_json_bytes().unwrap()),
            prepared.expectation().target().clone(),
        );
        assert!(matches!(
            RuntimeContentBase::open(
                &authority(payloads.path()),
                &expected,
                recipe.clone(),
                RuntimeContentLimits::default(),
            ),
            Err(RuntimeContentError::CodecNotSelected),
        ));
        probe.assert_not_started();
    }
    write_catalog(&payloads, prepared.base().catalog());
    let wrong_target = RuntimeContentExpectation::new(
        prepared.expectation().catalog_digest(),
        RuntimeContentTarget::new("other-host", "processed-palette").unwrap(),
    );
    assert!(matches!(
        RuntimeContentBase::open(
            &authority(payloads.path()),
            &wrong_target,
            recipe,
            RuntimeContentLimits::default(),
        ),
        Err(RuntimeContentError::TargetMismatch),
    ));
    probe.assert_not_started();
    drop(prepared);
    let removed_payloads = payloads.path().to_path_buf();
    drop(payloads);
    assert!(!removed_payloads.exists());
    let validation = control.validate_deep().unwrap();
    assert_eq!(validation.members(), 4);
    assert_eq!(validation.products(), 4);
    probe.assert_not_started();
    let observation = runtime::play(control);
    assert_eq!(observation.asset.product(), material_product());
    assert_eq!(probe.builds.load(Ordering::SeqCst), 1);
    assert_eq!(probe.frames.load(Ordering::SeqCst), 2);
    assert!(!removed_payloads.exists());
}
