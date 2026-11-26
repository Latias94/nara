use std::{collections::BTreeSet, error::Error, process::Command};

use nara::reflect::{ComponentCapability, ComponentSchemaCatalog};

#[test]
fn schema_binary_exports_the_round_rules() -> Result<(), Box<dyn Error>> {
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
    assert!(rules.has_capability(ComponentCapability::Edit));
    let fields: BTreeSet<_> = rules
        .fields()
        .iter()
        .map(|field| field.id().as_str())
        .collect();
    assert_eq!(
        fields,
        BTreeSet::from(["move-speed", "shield-charges", "time-limit-seconds"])
    );
    Ok(())
}
