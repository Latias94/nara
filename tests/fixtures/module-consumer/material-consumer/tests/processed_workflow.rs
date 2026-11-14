use std::sync::atomic::Ordering;

use nara::{
    asset::RuntimeAssetCodec,
    core::{FormatKind, FormatVersion},
    fs::ContentDigest,
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
    palette_product, parameters, publish_payloads, scene_product, write_catalog,
};

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
    let observation = runtime::play(control);
    assert_eq!(observation.asset.product(), material_product());
    assert_eq!(probe.builds.load(Ordering::SeqCst), 1);
    assert_eq!(probe.frames.load(Ordering::SeqCst), 2);
    assert!(!removed_payloads.exists());
}
