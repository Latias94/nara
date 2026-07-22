use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use nara_app::{
    AddPluginsError, App, Plugin, PluginCapability, PluginCategory, PluginDeclaration,
    PluginDefinition, PluginDefinitionId, PluginError, PluginGroup, PluginGroupBuilder,
    PluginGroupId, PluginHook, PluginHookMutation, PluginId, PluginInstantiationError,
    PluginLifecycleState, PluginPlan, PluginPlanError, PluginPrepareFailure, PluginServiceId,
    PluginShutdownContext, PluginShutdownError, PluginShutdownObligationId, PluginSlot,
    PluginSlotId, RuntimeAdmissionReservation, RuntimeCandidateRetirementState,
    RuntimeCloseContext, RuntimeCloseParticipant, RuntimeCloseParticipantError,
    RuntimeCloseParticipantId, RuntimeClosePolicy, RuntimeCloseProgress, RuntimeConstructionError,
    RuntimeObligationLedger,
};
use nara_ecs::Resource;

const CORE_ID: PluginId = PluginId::new("nara.test.plan.core");
const OPTIONAL_ID: PluginId = PluginId::new("nara.test.plan.optional");
const CONSUMER_ID: PluginId = PluginId::new("nara.test.plan.consumer");
const ALTERNATE_ID: PluginId = PluginId::new("nara.test.plan.alternate");
const VIOLATOR_ID: PluginId = PluginId::new("nara.test.plan.violator");
const MISSING_ID: PluginId = PluginId::new("nara.test.plan.missing");
const OPTIONAL_CONSUMER_ID: PluginId = PluginId::new("nara.test.plan.optional-consumer");
const CAPABILITY_CONSUMER_ID: PluginId = PluginId::new("nara.test.plan.capability-consumer");
const SERVICE_CONSUMER_ID: PluginId = PluginId::new("nara.test.plan.service-consumer");
const CONFLICT_A_ID: PluginId = PluginId::new("nara.test.plan.conflict-a");
const CONFLICT_B_ID: PluginId = PluginId::new("nara.test.plan.conflict-b");
const CYCLE_A_ID: PluginId = PluginId::new("nara.test.plan.cycle-a");
const CYCLE_B_ID: PluginId = PluginId::new("nara.test.plan.cycle-b");
const OBLIGATION_PLUGIN_ID: PluginId = PluginId::new("nara.test.plan.obligation");
const PREFLIGHT_PLUGIN_ID: PluginId = PluginId::new("nara.test.plan.preflight");
const FAILING_BUILD_PLUGIN_ID: PluginId = PluginId::new("nara.test.plan.failing-build");
const FAILING_FINISH_PLUGIN_ID: PluginId = PluginId::new("nara.test.plan.failing-finish");
const FAILING_SHUTDOWN_PLUGIN_ID: PluginId = PluginId::new("nara.test.plan.failing-shutdown");
const COMMITTED_PROBE_PLUGIN_ID: PluginId = PluginId::new("nara.test.plan.committed-probe");
const SHUTDOWN_A_PLUGIN_ID: PluginId = PluginId::new("nara.test.plan.shutdown-a");
const SHUTDOWN_B_PLUGIN_ID: PluginId = PluginId::new("nara.test.plan.shutdown-b");
const SHUTDOWN_C_PLUGIN_ID: PluginId = PluginId::new("nara.test.plan.shutdown-c");
const CORE_SLOT: PluginSlotId = PluginSlotId::new("nara.test.plan.slot.core");
const OPTIONAL_SLOT: PluginSlotId = PluginSlotId::new("nara.test.plan.slot.optional");
const ABSENT_SLOT: PluginSlotId = PluginSlotId::new("nara.test.plan.slot.absent");
const TEST_GROUP: PluginGroupId = PluginGroupId::new("nara.test.plan.group");
const TEST_SERVICE: PluginServiceId = PluginServiceId::new("nara.test.plan.service");
const ABSENT_SERVICE: PluginServiceId = PluginServiceId::new("nara.test.plan.absent-service");
const ABSENT_CAPABILITY: PluginCapability =
    PluginCapability::new("nara.test.plan.absent-capability");
const TEST_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.test.plan.obligation.owner");
const CORE_POLICY_A: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.plan.core.policy-a", 1);
const CORE_POLICY_B: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.plan.core.policy-b", 1);
const ABSENT_ID: PluginId = PluginId::new("nara.test.plan.absent");
const CORE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(CORE_ID, PluginCategory::Core).provides_services(&[TEST_SERVICE]);
const CORE_VARIANT_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(CORE_ID, PluginCategory::Runtime).provides_services(&[TEST_SERVICE]);
const OPTIONAL_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(OPTIONAL_ID, PluginCategory::Runtime);
const ALTERNATE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(ALTERNATE_ID, PluginCategory::Runtime);
const CONSUMER_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(CONSUMER_ID, PluginCategory::Runtime)
        .requires_plugins(&[CORE_ID])
        .requires_services(&[TEST_SERVICE]);
const MISSING_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(MISSING_ID, PluginCategory::Runtime).requires_plugins(&[ABSENT_ID]);
const OPTIONAL_CONSUMER_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(OPTIONAL_CONSUMER_ID, PluginCategory::Runtime)
        .requires_plugins(&[OPTIONAL_ID]);
const CAPABILITY_CONSUMER_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(CAPABILITY_CONSUMER_ID, PluginCategory::Runtime)
        .requires_capabilities(&[ABSENT_CAPABILITY]);
const SERVICE_CONSUMER_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(SERVICE_CONSUMER_ID, PluginCategory::Runtime)
        .requires_services(&[ABSENT_SERVICE]);
const CONFLICT_A_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(CONFLICT_A_ID, PluginCategory::Runtime).conflicts(&[CONFLICT_B_ID]);
const CONFLICT_B_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(CONFLICT_B_ID, PluginCategory::Runtime);
const CYCLE_A_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(CYCLE_A_ID, PluginCategory::Runtime).requires_plugins(&[CYCLE_B_ID]);
const CYCLE_B_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(CYCLE_B_ID, PluginCategory::Runtime).requires_plugins(&[CYCLE_A_ID]);
const OBLIGATION_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(OBLIGATION_PLUGIN_ID, PluginCategory::Service)
        .shutdown_obligations(&[TEST_OBLIGATION]);
const PREFLIGHT_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(PREFLIGHT_PLUGIN_ID, PluginCategory::Runtime);
const FAILING_BUILD_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(FAILING_BUILD_PLUGIN_ID, PluginCategory::Runtime);
const FAILING_FINISH_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(FAILING_FINISH_PLUGIN_ID, PluginCategory::Runtime);
const FAILING_SHUTDOWN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(FAILING_SHUTDOWN_PLUGIN_ID, PluginCategory::Runtime);
const VIOLATOR_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(VIOLATOR_ID, PluginCategory::Runtime);
const COMMITTED_PROBE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(COMMITTED_PROBE_PLUGIN_ID, PluginCategory::Runtime);
const SHUTDOWN_A_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(SHUTDOWN_A_PLUGIN_ID, PluginCategory::Runtime);
const SHUTDOWN_B_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(SHUTDOWN_B_PLUGIN_ID, PluginCategory::Runtime);
const SHUTDOWN_C_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(SHUTDOWN_C_PLUGIN_ID, PluginCategory::Runtime);

macro_rules! noop_plugin {
    ($plugin:ident, $declaration:ident) => {
        #[derive(Debug, Default)]
        struct $plugin;

        impl Plugin for $plugin {
            fn declaration() -> &'static PluginDeclaration {
                &$declaration
            }

            fn build(&self, _app: &mut App) -> Result<(), PluginError> {
                Ok(())
            }
        }
    };
}

noop_plugin!(OptionalConsumerPlugin, OPTIONAL_CONSUMER_DECLARATION);
noop_plugin!(CapabilityConsumerPlugin, CAPABILITY_CONSUMER_DECLARATION);
noop_plugin!(ServiceConsumerPlugin, SERVICE_CONSUMER_DECLARATION);
noop_plugin!(ConflictAPlugin, CONFLICT_A_DECLARATION);
noop_plugin!(ConflictBPlugin, CONFLICT_B_DECLARATION);
noop_plugin!(CycleAPlugin, CYCLE_A_DECLARATION);
noop_plugin!(CycleBPlugin, CYCLE_B_DECLARATION);

#[derive(Debug, Resource)]
struct InstanceProbe(u64);

#[derive(Debug)]
struct CorePlugin {
    instance: u64,
}

impl Plugin for CorePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &CORE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), nara_app::PluginError> {
        app.insert_resource(InstanceProbe(self.instance))?;
        Ok(())
    }
}

#[derive(Debug)]
struct CoreVariantPlugin;

impl Plugin for CoreVariantPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &CORE_VARIANT_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

#[derive(Debug, Default)]
struct OptionalPlugin;

impl Plugin for OptionalPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &OPTIONAL_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), nara_app::PluginError> {
        Ok(())
    }
}

#[derive(Debug, Default)]
struct AlternatePlugin;

impl Plugin for AlternatePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &ALTERNATE_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), nara_app::PluginError> {
        Ok(())
    }
}

#[derive(Debug, Default)]
struct ConsumerPlugin;

impl Plugin for ConsumerPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &CONSUMER_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), nara_app::PluginError> {
        Ok(())
    }
}

#[derive(Debug, Default)]
struct MissingDependencyPlugin;

impl Plugin for MissingDependencyPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &MISSING_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), nara_app::PluginError> {
        Ok(())
    }
}

fn core_definition(sequence: Arc<AtomicU64>) -> PluginDefinition {
    PluginDefinition::infallible::<CorePlugin, _>(CORE_POLICY_A, b"core-config-v1", move || {
        CorePlugin {
            instance: sequence.fetch_add(1, Ordering::SeqCst),
        }
    })
}

#[derive(Debug, Default)]
struct PanickingDeclarationPlugin;

impl Plugin for PanickingDeclarationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        panic!("declaration probe")
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

#[derive(Debug, Default)]
struct PanickingDefinitionGroup;

impl PluginGroup for PanickingDefinitionGroup {
    const ID: PluginGroupId = PluginGroupId::new("nara.test.plan.panicking-definition-group");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_definition(PluginDefinition::for_default::<PanickingDeclarationPlugin>())
    }
}

#[derive(Clone)]
struct TestPlugins {
    sequence: Arc<AtomicU64>,
}

impl PluginGroup for TestPlugins {
    const ID: PluginGroupId = TEST_GROUP;

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_slot(
                PluginSlot::required(CORE_SLOT, CORE_ID),
                core_definition(self.sequence),
            )
            .add_slot(
                PluginSlot::optional(OPTIONAL_SLOT, OPTIONAL_ID),
                PluginDefinition::for_default::<OptionalPlugin>(),
            )
            .add_definition(PluginDefinition::for_default::<ConsumerPlugin>())
    }
}

#[derive(Debug, Default)]
struct DuplicateSlotPlugins;

impl PluginGroup for DuplicateSlotPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.test.plan.duplicate-slot-group");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_slot(
                PluginSlot::optional(OPTIONAL_SLOT, OPTIONAL_ID),
                PluginDefinition::for_default::<OptionalPlugin>(),
            )
            .add_slot(
                PluginSlot::optional(OPTIONAL_SLOT, ALTERNATE_ID),
                PluginDefinition::for_default::<AlternatePlugin>(),
            )
    }
}

#[derive(Debug, Default)]
struct OptionalDependencyPlugins;

impl PluginGroup for OptionalDependencyPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.test.plan.optional-dependency-group");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_slot(
                PluginSlot::optional(OPTIONAL_SLOT, OPTIONAL_ID),
                PluginDefinition::for_default::<OptionalPlugin>(),
            )
            .add_definition(PluginDefinition::for_default::<OptionalConsumerPlugin>())
    }
}

#[derive(Debug, Default)]
struct ConflictPlugins;

impl PluginGroup for ConflictPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.test.plan.conflict-group");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_definition(PluginDefinition::for_default::<ConflictAPlugin>())
            .add_definition(PluginDefinition::for_default::<ConflictBPlugin>())
    }
}

#[derive(Debug, Default)]
struct OrderingCyclePlugins;

impl PluginGroup for OrderingCyclePlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.test.plan.ordering-cycle-group");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_definition(PluginDefinition::for_default::<CycleAPlugin>())
            .add_definition(PluginDefinition::for_default::<CycleBPlugin>())
    }
}

#[derive(Debug, Default)]
struct GroupCycleA;

#[derive(Debug, Default)]
struct GroupCycleB;

impl PluginGroup for GroupCycleA {
    const ID: PluginGroupId = PluginGroupId::new("nara.test.plan.group-cycle-a");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_group(GroupCycleB)
    }
}

impl PluginGroup for GroupCycleB {
    const ID: PluginGroupId = PluginGroupId::new("nara.test.plan.group-cycle-b");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_group(GroupCycleA)
    }
}

#[derive(Clone)]
struct SharedCoreA(Arc<AtomicU64>);

impl PluginGroup for SharedCoreA {
    const ID: PluginGroupId = PluginGroupId::new("nara.test.plan.shared-core-a");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_slot(
            PluginSlot::required(CORE_SLOT, CORE_ID),
            core_definition(self.0),
        )
    }
}

#[derive(Clone)]
struct SharedCoreB(Arc<AtomicU64>);

impl PluginGroup for SharedCoreB {
    const ID: PluginGroupId = PluginGroupId::new("nara.test.plan.shared-core-b");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_slot(
            PluginSlot::required(CORE_SLOT, CORE_ID),
            core_definition(self.0),
        )
    }
}

#[derive(Clone)]
struct OverlappingCorePlugins(Arc<AtomicU64>);

impl PluginGroup for OverlappingCorePlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.test.plan.overlapping-core");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_group(SharedCoreA(Arc::clone(&self.0)))
            .add_group(SharedCoreB(self.0))
    }
}

#[derive(Debug, Default)]
struct OptionalCoreContract;

impl PluginGroup for OptionalCoreContract {
    const ID: PluginGroupId = PluginGroupId::new("nara.test.plan.optional-core-contract");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_slot(
            PluginSlot::optional(CORE_SLOT, CORE_ID),
            PluginDefinition::infallible::<CorePlugin, _>(CORE_POLICY_A, b"core-config-v1", || {
                CorePlugin { instance: 1 }
            }),
        )
    }
}

#[derive(Debug, Default)]
struct RequiredCoreContract;

impl PluginGroup for RequiredCoreContract {
    const ID: PluginGroupId = PluginGroupId::new("nara.test.plan.required-core-contract");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_slot(
            PluginSlot::required(CORE_SLOT, CORE_ID),
            PluginDefinition::infallible::<CorePlugin, _>(CORE_POLICY_A, b"core-config-v1", || {
                CorePlugin { instance: 1 }
            }),
        )
    }
}

#[derive(Debug, Default)]
struct DivergentCoreContracts;

impl PluginGroup for DivergentCoreContracts {
    const ID: PluginGroupId = PluginGroupId::new("nara.test.plan.divergent-core-contracts");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_group(OptionalCoreContract)
            .add_group(RequiredCoreContract)
    }
}

#[derive(Debug, Default)]
struct NestedLeafA;

impl PluginGroup for NestedLeafA {
    const ID: PluginGroupId = PluginGroupId::new("nara.test.plan.nested-leaf-a");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_definition(PluginDefinition::for_default::<OptionalPlugin>())
    }
}

#[derive(Debug, Default)]
struct NestedLeafB;

impl PluginGroup for NestedLeafB {
    const ID: PluginGroupId = PluginGroupId::new("nara.test.plan.nested-leaf-b");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_definition(PluginDefinition::for_default::<OptionalPlugin>())
    }
}

const DIVERGENT_NESTED_GROUP_ID: PluginGroupId =
    PluginGroupId::new("nara.test.plan.divergent-nested-group");

#[derive(Debug, Default)]
struct NestedOuterA;

impl PluginGroup for NestedOuterA {
    const ID: PluginGroupId = DIVERGENT_NESTED_GROUP_ID;

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_group(NestedLeafA)
    }
}

#[derive(Debug, Default)]
struct NestedOuterB;

impl PluginGroup for NestedOuterB {
    const ID: PluginGroupId = DIVERGENT_NESTED_GROUP_ID;

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_group(NestedLeafB)
    }
}

const DIVERGENT_EDIT_GROUP_ID: PluginGroupId =
    PluginGroupId::new("nara.test.plan.divergent-edit-group");

#[derive(Clone)]
struct NestedEditOuterA(Arc<AtomicU64>);

impl PluginGroup for NestedEditOuterA {
    const ID: PluginGroupId = DIVERGENT_EDIT_GROUP_ID;

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_group(SharedCoreA(self.0))
    }
}

#[derive(Clone)]
struct NestedEditOuterB(Arc<AtomicU64>);

impl PluginGroup for NestedEditOuterB {
    const ID: PluginGroupId = DIVERGENT_EDIT_GROUP_ID;

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_edited_group(
            SharedCoreA(Arc::clone(&self.0))
                .edit()
                .configure(core_definition(self.0)),
        )
    }
}

#[test]
fn construction_policy_identity_is_explicit_and_part_of_the_plan() {
    let first =
        PluginDefinition::infallible::<CorePlugin, _>(CORE_POLICY_A, b"shared-config", || {
            CorePlugin { instance: 1 }
        });
    let second =
        PluginDefinition::infallible::<CorePlugin, _>(CORE_POLICY_B, b"shared-config", || {
            CorePlugin { instance: 1 }
        });

    assert_ne!(first.key(), second.key());
    assert_ne!(
        PluginPlan::resolve(first).unwrap().fingerprint(),
        PluginPlan::resolve(second).unwrap().fingerprint()
    );

    let rebound = PluginPlan::resolve((
        PluginDefinition::infallible::<CorePlugin, _>(CORE_POLICY_A, b"shared-config", || {
            CorePlugin { instance: 1 }
        }),
        PluginDefinition::infallible::<CorePlugin, _>(CORE_POLICY_A, b"shared-config", || {
            CorePlugin { instance: 2 }
        }),
    ))
    .unwrap_err();
    assert_eq!(
        rebound,
        PluginPlanError::DivergentDefinition { plugin: CORE_ID }
    );
}

#[test]
fn plan_identity_includes_the_canonical_plugin_declaration() {
    let core = PluginPlan::resolve(PluginDefinition::infallible::<CorePlugin, _>(
        CORE_POLICY_A,
        b"shared-config",
        || CorePlugin { instance: 1 },
    ))
    .unwrap();
    let variant = PluginPlan::resolve(PluginDefinition::infallible::<CoreVariantPlugin, _>(
        CORE_POLICY_A,
        b"shared-config",
        || CoreVariantPlugin,
    ))
    .unwrap();

    assert_ne!(core.fingerprint(), variant.fingerprint());
}

#[test]
fn group_identity_includes_nested_group_structure() {
    assert_eq!(
        PluginPlan::resolve((NestedOuterA, NestedOuterB)).unwrap_err(),
        PluginPlanError::DivergentGroup {
            group: DIVERGENT_NESTED_GROUP_ID,
        }
    );
}

#[test]
fn group_identity_includes_intrinsic_nested_edits() {
    assert_eq!(
        PluginPlan::resolve((
            NestedEditOuterA(Arc::new(AtomicU64::new(1))),
            NestedEditOuterB(Arc::new(AtomicU64::new(1))),
        ))
        .unwrap_err(),
        PluginPlanError::DivergentGroup {
            group: DIVERGENT_EDIT_GROUP_ID,
        }
    );
}

#[test]
fn declaration_panics_are_reported_from_every_planning_input_shape() {
    let definition =
        std::panic::catch_unwind(PluginDefinition::for_default::<PanickingDeclarationPlugin>)
            .expect("definition construction must defer static declaration evaluation");
    assert_eq!(
        PluginPlan::resolve(definition).unwrap_err(),
        PluginPlanError::DeclarationPanicked
    );
    assert_eq!(
        PluginPlan::resolve(PanickingDefinitionGroup).unwrap_err(),
        PluginPlanError::DeclarationPanicked
    );
    assert_eq!(
        PluginPlan::resolve(
            TestPlugins {
                sequence: Arc::new(AtomicU64::new(1)),
            }
            .edit()
            .disable::<PanickingDeclarationPlugin>(),
        )
        .unwrap_err(),
        PluginPlanError::DeclarationPanicked
    );
}

#[test]
fn incremental_composition_preserves_repeatable_definition_witnesses() {
    let sequence = Arc::new(AtomicU64::new(1));
    let mut app = App::new();
    app.add_plugins(TestPlugins {
        sequence: Arc::clone(&sequence),
    })
    .unwrap();

    let original_core = app
        .installed_plugin_entries()
        .find(|entry| entry.plugin_id() == CORE_ID)
        .unwrap()
        .clone();
    assert!(original_core.definition_key().is_some());
    assert_eq!(app.world().resource::<InstanceProbe>().0, 1);

    app.add_plugins(PluginDefinition::for_default::<AlternatePlugin>())
        .unwrap();
    let after_suffix = app
        .installed_plugin_entries()
        .find(|entry| entry.plugin_id() == CORE_ID)
        .unwrap();
    assert_eq!(
        after_suffix.definition_key(),
        original_core.definition_key()
    );
    assert_eq!(
        after_suffix.group_provenance(),
        original_core.group_provenance()
    );

    app.add_plugins(TestPlugins { sequence }).unwrap();
    let after_repeated_group = app
        .installed_plugin_entries()
        .find(|entry| entry.plugin_id() == CORE_ID)
        .unwrap();
    assert_eq!(
        after_repeated_group.definition_key(),
        original_core.definition_key()
    );
    assert_eq!(app.world().resource::<InstanceProbe>().0, 1);

    let mut opaque = App::new();
    opaque.add_plugins(OptionalPlugin).unwrap();
    assert!(matches!(
        opaque.add_plugins(PluginDefinition::for_default::<OptionalPlugin>()),
        Err(AddPluginsError::Plan(
            PluginPlanError::DivergentDefinition {
                plugin: OPTIONAL_ID
            }
        ))
    ));
}

#[test]
fn edits_apply_once_to_overlapping_logical_occurrences() {
    let configured = Arc::new(AtomicU64::new(41));
    let plan = PluginPlan::resolve(
        OverlappingCorePlugins(Arc::new(AtomicU64::new(1)))
            .edit()
            .configure(core_definition(Arc::clone(&configured)))
            .insert_after::<CorePlugin>(PluginDefinition::for_default::<AlternatePlugin>()),
    )
    .unwrap();

    assert_eq!(
        plan.entries()
            .iter()
            .map(|entry| entry.plugin_id())
            .collect::<Vec<_>>(),
        [CORE_ID, ALTERNATE_ID]
    );
    let app = plan.instantiate().unwrap();
    assert_eq!(app.world().resource::<InstanceProbe>().0, 41);
    assert_eq!(configured.load(Ordering::SeqCst), 42);
}

#[test]
fn the_same_slot_cannot_mix_required_and_optional_contracts() {
    assert_eq!(
        PluginPlan::resolve(DivergentCoreContracts).unwrap_err(),
        PluginPlanError::DivergentSlotContract { slot: CORE_SLOT }
    );
}

#[test]
fn different_plugins_cannot_claim_the_same_stable_slot() {
    assert_eq!(
        PluginPlan::resolve(DuplicateSlotPlugins).unwrap_err(),
        PluginPlanError::DuplicateSlot {
            slot: OPTIONAL_SLOT,
            first: OPTIONAL_ID,
            duplicate: ALTERNATE_ID,
        }
    );
}

#[test]
fn pure_plan_reports_missing_capability_service_conflict_and_cycles() {
    assert_eq!(
        PluginPlan::resolve(PluginDefinition::for_default::<CapabilityConsumerPlugin>())
            .unwrap_err(),
        PluginPlanError::MissingCapability {
            plugin: CAPABILITY_CONSUMER_ID,
            required: ABSENT_CAPABILITY,
        }
    );
    assert_eq!(
        PluginPlan::resolve(PluginDefinition::for_default::<ServiceConsumerPlugin>()).unwrap_err(),
        PluginPlanError::MissingService {
            plugin: SERVICE_CONSUMER_ID,
            required: ABSENT_SERVICE,
        }
    );
    assert_eq!(
        PluginPlan::resolve(ConflictPlugins).unwrap_err(),
        PluginPlanError::Conflict {
            plugin: CONFLICT_A_ID,
            conflict: CONFLICT_B_ID,
        }
    );
    assert!(matches!(
        PluginPlan::resolve(OrderingCyclePlugins).unwrap_err(),
        PluginPlanError::OrderingCycle { .. }
    ));
    assert_eq!(
        PluginPlan::resolve(GroupCycleA).unwrap_err(),
        PluginPlanError::GroupCycle {
            chain: vec![GroupCycleA::ID, GroupCycleB::ID, GroupCycleA::ID],
        }
    );
}

#[test]
fn disabled_prerequisites_and_invalid_slot_edits_reject_during_planning() {
    assert_eq!(
        PluginPlan::resolve(OptionalDependencyPlugins.edit().disable::<OptionalPlugin>())
            .unwrap_err(),
        PluginPlanError::MissingPlugin {
            plugin: OPTIONAL_CONSUMER_ID,
            required: OPTIONAL_ID,
        }
    );

    let plugins = || TestPlugins {
        sequence: Arc::new(AtomicU64::new(1)),
    };
    assert_eq!(
        PluginPlan::resolve(plugins().edit().disable_slot(ABSENT_SLOT)).unwrap_err(),
        PluginPlanError::MissingEditTarget
    );
    assert_eq!(
        PluginPlan::resolve(plugins().edit().disable::<CorePlugin>()).unwrap_err(),
        PluginPlanError::RequiredSlotDisabled { slot: CORE_SLOT }
    );
}

#[test]
fn group_resolution_is_pure_repeatable_and_prepares_fresh_owners() {
    let sequence = Arc::new(AtomicU64::new(1));
    let configured_sequence = Arc::new(AtomicU64::new(41));
    let edited = TestPlugins {
        sequence: Arc::clone(&sequence),
    }
    .edit()
    .disable::<OptionalPlugin>()
    .configure(core_definition(Arc::clone(&configured_sequence)))
    .insert_before::<ConsumerPlugin>(PluginDefinition::for_default::<AlternatePlugin>());

    let plan = PluginPlan::resolve(edited).unwrap();
    let repeated = PluginPlan::resolve(
        TestPlugins {
            sequence: Arc::clone(&sequence),
        }
        .edit()
        .disable::<OptionalPlugin>()
        .configure(core_definition(Arc::clone(&configured_sequence)))
        .insert_before::<ConsumerPlugin>(PluginDefinition::for_default::<AlternatePlugin>()),
    )
    .unwrap();

    assert_eq!(plan.fingerprint(), repeated.fingerprint());
    assert_eq!(plan.entries().len(), 3);
    assert!(plan.disabled_slots().contains(&OPTIONAL_SLOT));
    assert_eq!(plan.entries()[0].plugin_id(), CORE_ID);
    assert_eq!(plan.entries()[1].plugin_id(), ALTERNATE_ID);
    assert_eq!(plan.entries()[2].plugin_id(), CONSUMER_ID);

    let first = plan.instantiate().unwrap();
    let second = plan.instantiate().unwrap();
    assert_eq!(first.plugin_plan_fingerprint(), plan.fingerprint());
    assert_eq!(second.plugin_plan_fingerprint(), plan.fingerprint());
    assert_eq!(first.world().resource::<InstanceProbe>().0, 41);
    assert_eq!(second.world().resource::<InstanceProbe>().0, 42);
    assert_eq!(sequence.load(Ordering::SeqCst), 1);
}

#[test]
fn explicit_dependency_tuple_resolves_and_commits_the_migration_pattern() {
    let mut app = App::new();

    app.add_plugins((CorePlugin { instance: 7 }, ConsumerPlugin))
        .unwrap();

    assert_eq!(
        app.installed_plugins()
            .map(|declaration| declaration.id)
            .collect::<Vec<_>>(),
        vec![CORE_ID, CONSUMER_ID]
    );
    assert_eq!(app.world().resource::<InstanceProbe>().0, 7);
}

#[test]
fn plan_failures_happen_before_app_mutation_and_are_retryable() {
    let mut app = App::new();
    let before = app.configuration_fingerprint();

    let Err(error) = app.add_plugins(MissingDependencyPlugin) else {
        panic!("missing dependency must reject during pure planning");
    };
    assert!(matches!(
        error,
        AddPluginsError::Plan(PluginPlanError::MissingPlugin { .. })
    ));
    assert_eq!(app.configuration_fingerprint(), before);

    app.add_plugins(CorePlugin { instance: 7 }).unwrap();
    assert_eq!(app.world().resource::<InstanceProbe>().0, 7);
}

#[test]
fn preparation_failure_creates_no_app_and_a_corrected_plan_still_instantiates() {
    let failing =
        PluginDefinition::fallible::<CorePlugin, _>(CORE_POLICY_A, b"core-config-v1", || {
            Err(PluginPrepareFailure::new("test.prepare.rejected"))
        });
    let plan = PluginPlan::resolve(failing).unwrap();

    let error = plan.instantiate().unwrap_err();
    assert_eq!(
        error.prepare_error().unwrap().code(),
        "test.prepare.rejected"
    );

    let panicking =
        PluginDefinition::fallible::<CorePlugin, _>(CORE_POLICY_A, b"core-config-v1", || {
            panic!("prepare probe")
        });
    let error = PluginPlan::resolve(panicking)
        .unwrap()
        .instantiate()
        .unwrap_err();
    assert_eq!(
        error.prepare_error().unwrap().code(),
        "plugin.prepare.panicked"
    );

    let sequence = Arc::new(AtomicU64::new(9));
    let corrected = PluginPlan::resolve(core_definition(sequence)).unwrap();
    let app = corrected.instantiate().unwrap();
    assert_eq!(app.world().resource::<InstanceProbe>().0, 9);
}

const RETAINED_OWNER_PLUGIN_ID: PluginId = PluginId::new("nara.test.plan.retained-owner");
const RETAINED_FAILURE_PLUGIN_ID: PluginId = PluginId::new("nara.test.plan.retained-failure");
const RETAINED_OWNER_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.test.plan.retained-owner");
const RETAINED_OWNER_PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.test.plan.retained-owner");
const RETAINED_OWNER_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.plan.retained-owner", 1);
const RETAINED_FAILURE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.plan.retained-failure", 1);
const RETAINED_OWNER_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(RETAINED_OWNER_PLUGIN_ID, PluginCategory::Service)
        .shutdown_obligations(&[RETAINED_OWNER_OBLIGATION]);
const RETAINED_FAILURE_REQUIREMENT: &[PluginId] = &[RETAINED_OWNER_PLUGIN_ID];
const RETAINED_FAILURE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(RETAINED_FAILURE_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(RETAINED_FAILURE_REQUIREMENT);
const ORDERED_OWNER_PLUGIN_ID: PluginId = PluginId::new("nara.test.plan.ordered-owner");
const ORDERED_FAILURE_PLUGIN_ID: PluginId = PluginId::new("nara.test.plan.ordered-failure");
const ORDERED_OWNER_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.test.plan.ordered-owner");
const ORDERED_HOST_PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.test.plan.ordered-host");
const ORDERED_PLUGIN_PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.test.plan.ordered-plugin");
const ORDERED_OWNER_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.plan.ordered-owner", 1);
const ORDERED_FAILURE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.plan.ordered-failure", 1);
const ORDERED_OWNER_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(ORDERED_OWNER_PLUGIN_ID, PluginCategory::Service)
        .shutdown_obligations(&[ORDERED_OWNER_OBLIGATION]);
const ORDERED_FAILURE_REQUIREMENT: &[PluginId] = &[ORDERED_OWNER_PLUGIN_ID];
const ORDERED_FAILURE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(ORDERED_FAILURE_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(ORDERED_FAILURE_REQUIREMENT);

#[derive(Debug)]
struct RetainedOwnerPlugin {
    released: Arc<AtomicBool>,
    polls: Arc<AtomicU64>,
}

impl Plugin for RetainedOwnerPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &RETAINED_OWNER_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.register_plugin_runtime_close_participant(
            RETAINED_OWNER_OBLIGATION,
            RETAINED_OWNER_PARTICIPANT,
            RetainedOwnerParticipant {
                released: Arc::clone(&self.released),
                polls: Arc::clone(&self.polls),
            },
        )?;
        Ok(())
    }
}

struct RetainedOwnerParticipant {
    released: Arc<AtomicBool>,
    polls: Arc<AtomicU64>,
}

impl RuntimeCloseParticipant for RetainedOwnerParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Ok(if self.released.load(Ordering::SeqCst) {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        })
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Ok(if self.released.load(Ordering::SeqCst) {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        })
    }
}

struct OrderedParticipant {
    label: &'static str,
    log: Arc<Mutex<Vec<&'static str>>>,
}

struct RetainedOrderedParticipant {
    label: &'static str,
    log: Arc<Mutex<Vec<&'static str>>>,
    released: Arc<AtomicBool>,
    begins: Arc<AtomicU64>,
    polls: Arc<AtomicU64>,
}

impl RuntimeCloseParticipant for RetainedOrderedParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.begins.fetch_add(1, Ordering::SeqCst);
        self.log
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(self.label);
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Ok(if self.released.load(Ordering::SeqCst) {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        })
    }
}

impl RuntimeCloseParticipant for OrderedParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.log
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(self.label);
        Ok(RuntimeCloseProgress::Complete)
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        panic!("an immediately complete participant must not be polled")
    }
}

#[derive(Debug)]
struct OrderedOwnerPlugin {
    log: Arc<Mutex<Vec<&'static str>>>,
}

impl Plugin for OrderedOwnerPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &ORDERED_OWNER_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.register_plugin_runtime_close_participant(
            ORDERED_OWNER_OBLIGATION,
            ORDERED_PLUGIN_PARTICIPANT,
            OrderedParticipant {
                label: "plugin",
                log: Arc::clone(&self.log),
            },
        )?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct OrderedFailurePlugin;

impl Plugin for OrderedFailurePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &ORDERED_FAILURE_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Err(PluginError::SetupFailed {
            plugin: ORDERED_FAILURE_PLUGIN_ID,
            message: "ordered failure probe".to_owned(),
        })
    }
}

#[derive(Debug, Default)]
struct OrderedFinishFailurePlugin;

impl Plugin for OrderedFinishFailurePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &ORDERED_FAILURE_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn finish(&self, _app: &mut App) -> Result<(), PluginError> {
        Err(PluginError::SetupFailed {
            plugin: ORDERED_FAILURE_PLUGIN_ID,
            message: "ordered finish failure probe".to_owned(),
        })
    }
}

fn ordered_owner_definition(log: &Arc<Mutex<Vec<&'static str>>>) -> PluginDefinition {
    let log = Arc::clone(log);
    PluginDefinition::infallible::<OrderedOwnerPlugin, _>(
        ORDERED_OWNER_DEFINITION_ID,
        b"ordered-owner-v1",
        move || OrderedOwnerPlugin {
            log: Arc::clone(&log),
        },
    )
}

fn ordered_host_obligations(log: &Arc<Mutex<Vec<&'static str>>>) -> RuntimeObligationLedger {
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            ORDERED_HOST_PARTICIPANT,
            OrderedParticipant {
                label: "host",
                log: Arc::clone(log),
            },
        )
        .unwrap();
    obligations
}

fn retained_ordered_host_obligations(
    log: &Arc<Mutex<Vec<&'static str>>>,
    released: &Arc<AtomicBool>,
    begins: &Arc<AtomicU64>,
    polls: &Arc<AtomicU64>,
) -> RuntimeObligationLedger {
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            ORDERED_HOST_PARTICIPANT,
            RetainedOrderedParticipant {
                label: "host",
                log: Arc::clone(log),
                released: Arc::clone(released),
                begins: Arc::clone(begins),
                polls: Arc::clone(polls),
            },
        )
        .unwrap();
    obligations
}

#[derive(Debug, Default)]
struct RetainedFailurePlugin;

impl Plugin for RetainedFailurePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &RETAINED_FAILURE_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Err(PluginError::SetupFailed {
            plugin: RETAINED_FAILURE_PLUGIN_ID,
            message: "retained failure probe".to_owned(),
        })
    }
}

#[test]
fn retained_plan_failure_keeps_unfinished_close_owner_retryable() {
    let released = Arc::new(AtomicBool::new(false));
    let polls = Arc::new(AtomicU64::new(0));
    let owner_definition = PluginDefinition::infallible::<RetainedOwnerPlugin, _>(
        RETAINED_OWNER_DEFINITION_ID,
        b"retained-owner-v1",
        {
            let released = Arc::clone(&released);
            let polls = Arc::clone(&polls);
            move || RetainedOwnerPlugin {
                released: Arc::clone(&released),
                polls: Arc::clone(&polls),
            }
        },
    );
    let failure_definition = PluginDefinition::infallible::<RetainedFailurePlugin, _>(
        RETAINED_FAILURE_DEFINITION_ID,
        b"retained-failure-v1",
        RetainedFailurePlugin::default,
    );
    let plan = PluginPlan::resolve((owner_definition, failure_definition)).unwrap();

    let mut failure = plan
        .instantiate_retained_with_close_policy(RuntimeClosePolicy::new(Duration::ZERO))
        .unwrap_err();

    assert_eq!(
        failure.retirement_state(),
        RuntimeCandidateRetirementState::Retiring
    );
    assert_eq!(
        failure.drive_retirement(),
        RuntimeCandidateRetirementState::RetirementIncomplete
    );
    let first_polls = polls.load(Ordering::SeqCst);

    released.store(true, Ordering::SeqCst);
    assert_eq!(
        failure.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    assert_eq!(first_polls, 2);
    assert_eq!(polls.load(Ordering::SeqCst), first_polls + 1);
}

#[test]
fn runtime_construction_uses_one_reverse_ordered_obligation_ledger() {
    let success_log = Arc::new(Mutex::new(Vec::new()));
    let success_plan = PluginPlan::resolve(ordered_owner_definition(&success_log)).unwrap();
    let candidate = success_plan
        .instantiate_runtime_candidate(
            RuntimeAdmissionReservation::try_acquire().unwrap(),
            ordered_host_obligations(&success_log),
            RuntimeClosePolicy::default(),
        )
        .unwrap();
    let mut retirement = candidate.begin_retirement();

    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    assert_eq!(
        *success_log
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
        ["plugin", "host"]
    );

    let failure_log = Arc::new(Mutex::new(Vec::new()));
    let failure_definition = PluginDefinition::infallible::<OrderedFailurePlugin, _>(
        ORDERED_FAILURE_DEFINITION_ID,
        b"ordered-failure-v1",
        OrderedFailurePlugin::default,
    );
    let failure_plan =
        PluginPlan::resolve((ordered_owner_definition(&failure_log), failure_definition)).unwrap();
    let mut failure = failure_plan
        .instantiate_runtime_candidate(
            RuntimeAdmissionReservation::try_acquire().unwrap(),
            ordered_host_obligations(&failure_log),
            RuntimeClosePolicy::default(),
        )
        .unwrap_err();

    assert_eq!(
        failure.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    assert_eq!(
        *failure_log
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
        ["plugin", "host"]
    );
}

#[test]
fn runtime_prepare_failure_retains_the_host_owner_for_exactly_once_retry() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let released = Arc::new(AtomicBool::new(false));
    let begins = Arc::new(AtomicU64::new(0));
    let polls = Arc::new(AtomicU64::new(0));
    let failing = PluginDefinition::fallible::<CorePlugin, _>(
        CORE_POLICY_A,
        b"runtime-prepare-failure-v1",
        || Err(PluginPrepareFailure::new("test.runtime.prepare.rejected")),
    );
    let plan = PluginPlan::resolve(failing).unwrap();

    let mut failure = plan
        .instantiate_runtime_candidate(
            RuntimeAdmissionReservation::try_acquire().unwrap(),
            retained_ordered_host_obligations(&log, &released, &begins, &polls),
            RuntimeClosePolicy::new(Duration::ZERO),
        )
        .unwrap_err();

    assert!(matches!(
        failure.error(),
        RuntimeConstructionError::Plugin(PluginInstantiationError::Prepare(error))
            if error.code() == "test.runtime.prepare.rejected"
    ));
    assert_eq!(
        failure.retirement_state(),
        RuntimeCandidateRetirementState::Retiring
    );
    assert_eq!(
        failure.drive_retirement(),
        RuntimeCandidateRetirementState::RetirementIncomplete
    );
    assert_eq!(begins.load(Ordering::SeqCst), 1);
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    assert_eq!(
        *log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
        ["host"]
    );

    released.store(true, Ordering::SeqCst);
    assert_eq!(
        failure.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    assert_eq!(
        failure.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    assert_eq!(begins.load(Ordering::SeqCst), 1);
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    assert_eq!(
        *log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
        ["host"]
    );
}

#[test]
fn runtime_finish_failure_retires_plugin_then_host_and_retries_only_pending_owner() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let released = Arc::new(AtomicBool::new(false));
    let begins = Arc::new(AtomicU64::new(0));
    let polls = Arc::new(AtomicU64::new(0));
    let failure_definition = PluginDefinition::infallible::<OrderedFinishFailurePlugin, _>(
        ORDERED_FAILURE_DEFINITION_ID,
        b"ordered-finish-failure-v1",
        OrderedFinishFailurePlugin::default,
    );
    let plan = PluginPlan::resolve((ordered_owner_definition(&log), failure_definition)).unwrap();

    let mut failure = plan
        .instantiate_runtime_candidate(
            RuntimeAdmissionReservation::try_acquire().unwrap(),
            retained_ordered_host_obligations(&log, &released, &begins, &polls),
            RuntimeClosePolicy::new(Duration::ZERO),
        )
        .unwrap_err();

    assert!(matches!(
        failure.error(),
        RuntimeConstructionError::Plugin(PluginInstantiationError::Plugin(
            PluginError::SetupFailed { plugin, .. }
        )) if *plugin == ORDERED_FAILURE_PLUGIN_ID
    ));
    assert_eq!(
        failure.drive_retirement(),
        RuntimeCandidateRetirementState::RetirementIncomplete
    );
    assert_eq!(begins.load(Ordering::SeqCst), 1);
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    assert_eq!(
        *log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
        ["plugin", "host"]
    );

    released.store(true, Ordering::SeqCst);
    assert_eq!(
        failure.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    assert_eq!(
        failure.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    assert_eq!(begins.load(Ordering::SeqCst), 1);
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    assert_eq!(
        *log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
        ["plugin", "host"]
    );
}

#[derive(Debug, Clone, Copy)]
enum HookViolation {
    Plugin,
    Group,
    Runner,
}

#[derive(Debug, Default)]
struct HookNestedGroup;

impl PluginGroup for HookNestedGroup {
    const ID: PluginGroupId = PluginGroupId::new("nara.test.plan.hook-nested-group");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_definition(PluginDefinition::for_default::<AlternatePlugin>())
    }
}

#[derive(Debug)]
struct HookViolationPlugin(HookViolation);

impl Plugin for HookViolationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &VIOLATOR_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), nara_app::PluginError> {
        match self.0 {
            HookViolation::Plugin => {
                let _ = app.add_plugins(OptionalPlugin);
            }
            HookViolation::Group => {
                let _ = app.add_plugins(HookNestedGroup);
            }
            HookViolation::Runner => {
                let _ = app.set_runner(|_| Ok(nara_app::AppExit::Success));
            }
        }
        Ok(())
    }
}

#[test]
fn ignored_hook_plugin_group_and_runner_mutation_are_sticky_failures() {
    for (violation, expected_mutation) in [
        (HookViolation::Plugin, PluginHookMutation::PluginMembership),
        (HookViolation::Group, PluginHookMutation::PluginMembership),
        (HookViolation::Runner, PluginHookMutation::RunnerSelection),
    ] {
        let mut app = App::new();
        let Err(error) = app.add_plugins(HookViolationPlugin(violation)) else {
            panic!("hook mutation must reject");
        };
        assert!(matches!(
            error,
            AddPluginsError::Plugin(PluginError::HookMutationForbidden {
                plugin: VIOLATOR_ID,
                hook: PluginHook::Build,
                mutation,
            }) if mutation == expected_mutation
        ));
        assert!(app.plugin_failure_report().is_some());
        assert!(!app.has_plugin(OPTIONAL_ID));
        assert!(!app.has_plugin(ALTERNATE_ID));
        assert!(!app.has_raw_runner());
        assert!(app.seal().is_err());
    }
}

#[derive(Debug)]
struct FinishHookViolationPlugin(HookViolation);

impl Plugin for FinishHookViolationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &VIOLATOR_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        match self.0 {
            HookViolation::Plugin => {
                let _ = app.add_plugins(OptionalPlugin);
            }
            HookViolation::Group => {
                let _ = app.add_plugins(HookNestedGroup);
            }
            HookViolation::Runner => {
                let _ = app.set_runner(|_| Ok(nara_app::AppExit::Success));
            }
        }
        Ok(())
    }
}

#[test]
fn ignored_finish_hook_composition_and_runner_mutation_are_sticky_failures() {
    for (violation, expected_mutation) in [
        (HookViolation::Plugin, PluginHookMutation::PluginMembership),
        (HookViolation::Group, PluginHookMutation::PluginMembership),
        (HookViolation::Runner, PluginHookMutation::RunnerSelection),
    ] {
        let mut app = App::new();
        app.add_plugins(FinishHookViolationPlugin(violation))
            .unwrap();
        let error = app.run_once(Duration::ZERO).unwrap_err();
        assert!(matches!(
            error.plugin_error(),
            Some(PluginError::HookMutationForbidden {
                plugin: VIOLATOR_ID,
                hook: PluginHook::Finish,
                mutation,
            }) if *mutation == expected_mutation
        ));
        assert_eq!(
            app.plugin_failure_report()
                .and_then(|report| report.primary())
                .map(nara_app::PluginFailure::hook),
            Some(PluginHook::Finish)
        );
        assert!(!app.has_plugin(OPTIONAL_ID));
        assert!(!app.has_plugin(ALTERNATE_ID));
        assert!(!app.has_raw_runner());
    }
}

#[derive(Debug)]
struct ObligationPlugin {
    register: bool,
}

impl Plugin for ObligationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &OBLIGATION_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        if self.register {
            app.register_plugin_shutdown_obligation(TEST_OBLIGATION)?;
        }
        Ok(())
    }
}

#[test]
fn direct_app_sealing_requires_declared_shutdown_obligations() {
    let mut missing = App::new();
    missing
        .add_plugins(ObligationPlugin { register: false })
        .unwrap();
    assert_eq!(
        missing.seal().unwrap_err(),
        PluginError::MissingShutdownObligation {
            plugin: OBLIGATION_PLUGIN_ID,
            obligation: TEST_OBLIGATION,
        }
    );

    let mut registered = App::new();
    registered
        .add_plugins(ObligationPlugin { register: true })
        .unwrap();
    let sealed = registered.seal().unwrap();
    assert!(!sealed.started());
    assert!(!sealed.has_raw_runner());
}

#[derive(Debug, Clone, Copy)]
enum PreflightBehavior {
    Accept,
    Reject,
    Panic,
}

#[derive(Debug)]
struct PreflightPlugin(PreflightBehavior);

impl Plugin for PreflightPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &PREFLIGHT_DECLARATION
    }

    fn preflight(
        &self,
        _context: &nara_app::PluginPreflightContext<'_>,
    ) -> Result<(), PluginError> {
        match self.0 {
            PreflightBehavior::Accept => Ok(()),
            PreflightBehavior::Reject => Err(PluginError::SetupFailed {
                plugin: PREFLIGHT_PLUGIN_ID,
                message: "preflight rejected".to_owned(),
            }),
            PreflightBehavior::Panic => panic!("preflight panicked"),
        }
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

#[derive(Debug)]
struct CommittedProbePlugin(Arc<AtomicU64>);

impl Plugin for CommittedProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &COMMITTED_PROBE_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn shutdown(&self, _context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[test]
fn later_preflight_rejection_retires_the_committed_prefix_once() {
    let shutdowns = Arc::new(AtomicU64::new(0));
    let mut app = App::new();
    let Err(error) = app.add_plugins((
        CommittedProbePlugin(Arc::clone(&shutdowns)),
        PreflightPlugin(PreflightBehavior::Reject),
    )) else {
        panic!("the later preflight must reject after the first commit");
    };

    assert!(matches!(
        error,
        AddPluginsError::Plugin(PluginError::CommittedPreflightRejected {
            plugin: PREFLIGHT_PLUGIN_ID,
            ..
        })
    ));
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    assert_eq!(app.plugin_lifecycle_state(), PluginLifecycleState::Poisoned);
    assert!(app.has_plugin(COMMITTED_PROBE_PLUGIN_ID));
    assert!(!app.has_plugin(PREFLIGHT_PLUGIN_ID));
    let report = app.plugin_failure_report().unwrap();
    assert_eq!(report.primary().unwrap().hook(), PluginHook::Preflight);
    assert!(report.shutdown_complete());

    drop(app);
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
}

#[test]
fn first_preflight_rejection_is_retryable_but_a_panic_poison_is_sticky() {
    let mut retryable = App::new();
    let Err(AddPluginsError::Plugin(rejection)) =
        retryable.add_plugins(PreflightPlugin(PreflightBehavior::Reject))
    else {
        panic!("typed preflight rejection must stay in the plugin phase");
    };
    assert!(matches!(rejection, PluginError::SetupFailed { .. }));
    assert_eq!(
        retryable.plugin_lifecycle_state(),
        PluginLifecycleState::Configuring
    );
    assert!(!retryable.has_plugin(PREFLIGHT_PLUGIN_ID));
    retryable
        .add_plugins(PreflightPlugin(PreflightBehavior::Accept))
        .unwrap();
    retryable.seal().unwrap();

    let mut poisoned = App::new();
    let Err(AddPluginsError::Plugin(panic_error)) =
        poisoned.add_plugins(PreflightPlugin(PreflightBehavior::Panic))
    else {
        panic!("preflight panic must be captured in the plugin phase");
    };
    assert_eq!(
        panic_error,
        PluginError::HookPanicked {
            plugin: PREFLIGHT_PLUGIN_ID,
            hook: PluginHook::Preflight,
        }
    );
    assert_eq!(
        poisoned.plugin_lifecycle_state(),
        PluginLifecycleState::Poisoned
    );
    assert!(poisoned.seal().is_err());
}

#[derive(Debug)]
struct FailingBuildPlugin(Arc<AtomicU64>);

impl Plugin for FailingBuildPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &FAILING_BUILD_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Err(PluginError::SetupFailed {
            plugin: FAILING_BUILD_PLUGIN_ID,
            message: "build rejected".to_owned(),
        })
    }

    fn shutdown(&self, _context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[derive(Debug)]
struct FailingFinishPlugin(Arc<AtomicU64>);

impl Plugin for FailingFinishPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &FAILING_FINISH_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn finish(&self, _app: &mut App) -> Result<(), PluginError> {
        Err(PluginError::SetupFailed {
            plugin: FAILING_FINISH_PLUGIN_ID,
            message: "finish rejected".to_owned(),
        })
    }

    fn shutdown(&self, _context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[derive(Debug, Default)]
struct FailingShutdownPlugin;

impl Plugin for FailingShutdownPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &FAILING_SHUTDOWN_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn shutdown(&self, _context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        Err(PluginError::SetupFailed {
            plugin: FAILING_SHUTDOWN_PLUGIN_ID,
            message: "shutdown rejected".to_owned(),
        })
    }
}

#[derive(Debug)]
struct ShutdownProbePlugin<const INDEX: u8> {
    log: Arc<Mutex<Vec<&'static str>>>,
}

impl<const INDEX: u8> Plugin for ShutdownProbePlugin<INDEX> {
    fn declaration() -> &'static PluginDeclaration {
        match INDEX {
            0 => &SHUTDOWN_A_DECLARATION,
            1 => &SHUTDOWN_B_DECLARATION,
            2 => &SHUTDOWN_C_DECLARATION,
            _ => panic!("unsupported shutdown probe index"),
        }
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn shutdown(&self, _context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        let label = match INDEX {
            0 => "a",
            1 => "b",
            2 => "c",
            _ => panic!("unsupported shutdown probe index"),
        };
        self.log
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(label);
        match INDEX {
            0 => Ok(()),
            1 => Err(PluginError::SetupFailed {
                plugin: SHUTDOWN_B_PLUGIN_ID,
                message: "shutdown b rejected".to_owned(),
            }),
            2 => panic!("shutdown c panicked"),
            _ => unreachable!("the declaration rejects unsupported probe indices"),
        }
    }
}

#[test]
fn shutdown_is_reverse_once_only_and_aggregates_error_and_panic() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut app = App::new();
    app.add_plugins((
        ShutdownProbePlugin::<0> {
            log: Arc::clone(&log),
        },
        ShutdownProbePlugin::<1> {
            log: Arc::clone(&log),
        },
        ShutdownProbePlugin::<2> {
            log: Arc::clone(&log),
        },
    ))
    .unwrap();

    let PluginShutdownError::Failure(report) = app.shutdown_plugins().unwrap_err() else {
        panic!("shutdown failures must be aggregated");
    };
    assert_eq!(
        *log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
        ["c", "b", "a"]
    );
    assert_eq!(
        report
            .shutdown_failures()
            .iter()
            .map(nara_app::PluginFailure::plugin)
            .collect::<Vec<_>>(),
        [SHUTDOWN_C_PLUGIN_ID, SHUTDOWN_B_PLUGIN_ID]
    );
    assert!(report.shutdown_complete());

    assert!(matches!(
        app.shutdown_plugins(),
        Err(PluginShutdownError::Failure(_))
    ));
    assert_eq!(
        *log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
        ["c", "b", "a"]
    );
    drop(app);
    assert_eq!(
        *log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
        ["c", "b", "a"]
    );
}

#[test]
fn app_drop_catches_shutdown_panic_during_an_outer_unwind() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let unwind_log = Arc::clone(&log);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let mut app = App::new();
        app.add_plugins(ShutdownProbePlugin::<2> { log: unwind_log })
            .unwrap();
        panic!("outer unwind probe");
    }));

    assert!(result.is_err());
    assert_eq!(
        *log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
        ["c"]
    );
}

#[test]
fn committed_hook_failures_shutdown_immediately_and_preserve_typed_reports() {
    let build_shutdowns = Arc::new(AtomicU64::new(0));
    let mut build_app = App::new();
    let Err(AddPluginsError::Plugin(build_error)) =
        build_app.add_plugins(FailingBuildPlugin(Arc::clone(&build_shutdowns)))
    else {
        panic!("build rejection must stay in the plugin phase");
    };
    assert!(matches!(build_error, PluginError::SetupFailed { .. }));
    assert_eq!(build_shutdowns.load(Ordering::SeqCst), 1);
    assert_eq!(
        build_app.plugin_lifecycle_state(),
        PluginLifecycleState::Poisoned
    );
    let build_report = build_app.plugin_failure_report().unwrap();
    assert_eq!(build_report.primary().unwrap().hook(), PluginHook::Build);
    assert!(build_report.shutdown_complete());

    let finish_shutdowns = Arc::new(AtomicU64::new(0));
    let mut finish_app = App::new();
    finish_app
        .add_plugins(FailingFinishPlugin(Arc::clone(&finish_shutdowns)))
        .unwrap();
    assert!(matches!(
        finish_app.seal().unwrap_err(),
        PluginError::SetupFailed { .. }
    ));
    assert_eq!(finish_shutdowns.load(Ordering::SeqCst), 1);

    let mut shutdown_app = App::new();
    shutdown_app.add_plugins(FailingShutdownPlugin).unwrap();
    let PluginShutdownError::Failure(report) = shutdown_app.shutdown_plugins().unwrap_err() else {
        panic!("shutdown rejection must produce a typed failure report");
    };
    assert!(report.primary().is_none());
    assert_eq!(report.shutdown_failures().len(), 1);
    assert_eq!(report.shutdown_failures()[0].hook(), PluginHook::Shutdown);
    assert!(report.shutdown_complete());
}
