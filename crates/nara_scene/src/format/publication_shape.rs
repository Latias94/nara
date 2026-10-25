//! Accept-only shape proof for the current source-local entity-document wire grammar.
//!
//! This is not an input validator. Failure to prove a bound selects the original JSON/RON
//! preflight, including its acceptance and error order. Actual encoders still enforce exact
//! byte limits. Embedded Prefabs and patches deliberately use that fallback.

use nara_core::{PersistentFileEnvelope, SerdeShapeLimits};
use nara_identity::EntityReference;
use nara_reflect::ComponentValue;

use crate::{SceneComponentRecord, SceneEntityId, SceneEntityRecord};

// Both current parsers have a recursion budget of 128. RON guards collection visitation and
// child dispatch separately; keep the supported wire depth well below those budgets. Deeper
// values use the unchanged encoders/parsers even when the caller admits a wider shape limit.
const CERTIFICATE_DEPTH: usize = 32;

pub(super) fn certifies_entity_document<T>(
    envelope: &PersistentFileEnvelope<T>,
    roots: &[SceneEntityId],
    entities: &[SceneEntityRecord],
    limits: SerdeShapeLimits,
) -> bool {
    Certificate {
        limits,
        nodes: 0,
        string_bytes: 0,
    }
    .envelope(envelope, roots, entities)
    .is_some()
}

struct Certificate {
    limits: SerdeShapeLimits,
    nodes: usize,
    string_bytes: usize,
}

impl Certificate {
    fn node(&mut self, depth: usize) -> Option<()> {
        if depth > CERTIFICATE_DEPTH || depth > self.limits.depth().get() {
            return None;
        }
        self.nodes = self.nodes.checked_add(1)?;
        (self.nodes <= self.limits.nodes().get()).then_some(())
    }

    fn string(&mut self, depth: usize, value: &str) -> Option<()> {
        self.node(depth)?;
        if value.len() > self.limits.string_bytes().get() {
            return None;
        }
        self.string_bytes = self.string_bytes.checked_add(value.len())?;
        (self.string_bytes <= self.limits.total_string_bytes().get()).then_some(())
    }

    fn container(&mut self, depth: usize, items: usize) -> Option<()> {
        self.node(depth)?;
        (items <= self.limits.container_items().get()).then_some(())
    }

    fn structure(&mut self, depth: usize, fields: &[&str]) -> Option<()> {
        self.container(depth, fields.len())?;
        for field in fields {
            self.string(depth + 1, field)?;
        }
        Some(())
    }

    fn envelope<T>(
        &mut self,
        envelope: &PersistentFileEnvelope<T>,
        roots: &[SceneEntityId],
        entities: &[SceneEntityRecord],
    ) -> Option<()> {
        // PersistentFileEnvelope and FormatGenerator serialize these exact named fields.
        // Their ID/version wrappers serialize as strings or integers, without RON newtypes.
        self.structure(
            1,
            &[
                "kind",
                "format_version",
                "engine_min_version",
                "generator",
                "payload",
            ],
        )?;
        self.string(2, envelope.kind().as_str())?;
        self.node(2)?;
        self.string(2, &envelope.engine_min_version().to_string())?;
        self.structure(2, &["name", "version"])?;
        self.string(3, envelope.generator().name())?;
        self.string(3, &envelope.generator().version().to_string())?;
        self.structure(2, &["roots", "entities"])?;
        self.ids(3, roots)?;
        self.container(3, entities.len())?;
        for entity in entities {
            self.entity(4, entity)?;
        }
        Some(())
    }

    fn ids(&mut self, depth: usize, ids: &[SceneEntityId]) -> Option<()> {
        self.container(depth, ids.len())?;
        for id in ids {
            self.string(depth + 1, id.as_str())?;
        }
        Some(())
    }

    fn entity(&mut self, depth: usize, entity: &SceneEntityRecord) -> Option<()> {
        // SceneEntityRecord omits prefab=None. All other entity fields are always present.
        if entity.prefab.is_some() {
            return None;
        }
        self.structure(depth, &["id", "children", "components"])?;
        self.string(depth + 1, entity.id.as_str())?;
        self.ids(depth + 1, &entity.children)?;
        self.container(depth + 1, entity.components.len())?;
        for (component, record) in &entity.components {
            self.string(depth + 2, component.as_str())?;
            self.record(depth + 2, record)?;
        }
        Some(())
    }

    fn record(&mut self, depth: usize, record: &SceneComponentRecord) -> Option<()> {
        if let Some(owner) = &record.owner {
            self.structure(depth, &["owner", "version", "value"])?;
            // JSON Some is transparent; RON Some adds a node and child depth. Counting
            // the RON wrapper is therefore an upper bound for both formats.
            self.node(depth + 1)?;
            self.string(depth + 2, owner.as_str())?;
        } else {
            self.structure(depth, &["version", "value"])?;
        }
        self.node(depth + 1)?;
        self.value(depth + 1, &record.value)
    }

    fn value(&mut self, depth: usize, value: &ComponentValue) -> Option<()> {
        // ComponentValue's adjacent type/value tagging is part of the format contract.
        // JSON observes each variant name as a string; RON observes it as a unit. Counting
        // the string proves the larger string budget without approximating encoded escapes.
        // All maps here are BTreeMaps and struct fields are distinct, so neither format can
        // produce duplicate decoded keys. Any new value variant must make an explicit choice.
        let tag = match value {
            ComponentValue::Null => "null",
            ComponentValue::Bool(_) => "bool",
            ComponentValue::I64(_) => "i64",
            ComponentValue::U64(_) => "u64",
            ComponentValue::F64(_) => "f64",
            ComponentValue::String(_) => "string",
            ComponentValue::List(_) => "list",
            ComponentValue::Map(_) => "map",
            ComponentValue::EntityReference(_) => "entity_ref",
        };
        let fields: &[&str] = if matches!(value, ComponentValue::Null) {
            &["type"]
        } else {
            &["type", "value"]
        };
        self.structure(depth, fields)?;
        self.string(depth + 1, tag)?;
        match value {
            ComponentValue::Null => Some(()),
            ComponentValue::Bool(_)
            | ComponentValue::I64(_)
            | ComponentValue::U64(_)
            | ComponentValue::F64(_) => self.node(depth + 1),
            ComponentValue::String(value) => self.string(depth + 1, value),
            ComponentValue::List(values) => {
                self.container(depth + 1, values.len())?;
                for value in values {
                    self.value(depth + 2, value)?;
                }
                Some(())
            }
            ComponentValue::Map(values) => {
                self.container(depth + 1, values.len())?;
                for (key, value) in values {
                    self.string(depth + 2, key)?;
                    self.value(depth + 2, value)?;
                }
                Some(())
            }
            ComponentValue::EntityReference(reference) => {
                self.entity_reference(depth + 1, reference)
            }
        }
    }

    fn entity_reference(&mut self, depth: usize, reference: &EntityReference) -> Option<()> {
        self.structure(depth, &["kind", "entity"])?;
        // Unlike the outer ComponentValue tag, this internal tag is a string in both formats.
        let kind = match reference {
            EntityReference::SceneLocal { .. } => "scene_local",
            EntityReference::Persistent { .. } => "persistent",
        };
        self.string(depth + 1, kind)?;
        match reference {
            EntityReference::SceneLocal { entity } => self.string(depth + 1, entity.as_str()),
            EntityReference::Persistent { entity } => {
                self.structure(depth + 1, &["namespace", "entity"])?;
                self.string(depth + 2, entity.namespace.as_str())?;
                // The ID serializer uses this same Display representation, not its UUID bytes.
                self.string(depth + 2, &entity.entity.to_string())
            }
        }
    }
}
