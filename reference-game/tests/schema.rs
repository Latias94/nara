use std::{collections::BTreeSet, error::Error, process::Command};

use nara::reflect::{ComponentCapability, ComponentSchemaCatalog, ComponentValue};

#[test]
fn schema_binary_exports_the_round_rules_and_roles() -> Result<(), Box<dyn Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_nara_schema")).output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let catalog = ComponentSchemaCatalog::from_json_bytes(&output.stdout)?;
    let rules = catalog
        .components()
        .iter()
        .find(|component| component.id().as_str() == "night-relay.RunRules")
        .ok_or("the game schema is missing Run Rules")?;
    assert_eq!(rules.version().get(), 2);
    assert!(rules.has_capability(ComponentCapability::Edit));
    let fields: BTreeSet<_> = rules
        .fields()
        .iter()
        .map(|field| field.id().as_str())
        .collect();
    assert_eq!(
        fields,
        BTreeSet::from(["move_speed", "shield_charges", "time_limit_seconds"])
    );
    let fields = rules.fields();
    for (field, values) in [
        (
            "time_limit_seconds",
            [0, 1, 180, 181].map(ComponentValue::U64),
        ),
        ("shield_charges", [0, 1, 5, 6].map(ComponentValue::U64)),
        (
            "move_speed",
            [39.0, 40.0, 300.0, 301.0].map(|speed| ComponentValue::f64(speed).unwrap()),
        ),
    ] {
        let field = fields
            .iter()
            .find(|entry| entry.id().as_str() == field)
            .unwrap();
        assert_eq!(
            values.each_ref().map(|value| field.accepts_value(value)),
            [false, true, true, false]
        );
    }
    let ids: BTreeSet<_> = catalog
        .components()
        .iter()
        .map(|component| component.id().as_str())
        .collect();
    for role in [
        "night-relay.Pilot",
        "night-relay.PowerCell",
        "night-relay.PatrolDrone",
        "night-relay.ExtractionGate",
    ] {
        assert!(ids.contains(role));
    }
    Ok(())
}
