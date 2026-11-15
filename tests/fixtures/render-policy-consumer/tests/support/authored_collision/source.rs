use std::sync::{Arc, Mutex};

use motion_physics::authored::CollisionDefinition;
use nara::{
    core::{Color, Vec2},
    identity::{EntityLookup, SpawnedSceneInstance},
    material::AlphaMode2d,
    reflect::{
        ComponentFieldId, ComponentRegistrySnapshot, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue, component_registry,
    },
    scene::{
        PrefabEntityTarget, SceneAuthoringRevision, SceneAuthoringSession, SceneComponentRecord,
        SceneDocument, SceneEntityId, SceneEntityPlacement, SceneEntityRecord, SceneOccurrenceKey,
        ScenePatchDocument, ScenePatchOperation, ScenePatchReport, ScenePlacement,
        ScenePlacementParent, SourceDocumentKey, SourceDocumentKind,
    },
    sprite::Sprite,
    transform::Transform2d,
};
use nara_ecs::{Entity, Resource, World, bevy_ecs::error::BevyError};
use nara_render::{Camera2d, RenderTarget};

use super::model::{
    Authored, BODY_COLOR, CAMERA_CENTER, CAMERA_HEIGHT, CLEAR_COLOR, PLATFORM_COLOR, Variant,
    authored,
};

pub(super) const LOCAL_BOX: &str = "fixture.motion.LocalBox";
const TRANSFORM: &str = "nara.transform.Transform2d";

#[derive(Clone, Resource)]
pub(super) struct SourceHandle {
    pub state: Arc<Mutex<Source>>,
    pub target: RenderTarget,
}

pub(super) struct Source {
    pub session: SceneAuthoringSession,
    pub source: SourceDocumentKey,
    pub occurrence: SceneOccurrenceKey,
    pub facts: Authored,
    pub installed: Option<(Authored, SceneAuthoringRevision)>,
}

#[derive(Clone, Copy)]
pub(super) struct Entities {
    pub ancestor: Entity,
    pub platform: Entity,
    pub body: Entity,
    pub camera: Entity,
}

impl Source {
    pub fn new(document: SceneDocument, variant: Variant) -> Self {
        Self {
            session: SceneAuthoringSession::new(document),
            source: SourceDocumentKey::named(
                "24838b71-89e8-4826-8516-53f807639d89".parse().unwrap(),
                "visible-authored-collision",
                SourceDocumentKind::Scene,
            )
            .unwrap(),
            occurrence: SceneOccurrenceKey::new("fixture.visible-collision", "scene").unwrap(),
            facts: authored(variant),
            installed: None,
        }
    }

    pub fn entities(&self, world: &World) -> Result<Entities, BevyError> {
        let receipt = self
            .session
            .live_instance()
            .ok_or_else(|| refusal("the original authored Scene has no live activation receipt"))?;
        Ok(Entities {
            ancestor: resolve(world, receipt, "ancestor")?,
            platform: resolve(world, receipt, "platform")?,
            body: resolve(world, receipt, "body")?,
            camera: resolve(world, receipt, "camera")?,
        })
    }

    pub fn edit(
        &mut self,
        variant: Variant,
        registry: &ComponentRegistrySnapshot,
    ) -> ScenePatchReport {
        assert_ne!(self.facts.variant, variant);
        let expected = document(variant);
        let patch = ScenePatchDocument::new(expected.entities.iter().flat_map(|entity| {
            entity.components.iter().map(move |(component, value)| {
                ScenePatchOperation::ReplaceComponent {
                    entity: entity.id.clone(),
                    component: component.clone(),
                    value: value.clone(),
                }
            })
        }));
        let report = self.session.apply_patch(&patch, registry);
        assert!(report.applied, "{:?}", report.diagnostics);
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        assert_eq!(self.session.document(), &expected);
        self.facts = authored(variant);
        report
    }

    pub fn undo(&mut self, registry: &ComponentRegistrySnapshot) -> ScenePatchReport {
        let report = self.session.undo(registry);
        self.accept_history(&report);
        report
    }

    pub fn redo(&mut self, registry: &ComponentRegistrySnapshot) -> ScenePatchReport {
        let report = self.session.redo(registry);
        self.accept_history(&report);
        report
    }

    pub fn reject_invalid_edit(
        &mut self,
        registry: &ComponentRegistrySnapshot,
    ) -> ScenePatchReport {
        let previous = self.session.document().clone();
        let revision = self.session.revision();
        let history = self.session.history_status();
        let live = self.session.live_instance().cloned();
        let dirty = self.session.is_live_dirty();
        let report = self.session.apply_patch(
            &ScenePatchDocument::new([ScenePatchOperation::SetField {
                entity: id("platform"),
                component: ComponentTypeId::new(LOCAL_BOX),
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("half_height"),
                value: ComponentValue::f64(-1.0).unwrap(),
            }]),
            registry,
        );
        assert!(!report.applied);
        assert!(report.diagnostics.has_errors());
        assert!(report.inverse.is_none());
        assert_eq!(self.session.document(), &previous);
        assert_eq!(self.session.revision(), revision);
        assert_eq!(self.session.history_status(), history);
        assert_eq!(self.session.live_instance(), live.as_ref());
        assert_eq!(self.session.is_live_dirty(), dirty);
        report
    }

    pub fn assert_installed(&self, world: &World, entities: Entities) {
        assert_eq!(self.session.document(), &document(self.facts.variant));
        let (facts, _) = self
            .installed
            .expect("the original Scene source is installed");
        assert_eq!(
            world.get::<Transform2d>(entities.ancestor),
            Some(&facts.ancestor)
        );
        assert_eq!(
            world.get::<Transform2d>(entities.platform),
            Some(&facts.platform)
        );
        assert_eq!(
            world.get::<CollisionDefinition>(entities.platform),
            Some(&facts.platform_box)
        );
        assert_eq!(
            world.get::<CollisionDefinition>(entities.body),
            Some(&facts.body_box)
        );
        assert_eq!(
            world.get::<Transform2d>(entities.camera),
            Some(&camera_transform())
        );
        for (entity, definition, color) in [
            (entities.platform, facts.platform_box, PLATFORM_COLOR),
            (entities.body, facts.body_box, BODY_COLOR),
        ] {
            assert_eq!(
                world.get::<Sprite>(entity),
                Some(&sprite(definition, color))
            );
        }
    }

    fn accept_history(&mut self, report: &ScenePatchReport) {
        assert!(report.applied, "{:?}", report.diagnostics);
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        let variant = [Variant::Initial, Variant::Edited]
            .into_iter()
            .find(|variant| self.session.document() == &document(*variant))
            .expect("history restores one exact authored Scene document");
        self.facts = authored(variant);
    }
}

pub(super) fn synchronize(world: &mut World) -> Result<(), BevyError> {
    let registry = component_registry(world)
        .ok_or_else(|| refusal("Scene startup requires the finished Component Registry"))?
        .clone();
    let handle = world.resource::<SourceHandle>().clone();
    let mut source = handle.state.lock().unwrap();
    if source.session.live_instance().is_some() && !source.session.is_live_dirty() {
        return Ok(());
    }
    let Source {
        session,
        source: key,
        occurrence,
        ..
    } = &mut *source;
    let report = session
        .sync_world(world, &registry, key, occurrence)
        .map_err(BevyError::error)?;
    if !report.synced || report.diagnostics.has_errors() {
        return Err(refusal(format!(
            "authored Scene synchronization refused: {:?}",
            report.diagnostics
        )));
    }
    let entities = source.entities(world)?;
    if source.session.document() != &document(source.facts.variant) {
        return Err(refusal(
            "installed source must match the complete agreed authored Scene",
        ));
    }
    for (entity, transform) in [
        (entities.ancestor, source.facts.ancestor),
        (entities.platform, source.facts.platform),
        (entities.body, source.facts.body),
        (entities.camera, camera_transform()),
    ] {
        if world.get::<Transform2d>(entity) != Some(&transform) {
            return Err(refusal(
                "the persistent Scene Transform was not materialized exactly",
            ));
        }
    }
    for (entity, definition, color) in [
        (entities.platform, source.facts.platform_box, PLATFORM_COLOR),
        (entities.body, source.facts.body_box, BODY_COLOR),
    ] {
        if world.get::<CollisionDefinition>(entity) != Some(&definition) {
            return Err(refusal(
                "the persistent local box was not materialized exactly",
            ));
        }
        world.entity_mut(entity).insert(sprite(definition, color));
    }
    world.entity_mut(entities.camera).insert(Camera2d {
        target: handle.target,
        viewport: None,
        clear_color: Some(color(CLEAR_COLOR)),
        viewport_height: CAMERA_HEIGHT as f32,
        order: 0,
    });
    source.installed = Some((source.facts, source.session.revision()));
    source.assert_installed(world, entities);
    Ok(())
}

pub(super) fn document(variant: Variant) -> SceneDocument {
    let facts = authored(variant);
    SceneDocument::new(
        [
            SceneEntityPlacement::root(id("ancestor")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("platform")),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    id("ancestor"),
                ))),
            ),
            SceneEntityPlacement::root(id("body")),
            SceneEntityPlacement::root(id("camera")),
        ],
        [
            record("ancestor", facts.ancestor),
            record("platform", facts.platform).with_component(
                ComponentTypeId::new(LOCAL_BOX),
                collision_record(facts.platform_box),
            ),
            record("body", facts.body).with_component(
                ComponentTypeId::new(LOCAL_BOX),
                collision_record(facts.body_box),
            ),
            record("camera", camera_transform()),
        ],
    )
}

fn resolve(world: &World, receipt: &SpawnedSceneInstance, name: &str) -> Result<Entity, BevyError> {
    match receipt.resolve(world, &id(name)) {
        EntityLookup::Resolved(entity) => Ok(entity),
        other => Err(refusal(format!(
            "authored {name} has no original live entity: {other:?}"
        ))),
    }
}

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn camera_transform() -> Transform2d {
    Transform2d::from_translation(Vec2::new(CAMERA_CENTER[0] as f32, CAMERA_CENTER[1] as f32))
}

fn record(name: &str, transform: Transform2d) -> SceneEntityRecord {
    SceneEntityRecord::new(id(name)).with_component(
        ComponentTypeId::new(TRANSFORM),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([
                ("translation", vector(transform.translation)),
                ("rotation", scalar(transform.rotation)),
                ("scale", vector(transform.scale)),
            ]),
        ),
    )
}

fn collision_record(definition: CollisionDefinition) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([
            ("center", vector(definition.center)),
            ("half_width", scalar(definition.half_width)),
            ("half_height", scalar(definition.half_height)),
            ("dynamic", ComponentValue::Bool(definition.dynamic)),
        ]),
    )
}

fn vector(value: Vec2) -> ComponentValue {
    ComponentValue::map([("x", scalar(value.x)), ("y", scalar(value.y))])
}

fn scalar(value: f32) -> ComponentValue {
    ComponentValue::f64(f64::from(value)).unwrap()
}

fn sprite(definition: CollisionDefinition, rgba: [u8; 4]) -> Sprite {
    let size = Vec2::new(2.0 * definition.half_width, 2.0 * definition.half_height);
    let mut sprite = Sprite::from_color(size, color(rgba));
    sprite.anchor.normalized = -definition.center / size;
    sprite.material.alpha_mode = AlphaMode2d::Opaque;
    sprite
}

fn color(rgba: [u8; 4]) -> Color {
    Color::rgba(
        f32::from(rgba[0]) / 255.0,
        f32::from(rgba[1]) / 255.0,
        f32::from(rgba[2]) / 255.0,
        f32::from(rgba[3]) / 255.0,
    )
}

fn refusal(message: impl Into<String>) -> BevyError {
    BevyError::error(std::io::Error::other(message.into()))
}
