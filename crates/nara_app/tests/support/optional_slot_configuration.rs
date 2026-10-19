use super::*;

struct OptionalConfigurationOwner(bool);

impl PluginGroup for OptionalConfigurationOwner {
    const ID: PluginGroupId = PluginGroupId::new("test.optional-configuration-owner");

    fn build(self) -> PluginGroupBuilder {
        if self.0 {
            PluginGroupBuilder::new().add_edited_group(
                RequiredCoreContract.edit().configure_slot_if_present(
                    ABSENT_SLOT,
                    PluginDefinition::for_default::<AlternatePlugin>(),
                ),
            )
        } else {
            PluginGroupBuilder::new().add_group(RequiredCoreContract)
        }
    }
}

#[test]
fn optional_slot_configuration_records_an_absent_target_without_inserting_a_plugin() {
    let plain = PluginPlan::resolve(OptionalConfigurationOwner(false)).unwrap();
    let edited = PluginPlan::resolve(OptionalConfigurationOwner(true)).unwrap();
    assert_eq!(plain.entries().len(), edited.entries().len());
    assert_eq!(
        plain.entries()[0].definition_key(),
        edited.entries()[0].definition_key()
    );
    assert_ne!(plain.fingerprint(), edited.fingerprint());
    assert_eq!(
        PluginPlan::resolve(RequiredCoreContract.edit().configure_slot(
            ABSENT_SLOT,
            PluginDefinition::for_default::<AlternatePlugin>(),
        ))
        .unwrap_err(),
        PluginPlanError::MissingSlotEditTarget { slot: ABSENT_SLOT },
    );
}

#[test]
fn optional_slot_configuration_updates_all_overlapping_logical_occurrences() {
    let counter = Arc::new(AtomicU64::new(17));
    let plan = PluginPlan::resolve(
        OverlappingCorePlugins(Arc::new(AtomicU64::new(1)))
            .edit()
            .configure_slot_if_present(CORE_SLOT, core_definition(Arc::clone(&counter))),
    )
    .unwrap();
    assert_eq!(plan.entries().len(), 1);
    let app = plan.instantiate().unwrap();
    assert_eq!(app.world().resource::<InstanceProbe>().0, 17);
    assert_eq!(counter.load(Ordering::SeqCst), 18);
}

#[test]
fn optional_slot_configuration_preserves_present_slot_failures() {
    let definition = || core_definition(Arc::new(AtomicU64::new(1)));
    assert_eq!(
        PluginPlan::resolve(
            DivergentCoreContracts
                .edit()
                .configure_slot_if_present(CORE_SLOT, definition())
        )
        .unwrap_err(),
        PluginPlanError::DivergentSlotContract { slot: CORE_SLOT },
    );
    assert_eq!(
        PluginPlan::resolve(RequiredCoreContract.edit().configure_slot_if_present(
            CORE_SLOT,
            PluginDefinition::for_default::<AlternatePlugin>(),
        ))
        .unwrap_err(),
        PluginPlanError::SlotPluginMismatch {
            slot: CORE_SLOT,
            expected: CORE_ID,
            actual: ALTERNATE_ID
        },
    );
    assert_eq!(
        PluginPlan::resolve(DuplicateSlotPlugins.edit().configure_slot_if_present(
            OPTIONAL_SLOT,
            PluginDefinition::for_default::<OptionalPlugin>(),
        ))
        .unwrap_err(),
        PluginPlanError::DivergentSlotContract {
            slot: OPTIONAL_SLOT
        },
    );
    assert!(matches!(
        PluginPlan::resolve(
            OptionalDependencyPlugins
                .edit()
                .disable::<OptionalPlugin>()
                .configure_slot_if_present(
                    ABSENT_SLOT,
                    PluginDefinition::for_default::<AlternatePlugin>()
                )
        ),
        Err(PluginPlanError::MissingPlugin {
            plugin: OPTIONAL_CONSUMER_ID,
            required: OPTIONAL_ID
        }),
    ));
}
