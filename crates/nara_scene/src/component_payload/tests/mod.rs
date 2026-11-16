use std::{convert::Infallible, ptr, sync::Arc};

use nara_reflect::{
    ComponentFieldId, ComponentRegistryBuilder, ComponentSchemaVersion, ComponentTypeId,
    PersistentComponentSchemaOwnerId,
};

use super::*;
use crate::{
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityProvenance, SceneEntityRecord,
    compiled::admit_draft,
    draft::{ExpandedSceneEntity, SceneDraft},
    schema_authority::SchemaAuthority,
};

#[cfg(feature = "serde")]
mod wire;

fn value() -> ComponentValue {
    ComponentValue::map([(
        "items",
        ComponentValue::List(vec![
            ComponentValue::String("payload".repeat(256)),
            ComponentValue::U64(7),
        ]),
    )])
}

fn text(value: &ComponentValue) -> &str {
    let ComponentValue::Map(fields) = value else {
        panic!("expected a map payload");
    };
    let ComponentValue::List(items) = &fields["items"] else {
        panic!("expected a list payload");
    };
    items[0].as_str().unwrap()
}

fn record() -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value())
        .with_owner(PersistentComponentSchemaOwnerId::try_new("test.payload").unwrap())
        .with_contextual_default(
            ComponentFieldId::new("hidden"),
            ComponentValue::String("default".repeat(128)),
        )
}

#[test]
fn private_payload_clones_share_reads_and_isolate_mutation() {
    let original = ComponentPayload::from(value());
    let mut shared = original.clone();
    let independent = original.deep_clone();
    assert!(ptr::eq(&*original, &*shared));
    assert_eq!(independent, original);
    assert!(!ptr::eq(&*original, &*independent));
    assert_ne!(text(&original).as_ptr(), text(&independent).as_ptr());
    assert_eq!(format!("{original:?}"), format!("{:?}", &*original));

    let ComponentValue::Map(fields) = shared.make_mut() else {
        panic!("expected a map payload");
    };
    fields.insert("changed".to_owned(), ComponentValue::Bool(true));
    assert!(!ptr::eq(&*original, &*shared));
    assert_ne!(shared, original);
    assert_eq!(&*original, &value());
    assert_eq!(independent, original);
}

#[test]
fn public_record_and_document_clones_do_not_retain_original_storage() {
    let component = ComponentTypeId::new("test.Payload");
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("root").unwrap(),
    )
    .with_component(component.clone(), record())]);
    let original = &document.entities[0].components[&component];
    let cloned_record = original.clone();
    let cloned_document = document.clone();
    assert_eq!(document, cloned_document);
    let field = ComponentFieldId::new("hidden");
    for cloned in [
        &cloned_record,
        &cloned_document.entities[0].components[&component],
    ] {
        assert_eq!(original, cloned);
        assert!(!ptr::eq(original.value(), cloned.value()));
        assert_ne!(
            text(original.value()).as_ptr(),
            text(cloned.value()).as_ptr()
        );
        assert_ne!(
            original.owner().unwrap().as_str().as_ptr(),
            cloned.owner().unwrap().as_str().as_ptr(),
        );
        assert_ne!(
            original.contextual_defaults[&field]
                .as_str()
                .unwrap()
                .as_ptr(),
            cloned.contextual_defaults[&field]
                .as_str()
                .unwrap()
                .as_ptr(),
        );
    }
    let (_, _, shared) = original.clone_for_compilation().into_compiled_parts();
    let original_root = Arc::downgrade(&shared.0);
    drop(shared);
    drop(document);
    assert!(original_root.upgrade().is_none());
    assert_eq!(cloned_record.value(), &value());
    assert_eq!(
        cloned_document.entities[0].components[&component].value(),
        &value(),
    );
}

#[test]
fn compiler_copy_shares_only_the_main_payload_and_isolates_source_edits() {
    let mut source = record();
    let draft = source.clone_for_compilation();
    assert!(ptr::eq(source.value(), draft.value()));
    assert_eq!(source, draft);
    assert_ne!(
        source.owner().unwrap().as_str().as_ptr(),
        draft.owner().unwrap().as_str().as_ptr(),
    );
    let field = ComponentFieldId::new("hidden");
    assert_ne!(
        source.contextual_defaults[&field]
            .as_str()
            .unwrap()
            .as_ptr(),
        draft.contextual_defaults[&field].as_str().unwrap().as_ptr(),
    );

    let ComponentValue::Map(fields) = source.value_mut() else {
        panic!("expected a map payload");
    };
    fields.insert("changed".to_owned(), ComponentValue::Bool(true));
    source
        .contextual_defaults
        .insert(field, ComponentValue::Null);
    assert!(!ptr::eq(source.value(), draft.value()));
    assert_eq!(draft, record());
    source.set_value(ComponentValue::Null);
    assert_eq!(source.value(), &ComponentValue::Null);
    assert_eq!(draft.value(), &value());
}

#[test]
fn owned_extraction_moves_unique_values_and_copies_shared_values() {
    for shared in [false, true] {
        let mut source = record();
        let observer = shared.then(|| source.clone_for_compilation());
        let original_text = text(source.value()).as_ptr();
        let taken = source.take_value();
        assert_eq!(source.value(), &ComponentValue::Null);
        assert_eq!(taken, value());
        assert_eq!(text(&taken).as_ptr() == original_text, !shared);
        if let Some(observer) = observer {
            assert_eq!(observer.value(), &value());
            assert_eq!(text(observer.value()).as_ptr(), original_text);
        }

        source.set_value(taken);
        let observer = shared.then(|| source.clone_for_compilation());
        let original_text = text(source.value()).as_ptr();
        let mut owned = source.into_value();
        assert_eq!(owned, value());
        assert_eq!(text(&owned).as_ptr() == original_text, !shared);
        let ComponentValue::Map(fields) = &mut owned else {
            panic!("expected a map payload");
        };
        fields.insert("changed".to_owned(), ComponentValue::Bool(true));
        if let Some(observer) = observer {
            assert_eq!(observer.value(), &value());
        }
    }
}

#[test]
fn sealed_and_prepared_storage_survives_source_edits_and_owner_retirement() {
    let mut registry = ComponentRegistryBuilder::new();
    crate::register_scene_components(&mut registry).unwrap();
    let registry = registry.freeze().unwrap();
    let component = ComponentTypeId::new("nara.scene.Name");
    let id = SceneEntityId::new("root").unwrap();
    let original = ComponentValue::String("immutable".repeat(256));
    let mut source = SceneComponentRecord::new(ComponentSchemaVersion::ONE, original.clone());
    let draft = SceneDraft::new(
        [id.clone()],
        [ExpandedSceneEntity {
            id: id.clone(),
            children: Vec::new(),
            components: [(component.clone(), source.clone_for_compilation())]
                .into_iter()
                .collect(),
        }],
    );
    let document = admit_draft(
        draft,
        [(id.clone(), SceneEntityProvenance::local(id))]
            .into_iter()
            .collect(),
        SchemaAuthority::Native(&registry),
        Default::default(),
        None,
        |_, _, _| Ok::<_, Infallible>(()),
    )
    .unwrap();
    let compiled = &document.entities()[0].components()[&component];
    assert!(ptr::eq(source.value(), compiled.value()));
    let prepared = compiled.copy_for_native_preparation();
    assert!(ptr::eq(compiled.value(), prepared.value()));
    let backing = source.value_storage_retained_bytes();
    assert_eq!(compiled.value_storage_retained_bytes(), backing);
    assert_eq!(prepared.value_storage_retained_bytes(), backing);

    *source.value_mut() = ComponentValue::String("edited".to_owned());
    assert!(!ptr::eq(source.value(), compiled.value()));
    assert_eq!(compiled.value(), &original);
    drop(source);
    drop(document);
    assert_eq!(prepared.value(), &original);
    assert_eq!(prepared.value_storage_retained_bytes(), backing);
}
