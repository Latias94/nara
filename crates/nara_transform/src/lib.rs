//! Authored 2D transforms and completed runtime-global projection.

mod presentation;
mod propagation;
mod snapshot;
#[cfg(test)]
mod snapshot_tests;

pub use presentation::FixedTransformInterpolation;
pub use snapshot::{
    FixedSpatialSnapshot2d, HistoricalSpatialSnapshot2d, PresentedSpatialSnapshot2d,
    SpatialSnapshot2d, SpatialSnapshotError, SpatialSnapshotPhase, SpatialSnapshotStamp,
    fixed_spatial_snapshot, historical_spatial_snapshot, presented_spatial_snapshot,
};

use nara_app::{App, CoreStage, Plugin, PluginError, PluginPreflightContext};
use nara_core::{Mat3, Vec2};
use nara_ecs::{Component, schedule::IntoScheduleConfigs, upstream::system::ScheduleSystem};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistry, ComponentRegistryError, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Component)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Transform2d {
    pub translation: Vec2,
    pub rotation: f32,
    pub scale: Vec2,
}

impl Transform2d {
    pub const IDENTITY: Self = Self {
        translation: Vec2::ZERO,
        rotation: 0.0,
        scale: Vec2::ONE,
    };

    pub fn from_translation(translation: Vec2) -> Self {
        Self {
            translation,
            ..Self::IDENTITY
        }
    }

    pub fn matrix(&self) -> Mat3 {
        Mat3::from_scale_angle_translation(self.scale, self.rotation, self.translation)
    }
}

impl Default for Transform2d {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// An ordinary affine value read from a spatial snapshot, with no publication authority.
///
/// ```compile_fail
/// # use nara_ecs::World;
/// # use nara_transform::GlobalTransform2d;
/// # let mut world = World::new();
/// world.query::<&GlobalTransform2d>();
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobalTransform2d(Mat3);

impl GlobalTransform2d {
    /// Returns the completed world-space affine matrix.
    #[must_use]
    #[inline]
    pub const fn matrix(&self) -> Mat3 {
        self.0
    }

    /// Returns the completed world-space origin.
    #[must_use]
    #[inline]
    pub fn translation(&self) -> Vec2 {
        self.0.transform_point2(Vec2::ZERO)
    }
}

/// Registers fixed-tick systems that consume completed runtime 2D global transforms.
///
/// The transform module owns the concrete propagation set and its placement between simulation
/// mutation and finalization. Product systems use this semantic entry point instead of depending
/// on that private schedule topology.
/// Finalize/Capture systems read [`FixedSpatialSnapshot2d`] directly without using this helper.
pub fn add_fixed_transform_projection_systems<M>(
    app: &mut App,
    systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
) -> Result<&mut App, PluginError> {
    app.add_systems(
        CoreStage::FixedUpdate,
        systems
            .after(__private::TransformSet::Propagate)
            .before(nara_app::FixedUpdateSet::Finalize),
    )
}

/// Registers Extract readers after this frame's presentation snapshot is complete.
pub fn add_presented_transform_projection_systems<M>(
    app: &mut App,
    systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
) -> Result<&mut App, PluginError> {
    app.add_systems(
        CoreStage::Extract,
        systems.after(__private::TransformSet::Present),
    )
}

#[doc(hidden)]
pub mod __private {
    use nara_ecs::SystemSet;

    /// Installs the transform owner's optional fixed-history presentation projection.
    pub fn install_render_projection(app: &mut nara_app::App) -> Result<(), nara_app::PluginError> {
        crate::presentation::install(app)
    }

    /// Provisional first-party boundary after the runtime 2D projection is complete.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
    pub enum TransformSet {
        Propagate,
        Present,
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TransformPlugin;

pub const TRANSFORM_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.transform");
pub const TRANSFORM_SCHEMA_PROVIDER_ID: nara_app::PluginSchemaProviderId =
    nara_app::PluginSchemaProviderId::new("nara.transform.components");
pub const TRANSFORM_SCHEMA_OWNER_ID: nara_reflect::ComponentSchemaOwnerId =
    nara_reflect::ComponentSchemaOwnerId::new("nara.transform.components");
pub const TRANSFORM_SCHEMA_PROVIDER: nara_reflect::ComponentSchemaProviderDefinition =
    nara_reflect::ComponentSchemaProviderDefinition::with_validation(
        TRANSFORM_SCHEMA_OWNER_ID,
        TRANSFORM_SCHEMA_PROVIDER_ID,
        nara_reflect::ComponentSchemaProviderBindingId::new("nara.transform.components.native", 1),
        transform_schema_catalog,
        validate_transform_components,
        register_transform_components,
    )
    .with_predecessor(transform_schema_predecessor_catalog);
const TRANSFORM_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] = &[
    nara_reflect::COMPONENT_REGISTRY_PLUGIN_ID,
    nara_hierarchy::HIERARCHY_PLUGIN_ID,
];
pub const TRANSFORM_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(TRANSFORM_PLUGIN_ID, nara_app::PluginCategory::Core)
        .requires_plugins(TRANSFORM_PLUGIN_REQUIREMENTS)
        .provides_schema(&[TRANSFORM_SCHEMA_PROVIDER_ID]);

impl Plugin for TransformPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &TRANSFORM_PLUGIN_DECLARATION
    }

    fn preflight(&self, context: &PluginPreflightContext<'_>) -> Result<(), PluginError> {
        let component_id = ComponentTypeId::new("nara.transform.Transform2d");
        let registry = nara_reflect::registry_for_plugin_preflight(
            context,
            TRANSFORM_PLUGIN_ID,
            component_id.as_str(),
        )?;
        TRANSFORM_SCHEMA_PROVIDER
            .preflight(registry)
            .map_err(|error| {
                PluginError::component_registration(
                    TRANSFORM_PLUGIN_ID,
                    component_id.as_str(),
                    error,
                )
            })
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let component_id = ComponentTypeId::new("nara.transform.Transform2d");
        nara_reflect::register_schema_provider_for_plugin(
            app,
            TRANSFORM_PLUGIN_ID,
            component_id.as_str(),
            &TRANSFORM_SCHEMA_PROVIDER,
        )?;
        propagation::install(app)?;
        presentation::install(app)
    }
}

pub fn register_transform_components(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    validate_transform_components(registry)?;
    registry.register_persistent_component_with_codec::<Transform2d, _, _>(
        transform_schema(),
        |value| {
            Ok(Transform2d {
                translation: read_vec2(value.field("translation")?, "translation")?,
                rotation: read_f32(value.field("rotation")?, "rotation")?,
                scale: read_vec2(value.field("scale")?, "scale")?,
            })
        },
        |transform| {
            Ok(ComponentValue::map([
                ("translation", vec2_value(transform.translation)?),
                (
                    "rotation",
                    ComponentValue::f64(f64::from(transform.rotation))?,
                ),
                ("scale", vec2_value(transform.scale)?),
            ]))
        },
    )?;
    Ok(())
}

fn transform_schema_catalog()
-> Result<nara_reflect::ComponentSchemaCatalog, nara_reflect::ComponentSchemaProviderSourceError> {
    let predecessor = transform_schema_predecessor_catalog()?;
    let mut current = nara_reflect::ComponentSchemaCatalog::successor_of(&predecessor)
        .expect("the fixed generation-one Transform catalog has a successor");
    current.components.push(transform_schema());
    Ok(current)
}

fn transform_schema_predecessor_catalog()
-> Result<nara_reflect::ComponentSchemaCatalog, nara_reflect::ComponentSchemaProviderSourceError> {
    Ok(nara_reflect::ComponentSchemaCatalog {
        components: vec![transform_schema_predecessor()],
        ..nara_reflect::ComponentSchemaCatalog::default()
    })
}

fn transform_schema() -> ComponentSchema {
    transform_schema_predecessor().with_authoring_initial_value(ComponentValue::map([
        (
            "translation",
            vec2_value(Vec2::ZERO).expect("zero translation is finite"),
        ),
        (
            "rotation",
            ComponentValue::f64(0.0).expect("zero rotation is finite"),
        ),
        (
            "scale",
            vec2_value(Vec2::ONE).expect("unit scale is finite"),
        ),
    ]))
}

// Generation-one wire contract retained as owner-local catalog lineage data.
fn transform_schema_predecessor() -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("nara.transform.Transform2d"),
        "Transform 2D",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields(transform_fields())
}

fn validate_transform_components(
    registry: &ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    registry.validate_component_registration::<Transform2d>(&ComponentTypeId::new(
        "nara.transform.Transform2d",
    ))
}

fn transform_fields() -> [ComponentFieldSchema; 5] {
    [
        ComponentFieldSchema::required(
            ComponentFieldId::new("translation.x"),
            "Translation X",
            ComponentFieldPath::from_fields(["translation", "x"]),
            ComponentValueKind::F64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("translation.y"),
            "Translation Y",
            ComponentFieldPath::from_fields(["translation", "y"]),
            ComponentValueKind::F64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("rotation"),
            "Rotation",
            ComponentFieldPath::from_fields(["rotation"]),
            ComponentValueKind::F64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("scale.x"),
            "Scale X",
            ComponentFieldPath::from_fields(["scale", "x"]),
            ComponentValueKind::F64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("scale.y"),
            "Scale Y",
            ComponentFieldPath::from_fields(["scale", "y"]),
            ComponentValueKind::F64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ]
}

fn read_vec2(value: &ComponentValue, field: &str) -> Result<Vec2, ComponentCodecError> {
    Ok(Vec2::new(
        read_f32(value.field("x")?, &format!("{field}.x"))?,
        read_f32(value.field("y")?, &format!("{field}.y"))?,
    ))
}

fn read_f32(value: &ComponentValue, field: &str) -> Result<f32, ComponentCodecError> {
    let value = value
        .as_f64()
        .ok_or_else(|| ComponentCodecError::invalid_field(field, "finite f32"))?;
    if value < f64::from(f32::MIN) || value > f64::from(f32::MAX) {
        return Err(ComponentCodecError::invalid_field(field, "finite f32"));
    }
    Ok(value as f32)
}

fn vec2_value(value: Vec2) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([
        ("x", ComponentValue::f64(f64::from(value.x))?),
        ("y", ComponentValue::f64(f64::from(value.y))?),
    ]))
}

pub mod prelude {
    pub use crate::{
        FixedSpatialSnapshot2d, FixedTransformInterpolation, GlobalTransform2d,
        HistoricalSpatialSnapshot2d, PresentedSpatialSnapshot2d, SpatialSnapshot2d,
        SpatialSnapshotError, SpatialSnapshotPhase, SpatialSnapshotStamp, Transform2d,
        TransformPlugin, add_fixed_transform_projection_systems,
        add_presented_transform_projection_systems, fixed_spatial_snapshot,
        historical_spatial_snapshot, presented_spatial_snapshot,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::propagation::TransformCompletionError;
    use std::time::Duration;

    use nara_app::{
        __RuntimeDriverPort, CoreStage, FixedTime, FixedUpdateSet, PluginCategory,
        PluginDeclaration, PluginId, PluginLifecycleState, RuntimeAdmissionReservation,
        RuntimeCandidateRetirementState, RuntimeClosePolicy, RuntimeControl,
        RuntimeControlRequestResult, RuntimeObligationLedger, RuntimeState,
    };
    use nara_ecs::{Commands, Entity, Query, Res, ResMut, Resource, schedule::IntoScheduleConfigs};
    use nara_hierarchy::{
        Children, HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
    };
    use nara_reflect::ComponentRegistryPlugin;

    trait SnapshotTestRead {
        fn snapshot_global(&self, entity: Entity) -> Option<GlobalTransform2d>;
        fn snapshot_available(&self) -> bool;
    }

    impl SnapshotTestRead for nara_ecs::World {
        fn snapshot_global(&self, entity: Entity) -> Option<GlobalTransform2d> {
            historical_spatial_snapshot(self).ok()?.global(entity).ok()
        }
        fn snapshot_available(&self) -> bool {
            historical_spatial_snapshot(self).is_ok()
        }
    }

    #[test]
    fn creation_template_preserves_wire_lineage_and_decodes_to_identity_transform() {
        let predecessor = transform_schema_predecessor_catalog().unwrap();
        let current = transform_schema_catalog().unwrap();
        assert_eq!(predecessor.generation(), 1);
        assert_eq!(current.generation(), 2);
        assert_eq!(current.predecessor(), Some(&predecessor.fingerprint()));
        assert!(
            predecessor.components[0]
                .authoring_initial_value()
                .is_none()
        );
        assert_eq!(current.components[0].version(), ComponentSchemaVersion::ONE);
        let mut registry = ComponentRegistry::new();
        register_transform_components(&mut registry).unwrap();
        registry.freeze().unwrap();
        let snapshot = registry.snapshot().unwrap();
        let schema = registry
            .schema(&ComponentTypeId::new("nara.transform.Transform2d"))
            .unwrap();
        let mut world = nara_ecs::World::new();
        let entity = world.spawn_empty().id();
        snapshot
            .preflight_component(schema.id(), schema.authoring_initial_value().unwrap())
            .unwrap()
            .apply(&mut world, entity)
            .unwrap();
        assert_eq!(
            world.get::<Transform2d>(entity),
            Some(&Transform2d::default())
        );
        nara_reflect::__private::resolve_schema_provider(TRANSFORM_SCHEMA_PROVIDER).unwrap();
    }

    const CONFLICTING_TRANSFORM_PLUGIN_ID: PluginId = PluginId::new("nara.test.transform-conflict");
    const CONFLICTING_TRANSFORM_PROVIDER_ID: nara_app::PluginSchemaProviderId =
        nara_app::PluginSchemaProviderId::new("nara.test.transform-conflict.components");
    const CONFLICTING_TRANSFORM_PROVIDER: nara_reflect::ComponentSchemaProviderDefinition =
        nara_reflect::ComponentSchemaProviderDefinition::with_validation(
            nara_reflect::ComponentSchemaOwnerId::new("nara.test.transform-conflict.components"),
            CONFLICTING_TRANSFORM_PROVIDER_ID,
            nara_reflect::ComponentSchemaProviderBindingId::new(
                "nara.test.transform-conflict.native",
                1,
            ),
            transform_schema_catalog,
            validate_transform_components,
            register_transform_components,
        )
        .with_predecessor(transform_schema_predecessor_catalog);
    const CONFLICTING_TRANSFORM_DECLARATION: PluginDeclaration =
        PluginDeclaration::new(CONFLICTING_TRANSFORM_PLUGIN_ID, PluginCategory::Runtime)
            .requires_plugins(nara_reflect::COMPONENT_REGISTRY_PLUGIN_REQUIREMENT)
            .provides_schema(&[CONFLICTING_TRANSFORM_PROVIDER_ID]);

    struct ConflictingTransformPlugin;

    fn spatial_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            ComponentRegistryPlugin,
            nara_hierarchy::HierarchyPlugin,
            TransformPlugin,
        ))
        .expect("the spatial foundation should install");
        app.init_resource::<ProjectionEditPort>().unwrap();
        app.__register_driver_port::<ProjectionEditPort>(
            ProjectionEditPort::apply_driver_input_in_world,
        )
        .unwrap();
        app
    }

    enum ProjectionEdit {
        Scale { entity: Entity, scale: Vec2 },
        RemoveTransform(Entity),
        RetireParent { parent: Entity, child: Entity },
    }

    #[derive(Default, Resource)]
    struct ProjectionEditPort;

    impl __RuntimeDriverPort for ProjectionEditPort {
        type Input = ProjectionEdit;
        type Output = Result<(), TransformCompletionError>;

        fn apply_driver_input(&mut self, _input: ProjectionEdit) -> Self::Output {
            unreachable!("projection edits need the owned transform world")
        }
    }

    impl ProjectionEditPort {
        fn apply_driver_input_in_world(
            world: &mut nara_ecs::World,
            input: ProjectionEdit,
        ) -> Result<
            Result<(), TransformCompletionError>,
            (nara_app::RuntimeWorldAccessError, ProjectionEdit),
        > {
            match input {
                ProjectionEdit::Scale { entity, scale } => {
                    world.get_mut::<Transform2d>(entity).unwrap().scale = scale;
                }
                ProjectionEdit::RemoveTransform(entity) => {
                    world.entity_mut(entity).remove::<Transform2d>();
                }
                ProjectionEdit::RetireParent { parent, child } => {
                    let child_from = HierarchyPlacement::current(world, child).unwrap();
                    let parent_from = HierarchyPlacement::current(world, parent).unwrap();
                    HierarchyWriter::new(world)
                        .apply_batch(&[
                            HierarchyOperation::Move {
                                entity: child,
                                from: child_from,
                                to: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
                            },
                            HierarchyOperation::Remove {
                                entity: parent,
                                from: parent_from,
                            },
                        ])
                        .unwrap();
                    assert!(world.despawn(parent));
                    return Ok(Ok(()));
                }
            }
            Ok(propagation::complete_transform_projection(world))
        }
    }

    fn place_ordered_hierarchy(
        world: &mut nara_ecs::World,
        roots: &[Entity],
        edges: &[(Entity, Entity)],
    ) {
        let mut operations = Vec::with_capacity(roots.len() + edges.len());
        operations.extend(
            roots
                .iter()
                .copied()
                .map(|entity| HierarchyOperation::Place {
                    entity,
                    placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
                }),
        );
        operations.extend(edges.iter().copied().map(|(entity, parent)| {
            HierarchyOperation::Place {
                entity,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
            }
        }));
        HierarchyWriter::new(world)
            .apply_batch(&operations)
            .expect("the ordered hierarchy should be valid");
    }

    impl Plugin for ConflictingTransformPlugin {
        fn declaration() -> &'static PluginDeclaration {
            &CONFLICTING_TRANSFORM_DECLARATION
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            nara_reflect::register_schema_provider_for_plugin(
                app,
                CONFLICTING_TRANSFORM_PLUGIN_ID,
                CONFLICTING_TRANSFORM_PROVIDER_ID.as_str(),
                &CONFLICTING_TRANSFORM_PROVIDER,
            )
        }
    }

    #[test]
    fn plugin_preflight_reports_component_conflicts_without_poisoning_app() {
        let mut app = App::new();
        app.add_plugins((
            ComponentRegistryPlugin,
            nara_hierarchy::HierarchyPlugin,
            ConflictingTransformPlugin,
        ))
        .expect("the conflicting provider should install through the registry owner");

        let error = app
            .add_plugin(TransformPlugin)
            .err()
            .expect("duplicate component registration should fail preflight");

        assert!(matches!(
            error,
            nara_app::AddPluginsError::Plugin(ref failure) if matches!(failure.error(), PluginError::ComponentRegistrationFailed {
                plugin,
                component,
                ..
            } if *plugin == PluginId::new("nara.transform")
                && component == "nara.transform.Transform2d"
            )
        ));
        assert_eq!(
            app.plugin_lifecycle_state(),
            PluginLifecycleState::Configuring
        );
        assert!(app.plugin_failure_report().is_none());
        assert!(!app.has_plugin(TRANSFORM_PLUGIN_ID));
        app.seal()
            .expect("the corrected App should remain sealable");
    }

    #[test]
    fn transform_schema_exposes_authoring_fields() {
        let mut registry = ComponentRegistry::new();
        register_transform_components(&mut registry)
            .expect("component registration should succeed");
        registry.freeze().expect("component registry should freeze");

        let schema = registry
            .schema(&ComponentTypeId::new("nara.transform.Transform2d"))
            .unwrap();
        let mut fields = schema
            .fields
            .iter()
            .map(|field| (field.path.to_string(), field.value_kind(), field.required))
            .collect::<Vec<_>>();
        fields.sort_by(|left, right| left.0.cmp(&right.0));

        assert_eq!(
            fields,
            vec![
                ("rotation".to_string(), ComponentValueKind::F64, true),
                ("scale.x".to_string(), ComponentValueKind::F64, true),
                ("scale.y".to_string(), ComponentValueKind::F64, true),
                ("translation.x".to_string(), ComponentValueKind::F64, true),
                ("translation.y".to_string(), ComponentValueKind::F64, true),
            ]
        );
    }

    #[test]
    fn global_transform_is_an_ordinary_copyable_affine_value() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<GlobalTransform2d>();
    }

    #[test]
    fn startup_propagates_root_child_and_grandchild_globals() {
        let mut app = spatial_app();

        let (root, child, grandchild) = {
            let world = app
                .world_mut()
                .expect("the configuring App should expose its World");
            let root = world
                .spawn(Transform2d::from_translation(Vec2::new(10.0, 20.0)))
                .id();
            let child = world
                .spawn(Transform2d::from_translation(Vec2::new(3.0, 4.0)))
                .id();
            let grandchild = world
                .spawn(Transform2d::from_translation(Vec2::new(-2.0, 5.0)))
                .id();
            place_ordered_hierarchy(world, &[root], &[(child, root), (grandchild, child)]);
            (root, child, grandchild)
        };

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime
            .drive(Duration::ZERO)
            .expect("startup completion should succeed");

        let world = runtime.world();
        assert_eq!(
            world
                .snapshot_global(root)
                .expect("the root should receive derived global state")
                .translation(),
            Vec2::new(10.0, 20.0)
        );
        assert_eq!(
            world
                .snapshot_global(child)
                .expect("the child should receive derived global state")
                .translation(),
            Vec2::new(13.0, 24.0)
        );
        assert_eq!(
            world
                .snapshot_global(grandchild)
                .expect("the grandchild should receive derived global state")
                .translation(),
            Vec2::new(11.0, 29.0)
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn failed_completion_withholds_the_snapshot_and_preserves_previous_payload() {
        let mut app = spatial_app();
        let (root, child) = {
            let world = app.world_mut().unwrap();
            let root = world
                .spawn(Transform2d::from_translation(Vec2::new(5.0, 7.0)))
                .id();
            let child = world
                .spawn(Transform2d::from_translation(Vec2::new(2.0, 3.0)))
                .id();
            place_ordered_hierarchy(world, &[root], &[(child, root)]);
            (root, child)
        };
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        let previous = runtime.world().snapshot_global(child).unwrap().matrix();
        assert!(runtime.world().snapshot_available());

        let error = runtime
            .with_driver_scope(|scope| {
                scope
                    .__apply_port::<ProjectionEditPort>(ProjectionEdit::Scale {
                        entity: root,
                        scale: Vec2::new(f32::NAN, 1.0),
                    })
                    .unwrap()
            })
            .map(|outcome| {
                assert!(outcome.post_operation_fault.is_none());
                outcome.output
            })
            .unwrap()
            .expect_err("non-finite local state must reject the complete candidate");

        assert_eq!(
            error,
            TransformCompletionError::NonFiniteLocal { entity: root }
        );
        assert!(!runtime.world().snapshot_available());
        assert_eq!(
            historical_spatial_snapshot(runtime.world()).err(),
            Some(SpatialSnapshotError::Unavailable),
        );
        assert_eq!(
            runtime
                .world()
                .resource::<snapshot::SimulationSnapshot>()
                .payload
                .globals[&child]
                .matrix(),
            previous
        );

        runtime
            .with_driver_scope(|scope| {
                scope
                    .__apply_port::<ProjectionEditPort>(ProjectionEdit::Scale {
                        entity: root,
                        scale: Vec2::ONE,
                    })
                    .unwrap()
            })
            .map(|outcome| {
                assert!(outcome.post_operation_fault.is_none());
                outcome.output
            })
            .unwrap()
            .expect("a corrected generation should republish the projection");

        assert!(runtime.world().snapshot_available());
        runtime.finish().unwrap();
    }

    #[test]
    fn failed_hierarchy_completion_invalidates_the_spatial_publication() {
        let mut app = spatial_app();
        let (root, child) = spawn_parented_pair(&mut app);
        let corrupt_child = app.world_mut().unwrap().spawn(Transform2d::default()).id();
        app.insert_resource(CorruptHierarchyBeforeCompletion {
            root,
            child: corrupt_child,
        })
        .unwrap();
        app.add_systems(
            CoreStage::PostUpdate,
            corrupt_hierarchy_before_completion
                .before(nara_hierarchy::__private::HierarchySet::ValidateAndComplete),
        )
        .unwrap();

        let candidate = RuntimeAdmissionReservation::try_acquire()
            .unwrap()
            .admit(
                app.seal().unwrap(),
                RuntimeObligationLedger::new(),
                RuntimeClosePolicy::default(),
            )
            .unwrap();
        let mut runtime = candidate.complete_startup().unwrap().promote();
        let previous = runtime.world().snapshot_global(child).unwrap().matrix();
        assert!(runtime.world().snapshot_available());

        runtime
            .drive(Duration::ZERO)
            .expect_err("an inconsistent dirty hierarchy must fault the frame");

        assert!(
            !runtime
                .world()
                .contains_resource::<nara_hierarchy::__private::CompletedHierarchyProjection>(),
            "a dirty hierarchy must revoke its completion fact before validation"
        );
        assert!(historical_spatial_snapshot(runtime.world()).is_err());
        assert_eq!(
            runtime
                .world()
                .resource::<snapshot::SimulationSnapshot>()
                .payload
                .globals[&child]
                .matrix(),
            previous
        );
        assert!(
            !runtime.world().snapshot_available(),
            "the transform completion point must withhold a failed publication"
        );

        let mut retirement = runtime.begin_retirement();
        while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
            retirement.drive_retirement();
        }
    }

    #[derive(Resource)]
    struct CorruptHierarchyBeforeCompletion {
        root: nara_ecs::Entity,
        child: nara_ecs::Entity,
    }

    fn corrupt_hierarchy_before_completion(world: &mut nara_ecs::World) {
        let corruption = world.resource::<CorruptHierarchyBeforeCompletion>();
        let root = corruption.root;
        let child = corruption.child;
        HierarchyWriter::new(world)
            .place(
                child,
                HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent: root }),
            )
            .unwrap();
        world.flush();
        world.entity_mut(root).remove::<Children>();
    }

    #[test]
    fn removing_transform_removes_stale_global_projection() {
        let mut app = spatial_app();
        let entity = app
            .world_mut()
            .unwrap()
            .spawn(Transform2d::from_translation(Vec2::new(4.0, 8.0)))
            .id();
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        assert!(runtime.world().snapshot_global(entity).is_some());

        runtime
            .with_driver_scope(|scope| {
                scope
                    .__apply_port::<ProjectionEditPort>(ProjectionEdit::RemoveTransform(entity))
                    .unwrap()
            })
            .map(|outcome| {
                assert!(outcome.post_operation_fault.is_none());
                outcome.output
            })
            .unwrap()
            .unwrap();

        assert!(runtime.world().snapshot_global(entity).is_none());
        assert!(runtime.world().snapshot_available());
        runtime.finish().unwrap();
    }

    #[test]
    fn parent_transform_removal_starts_a_new_spatial_domain() {
        let mut app = spatial_app();
        let (root, child) = spawn_parented_pair(&mut app);
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        runtime
            .with_driver_scope(|scope| {
                scope
                    .__apply_port::<ProjectionEditPort>(ProjectionEdit::RemoveTransform(root))
                    .unwrap()
            })
            .map(|outcome| {
                assert!(outcome.post_operation_fault.is_none());
                outcome.output
            })
            .unwrap()
            .unwrap();
        let snapshot = historical_spatial_snapshot(runtime.world()).unwrap();
        assert_eq!(
            snapshot.global(child).unwrap().translation(),
            Vec2::new(3.0, 4.0)
        );
        assert_eq!(snapshot.effective_parent(child).unwrap(), None);
        assert_eq!(
            snapshot.global(root),
            Err(SpatialSnapshotError::MissingEntity { entity: root })
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn unchanged_completion_points_do_not_revisit_the_transform_forest() {
        let mut app = spatial_app();
        let world = app.world_mut().unwrap();
        let root = world.spawn(Transform2d::default()).id();
        let child = world.spawn(Transform2d::default()).id();
        place_ordered_hierarchy(world, &[root], &[(child, root)]);

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        let first = propagation::propagation_stats(runtime.world());
        assert_eq!((first.2, first.3, first.4), (1, 2, 1));

        runtime.drive(Duration::ZERO).unwrap();
        let second = propagation::propagation_stats(runtime.world());
        assert_eq!(
            (second.2, second.3, second.4),
            (1, 2, 1),
            "PostUpdate and Extract freshness fences must use the shared completion generation"
        );
        assert_eq!(
            (second.0 - first.0, second.1 - first.1),
            (2, 4),
            "each unchanged PostUpdate/Extract publication should perform one participant value scan only"
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn deep_and_wide_forests_complete_iteratively_in_one_linear_pass() {
        const DEPTH: usize = 2_048;
        const WIDTH: usize = 2_048;

        let mut app = spatial_app();
        let (deep_leaf, participant_count) = {
            let world = app.world_mut().unwrap();
            let deep_root = world.spawn(Transform2d::from_translation(Vec2::X)).id();
            let mut previous = deep_root;
            let mut edges = Vec::with_capacity(DEPTH.saturating_sub(1) + WIDTH);
            for _ in 1..DEPTH {
                let entity = world.spawn(Transform2d::from_translation(Vec2::X)).id();
                edges.push((entity, previous));
                previous = entity;
            }

            let wide_root = world.spawn(Transform2d::default()).id();
            for _ in 0..WIDTH {
                let child = world.spawn(Transform2d::default()).id();
                edges.push((child, wide_root));
            }
            place_ordered_hierarchy(world, &[deep_root, wide_root], &edges);
            (previous, DEPTH + WIDTH + 1)
        };

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        assert_eq!(
            runtime
                .world()
                .snapshot_global(deep_leaf)
                .unwrap()
                .translation(),
            Vec2::new(DEPTH as f32, 0.0)
        );
        assert_eq!(
            {
                let stats = propagation::propagation_stats(runtime.world());
                (stats.2, stats.3, stats.4)
            },
            (
                1,
                participant_count as u64,
                (DEPTH.saturating_sub(1) + WIDTH) as u64
            )
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn nonspatial_order_entries_do_not_become_transform_participants() {
        const NON_TRANSFORM_CHILDREN: usize = 4_096;

        let mut app = spatial_app();
        {
            let world = app.world_mut().unwrap();
            let root = world.spawn(Transform2d::default()).id();
            let mut edges = Vec::with_capacity(NON_TRANSFORM_CHILDREN);
            for _ in 0..NON_TRANSFORM_CHILDREN {
                let child = world.spawn_empty().id();
                edges.push((child, root));
            }
            place_ordered_hierarchy(world, &[root], &edges);
        }

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        let stats = propagation::propagation_stats(runtime.world());
        assert_eq!(
            (stats.2, stats.3, stats.4),
            (1, 1, 0),
            "nonspatial structural entries must not add affine evaluations or spatial edges"
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn fixed_direct_transform_write_is_visible_to_same_tick_consumer() {
        let app = fixed_spatial_app(false);

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();

        assert_eq!(
            runtime.world().resource::<FixedSpatialProbe>().observed,
            Some(Vec2::new(13.0, 24.0))
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn fixed_deferred_transform_write_is_visible_to_same_tick_consumer() {
        let app = fixed_spatial_app(true);

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();

        assert_eq!(
            runtime.world().resource::<FixedSpatialProbe>().observed,
            Some(Vec2::new(13.0, 24.0))
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn explicit_parent_retirement_recomputes_the_surviving_child_as_a_root() {
        let mut app = spatial_app();
        let (parent, child) = {
            let world = app.world_mut().unwrap();
            let parent = world
                .spawn(Transform2d::from_translation(Vec2::new(10.0, 20.0)))
                .id();
            let child = world
                .spawn(Transform2d::from_translation(Vec2::new(3.0, 4.0)))
                .id();
            place_ordered_hierarchy(world, &[parent], &[(child, parent)]);
            (parent, child)
        };
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(
            runtime
                .world()
                .snapshot_global(child)
                .unwrap()
                .translation(),
            Vec2::new(13.0, 24.0)
        );

        runtime
            .with_driver_scope(|scope| {
                scope
                    .__apply_port::<ProjectionEditPort>(ProjectionEdit::RetireParent {
                        parent,
                        child,
                    })
                    .unwrap()
            })
            .map(|outcome| {
                assert!(outcome.post_operation_fault.is_none());
                outcome.output
            })
            .unwrap()
            .unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        assert!(
            runtime
                .world()
                .get::<nara_hierarchy::Parent>(child)
                .is_none()
        );
        assert_eq!(
            runtime
                .world()
                .snapshot_global(child)
                .unwrap()
                .translation(),
            Vec2::new(3.0, 4.0)
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn post_update_transform_write_is_visible_before_extract() {
        let mut app = spatial_app();
        let (root, child) = spawn_parented_pair(&mut app);
        app.insert_resource(PostUpdateSpatialProbe {
            root,
            child,
            observed: None,
        })
        .unwrap();
        app.add_systems(
            CoreStage::PostUpdate,
            move_post_update_root.before(__private::TransformSet::Propagate),
        )
        .unwrap();
        app.add_systems(
            CoreStage::PostUpdate,
            observe_post_update_child.after(__private::TransformSet::Propagate),
        )
        .unwrap();

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        assert_eq!(
            runtime
                .world()
                .resource::<PostUpdateSpatialProbe>()
                .observed,
            Some(Vec2::new(13.0, 24.0))
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn paused_authoring_write_refreshes_extract_and_resume_has_no_stale_frame() {
        let mut app = spatial_app();
        let (root, child) = spawn_parented_pair(&mut app);
        app.insert_resource(PausedTransformEditPort {
            root,
            pending_translation: None,
        })
        .unwrap();
        app.insert_resource(ExtractSpatialProbe {
            child,
            observed: None,
            samples: 0,
        })
        .unwrap();
        app.add_systems(CoreStage::First, apply_paused_transform_edit)
            .unwrap();
        app.add_systems(
            CoreStage::Extract,
            observe_extract_child.after(__private::TransformSet::Present),
        )
        .unwrap();

        let candidate = RuntimeAdmissionReservation::try_acquire()
            .unwrap()
            .admit(
                app.seal().unwrap(),
                RuntimeObligationLedger::new(),
                RuntimeClosePolicy::default(),
            )
            .unwrap();
        let mut runtime = candidate.complete_startup().unwrap().promote();
        assert_eq!(
            runtime
                .world()
                .snapshot_global(child)
                .unwrap()
                .translation(),
            Vec2::new(3.0, 4.0),
            "startup must publish the first projection before any frame consumer"
        );

        assert!(matches!(
            runtime.request_control(RuntimeControl::Pause),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(runtime.state(), RuntimeState::Paused);

        runtime
            .with_driver_scope(|scope| {
                scope.__apply_port::<PausedTransformEditPort>(Vec2::new(10.0, 20.0))
            })
            .map(|outcome| {
                assert!(outcome.post_operation_fault.is_none());
                outcome.output
            })
            .unwrap()
            .unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(runtime.state(), RuntimeState::Paused);
        assert_eq!(
            runtime.world().resource::<ExtractSpatialProbe>().observed,
            Some(Vec2::new(13.0, 24.0)),
            "the next paused Extract must see the authoring write"
        );

        let paused_samples = runtime.world().resource::<ExtractSpatialProbe>().samples;
        assert!(matches!(
            runtime.request_control(RuntimeControl::Resume),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(runtime.state(), RuntimeState::Running);
        let probe = runtime.world().resource::<ExtractSpatialProbe>();
        assert_eq!(probe.observed, Some(Vec2::new(13.0, 24.0)));
        assert_eq!(probe.samples, paused_samples + 1);

        let mut retirement = runtime.begin_retirement();
        while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
            retirement.drive_retirement();
        }
    }

    fn spawn_parented_pair(app: &mut App) -> (nara_ecs::Entity, nara_ecs::Entity) {
        let world = app.world_mut().unwrap();
        let root = world.spawn(Transform2d::default()).id();
        let child = world
            .spawn(Transform2d::from_translation(Vec2::new(3.0, 4.0)))
            .id();
        place_ordered_hierarchy(world, &[root], &[(child, root)]);
        (root, child)
    }

    #[derive(Resource)]
    struct PostUpdateSpatialProbe {
        root: nara_ecs::Entity,
        child: nara_ecs::Entity,
        observed: Option<Vec2>,
    }

    fn move_post_update_root(
        probe: Res<PostUpdateSpatialProbe>,
        mut transforms: Query<&mut Transform2d>,
    ) {
        transforms.get_mut(probe.root).unwrap().translation = Vec2::new(10.0, 20.0);
    }

    fn observe_post_update_child(
        mut probe: ResMut<PostUpdateSpatialProbe>,
        globals: HistoricalSpatialSnapshot2d,
    ) {
        probe.observed = Some(globals.global(probe.child).unwrap().translation());
    }

    #[derive(Resource)]
    struct PausedTransformEditPort {
        root: nara_ecs::Entity,
        pending_translation: Option<Vec2>,
    }

    impl __RuntimeDriverPort for PausedTransformEditPort {
        type Input = Vec2;
        type Output = ();

        fn apply_driver_input(&mut self, input: Self::Input) -> Self::Output {
            self.pending_translation = Some(input);
        }
    }

    fn apply_paused_transform_edit(
        mut port: ResMut<PausedTransformEditPort>,
        mut transforms: Query<&mut Transform2d>,
    ) {
        let Some(translation) = port.pending_translation.take() else {
            return;
        };
        transforms.get_mut(port.root).unwrap().translation = translation;
    }

    #[derive(Resource)]
    struct ExtractSpatialProbe {
        child: nara_ecs::Entity,
        observed: Option<Vec2>,
        samples: u32,
    }

    fn observe_extract_child(
        mut probe: ResMut<ExtractSpatialProbe>,
        globals: PresentedSpatialSnapshot2d,
    ) {
        probe.observed = Some(globals.global(probe.child).unwrap().translation());
        probe.samples = probe.samples.saturating_add(1);
    }

    #[derive(Resource)]
    struct FixedSpatialProbe {
        root: nara_ecs::Entity,
        child: nara_ecs::Entity,
        observed: Option<Vec2>,
    }

    fn fixed_spatial_app(deferred_write: bool) -> App {
        let mut app = spatial_app();
        let (root, child) = {
            let world = app.world_mut().unwrap();
            let root = world.spawn(Transform2d::default()).id();
            let child = world
                .spawn(Transform2d::from_translation(Vec2::new(3.0, 4.0)))
                .id();
            place_ordered_hierarchy(world, &[root], &[(child, root)]);
            (root, child)
        };
        app.insert_resource(FixedSpatialProbe {
            root,
            child,
            observed: None,
        })
        .unwrap();
        if deferred_write {
            app.add_systems(
                CoreStage::FixedUpdate,
                replace_root_transform_deferred.in_set(FixedUpdateSet::Simulate),
            )
            .unwrap();
        } else {
            app.add_systems(
                CoreStage::FixedUpdate,
                move_root_transform_direct.in_set(FixedUpdateSet::Simulate),
            )
            .unwrap();
        }
        add_fixed_transform_projection_systems(&mut app, observe_child_global).unwrap();
        app
    }

    fn move_root_transform_direct(
        probe: Res<FixedSpatialProbe>,
        mut transforms: Query<&mut Transform2d>,
    ) {
        transforms.get_mut(probe.root).unwrap().translation = Vec2::new(10.0, 20.0);
    }

    fn replace_root_transform_deferred(probe: Res<FixedSpatialProbe>, mut commands: Commands) {
        commands
            .entity(probe.root)
            .insert(Transform2d::from_translation(Vec2::new(10.0, 20.0)));
    }

    fn observe_child_global(mut probe: ResMut<FixedSpatialProbe>, globals: FixedSpatialSnapshot2d) {
        probe.observed = Some(globals.global(probe.child).unwrap().translation());
    }
}
