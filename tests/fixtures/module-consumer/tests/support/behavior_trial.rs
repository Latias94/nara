//! Private representation comparison. This is not a supported Runtime or language adapter.

use std::{collections::BTreeMap, convert::Infallible, sync::Arc};

use nara_asset::AssetRef;
use nara_ecs::{Entity, World, bevy_ecs::world::WorldId};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentRegistry, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueShape, EntityReference, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::{AuthoringAdapterReceipt, try_expand_authoring_prefabs_with_loader},
    AuthoringDocumentSource, AuthoringSchemaBasis, Name, PrefabDocument, PrefabInstance,
    PreparedScene, ResolvedPrefabSource, SceneAuthoringSession, SceneComponentRecord,
    SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, SceneEntitySource,
    SceneOccurrenceKey, SourceDocumentKey, SourceDocumentKind, SpawnedSceneInstance, export_scene,
    prepare_scene, register_scene_components, retire_and_despawn_scene_instance,
};

#[path = "behavior_trial/storage.rs"]
mod storage;
pub use storage::StorageKind;

pub fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

pub fn mover() -> ComponentTypeId {
    ComponentTypeId::new("trial.behavior.Mover")
}
pub fn sensor() -> ComponentTypeId {
    ComponentTypeId::new("trial.behavior.Sensor")
}
fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("trial.behaviors").unwrap()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Source,
    BindingUnavailable,
    InvalidValue,
    StalePublication,
    StaleEntity,
    Absent,
    Export,
    EncodeFailure,
    Retirement,
}

pub struct Definition {
    pub basis: AuthoringSchemaBasis,
    pub schemas: BTreeMap<ComponentTypeId, ComponentSchema>,
    available: bool,
}

impl Definition {
    pub fn new(mover_version: u32, available: bool) -> Arc<Self> {
        let field = |name, kind: ComponentValueShape| {
            ComponentFieldSchema::required(
                ComponentFieldId::new(name),
                name,
                ComponentFieldPath::from_fields([name]),
                kind,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        };
        let amount = if mover_version == 1 {
            "amount"
        } else {
            "distance"
        };
        let schemas = [
            ComponentSchema::new(
                mover(),
                "Mover",
                ComponentSchemaVersion::new(mover_version).unwrap(),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([field(amount, ComponentValueKind::F64.into())]),
            ComponentSchema::new(sensor(), "Sensor", ComponentSchemaVersion::ONE)
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([
                    field(
                        "targets",
                        ComponentValueShape::list(ComponentValueShape::optional(
                            ComponentValueShape::EntityRef,
                        )),
                    )
                    .with_capabilities([
                        ComponentCapability::Scene,
                        ComponentCapability::Inspect,
                        ComponentCapability::Edit,
                        ComponentCapability::EntityRef,
                    ]),
                    field(
                        "images",
                        ComponentValueShape::list(ComponentValueShape::AssetRef),
                    )
                    .with_capabilities([
                        ComponentCapability::Scene,
                        ComponentCapability::Inspect,
                        ComponentCapability::Edit,
                        ComponentCapability::AssetRef,
                    ]),
                ]),
        ];
        let native = AuthoringSchemaBundle::from_trusted_provider_definitions([
            nara_scene::SCENE_COMPONENTS_SCHEMA_PROVIDER,
        ])
        .unwrap();
        let bundle = AuthoringSchemaBundle::new(native.owners().iter().cloned().chain([
            AuthoringSchemaBundleOwner::new(
                owner(),
                ComponentSchemaCatalog {
                    components: schemas.to_vec(),
                    ..Default::default()
                },
                None,
            ),
        ]))
        .unwrap();
        let native_provider = nara_reflect::__private::resolve_schema_provider(
            nara_scene::SCENE_COMPONENTS_SCHEMA_PROVIDER,
        )
        .unwrap();
        let native_coverage = native
            .owners()
            .iter()
            .flat_map(|owner| {
                owner
                    .current()
                    .components()
                    .iter()
                    .map(|schema| schema.id().clone())
            })
            .collect();
        let mut adapters = bundle
            .owners()
            .iter()
            .filter_map(|owner| AuthoringAdapterReceipt::from_schema_owner(owner, &native_coverage))
            .collect::<Vec<_>>();
        adapters.push(
            AuthoringAdapterReceipt::from_trusted_provider(
                native_provider.contribution_receipt(),
                "trial.native",
                1,
                1,
                native_coverage,
            )
            .unwrap(),
        );
        let basis = AuthoringSchemaBasis::admit(&bundle, &adapters).unwrap();
        Arc::new(Self {
            basis,
            schemas: schemas
                .into_iter()
                .map(|schema| (schema.id().clone(), schema))
                .collect(),
            available,
        })
    }

    fn validate(
        &self,
        component: &ComponentTypeId,
        record: &SceneComponentRecord,
    ) -> Result<(), Error> {
        if !self.available {
            return Err(Error::BindingUnavailable);
        }
        let schema = self
            .schemas
            .get(component)
            .ok_or(Error::BindingUnavailable)?;
        if record.version != schema.version() || record.owner() != Some(&owner()) {
            return Err(Error::InvalidValue);
        }
        nara_reflect::validate_component_value(schema, &record.value)
            .map_err(|_| Error::InvalidValue)
    }
}

pub fn record(value: ComponentValue, version: u32) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::new(version).unwrap(), value)
        .with_owner(owner())
}

pub fn source() -> SceneDocument {
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("actor"))
            .with_component(
                ComponentTypeId::new("nara.scene.Name"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("Actor".into()),
                ),
            )
            .with_component(
                mover(),
                record(
                    ComponentValue::map([("amount", ComponentValue::f64(2.0).unwrap())]),
                    1,
                ),
            )
            .with_component(
                sensor(),
                record(
                    ComponentValue::map([
                        (
                            "targets",
                            ComponentValue::List(vec![
                                ComponentValue::EntityReference(EntityReference::SceneLocal {
                                    entity: id("friend"),
                                }),
                                ComponentValue::Null,
                            ]),
                        ),
                        (
                            "images",
                            ComponentValue::List(vec![ComponentValue::map([
                                ("kind", ComponentValue::String("path".into())),
                                (
                                    "value",
                                    ComponentValue::String("textures/sensor.png".into()),
                                ),
                            ])]),
                        ),
                    ]),
                    1,
                ),
            ),
        SceneEntityRecord::new(id("friend")).with_component(
            ComponentTypeId::new("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Friend".into()),
            ),
        ),
    ])
}

pub fn session(document: &SceneDocument, definition: &Definition) -> SceneAuthoringSession {
    let published = SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("scenes/trial.scene.json").unwrap()),
            &definition.basis,
        )
        .unwrap()
        .publish();
    SceneAuthoringSession::from_published_authoring_document(published)
}

fn source_key(name: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "f1390a84-11df-49b0-a901-d7a045141039".parse().unwrap(),
        name,
        kind,
    )
    .unwrap()
}

pub fn compile(
    document: &SceneDocument,
    definition: &Definition,
    prefab: Option<&PrefabDocument>,
) -> Result<nara_scene::__private::AuthoringPrefabExpansionReport, Error> {
    let report = try_expand_authoring_prefabs_with_loader(
        document,
        &definition.basis,
        Default::default(),
        None,
        |_, _| {
            Ok::<_, Infallible>(prefab.map(|document| {
                ResolvedPrefabSource::bound_authoring_source(
                    Arc::new(document.clone()),
                    definition.basis.clone(),
                    source_key("base", SourceDocumentKind::Prefab),
                )
            }))
        },
    )
    .unwrap();
    if report.diagnostics.has_errors()
        || report.candidate.is_none()
        || report.omissions.iter().any(|omission| {
            matches!(
                omission.reason,
                nara_scene::__private::AuthoringPrefabOmissionReason::RecordUnavailable(_)
            )
        })
    {
        Err(Error::Source)
    } else {
        Ok(report)
    }
}

pub fn prefab_source() -> (SceneDocument, PrefabDocument) {
    let prefab = PrefabDocument::new(source().roots, source().entities);
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("base.prefab.json").unwrap(),
        overrides: None,
    });
    (SceneDocument::from_ordered_roots([anchor]), prefab)
}

pub fn prefab_key() -> SourceDocumentKey {
    source_key("base", SourceDocumentKind::Prefab)
}

/// Both alternatives stage a complete unpublished fresh World. No in-place rollback is claimed.
pub struct PreparedRuntime {
    world: World,
    native: PreparedScene,
    registry: ComponentRegistry,
    records: Vec<(
        SceneEntityId,
        BTreeMap<ComponentTypeId, SceneComponentRecord>,
    )>,
    definition: Arc<Definition>,
    kind: StorageKind,
}

impl PreparedRuntime {
    pub fn prepare(
        kind: StorageKind,
        definition: Arc<Definition>,
        document: &SceneDocument,
        prefab: Option<&PrefabDocument>,
    ) -> Result<Self, Error> {
        let report = compile(document, &definition, prefab)?;
        let candidate = report.candidate.as_ref().unwrap();
        let mut records = Vec::new();
        for entity in &candidate.document().entities {
            let mut members = BTreeMap::new();
            for (component, record) in &entity.components {
                if definition.schemas.contains_key(component) {
                    definition.validate(component, record)?;
                    members.insert(component.clone(), record.clone());
                }
            }
            if !members.is_empty() {
                records.push((entity.id.clone(), members));
            }
        }
        let (native, omissions) = report.into_native_preview().map_err(|_| Error::Source)?;
        if omissions.iter().any(|omission| {
            let component = match omission.location() {
                nara_scene::__private::AuthoringPreviewOmissionLocation::DirectEntityComponent { component, .. }
                | nara_scene::__private::AuthoringPreviewOmissionLocation::ProjectedEntityComponent { component, .. } => component,
                _ => return true,
            };
            !definition.schemas.contains_key(component)
        }) { return Err(Error::BindingUnavailable); }
        let candidate = native.bind_source(source_key("scene", SourceDocumentKind::Scene));
        let mut registry = ComponentRegistry::new();
        register_scene_components(&mut registry).unwrap();
        registry.freeze().unwrap();
        let mut world = World::new();
        let native = prepare_scene(
            &mut world,
            &registry,
            &candidate,
            &SceneOccurrenceKey::new("trial", "main").unwrap(),
            None,
        )
        .map_err(|_| Error::Source)?;
        Ok(Self {
            world,
            native,
            registry,
            records,
            definition,
            kind,
        })
    }

    pub fn activate(mut self, current: &Arc<Definition>) -> Result<Runtime, Error> {
        if !Arc::ptr_eq(&self.definition, current) {
            return Err(Error::StalePublication);
        }
        let report = self.native.commit(&mut self.world);
        let instance = report.instance.ok_or(Error::Source)?;
        let entities = self
            .world
            .iter_entities()
            .filter_map(|entity| {
                entity
                    .get::<SceneEntitySource>()
                    .map(|source| (source.entity_id.clone(), entity.id()))
            })
            .collect::<BTreeMap<_, _>>();
        for (source, members) in self.records {
            let target = *entities.get(&source).ok_or(Error::Source)?;
            self.kind.install(&mut self.world, target, members);
        }
        Ok(Runtime {
            world: self.world,
            registry: self.registry,
            instance,
            entities,
            definition: self.definition,
            kind: self.kind,
        })
    }
}

pub struct Runtime {
    pub world: World,
    registry: ComponentRegistry,
    instance: SpawnedSceneInstance,
    entities: BTreeMap<SceneEntityId, Entity>,
    definition: Arc<Definition>,
    pub kind: StorageKind,
}

#[derive(Clone)]
pub struct Target {
    world: WorldId,
    entity: Entity,
    definition: Arc<Definition>,
}

impl Runtime {
    pub fn entity(&self, target: &Target) -> Result<Entity, Error> {
        self.validate_target(target)
    }

    pub fn components(&self, target: &Target) -> Result<Vec<ComponentTypeId>, Error> {
        Ok(self
            .kind
            .components(&self.world, self.validate_target(target)?))
    }

    pub fn retained_rows(&self) -> usize {
        self.kind.retained_rows(&self.world)
    }

    pub fn query(&self, component: &ComponentTypeId) -> (Vec<Entity>, usize) {
        let result = self.kind.query(&self.world, component);
        (result.entities, result.inspected)
    }

    pub fn target(&self, source: &SceneEntityId) -> Target {
        Target {
            world: self.world.id(),
            entity: self.entities[source],
            definition: Arc::clone(&self.definition),
        }
    }

    fn validate_target(&self, target: &Target) -> Result<Entity, Error> {
        if target.world != self.world.id() || !Arc::ptr_eq(&target.definition, &self.definition) {
            return Err(Error::StalePublication);
        }
        if self.world.get_entity(target.entity).is_err() {
            return Err(Error::StaleEntity);
        }
        Ok(target.entity)
    }

    pub fn read(
        &self,
        target: &Target,
        component: &ComponentTypeId,
    ) -> Result<Option<&SceneComponentRecord>, Error> {
        let entity = self.validate_target(target)?;
        if !self.definition.available {
            return Err(Error::BindingUnavailable);
        }
        if !self.definition.schemas.contains_key(component) {
            return Err(Error::BindingUnavailable);
        }
        Ok(self.kind.read(&self.world, entity, component))
    }

    pub fn set(
        &mut self,
        target: &Target,
        component: &ComponentTypeId,
        value: SceneComponentRecord,
    ) -> Result<(), Error> {
        let entity = self.validate_target(target)?;
        self.definition.validate(component, &value)?;
        let references = nara_reflect::__private::plan_declared_entity_references::<Error>(
            &self.definition.schemas[component],
            &value.value,
            nara_reflect::EntityReferenceTraversalLimits::default(),
        )
        .map_err(|_| Error::InvalidValue)?;
        references
            .visit(&value.value, |_, reference| match reference {
                EntityReference::SceneLocal { entity } => self
                    .entities
                    .get(entity)
                    .filter(|entity| self.world.get_entity(**entity).is_ok())
                    .map(|_| ())
                    .ok_or(Error::InvalidValue),
                _ => Err(Error::InvalidValue),
            })
            .map_err(|_| Error::InvalidValue)?;
        if self.kind.replace(&mut self.world, entity, component, value) {
            Ok(())
        } else {
            Err(Error::Absent)
        }
    }

    pub fn remove(&mut self, target: &Target, component: &ComponentTypeId) -> Result<(), Error> {
        let entity = self.validate_target(target)?;
        self.kind
            .remove(&mut self.world, entity, component)
            .map(|_| ())
            .ok_or(Error::Absent)
    }

    pub fn tick(&mut self, target: &Target) -> Result<(), Error> {
        let record = self.read(target, &mover())?.ok_or(Error::Absent)?;
        let field = if record.version == ComponentSchemaVersion::ONE {
            "amount"
        } else {
            "distance"
        };
        let value = record
            .value
            .get(field)
            .and_then(ComponentValue::as_f64)
            .ok_or(Error::InvalidValue)?;
        let entity = self.validate_target(target)?;
        self.world.get_mut::<Name>(entity).ok_or(Error::Absent)?.0 = format!("Moved {value}");
        Ok(())
    }

    pub fn signal(&mut self, target: &Target) -> Result<usize, Error> {
        let record = self.read(target, &sensor())?.ok_or(Error::Absent)?;
        let plan = nara_reflect::__private::plan_declared_entity_references::<Error>(
            &self.definition.schemas[&sensor()],
            &record.value,
            nara_reflect::EntityReferenceTraversalLimits::default(),
        )
        .map_err(|_| Error::InvalidValue)?;
        let mut targets = Vec::new();
        plan.visit(&record.value, |_, reference| match reference {
            EntityReference::SceneLocal { entity } => {
                targets.push(*self.entities.get(entity).ok_or(Error::StaleEntity)?);
                Ok(())
            }
            _ => Err(Error::InvalidValue),
        })
        .map_err(|_| Error::InvalidValue)?;
        if targets
            .iter()
            .any(|entity| self.world.get::<Name>(*entity).is_none())
        {
            return Err(Error::InvalidValue);
        }
        let count = targets.len();
        for entity in targets {
            self.world
                .get_mut::<Name>(entity)
                .unwrap()
                .0
                .push_str(" signalled");
        }
        Ok(count)
    }

    pub fn native_name(&self, target: &Target) -> &str {
        self.world.get::<Name>(target.entity).unwrap().as_str()
    }

    pub fn export(&self) -> Result<SceneDocument, Error> {
        let mut output = export_scene(&self.world, &self.registry)
            .into_output()
            .ok_or(Error::Export)?;
        let remap = self
            .entities
            .iter()
            .map(|(source, entity)| {
                let locator = self.instance.locator(source).ok_or(Error::Export)?;
                let exported = output.remap.get(&locator).ok_or(Error::Export)?;
                Ok((exported.clone(), *entity))
            })
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        let ids = self
            .entities
            .keys()
            .map(|source| {
                let locator = self.instance.locator(source).ok_or(Error::Export)?;
                Ok((
                    source.clone(),
                    output.remap.get(&locator).ok_or(Error::Export)?.clone(),
                ))
            })
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        for record in &mut output.document.entities {
            let entity = *remap.get(&record.id).ok_or(Error::Export)?;
            for component in self.kind.components(&self.world, entity) {
                let value = self
                    .kind
                    .read(&self.world, entity, &component)
                    .ok_or(Error::Export)?;
                self.definition
                    .validate(&component, value)
                    .map_err(|_| Error::EncodeFailure)?;
                let mut value = value.clone();
                value.value = nara_reflect::rewrite_declared_entity_references(
                    &self.definition.schemas[&component],
                    &value.value,
                    nara_reflect::EntityReferenceTraversalLimits::default(),
                    |_, reference| match reference {
                        EntityReference::SceneLocal { entity } => ids
                            .get(entity)
                            .map(|entity| EntityReference::SceneLocal {
                                entity: entity.clone(),
                            })
                            .ok_or(Error::Export),
                        reference => Ok(reference.clone()),
                    },
                )
                .map_err(|_| Error::Export)?;
                record.components.insert(component, value);
            }
        }
        Ok(output.document)
    }

    pub fn retire(&mut self) -> Result<(), Error> {
        let entities = self.entities.values().copied().collect::<Vec<_>>();
        retire_and_despawn_scene_instance(&mut self.world, &self.instance)
            .map_err(|_| Error::Retirement)?;
        // Strict Scene admission may reject hooks without changing the active instance.
        // Resource-owned rows are released only after that fallible step succeeds.
        self.kind.retire(&mut self.world, &entities);
        Ok(())
    }

    pub fn ordinary_retire(&mut self) -> Result<(), Error> {
        retire_and_despawn_scene_instance(&mut self.world, &self.instance)
            .map(|_| ())
            .map_err(|_| Error::Retirement)
    }

    pub fn enable_rows_cleanup_hook(&mut self, target: &Target) {
        let entity = self.validate_target(target).unwrap();
        storage::install_membership_hook(&mut self.world, entity);
    }

    pub fn remove_rows_cleanup_hook(&mut self, target: &Target) {
        let entity = self.validate_target(target).unwrap();
        storage::remove_membership_hook(&mut self.world, entity);
    }

    pub fn corrupt_for_export_probe(&mut self, target: &Target) {
        let entity = self.validate_target(target).unwrap();
        assert!(self.kind.replace(
            &mut self.world,
            entity,
            &sensor(),
            record(ComponentValue::Null, 1)
        ));
    }
}

/// Direct World destruction intentionally bypasses the trial owner to expose storage obligations.
pub fn direct_despawn_probe(kind: StorageKind) -> (usize, usize, usize, usize) {
    let mut world = World::new();
    let first = world.spawn_empty().id();
    let second = world.spawn_empty().id();
    let members = || {
        BTreeMap::from([
            (
                mover(),
                record(
                    ComponentValue::map([("amount", ComponentValue::f64(1.0).unwrap())]),
                    1,
                ),
            ),
            (sensor(), record(ComponentValue::Null, 1)),
        ])
    };
    kind.install(&mut world, first, members());
    kind.install(&mut world, second, members());
    world.despawn(first);
    let queried = kind.query(&world, &mover());
    assert_eq!(queried.entities, vec![second]);
    let retained = kind.retained_rows(&world);
    kind.retire(&mut world, &[first]);
    (
        queried.entities.len(),
        queried.inspected,
        retained,
        kind.retained_rows(&world),
    )
}
