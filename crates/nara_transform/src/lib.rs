//! Authored 2D transforms and completed runtime-global projection.

mod propagation;

use nara_app::{App, Plugin, PluginError, PluginPreflightContext};
use nara_core::{Mat3, Vec2};
use nara_ecs::Component;
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

#[derive(Debug, PartialEq, Component)]
#[component(immutable)]
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

#[doc(hidden)]
pub mod __private {
    use nara_ecs::{Resource, SystemSet};

    /// Capability token proving that the current runtime 2D projection completed successfully.
    ///
    /// First-party consumers require this token before publishing world-space output. The
    /// transform owner removes it before a dirty completion attempt and restores it only after the
    /// complete candidate projection has been committed.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
    pub struct CompletedTransformProjection {
        generation: u64,
        hierarchy_generation: u64,
    }

    impl CompletedTransformProjection {
        pub(super) const fn new(generation: u64, hierarchy_generation: u64) -> Self {
            Self {
                generation,
                hierarchy_generation,
            }
        }

        /// Returns the private transform completion generation.
        #[must_use]
        pub const fn generation(self) -> u64 {
            self.generation
        }

        /// Returns the hierarchy generation consumed by this projection.
        #[must_use]
        pub const fn hierarchy_generation(self) -> u64 {
            self.hierarchy_generation
        }
    }

    /// Provisional first-party boundary after the runtime 2D projection is complete.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
    pub enum TransformSet {
        Propagate,
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
    );
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
        propagation::install(app)
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
    Ok(nara_reflect::ComponentSchemaCatalog {
        components: vec![transform_schema()],
        ..nara_reflect::ComponentSchemaCatalog::default()
    })
}

fn transform_schema() -> ComponentSchema {
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
    pub use crate::{GlobalTransform2d, Transform2d, TransformPlugin};
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
    use nara_ecs::{
        Commands, Query, Res, ResMut, Resource, relationship::RelationshipTarget,
        schedule::IntoScheduleConfigs,
    };
    use nara_hierarchy::{Children, HierarchyConstructionEdge, HierarchyConstructionWriter};
    use nara_reflect::ComponentRegistryPlugin;

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
        );
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
        app
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
            nara_app::AddPluginsError::Plugin(PluginError::ComponentRegistrationFailed {
                plugin,
                component,
                ..
            }) if plugin == PluginId::new("nara.transform")
                && component == "nara.transform.Transform2d"
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
            .map(|field| (field.path.to_string(), field.value_kind, field.required))
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
    fn global_transform_is_an_immutable_derived_component() {
        fn assert_immutable_component<
            T: nara_ecs::Component<Mutability = nara_ecs::component::Immutable>,
        >() {
        }

        assert_immutable_component::<GlobalTransform2d>();
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
            HierarchyConstructionWriter::new(world)
                .attach_batch(&[
                    HierarchyConstructionEdge::new(child, root),
                    HierarchyConstructionEdge::new(grandchild, child),
                ])
                .expect("the hierarchy should be valid");
            (root, child, grandchild)
        };

        app.run_once(Duration::ZERO)
            .expect("startup completion should succeed");

        let world = app.world();
        assert_eq!(
            world
                .get::<GlobalTransform2d>(root)
                .expect("the root should receive derived global state")
                .translation(),
            Vec2::new(10.0, 20.0)
        );
        assert_eq!(
            world
                .get::<GlobalTransform2d>(child)
                .expect("the child should receive derived global state")
                .translation(),
            Vec2::new(13.0, 24.0)
        );
        assert_eq!(
            world
                .get::<GlobalTransform2d>(grandchild)
                .expect("the grandchild should receive derived global state")
                .translation(),
            Vec2::new(11.0, 29.0)
        );
    }

    #[test]
    fn failed_completion_withholds_token_and_preserves_previous_projection() {
        let mut app = spatial_app();
        let (root, child) = {
            let world = app.world_mut().unwrap();
            let root = world
                .spawn(Transform2d::from_translation(Vec2::new(5.0, 7.0)))
                .id();
            let child = world
                .spawn(Transform2d::from_translation(Vec2::new(2.0, 3.0)))
                .id();
            HierarchyConstructionWriter::new(world)
                .attach(child, root)
                .unwrap();
            (root, child)
        };
        app.run_once(Duration::ZERO).unwrap();

        let previous = app
            .world()
            .get::<GlobalTransform2d>(child)
            .unwrap()
            .matrix();
        assert!(
            app.world()
                .contains_resource::<__private::CompletedTransformProjection>()
        );

        app.world_mut()
            .unwrap()
            .get_mut::<Transform2d>(root)
            .unwrap()
            .scale
            .x = f32::NAN;
        let error = propagation::complete_transform_projection(app.world_mut().unwrap())
            .expect_err("non-finite local state must reject the complete candidate");

        assert_eq!(
            error,
            TransformCompletionError::NonFiniteLocal { entity: root }
        );
        assert!(
            !app.world()
                .contains_resource::<__private::CompletedTransformProjection>()
        );
        assert_eq!(
            app.world()
                .get::<GlobalTransform2d>(child)
                .unwrap()
                .matrix(),
            previous,
            "a failed candidate must not partially replace the prior projection"
        );

        app.world_mut()
            .unwrap()
            .get_mut::<Transform2d>(root)
            .unwrap()
            .scale = Vec2::ONE;
        propagation::complete_transform_projection(app.world_mut().unwrap())
            .expect("a corrected generation should republish the projection");
        assert!(
            app.world()
                .contains_resource::<__private::CompletedTransformProjection>()
        );
    }

    #[test]
    fn failed_hierarchy_completion_invalidates_the_prior_transform_token() {
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
        let previous = runtime
            .world()
            .get::<GlobalTransform2d>(child)
            .unwrap()
            .matrix();
        assert!(
            runtime
                .world()
                .contains_resource::<__private::CompletedTransformProjection>()
        );

        runtime
            .drive(Duration::ZERO)
            .expect_err("an inconsistent dirty hierarchy must fault the frame");

        assert!(
            !runtime
                .world()
                .contains_resource::<nara_hierarchy::__private::CompletedHierarchyProjection>(),
            "a dirty hierarchy must revoke its completion fact before validation"
        );
        assert_eq!(
            runtime
                .world()
                .get::<GlobalTransform2d>(child)
                .unwrap()
                .matrix(),
            previous,
            "failure may retain the old opaque projection, but it must be unusable without a token"
        );
        assert!(
            !runtime
                .world()
                .contains_resource::<__private::CompletedTransformProjection>(),
            "the transform completion point must revoke the stale transform token"
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
        HierarchyConstructionWriter::new(world)
            .attach(child, root)
            .unwrap();
        world.flush();
        world
            .get_mut::<Children>(root)
            .unwrap()
            .collection_mut_risky()
            .clear();
    }

    #[test]
    fn removing_transform_removes_stale_global_projection() {
        let mut app = spatial_app();
        let entity = app
            .world_mut()
            .unwrap()
            .spawn(Transform2d::from_translation(Vec2::new(4.0, 8.0)))
            .id();
        app.run_once(Duration::ZERO).unwrap();
        assert!(app.world().get::<GlobalTransform2d>(entity).is_some());

        app.world_mut()
            .unwrap()
            .entity_mut(entity)
            .remove::<Transform2d>();
        propagation::complete_transform_projection(app.world_mut().unwrap()).unwrap();

        assert!(app.world().get::<GlobalTransform2d>(entity).is_none());
        assert!(
            app.world()
                .contains_resource::<__private::CompletedTransformProjection>()
        );
    }

    #[test]
    fn parent_transform_removal_rejects_continuity_without_partial_publish() {
        let mut app = spatial_app();
        let (root, child) = {
            let world = app.world_mut().unwrap();
            let root = world
                .spawn(Transform2d::from_translation(Vec2::new(5.0, 7.0)))
                .id();
            let child = world
                .spawn(Transform2d::from_translation(Vec2::new(2.0, 3.0)))
                .id();
            HierarchyConstructionWriter::new(world)
                .attach(child, root)
                .unwrap();
            (root, child)
        };
        app.run_once(Duration::ZERO).unwrap();
        let previous = app
            .world()
            .get::<GlobalTransform2d>(child)
            .unwrap()
            .matrix();

        app.world_mut()
            .unwrap()
            .entity_mut(root)
            .remove::<Transform2d>();
        let error = propagation::complete_transform_projection(app.world_mut().unwrap())
            .expect_err("a transform chain may not skip its structural parent");

        assert_eq!(
            error,
            TransformCompletionError::MissingParentTransform {
                child,
                parent: root,
            }
        );
        assert!(
            !app.world()
                .contains_resource::<__private::CompletedTransformProjection>()
        );
        assert_eq!(
            app.world()
                .get::<GlobalTransform2d>(child)
                .unwrap()
                .matrix(),
            previous
        );
    }

    #[test]
    fn unchanged_completion_points_do_not_revisit_the_transform_forest() {
        let mut app = spatial_app();
        let world = app.world_mut().unwrap();
        let root = world.spawn(Transform2d::default()).id();
        let child = world.spawn(Transform2d::default()).id();
        HierarchyConstructionWriter::new(world)
            .attach(child, root)
            .unwrap();

        app.run_once(Duration::ZERO).unwrap();
        let first = propagation::propagation_stats(app.world());
        assert_eq!((first.2, first.3, first.4), (1, 2, 1));

        app.run_once(Duration::ZERO).unwrap();
        let second = propagation::propagation_stats(app.world());
        assert_eq!(
            (second.2, second.3, second.4),
            (1, 2, 1),
            "PostUpdate and Extract freshness fences must use the shared completion generation"
        );
        assert_eq!(
            (second.0 - first.0, second.1 - first.1),
            (2, 4),
            "each unchanged PostUpdate/Extract completion point should perform one participant change-tick scan only"
        );
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
                edges.push(HierarchyConstructionEdge::new(entity, previous));
                previous = entity;
            }

            let wide_root = world.spawn(Transform2d::default()).id();
            for _ in 0..WIDTH {
                let child = world.spawn(Transform2d::default()).id();
                edges.push(HierarchyConstructionEdge::new(child, wide_root));
            }
            HierarchyConstructionWriter::new(world)
                .attach_batch(&edges)
                .unwrap();
            (previous, DEPTH + WIDTH + 1)
        };

        app.run_once(Duration::ZERO).unwrap();

        assert_eq!(
            app.world()
                .get::<GlobalTransform2d>(deep_leaf)
                .unwrap()
                .translation(),
            Vec2::new(DEPTH as f32, 0.0)
        );
        assert_eq!(
            {
                let stats = propagation::propagation_stats(app.world());
                (stats.2, stats.3, stats.4)
            },
            (
                1,
                participant_count as u64,
                (DEPTH.saturating_sub(1) + WIDTH) as u64
            )
        );
    }

    #[test]
    fn transform_propagation_does_not_scan_non_transform_children() {
        const NON_TRANSFORM_CHILDREN: usize = 4_096;

        let mut app = spatial_app();
        {
            let world = app.world_mut().unwrap();
            let root = world.spawn(Transform2d::default()).id();
            let mut edges = Vec::with_capacity(NON_TRANSFORM_CHILDREN);
            for _ in 0..NON_TRANSFORM_CHILDREN {
                let child = world.spawn_empty().id();
                edges.push(HierarchyConstructionEdge::new(child, root));
            }
            HierarchyConstructionWriter::new(world)
                .attach_batch(&edges)
                .unwrap();
        }

        app.run_once(Duration::ZERO).unwrap();

        let stats = propagation::propagation_stats(app.world());
        assert_eq!(
            (stats.2, stats.3, stats.4),
            (1, 1, 0),
            "transform completion must scale with transform participants and transform edges"
        );
    }

    #[test]
    fn fixed_direct_transform_write_is_visible_to_same_tick_finalize() {
        let mut app = fixed_spatial_app(false);

        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        assert_eq!(
            app.world().resource::<FixedSpatialProbe>().observed,
            Some(Vec2::new(13.0, 24.0))
        );
    }

    #[test]
    fn fixed_deferred_transform_write_is_visible_to_same_tick_finalize() {
        let mut app = fixed_spatial_app(true);

        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        assert_eq!(
            app.world().resource::<FixedSpatialProbe>().observed,
            Some(Vec2::new(13.0, 24.0))
        );
    }

    #[test]
    fn parent_despawn_recomputes_the_surviving_child_as_a_root() {
        let mut app = spatial_app();
        let (parent, child) = {
            let world = app.world_mut().unwrap();
            let parent = world
                .spawn(Transform2d::from_translation(Vec2::new(10.0, 20.0)))
                .id();
            let child = world
                .spawn(Transform2d::from_translation(Vec2::new(3.0, 4.0)))
                .id();
            HierarchyConstructionWriter::new(world)
                .attach(child, parent)
                .unwrap();
            (parent, child)
        };
        app.run_once(Duration::ZERO).unwrap();
        assert_eq!(
            app.world()
                .get::<GlobalTransform2d>(child)
                .unwrap()
                .translation(),
            Vec2::new(13.0, 24.0)
        );

        assert!(app.world_mut().unwrap().despawn(parent));
        app.run_once(Duration::ZERO).unwrap();

        assert!(app.world().get::<nara_hierarchy::Parent>(child).is_none());
        assert_eq!(
            app.world()
                .get::<GlobalTransform2d>(child)
                .unwrap()
                .translation(),
            Vec2::new(3.0, 4.0)
        );
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

        app.run_once(Duration::ZERO).unwrap();

        assert_eq!(
            app.world().resource::<PostUpdateSpatialProbe>().observed,
            Some(Vec2::new(13.0, 24.0))
        );
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
            observe_extract_child.after(__private::TransformSet::Propagate),
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
                .get::<GlobalTransform2d>(child)
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
        HierarchyConstructionWriter::new(world)
            .attach(child, root)
            .unwrap();
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
        globals: Query<&GlobalTransform2d>,
    ) {
        probe.observed = Some(globals.get(probe.child).unwrap().translation());
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
        globals: Query<&GlobalTransform2d>,
    ) {
        probe.observed = Some(globals.get(probe.child).unwrap().translation());
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
            HierarchyConstructionWriter::new(world)
                .attach(child, root)
                .unwrap();
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
        app.add_systems(
            CoreStage::FixedUpdate,
            observe_child_global.in_set(FixedUpdateSet::Finalize),
        )
        .unwrap();
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

    fn observe_child_global(
        mut probe: ResMut<FixedSpatialProbe>,
        globals: Query<&GlobalTransform2d>,
    ) {
        probe.observed = Some(globals.get(probe.child).unwrap().translation());
    }
}
