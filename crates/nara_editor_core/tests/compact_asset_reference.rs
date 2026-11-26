use nara_asset::{AssetProductRef, AssetRef, ImportedProductId};
use nara_core::{AssetId, vfs::AssetPath};
use nara_editor_core::value_draft::parse_scalar;
use nara_reflect::{
    AssetReferenceDemand, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueShape,
    encode_asset_reference_value,
};
use nara_scene::authoring::{
    CompactValueError, SceneAsset, SceneComponent, SceneDocument, SceneDocumentError, SceneEntity,
};

const SOURCE_UUID: &str = "0193a100-1111-7000-8000-000000000001";
const COMPONENT: &str = "game.ImageLocator";

fn field() -> ComponentFieldSchema {
    ComponentFieldSchema::required(
        ComponentFieldId::new("image"),
        "Image",
        ComponentFieldPath::from_fields(["image"]),
        ComponentValueShape::asset_ref(AssetReferenceDemand::Immediate),
    )
}

fn catalog(field: ComponentFieldSchema) -> [ComponentSchema; 1] {
    [ComponentSchema::new(
        ComponentTypeId::new(COMPONENT),
        "Image Locator",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([field])]
}

fn scene(value: ComponentValue) -> SceneDocument {
    let mut scene = SceneDocument::new("01234567-89ab-cdef-0123-456789abcdef".parse().unwrap());
    let mut row = SceneEntity::new(
        "c62cea0c-0539-4cef-93d5-71c8ff7ee454".parse().unwrap(),
        "Source",
    );
    row.components.insert(
        ComponentTypeId::new(COMPONENT),
        SceneComponent::known(ComponentSchemaVersion::ONE, value),
    );
    scene.entities.push(row);
    scene
}

#[test]
fn native_locator_and_inspector_save_the_same_id_and_resolve_aliases() {
    let field = field();
    let id: AssetId = SOURCE_UUID.parse().unwrap();
    let from_inspector = parse_scalar(field.shape(), SOURCE_UUID).unwrap();
    let from_native = encode_asset_reference_value(&AssetRef::StableId(id.into())).unwrap();
    assert_eq!(from_inspector, ComponentValue::AssetReference(id));
    assert_eq!(from_inspector, from_native);

    let mut scene = scene(ComponentValue::map([("image", from_native)]));
    let catalog = catalog(field);
    for wire in [SOURCE_UUID, "@source"] {
        if wire.starts_with('@') {
            scene.assets.insert(
                "source".into(),
                SceneAsset {
                    id,
                    hint: AssetPath::new("textures/new.png").unwrap(),
                },
            );
        }
        let saved = scene.to_ron_with(&catalog).unwrap();
        assert!(saved.contains(&format!("image: \"{wire}\"")), "{saved}");
        assert_eq!(SceneDocument::parse_with(&saved, &catalog).unwrap(), scene);
    }
}

#[test]
fn compact_never_accepts_paths_products_or_unrepresentable_defaults() {
    let id: AssetId = SOURCE_UUID.parse().unwrap();
    let schemas = catalog(field());
    let stable = scene(ComponentValue::map([(
        "image",
        ComponentValue::AssetReference(id),
    )]));
    let valid = stable.to_ron_with(&schemas).unwrap();
    for locator in [
        AssetRef::path("textures/image.png").unwrap(),
        AssetRef::Product(AssetProductRef::new(id.into(), ImportedProductId::PRIMARY).unwrap()),
    ] {
        let locator_text = locator.to_string();
        assert!(parse_scalar(field().shape(), &locator_text).is_err());
        let value = encode_asset_reference_value(&locator).unwrap();
        assert!(field().accepts_value(&value));
        let source = scene(ComponentValue::map([("image", value.clone())]));
        assert!(matches!(
            source.to_ron_with(&schemas),
            Err(SceneDocumentError::InvalidComponentValue {
                source: CompactValueError::InvalidAssetId(rejected),
                ..
            }) if rejected == locator_text
        ));
        let with_default = catalog(field().with_default_value(value));
        assert!(source.to_ron_with(&with_default).is_err());
        let invalid = valid.replace(SOURCE_UUID, &locator_text);
        assert!(SceneDocument::parse_with(&invalid, &schemas).is_err());
        let omitted = valid.replace(&format!("image: \"{SOURCE_UUID}\""), "");
        assert!(SceneDocument::parse_with(&omitted, &with_default).is_err());
    }
    let nil = AssetRef::stable_id("00000000-0000-0000-0000-000000000000").unwrap();
    assert!(encode_asset_reference_value(&nil).is_err());
}
