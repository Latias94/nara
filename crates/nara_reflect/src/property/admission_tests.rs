use super::*;
use crate::{ComponentShapeMember, ComponentTypeId};

fn id(value: &str) -> ComponentFieldId {
    ComponentFieldId::new(value)
}

fn schema(shape: ComponentValueShape) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("test.PropertyAdmission"),
        "Property admission",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([ComponentFieldSchema::required(
        id("root"),
        "Root",
        ComponentFieldPath::from_fields(["root"]),
        shape,
    )])
}

fn budget(entries: usize, bytes: usize) -> ComponentPropertyReadBudget {
    ComponentPropertyReadBudget::new(
        ItemLimit::new(entries).unwrap(),
        ByteLimit::new(bytes).unwrap(),
    )
}

#[test]
fn admission_counts_the_same_selector_walk_and_preserves_the_shared_budget() {
    let mut schema = schema(ComponentValueShape::optional(ComponentValueShape::map(
        ComponentValueShape::optional(ComponentValueShape::record_members([
            ComponentShapeMember::new("wire_image", ComponentValueShape::AssetRef)
                .with_id(id("image")),
        ])),
    )))
    .with_field_tombstones([id("gone")]);
    schema.fields[0]
        .member_tombstones
        .push(ComponentMemberPath::new([
            ComponentMemberPathSegment::MapEntry,
            ComponentMemberPathSegment::Member(id("gone")),
        ]));
    let path = ComponentPropertyPath::new(id("root")).with_steps([
        ComponentPropertyStep::MapEntry("a.b".into()),
        ComponentPropertyStep::Member(id("image")),
    ]);
    let mut ordinary = budget(9, 42);
    let expected =
        resolve_component_property_schema_claim(&schema, schema.version(), &path, &mut ordinary)
            .unwrap();
    let mut shared = budget(18, 84);
    let mut observed = (0, 0, 0);
    for _ in 0..2 {
        let selected = resolve_component_property_schema_claim_with_admission(
            &schema,
            schema.version(),
            &path,
            &mut shared,
            |nodes, bytes, allocated| {
                observed.0 += nodes;
                observed.1 += bytes;
                observed.2 += allocated;
                Ok::<_, ()>(())
            },
        )
        .unwrap();
        assert!(std::ptr::eq(selected.field, expected.field));
        assert!(std::ptr::eq(selected.shape, expected.shape));
    }
    assert_eq!(observed.0, 2 * ordinary.scanned_entries());
    assert_eq!(observed.1, 2 * ordinary.scanned_logical_bytes());
    assert!(
        observed.2 > 0,
        "the member declaration path owns temporary storage"
    );
    let exhausted = resolve_component_property_schema_claim_with_admission(
        &schema,
        schema.version(),
        &path,
        &mut shared,
        |_, _, _| -> Result<(), ()> { panic!("exhausted read budget must refuse first") },
    )
    .unwrap_err();
    assert_eq!(
        exhausted,
        ComponentPropertyAdmissionError::Property(ComponentPropertyError::ReadBudgetExceeded,)
    );
    assert_eq!(shared.scanned_entries(), 18);
    assert_eq!(shared.scanned_logical_bytes(), 84);
}

#[test]
fn every_selector_callback_can_refuse_with_the_exact_nonclone_caller_error() {
    let schema = schema(ComponentValueShape::record_members([
        ComponentShapeMember::new("wire_selected", ComponentValueKind::U64).with_id(id("selected")),
    ]));
    let path = ComponentPropertyPath::new(id("root"))
        .with_steps([ComponentPropertyStep::Member(id("selected"))]);
    let mut charges = Vec::new();
    resolve_component_property_schema_claim_with_admission(
        &schema,
        schema.version(),
        &path,
        &mut budget(1024, 1024),
        |nodes, bytes, allocated| {
            charges.push((nodes, bytes, allocated));
            Ok::<_, ()>(())
        },
    )
    .unwrap();
    assert!(charges.iter().any(|charge| charge.2 > 0));

    #[derive(Debug)]
    struct Refusal<'a>(&'a str);
    let message = String::from("selector caller refusal");
    for denied in 0..charges.len() {
        let mut calls = 0;
        let mut shared = budget(1024, 1024);
        let result = resolve_component_property_schema_claim_with_admission(
            &schema,
            schema.version(),
            &path,
            &mut shared,
            |nodes, bytes, allocated| {
                let call = calls;
                calls += 1;
                assert_eq!((nodes, bytes, allocated), charges[call]);
                if call == denied {
                    Err(Refusal(&message))
                } else {
                    Ok(())
                }
            },
        );
        let Err(ComponentPropertyAdmissionError::Admission(Refusal(returned))) = result else {
            panic!("selector refusal must retain its caller error");
        };
        assert!(std::ptr::eq(returned, message.as_str()));
        assert_eq!(calls, denied + 1);
        assert_eq!(
            shared.scanned_entries(),
            charges[..denied].iter().map(|c| c.0).sum::<usize>()
        );
        assert_eq!(
            shared.scanned_logical_bytes(),
            charges[..denied].iter().map(|c| c.1).sum::<usize>()
        );
    }
}

#[test]
fn admitted_schema_claims_keep_version_selector_and_declaration_depth_guards() {
    let simple = schema(ComponentValueShape::map(ComponentValueKind::U64.into()));
    let root = ComponentPropertyPath::new(id("root"));
    for (version, path) in [
        (ComponentSchemaVersion::new(2).unwrap(), root.clone()),
        (
            ComponentSchemaVersion::ONE,
            ComponentPropertyPath::new(id(&"private".repeat(1024))),
        ),
        (
            ComponentSchemaVersion::ONE,
            root.clone().with_steps(std::iter::repeat_n(
                ComponentPropertyStep::MapEntry("key".into()),
                ComponentPropertyPath::MAX_STEPS + 1,
            )),
        ),
        (
            ComponentSchemaVersion::ONE,
            root.clone()
                .with_steps([ComponentPropertyStep::MapEntry("x".repeat(MAX_SHAPE_BYTES))]),
        ),
    ] {
        let expected = resolve_component_property_schema_claim(
            &simple,
            version,
            &path,
            &mut budget(1024, MAX_SHAPE_BYTES),
        )
        .unwrap_err();
        let actual = resolve_component_property_schema_claim_with_admission(
            &simple,
            version,
            &path,
            &mut budget(1024, MAX_SHAPE_BYTES),
            |_, _, _| -> Result<(), ()> { panic!("invalid selectors must not reach the walker") },
        )
        .unwrap_err();
        assert_eq!(actual, ComponentPropertyAdmissionError::Property(expected));
    }

    let mut shape = ComponentValueShape::map(ComponentValueKind::U64.into());
    for _ in 0..MAX_SHAPE_DEPTH {
        shape = ComponentValueShape::Optional {
            value: Box::new(shape),
        };
    }
    let deep = schema(shape);
    let path = root.with_steps([ComponentPropertyStep::MapEntry("key".into())]);
    assert_eq!(
        resolve_component_property_schema_claim_with_admission(
            &deep,
            deep.version(),
            &path,
            &mut budget(1024, 1024),
            |_, _, _| Ok::<_, ()>(()),
        )
        .unwrap_err(),
        ComponentPropertyAdmissionError::Property(ComponentPropertyError::PathDepthLimit)
    );
}
