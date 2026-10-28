//! Produces the actual managed release metadata through Reflect's catalog admission.

use std::{env, error::Error, fs, path::PathBuf};

use nara_reflect::{
    ComponentFieldPath, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot,
    ComponentSchemaVersion, ComponentValue, ComponentValueKind, PersistentComponentSchemaOwnerId,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let input = PathBuf::from(arguments.next().ok_or("missing captured V1 catalog")?);
    let output = PathBuf::from(arguments.next().ok_or("missing output directory")?);
    if arguments.next().is_some() {
        return Err("expected a captured V1 catalog and an output directory".into());
    }
    let captured = fs::read(input)?;
    let original = ComponentSchemaCatalog::from_json_bytes(&captured)?;
    assert_eq!(original.generation(), 1);
    assert_eq!(original.components().len(), 1);
    assert_eq!(
        original.components()[0].version(),
        ComponentSchemaVersion::ONE,
    );

    for variant in [
        "creation_v2",
        "creation_wrong_predecessor",
        "creation_unsupported",
    ] {
        let mut predecessor = original.clone();
        if variant == "creation_wrong_predecessor" {
            predecessor.components[0].authoring_initial_value = Some(ComponentValue::map([
                ("wire_health", ComponentValue::I64(90)),
                ("wire_armor", ComponentValue::I64(9)),
            ]));
        }
        let mut successor = ComponentSchemaCatalog::successor_of(&predecessor)?;
        successor.components = predecessor.components.clone();
        successor.components[0].authoring_initial_value = Some(ComponentValue::map([
            ("wire_health", ComponentValue::I64(250)),
            ("wire_armor", ComponentValue::I64(25)),
        ]));
        if variant == "creation_unsupported" {
            // A valid catalog lineage that requires stored-value migration, outside this intent.
            successor.components[0].version = ComponentSchemaVersion::new(2).unwrap();
            let health = successor.components[0]
                .fields
                .iter_mut()
                .find(|field| field.id.as_str() == "health")
                .unwrap();
            health.shape = ComponentValueKind::String.into();
            successor.components[0]
                .authoring_initial_value
                .as_mut()
                .unwrap()
                .set_path(
                    &ComponentFieldPath::from_fields(["wire_health"]),
                    ComponentValue::String("250".into()),
                )?;
        }
        let owner = ComponentSchemaOwnerSnapshot::admit(
            PersistentComponentSchemaOwnerId::try_new("trial.enemy-stats")?,
            successor,
            Some(predecessor.clone()),
        )?;
        assert_eq!(owner.current().generation(), 2);
        assert_eq!(owner.predecessor(), Some(&predecessor));
        let directory = output.join(variant);
        fs::create_dir_all(&directory)?;
        fs::write(
            directory.join("enemy-stats.catalog.json"),
            owner
                .current()
                .to_json_string_with_predecessor(Some(&predecessor))?,
        )?;
        fs::write(
            directory.join("enemy-stats.predecessor.catalog.json"),
            if variant == "creation_wrong_predecessor" {
                predecessor.to_json_string()?.into_bytes()
            } else {
                captured.clone()
            },
        )?;
    }
    Ok(())
}
