//! Executable private alternatives, not production Behavior support or a language SDK.

#[path = "support/behavior_trial.rs"]
mod trial;

use std::sync::Arc;

use nara_reflect::{ComponentFieldId, ComponentPropertyPath, ComponentValue};
use nara_scene::{
    AuthoringDocumentSource, AuthoringRecordReadiness, SceneDocumentCandidate, ScenePatchDocument,
    ScenePatchOperation,
};
use trial::{Definition, Error, PreparedRuntime, StorageKind, id, mover, record, sensor};

const KINDS: [StorageKind; 2] = [StorageKind::Envelope, StorageKind::Rows];

#[test]
fn native_registry_rejects_two_behavior_schemas_without_native_bindings() {
    let definition = Definition::new(1, true);
    let mut registry = nara_reflect::ComponentRegistry::new();
    for schema in definition.schemas.values() {
        registry.register_component_schema(schema.clone()).unwrap();
    }
    assert!(matches!(
        registry.freeze(),
        Err(nara_reflect::ComponentRegistryError::MissingNativeBinding { .. })
    ));
}

#[test]
fn both_storage_alternatives_run_source_history_disk_and_mixed_native_execution() {
    for kind in KINDS {
        let definition = Definition::new(1, true);
        let original = trial::source();
        let sensor_before = original
            .entities
            .iter()
            .find(|entity| entity.id == id("actor"))
            .unwrap()
            .components[&sensor()]
            .clone();
        let mut session = trial::session(&original, &definition);
        let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
            entity: id("actor"),
            component: mover(),
            component_version: nara_reflect::ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("amount"),
            value: ComponentValue::f64(7.0).unwrap(),
        }]);
        let prepared = session
            .prepare_authoring_edit(session.revision(), &patch)
            .unwrap();
        assert_eq!(session.document(), &original);
        assert!(session.commit_authoring_edit(prepared).applied);
        assert_eq!(session.history_usage().records, 1);
        let edited = session.document().clone();
        assert!(session.undo_authoring().applied);
        assert_eq!(session.document(), &original);
        assert!(session.redo_authoring().applied);
        assert_eq!(session.document(), &edited);
        let directory = tempfile::tempdir().unwrap();
        for json in [true, false] {
            let path = directory.path().join(if json {
                "source.scene.json"
            } else {
                "source.scene.ron"
            });
            std::fs::write(
                &path,
                if json {
                    edited.to_json_string().unwrap()
                } else {
                    edited.to_ron_string().unwrap()
                },
            )
            .unwrap();
            let encoded = std::fs::read(&path).unwrap();
            let candidate = if json {
                SceneDocumentCandidate::decode_json_bytes(&encoded)
            } else {
                SceneDocumentCandidate::decode_ron_bytes(&encoded)
            }
            .unwrap();
            let reopened = session.try_reopen_authoring_candidate(candidate).unwrap();
            assert_eq!(reopened.document(), &edited);
            assert_eq!(reopened.history_usage().records, 0);
        }
        let mut runtime = PreparedRuntime::prepare(kind, Arc::clone(&definition), &edited, None)
            .unwrap()
            .activate(&definition)
            .unwrap();
        let target = runtime.target(&id("actor"));
        assert_eq!(
            runtime.read(&target, &sensor()).unwrap(),
            Some(&sensor_before)
        );
        assert_eq!(runtime.components(&target).unwrap().len(), 2);
        runtime.tick(&target).unwrap();
        assert_eq!(runtime.native_name(&target), "Moved 7");
        assert_eq!(runtime.signal(&target).unwrap(), 1);
        assert_eq!(
            runtime.native_name(&runtime.target(&id("friend"))),
            "Friend signalled"
        );
        let exported = runtime.export().unwrap();
        let entity = exported
            .entities
            .iter()
            .find(|entity| entity.components.contains_key(&mover()))
            .unwrap();
        assert_eq!(entity.components[&sensor()], sensor_before);
        assert!(
            entity
                .components
                .contains_key(&nara_reflect::ComponentTypeId::new("nara.scene.Name"))
        );
        runtime
            .set(
                &target,
                &mover(),
                record(
                    ComponentValue::map([("amount", ComponentValue::f64(9.0).unwrap())]),
                    1,
                ),
            )
            .unwrap();
        assert_eq!(
            runtime.read(&target, &sensor()).unwrap(),
            Some(&sensor_before)
        );
        runtime.tick(&target).unwrap();
        assert_eq!(runtime.native_name(&target), "Moved 9");
        assert_eq!(
            runtime.read(
                &target,
                &nara_reflect::ComponentTypeId::new("trial.Missing")
            ),
            Err(Error::BindingUnavailable)
        );
        runtime.remove(&target, &mover()).unwrap();
        assert!(runtime.read(&target, &mover()).unwrap().is_none());
        assert_eq!(
            runtime.read(&target, &sensor()).unwrap(),
            Some(&sensor_before)
        );
        let without = runtime.export().unwrap();
        assert_eq!(
            without
                .entities
                .iter()
                .filter(|entity| entity.components.contains_key(&mover()))
                .count(),
            0
        );
        assert_eq!(
            without
                .entities
                .iter()
                .filter(|entity| entity.components.contains_key(&sensor()))
                .count(),
            1
        );
        runtime.retire().unwrap();
        assert_eq!(runtime.retained_rows(), 0);
        assert_eq!(runtime.read(&target, &sensor()), Err(Error::StaleEntity));
    }
}

#[test]
fn prefab_leaf_intent_and_recursive_references_use_the_existing_compiler() {
    for kind in KINDS {
        let definition = Definition::new(1, true);
        let (mut document, prefab) = trial::prefab_source();
        let mut overlay = nara_scene::PrefabOverlay::new(trial::prefab_key());
        overlay
            .components
            .push(nara_scene::PrefabComponentIntent::Properties {
                target: nara_scene::PrefabEntityTarget::source(id("actor")),
                component: mover(),
                version: nara_reflect::ComponentSchemaVersion::ONE,
                claims: vec![nara_scene::PrefabPropertyClaim {
                    path: ComponentPropertyPath::new(ComponentFieldId::new("amount")),
                    intent: nara_scene::PrefabPropertyIntent::Set(
                        ComponentValue::f64(11.0).unwrap(),
                    ),
                    bindings: Vec::new(),
                }],
            });
        document.entities[0].prefab.as_mut().unwrap().overrides = Some(overlay);
        let report = trial::compile(&document, &definition, Some(&prefab)).unwrap();
        assert_eq!(report.effective_asset_references().len(), 1);
        let candidate = report.candidate.unwrap();
        let actor = candidate
            .document()
            .entities
            .iter()
            .find(|record| record.components.contains_key(&mover()))
            .unwrap();
        let friend = candidate
            .document()
            .entities
            .iter()
            .find(|record| candidate.provenance()[&record.id].local_entity_id() == &id("friend"))
            .unwrap();
        assert_eq!(
            actor.components[&mover()]
                .value
                .get("amount")
                .unwrap()
                .as_f64(),
            Some(11.0)
        );
        let target_ref = actor.components[&sensor()].value.get("targets").unwrap();
        assert_eq!(
            target_ref,
            &ComponentValue::List(vec![
                ComponentValue::EntityReference(nara_reflect::EntityReference::SceneLocal {
                    entity: friend.id.clone()
                }),
                ComponentValue::Null,
            ])
        );
        let actor_id = actor.id.clone();
        let mut runtime =
            PreparedRuntime::prepare(kind, Arc::clone(&definition), &document, Some(&prefab))
                .unwrap()
                .activate(&definition)
                .unwrap();
        let target = runtime.target(&actor_id);
        runtime.tick(&target).unwrap();
        assert_eq!(runtime.native_name(&target), "Moved 11");
        let exported = runtime.export().unwrap();
        assert!(trial::compile(&exported, &definition, None).is_ok());
        runtime.retire().unwrap();
    }
}

#[test]
fn missing_module_and_invalid_last_record_preserve_source_and_active_predecessor() {
    for kind in KINDS {
        let current = Definition::new(1, true);
        let original = trial::source();
        let mut active = PreparedRuntime::prepare(kind, Arc::clone(&current), &original, None)
            .unwrap()
            .activate(&current)
            .unwrap();
        let target = active.target(&id("actor"));
        let missing = Definition::new(1, false);
        assert!(matches!(
            PreparedRuntime::prepare(kind, missing, &original, None),
            Err(Error::BindingUnavailable)
        ));
        active.tick(&target).unwrap();
        assert_eq!(active.native_name(&target), "Moved 2");
        let mut invalid = original.clone();
        invalid
            .entities
            .iter_mut()
            .find(|record| record.id == id("actor"))
            .unwrap()
            .components
            .get_mut(&sensor())
            .unwrap()
            .value = ComponentValue::Null;
        assert!(PreparedRuntime::prepare(kind, Arc::clone(&current), &invalid, None).is_err());
        assert!(active.read(&target, &sensor()).unwrap().is_some());
        assert_eq!(trial::session(&original, &current).document(), &original);
        let absent = nara_reflect::AuthoringSchemaBundle::new([]).unwrap();
        let published =
            SceneDocumentCandidate::decode_json_str(&original.to_json_string().unwrap())
                .unwrap()
                .admit_authoring(
                    AuthoringDocumentSource::new(
                        nara_asset::AssetRef::path("scenes/missing.scene.json").unwrap(),
                    ),
                    &absent,
                    &[],
                )
                .unwrap()
                .publish();
        assert_eq!(published.document(), &original);
        assert!(
            published
                .record_facts()
                .iter()
                .any(|fact| fact.readiness() == AuthoringRecordReadiness::UnknownSchema)
        );
        let restored = trial::session(published.document(), &current);
        assert_eq!(restored.document(), &original);
        active.retire().unwrap();
    }
}

#[test]
fn one_record_migrates_without_mutating_the_other_and_old_publications_reject() {
    for kind in KINDS {
        let old = Definition::new(1, true);
        let new = Definition::new(2, true);
        let original = trial::source();
        let prepared = PreparedRuntime::prepare(kind, Arc::clone(&old), &original, None).unwrap();
        assert!(matches!(
            prepared.activate(&new),
            Err(Error::StalePublication)
        ));
        let mut previous = PreparedRuntime::prepare(kind, Arc::clone(&old), &original, None)
            .unwrap()
            .activate(&old)
            .unwrap();
        let stale = previous.target(&id("actor"));
        let mut migrated = original.clone();
        let actor = migrated
            .entities
            .iter_mut()
            .find(|record| record.id == id("actor"))
            .unwrap();
        let sensor_before = actor.components[&sensor()].clone();
        let amount = actor.components[&mover()]
            .value
            .get("amount")
            .unwrap()
            .clone();
        actor.components.insert(
            mover(),
            record(ComponentValue::map([("distance", amount)]), 2),
        );
        assert_eq!(actor.components[&sensor()], sensor_before);
        let mut next = PreparedRuntime::prepare(kind, Arc::clone(&new), &migrated, None)
            .unwrap()
            .activate(&new)
            .unwrap();
        assert_eq!(next.read(&stale, &sensor()), Err(Error::StalePublication));
        let target = next.target(&id("actor"));
        next.tick(&target).unwrap();
        assert_eq!(next.native_name(&target), "Moved 2");
        assert_eq!(next.read(&target, &sensor()).unwrap(), Some(&sensor_before));
        let same_bytes_other_project = Definition::new(2, true);
        let cross = PreparedRuntime::prepare(kind, Arc::clone(&new), &migrated, None).unwrap();
        assert!(matches!(
            cross.activate(&same_bytes_other_project),
            Err(Error::StalePublication)
        ));
        previous.retire().unwrap();
        next.retire().unwrap();
    }
}

#[test]
fn storage_queries_and_direct_despawn_expose_real_costs_and_retirement_obligations() {
    let definition = Definition::new(1, true);
    let mut document = trial::source();
    let sensor_record = document
        .entities
        .iter()
        .find(|entity| entity.id == id("actor"))
        .unwrap()
        .components[&sensor()]
        .clone();
    for index in 0..32 {
        let entity = nara_scene::SceneEntityRecord::new(id(&format!("sensor-{index}")))
            .with_component(sensor(), sensor_record.clone());
        document.roots.push(entity.id.clone());
        document.entities.push(entity);
    }
    for kind in KINDS {
        let mut runtime = PreparedRuntime::prepare(kind, Arc::clone(&definition), &document, None)
            .unwrap()
            .activate(&definition)
            .unwrap();
        for index in 0..256 {
            runtime
                .world
                .spawn(nara_scene::Name::new(format!("native-{index}")));
        }
        let (matches, inspected) = runtime.query(&mover());
        assert_eq!(matches.len(), 1);
        assert_eq!(
            inspected,
            match kind {
                StorageKind::Envelope => 33,
                StorageKind::Rows => 1,
            }
        );
        let (matches, inspected) = runtime.query(&sensor());
        assert_eq!((matches.len(), inspected), (33, 33));
        assert_eq!(runtime.retained_rows(), 34);
        runtime.retire().unwrap();
        assert_eq!(runtime.retained_rows(), 0);
        assert_eq!(
            runtime
                .world
                .iter_entities()
                .filter(|entity| entity.contains::<nara_scene::Name>())
                .count(),
            256
        );
    }
    assert_eq!(
        trial::direct_despawn_probe(StorageKind::Envelope),
        (1, 1, 2, 2)
    );
    assert_eq!(trial::direct_despawn_probe(StorageKind::Rows), (1, 2, 4, 2));
}

#[test]
fn export_failure_and_ordinary_retirement_do_not_hide_missing_owner_cleanup() {
    let definition = Definition::new(1, true);
    for kind in KINDS {
        let mut runtime =
            PreparedRuntime::prepare(kind, Arc::clone(&definition), &trial::source(), None)
                .unwrap()
                .activate(&definition)
                .unwrap();
        let target = runtime.target(&id("actor"));
        runtime.corrupt_for_export_probe(&target);
        assert!(matches!(runtime.export(), Err(Error::EncodeFailure)));
        assert_eq!(runtime.retained_rows(), 2);
        runtime.ordinary_retire().unwrap();
        assert_eq!(
            runtime.retained_rows(),
            match kind {
                StorageKind::Envelope => 0,
                StorageKind::Rows => 2,
            }
        );
    }
    let mut runtime = PreparedRuntime::prepare(
        StorageKind::Rows,
        Arc::clone(&definition),
        &trial::source(),
        None,
    )
    .unwrap()
    .activate(&definition)
    .unwrap();
    let target = runtime.target(&id("actor"));
    runtime.enable_rows_cleanup_hook(&target);
    assert_eq!(runtime.ordinary_retire(), Err(Error::Retirement));
    assert_eq!(runtime.retained_rows(), 2);
    assert!(runtime.read(&target, &mover()).unwrap().is_some());
    assert_eq!(runtime.retire(), Err(Error::Retirement));
    assert_eq!(runtime.retained_rows(), 2);
    runtime.tick(&target).unwrap();
    assert_eq!(runtime.native_name(&target), "Moved 2");
    runtime.remove_rows_cleanup_hook(&target);
    assert_eq!(runtime.retained_rows(), 0);
    runtime.ordinary_retire().unwrap();

    let mut runtime = PreparedRuntime::prepare(
        StorageKind::Rows,
        Arc::clone(&definition),
        &trial::source(),
        None,
    )
    .unwrap()
    .activate(&definition)
    .unwrap();
    let target = runtime.target(&id("actor"));
    runtime.enable_rows_cleanup_hook(&target);
    let entity = runtime.entity(&target).unwrap();
    runtime.world.despawn(entity);
    assert_eq!(runtime.retained_rows(), 0);
    assert_eq!(runtime.read(&target, &mover()), Err(Error::StaleEntity));
}

#[test]
fn repeated_prefab_occurrences_signal_only_their_own_targets_and_export_rebinds() {
    for kind in KINDS {
        let definition = Definition::new(1, true);
        let (mut document, prefab) = trial::prefab_source();
        let mut second = document.entities[0].clone();
        second.id = id("second-anchor");
        document.roots.push(second.id.clone());
        document.entities.push(second);
        let report = trial::compile(&document, &definition, Some(&prefab)).unwrap();
        let candidate = report.candidate.unwrap();
        let actors = candidate
            .document()
            .entities
            .iter()
            .filter(|entity| entity.components.contains_key(&mover()))
            .collect::<Vec<_>>();
        assert_eq!(actors.len(), 2);
        let friend_for = |actor: &nara_scene::SceneEntityRecord| {
            let ComponentValue::List(targets) =
                actor.components[&sensor()].value.get("targets").unwrap()
            else {
                panic!("targets are a list")
            };
            let ComponentValue::EntityReference(nara_reflect::EntityReference::SceneLocal {
                entity,
            }) = &targets[0]
            else {
                panic!("first target is Scene-local")
            };
            entity.clone()
        };
        let friends = actors
            .iter()
            .map(|actor| friend_for(actor))
            .collect::<Vec<_>>();
        assert_ne!(actors[0].id, actors[1].id);
        assert_ne!(friends[0], friends[1]);
        assert_ne!(
            candidate.provenance()[&actors[0].id]
                .projection_path()
                .unwrap()
                .root_anchor(),
            candidate.provenance()[&actors[1].id]
                .projection_path()
                .unwrap()
                .root_anchor()
        );
        for (actor, friend) in actors.iter().zip(&friends) {
            let actor_origin = &candidate.provenance()[&actor.id];
            let friend_origin = &candidate.provenance()[friend];
            assert_eq!(
                actor_origin.projection_path().unwrap().root_anchor(),
                friend_origin.projection_path().unwrap().root_anchor()
            );
            assert_eq!(friend_origin.local_entity_id(), &id("friend"));
        }
        let mut runtime =
            PreparedRuntime::prepare(kind, Arc::clone(&definition), &document, Some(&prefab))
                .unwrap()
                .activate(&definition)
                .unwrap();
        let actor_a = runtime.target(&actors[0].id);
        let actor_b = runtime.target(&actors[1].id);
        let friend_a = runtime.target(&friends[0]);
        let friend_b = runtime.target(&friends[1]);
        runtime
            .set(
                &actor_a,
                &mover(),
                record(
                    ComponentValue::map([("amount", ComponentValue::f64(3.0).unwrap())]),
                    1,
                ),
            )
            .unwrap();
        runtime.tick(&actor_a).unwrap();
        runtime.tick(&actor_b).unwrap();
        assert_eq!(runtime.signal(&actor_a).unwrap(), 1);
        assert_eq!(runtime.native_name(&friend_a), "Friend signalled");
        assert_eq!(runtime.native_name(&friend_b), "Friend");
        runtime.signal(&actor_b).unwrap();
        runtime.signal(&actor_b).unwrap();
        assert_eq!(runtime.native_name(&friend_a), "Friend signalled");
        assert_eq!(runtime.native_name(&friend_b), "Friend signalled signalled");
        let exported = runtime.export().unwrap();
        assert_eq!(
            exported
                .entities
                .iter()
                .filter(|entity| entity.components.contains_key(&sensor()))
                .count(),
            2
        );
        let named = |name: &str| {
            exported
                .entities
                .iter()
                .find(|entity| {
                    entity
                        .components
                        .get(&nara_reflect::ComponentTypeId::new("nara.scene.Name"))
                        .is_some_and(|record| record.value == ComponentValue::String(name.into()))
                })
                .unwrap()
        };
        let exported_a = named("Moved 3");
        let exported_b = named("Moved 2");
        let exported_friend_a = named("Friend signalled");
        let exported_friend_b = named("Friend signalled signalled");
        // Native Name provides an oracle independent of the exported Sensor payload.
        assert_eq!(friend_for(exported_a), exported_friend_a.id);
        assert_eq!(friend_for(exported_b), exported_friend_b.id);
        let mut reopened = PreparedRuntime::prepare(kind, Arc::clone(&definition), &exported, None)
            .unwrap()
            .activate(&definition)
            .unwrap();
        let first_friend = reopened.target(&exported_friend_a.id);
        let second_friend = reopened.target(&exported_friend_b.id);
        reopened.signal(&reopened.target(&exported_a.id)).unwrap();
        assert_eq!(
            reopened.native_name(&first_friend),
            "Friend signalled signalled"
        );
        assert_eq!(
            reopened.native_name(&second_friend),
            "Friend signalled signalled"
        );
        reopened.retire().unwrap();
        runtime.retire().unwrap();
    }
}

#[test]
fn runtime_reference_edits_reject_missing_and_dead_targets_before_replacement() {
    for kind in KINDS {
        let definition = Definition::new(1, true);
        let mut runtime =
            PreparedRuntime::prepare(kind, Arc::clone(&definition), &trial::source(), None)
                .unwrap()
                .activate(&definition)
                .unwrap();
        let target = runtime.target(&id("actor"));
        let original = runtime.read(&target, &sensor()).unwrap().unwrap().clone();
        let mover_before = runtime.read(&target, &mover()).unwrap().unwrap().clone();
        let mut invalid = original.clone();
        invalid
            .value
            .set_path(
                &nara_reflect::ComponentFieldPath::from_fields(["targets"]),
                ComponentValue::List(vec![ComponentValue::EntityReference(
                    nara_reflect::EntityReference::SceneLocal {
                        entity: id("missing"),
                    },
                )]),
            )
            .unwrap();
        assert_eq!(
            runtime.set(&target, &sensor(), invalid),
            Err(Error::InvalidValue)
        );
        assert_eq!(runtime.read(&target, &sensor()).unwrap(), Some(&original));
        let friend = runtime.entity(&runtime.target(&id("friend"))).unwrap();
        runtime.world.despawn(friend);
        assert_eq!(
            runtime.set(&target, &sensor(), original.clone()),
            Err(Error::InvalidValue)
        );
        assert_eq!(runtime.read(&target, &sensor()).unwrap(), Some(&original));
        assert_eq!(
            runtime.read(&target, &mover()).unwrap(),
            Some(&mover_before)
        );
    }
}
