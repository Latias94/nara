use nara_asset::{AssetPath, AssetRecord, AssetSourceKind, StableAssetId};

use super::*;
use crate::material::{
    AlphaMode2d, Material2dDeclaration, Material2dDomain, Material2dGeneration,
    MaterialParameterKind, MaterialParameterLayout, MaterialTextureDeclaration,
    MaterialUniformField,
};

const BODY: &str = r#"
fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    return sampled * tint * material_parameters.tint;
}
"#;

fn product(last: u8) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(&format!("84d00000-0000-4000-8000-{last:012}")).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn declaration(revision: u32, textures: Vec<MaterialTextureDeclaration>) -> Material2dDeclaration {
    Material2dDeclaration::new(
        "test.material",
        revision,
        AlphaMode2d::Blend,
        MaterialParameterLayout::new(
            vec![
                MaterialUniformField::new("tint", "tint", MaterialParameterKind::Vec4, 0).unwrap(),
            ],
            16,
        )
        .unwrap(),
        textures,
    )
    .unwrap()
}

fn generation(declaration: Material2dDeclaration, path: &str, body: &str) -> Material2dGeneration {
    Material2dGeneration::from_shader_inputs(
        declaration,
        vec![(AssetPath::new(path).unwrap(), Arc::from(body))],
    )
    .unwrap()
}

fn material(product: AssetProductRef) -> Material2dAsset {
    Material2dAsset::new(
        product,
        generation(declaration(1, Vec::new()), "material.wgsl", BODY),
        BTreeMap::from([(
            "tint".to_owned(),
            MaterialParameterValue::Vec4([0.25, 0.0, 0.75, 1.0]),
        )]),
        BTreeMap::new(),
    )
    .unwrap()
}

#[test]
fn sharing_compares_full_content_across_independent_generations() {
    let original = material(product(1));
    let independent = material(product(1));
    assert_ne!(original.generation(), independent.generation());
    assert!(same_material(&original, &independent));

    for changed in [
        generation(declaration(2, Vec::new()), "material.wgsl", BODY),
        generation(declaration(1, Vec::new()), "renamed.wgsl", BODY),
        generation(
            declaration(1, Vec::new()),
            "material.wgsl",
            &BODY.replace("sampled * tint", "sampled + tint"),
        ),
    ] {
        let changed = Material2dAsset {
            generation: changed,
            ..original.clone()
        };
        assert!(!same_material(&original, &changed));
    }
    assert!(!same_material(&original, &material(product(2))));

    let generation = generation(
        declaration(
            1,
            vec![MaterialTextureDeclaration::new("overlay", "overlay_texture", 1).unwrap()],
        ),
        "material.wgsl",
        BODY,
    );
    let textured = |reference| {
        Material2dAsset::new(
            original.product(),
            generation.clone(),
            original.default_parameters().clone(),
            BTreeMap::from([("overlay".to_owned(), reference)]),
        )
        .unwrap()
    };
    let source = textured(AssetRef::Path(AssetPath::new("overlay.png").unwrap()));
    let canonical = textured(AssetRef::Product(product(3)));
    assert!(!same_material(&source, &canonical));
    assert!(same_material(
        &canonical,
        &textured(AssetRef::Product(product(3)))
    ));
}

#[test]
fn sharing_compares_float_parameter_bits() {
    use MaterialParameterValue::{F32, Vec2, Vec3, Vec4};

    for (positive, negative) in [
        (F32(0.0), F32(-0.0)),
        (Vec2([1.0, 0.0]), Vec2([1.0, -0.0])),
        (Vec3([0.0, 1.0, 2.0]), Vec3([-0.0, 1.0, 2.0])),
        (Vec4([1.0, 2.0, 3.0, 0.0]), Vec4([1.0, 2.0, 3.0, -0.0])),
    ] {
        assert!(!same_parameter(positive, negative));
        assert!(same_parameter(negative, negative));
    }
}

#[test]
fn replacement_checks_every_binding_before_publishing_or_retiring() {
    let incoming = material(product(1));
    let incumbent = material(product(2));
    let outgoing = material(product(3));
    let source_record = |path| {
        AssetRecord::new(
            incumbent.product().source(),
            AssetSourcePath::new(path).unwrap(),
            AssetSourceKind::Other(Material2dDomain::SOURCE_KIND.to_owned()),
        )
    };
    let original_path = source_record("original.material.json");
    let relocated_path = source_record("relocated.material.json");
    let mut live = AssetServer::new();
    let first = live.reserve_product(incoming.product()).unwrap();
    let moved = live.reserve_record(&original_path).unwrap();
    let retired = live.reserve_product(outgoing.product()).unwrap();
    let mut values = Assets::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let input = |handle, path, value: &Material2dAsset, server: &AssetServer| {
        Material2dPublicationInput::resolve::<Material2dPublicationError>(
            handle,
            path,
            value,
            server,
            Arc::new(()),
            Arc::new(()),
            |_| unreachable!("test Material declares no texture references"),
        )
    };
    let owned = PreparedMaterial2dPublications::prepare(
        vec![
            input(moved, Some(original_path.path().clone()), &incumbent, &live),
            input(retired, None, &outgoing, &live),
        ],
        std::iter::empty(),
        Material2dPublicationMode::Replace,
        &live,
        &values,
        &states,
    )
    .unwrap()
    .commit(&live, &mut values, &mut states, &mut events)
    .unwrap();
    let mut staged = live.clone();
    staged
        .relocate_primary_source(original_path.path(), &relocated_path)
        .unwrap();
    let replacement = |values: &Assets<Material2dAsset>, states: &AssetStates| {
        PreparedMaterial2dPublications::prepare(
            vec![
                input(first, None, &incoming, &staged),
                input(
                    moved,
                    Some(relocated_path.path().clone()),
                    &incumbent,
                    &staged,
                ),
            ],
            owned.iter(),
            Material2dPublicationMode::Replace,
            &live,
            values,
            states,
        )
        .unwrap()
    };
    let prior_slot = values.slot_revision(moved);
    let prior_events = events.iter().count();
    let prepared = replacement(&values, &states);
    assert!(prepared.validate(&live, &values, &states).is_ok());
    assert_eq!(
        prepared.validate(&staged, &values, &states),
        Err(Material2dPublicationError::Superseded),
    );
    assert!(matches!(
        prepared.commit(&live, &mut values, &mut states, &mut events),
        Err(Material2dPublicationError::Superseded),
    ));
    assert!(values.get(first).is_none());
    assert!(states.state(first.id()).is_none());
    assert!(values.get(retired).is_some());
    assert_eq!(values.slot_revision(moved), prior_slot);
    assert_eq!(events.iter().count(), prior_events);

    let receipts = replacement(&values, &states)
        .commit(&staged, &mut values, &mut states, &mut events)
        .unwrap();
    assert_eq!(
        receipts
            .iter()
            .map(Material2dContentReceipt::handle)
            .collect::<Vec<_>>(),
        [first, moved]
    );
    assert!(values.get(retired).is_none());
    assert_ne!(values.slot_revision(moved), prior_slot);
    assert_eq!(events.iter().count(), prior_events + 3);
}
