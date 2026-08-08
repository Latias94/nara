use std::error::Error;

use nara_app::{App, CoreStage};
use nara_ecs::{Entity, Res, ResMut, Resource, World};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};
use nara_input::{
    ActionId as InputActionId, ActionMap, ActionOutcomes, ActionPhase, InputSeatId, InputWindowId,
    KeyCode,
};
use nara_reflect::{ComponentRegistry, ComponentRegistryPlugin};
use nara_scene::{SceneDocument, SceneEntityId, SceneEntityRecord, SceneEntitySource, spawn_scene};
use nara_ui::{
    advanced::{UiSurfaceDescriptor, UiSurfaceId, install_ui_surface},
    prelude::{
        UiActionId, UiButton, UiColor, UiLayer, UiNode, UiPlugin, UiRoot, UiRuntimeKey,
        UiSemanticRole, UiSemantics, UiStyle, UiText, UiVisual,
    },
};

use super::{DOMAIN, GAMEPLAY_ACTION, PRIMARY_ACTION};

pub(super) struct LabEntities {
    pub(super) primary_button: Entity,
    pub(super) scene_overlay: Entity,
}

#[derive(Debug, Resource)]
pub(super) struct GameplayProbe {
    action: InputActionId,
    phases: Vec<ActionPhase>,
}

impl GameplayProbe {
    fn new(action: InputActionId) -> Self {
        Self {
            action,
            phases: Vec::new(),
        }
    }

    pub(super) fn phases(&self) -> &[ActionPhase] {
        &self.phases
    }
}

pub(super) fn build_lab() -> Result<(App, LabEntities), Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins((
        ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_input::InputPlugin,
        UiPlugin,
    ))?;
    let gameplay_action = InputActionId::new(GAMEPLAY_ACTION)?;
    app.world_mut()?
        .resource_mut::<ActionMap>()
        .bind_key(gameplay_action.clone(), KeyCode::Escape);
    app.insert_resource(GameplayProbe::new(gameplay_action))?;
    app.add_systems(CoreStage::Update, capture_gameplay_outcome)?;

    install_ui_surface(
        app.world_mut()?,
        UiSurfaceDescriptor::new(
            surface_id(1),
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            320.0,
            180.0,
            1.0,
        )?,
    )?;
    install_ui_surface(
        app.world_mut()?,
        UiSurfaceDescriptor::new(
            surface_id(2),
            InputWindowId::new(12),
            InputSeatId::PRIMARY,
            320.0,
            180.0,
            1.25,
        )?,
    )?;

    let scene_overlay = spawn_scene_overlay(app.world_mut()?)?;
    let world = app.world_mut()?;
    let root = world
        .spawn((
            UiRoot::new(UiLayer::Base),
            UiNode::new(UiStyle::fill()),
            UiRuntimeKey::new(DOMAIN, "root")?,
            UiSemantics::new(UiSemanticRole::Group, "Interaction controls")?,
        ))
        .id();

    let secondary_button = world
        .spawn(button_bundle(
            "secondary",
            "Secondary action",
            "gallery.secondary",
            UiStyle::absolute(140.0, 20.0, 100.0, 44.0),
            UiColor::rgba(0.18, 0.52, 0.38, 1.0),
        )?)
        .id();
    let primary_button = world
        .spawn(button_bundle(
            "primary",
            "Primary action",
            PRIMARY_ACTION,
            UiStyle::absolute(20.0, 20.0, 100.0, 44.0),
            UiColor::rgba(0.18, 0.36, 0.78, 1.0),
        )?)
        .id();

    HierarchyWriter::new(world).apply_batch(&[
        HierarchyOperation::Place {
            entity: root,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        },
        HierarchyOperation::Place {
            entity: primary_button,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent: root }),
        },
        HierarchyOperation::Place {
            entity: secondary_button,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent: root }),
        },
    ])?;

    Ok((
        app,
        LabEntities {
            primary_button,
            scene_overlay,
        },
    ))
}

fn capture_gameplay_outcome(outcomes: Res<ActionOutcomes>, mut probe: ResMut<GameplayProbe>) {
    let action = probe.action.clone();
    probe.phases = outcomes
        .as_slice()
        .iter()
        .filter(|outcome| outcome.action == action)
        .map(|outcome| outcome.phase)
        .collect();
}

fn spawn_scene_overlay(world: &mut World) -> Result<Entity, Box<dyn Error>> {
    let mut registry = ComponentRegistry::new();
    registry.freeze()?;
    let source_id = SceneEntityId::new("scene-overlay")?;
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(source_id.clone())]);
    let report = spawn_scene(world, &registry, &document);
    assert!(
        !report.diagnostics.has_errors(),
        "neutral Scene authoring should materialize: {:?}",
        report.diagnostics
    );
    let entity = world
        .iter_entities()
        .find(|entity| {
            entity
                .get::<SceneEntitySource>()
                .is_some_and(|source| source.entity_id == source_id)
        })
        .expect("materialized Scene root should retain provenance")
        .id();
    world.entity_mut(entity).insert((
        UiRoot::new(UiLayer::Overlay),
        UiNode::new(UiStyle::absolute(8.0, 126.0, 180.0, 34.0)),
        UiVisual::solid(UiColor::rgba(0.11, 0.13, 0.17, 0.96)),
        UiText::new("Scene-authored status", 14.0)?,
        UiSemantics::new(UiSemanticRole::Status, "Scene-authored status")?,
    ));
    Ok(entity)
}

fn button_bundle(
    local: &str,
    label: &str,
    action: &str,
    style: UiStyle,
    color: UiColor,
) -> Result<(UiButton, UiNode, UiVisual, UiRuntimeKey), Box<dyn Error>> {
    Ok((
        UiButton::new(label, UiActionId::new(action)?, 16.0)?,
        UiNode::new(style),
        UiVisual::solid(color),
        UiRuntimeKey::new(DOMAIN, local)?,
    ))
}

pub(super) fn surface_id(value: u64) -> UiSurfaceId {
    UiSurfaceId::new(value).expect("lab surface IDs are non-zero")
}
