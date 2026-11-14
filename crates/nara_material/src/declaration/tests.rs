use super::*;
use crate::MaterialParameters;

#[derive(MaterialParameters)]
#[material(crate = crate)]
struct Palette {
    #[material(id = "palette.tint")]
    tint: glam::Vec4,
}

fn field(id: &str, name: &str, kind: MaterialParameterKind, offset: u32) -> MaterialUniformField {
    MaterialUniformField::new(id, name, kind, offset).unwrap()
}

fn texture(id: &str, name: &str, binding: u32) -> MaterialTextureDeclaration {
    MaterialTextureDeclaration::new(id, name, binding).unwrap()
}

fn palette(textures: Vec<MaterialTextureDeclaration>) -> Material2dDeclaration {
    Material2dDeclaration::from_parameters::<Palette>(
        "example.palette",
        1,
        AlphaMode2d::Blend,
        textures,
    )
    .unwrap()
}

#[test]
fn typed_and_dynamic_authors_construct_the_same_family() {
    let typed = palette(vec![texture("palette.lookup", "lookup_texture", 1)]);
    let dynamic = Material2dDeclaration::new(
        "example.palette",
        1,
        AlphaMode2d::Blend,
        MaterialParameterLayout::new(
            vec![field(
                "palette.tint",
                "tint",
                MaterialParameterKind::Vec4,
                0,
            )],
            16,
        )
        .unwrap(),
        vec![texture("palette.lookup", "lookup_texture", 1)],
    )
    .unwrap();
    assert_eq!(typed, dynamic);
    assert_eq!(typed.id(), "example.palette");
    assert_eq!(typed.version(), 1);
    assert_eq!(typed.alpha_mode(), AlphaMode2d::Blend);
    assert_eq!(typed.textures()[0].id(), "palette.lookup");
    assert_eq!(typed.textures()[0].name(), "lookup_texture");
    assert_eq!(typed.textures()[0].binding(), 1);
    assert_eq!(typed.semantic_digest(), dynamic.semantic_digest());
    assert_eq!(typed.binding_digest(), dynamic.binding_digest());
    assert_ne!(typed.semantic_digest(), typed.binding_digest());
}

#[test]
fn shader_renames_and_abi_relocations_preserve_semantic_identity() {
    let original = palette(vec![texture("palette.lookup", "lookup_texture", 1)]);
    let renamed = Material2dDeclaration::new(
        original.id(),
        original.version(),
        original.alpha_mode(),
        MaterialParameterLayout::new(
            vec![field(
                "palette.tint",
                "renamed_tint",
                MaterialParameterKind::Vec4,
                16,
            )],
            32,
        )
        .unwrap(),
        vec![texture("palette.lookup", "renamed_lookup", 7)],
    )
    .unwrap();
    assert_eq!(original.semantic_digest(), renamed.semantic_digest());
    assert_ne!(original.binding_digest(), renamed.binding_digest());
}

#[test]
fn value_type_and_stable_ids_affect_semantics_independently_of_abi() {
    let original = palette(Vec::new());
    let new_identity = Material2dDeclaration::new(
        original.id(),
        original.version(),
        original.alpha_mode(),
        MaterialParameterLayout::new(
            vec![field(
                "different.tint",
                "tint",
                MaterialParameterKind::Vec4,
                0,
            )],
            16,
        )
        .unwrap(),
        Vec::new(),
    )
    .unwrap();
    assert_ne!(original.semantic_digest(), new_identity.semantic_digest());
    assert_eq!(original.binding_digest(), new_identity.binding_digest());
    let new_type = Material2dDeclaration::new(
        original.id(),
        original.version(),
        original.alpha_mode(),
        MaterialParameterLayout::new(
            vec![field("palette.tint", "tint", MaterialParameterKind::U32, 0)],
            16,
        )
        .unwrap(),
        Vec::new(),
    )
    .unwrap();
    assert_ne!(original.semantic_digest(), new_type.semantic_digest());
    assert_ne!(original.binding_digest(), new_type.binding_digest());
}

#[test]
fn family_version_alpha_and_texture_identity_have_explicit_digest_roles() {
    let original = palette(vec![texture("palette.lookup", "lookup_texture", 1)]);
    for (id, version, alpha) in [
        ("other.palette", 1, AlphaMode2d::Blend),
        ("example.palette", 2, AlphaMode2d::Blend),
        ("example.palette", 1, AlphaMode2d::Opaque),
    ] {
        let changed = Material2dDeclaration::new(
            id,
            version,
            alpha,
            original.parameters().clone(),
            original.textures().to_vec(),
        )
        .unwrap();
        assert_ne!(original.semantic_digest(), changed.semantic_digest());
        assert_eq!(original.binding_digest(), changed.binding_digest());
    }
    let changed = palette(vec![texture("different.lookup", "lookup_texture", 1)]);
    assert_ne!(original.semantic_digest(), changed.semantic_digest());
    assert_eq!(original.binding_digest(), changed.binding_digest());
}

#[test]
fn digests_ignore_input_order_but_binding_tracks_field_placement() {
    let fields = vec![
        field("surface.gain", "gain", MaterialParameterKind::F32, 0),
        field(
            "surface.direction",
            "direction",
            MaterialParameterKind::Vec3,
            16,
        ),
        field("surface.mode", "mode", MaterialParameterKind::U32, 28),
    ];
    let textures = vec![
        texture("surface.color", "color_texture", 1),
        texture("surface.mask", "mask_texture", 3),
    ];
    let original = Material2dDeclaration::new(
        "example.surface",
        3,
        AlphaMode2d::Opaque,
        MaterialParameterLayout::new(fields.clone(), 32).unwrap(),
        textures.clone(),
    )
    .unwrap();
    let reordered = Material2dDeclaration::new(
        original.id(),
        original.version(),
        original.alpha_mode(),
        MaterialParameterLayout::new(fields.into_iter().rev().collect(), 32).unwrap(),
        textures.into_iter().rev().collect(),
    )
    .unwrap();
    assert_eq!(original.semantic_digest(), reordered.semantic_digest());
    assert_eq!(original.binding_digest(), reordered.binding_digest());
    let relocated = Material2dDeclaration::new(
        original.id(),
        original.version(),
        original.alpha_mode(),
        MaterialParameterLayout::new(
            vec![
                field(
                    "surface.direction",
                    "direction",
                    MaterialParameterKind::Vec3,
                    0,
                ),
                field("surface.mode", "mode", MaterialParameterKind::U32, 12),
                field("surface.gain", "gain", MaterialParameterKind::F32, 16),
            ],
            32,
        )
        .unwrap(),
        original.textures().to_vec(),
    )
    .unwrap();
    assert_eq!(original.semantic_digest(), relocated.semantic_digest());
    assert_ne!(original.binding_digest(), relocated.binding_digest());
}

#[test]
fn texture_declarations_reject_empty_and_out_of_range_inputs() {
    for (id, name) in [("", "texture"), (" ", "texture"), ("id", ""), ("id", " \t")] {
        assert!(MaterialTextureDeclaration::new(id, name, 1).is_err());
    }
    for binding in [0, 9, u32::MAX] {
        assert_eq!(
            MaterialTextureDeclaration::new("id", "texture", binding),
            Err(MaterialDeclarationError::InvalidTextureBinding {
                id: "id".into(),
                binding,
            })
        );
    }
}

#[test]
fn material_rejects_duplicate_texture_ids_names_and_bindings() {
    for (textures, expected) in [
        (
            vec![texture("same", "first", 1), texture("same", "second", 2)],
            MaterialDeclarationError::DuplicateTextureId("same".into()),
        ),
        (
            vec![texture("first", "same", 1), texture("second", "same", 2)],
            MaterialDeclarationError::DuplicateTextureName("same".into()),
        ),
        (
            vec![texture("first", "first", 2), texture("second", "second", 2)],
            MaterialDeclarationError::DuplicateTextureBinding(2),
        ),
    ] {
        assert_eq!(
            Material2dDeclaration::from_parameters::<Palette>(
                "example.palette",
                1,
                AlphaMode2d::Blend,
                textures,
            ),
            Err(expected)
        );
    }
}

#[test]
fn material_accepts_eight_textures_and_rejects_nine_before_binding_use() {
    let textures: Vec<_> = (1..=8)
        .map(|binding| {
            texture(
                &format!("texture.{binding}"),
                &format!("texture_{binding}"),
                binding,
            )
        })
        .collect();
    assert_eq!(palette(textures.clone()).textures().len(), 8);
    let mut excess = textures;
    excess.push(texture("extra", "extra", 1));
    assert_eq!(
        Material2dDeclaration::from_parameters::<Palette>(
            "example.palette",
            1,
            AlphaMode2d::Blend,
            excess,
        ),
        Err(MaterialDeclarationError::TooManyTextures { count: 9, max: 8 })
    );
    assert!(matches!(
        Material2dDeclaration::from_parameters::<Palette>(" \t", 1, AlphaMode2d::Blend, Vec::new(),),
        Err(MaterialDeclarationError::EmptyId { target: "material" })
    ));
}

#[test]
fn zero_version_is_rejected_for_typed_and_dynamic_declarations() {
    assert_eq!(
        Material2dDeclaration::from_parameters::<Palette>(
            "example.palette",
            0,
            AlphaMode2d::Blend,
            Vec::new(),
        ),
        Err(MaterialDeclarationError::InvalidVersion(0))
    );
    assert_eq!(
        Material2dDeclaration::new(
            "example.palette",
            0,
            AlphaMode2d::Blend,
            Palette::parameter_layout().unwrap(),
            Vec::new(),
        ),
        Err(MaterialDeclarationError::InvalidVersion(0))
    );
}
