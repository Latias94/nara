//! Accept-only shape proof for the current source-local entity-document wire grammar.
//!
//! This is not an input validator. Failure to prove a bound selects the original JSON/RON
//! preflight, including its acceptance and error order. Actual encoders still enforce exact
//! byte limits. Overlays, explicit asset products and patches deliberately use that fallback.

use nara_core::{PersistentFileEnvelope, SerdeShapeLimits};
use nara_identity::{
    AuthoredReferenceTarget, EntityReference, PrefabEntityOrigin, ReferenceInputId,
    SourceDocumentKey, SourceDocumentKind, SourceDocumentOrigin,
};
use nara_reflect::ComponentValue;

use crate::{
    PrefabEntityTarget, PrefabInstance, PrefabInstanceBinding, SceneComponentRecord,
    SceneEntityPlacement, SceneEntityRecord, ScenePlacementParent, ScenePlacementSlot,
};

// Both current parsers have a recursion budget of 128. RON guards collection visitation and
// child dispatch separately; keep the supported wire depth well below those budgets. Deeper
// values use the unchanged encoders/parsers even when the caller admits a wider shape limit.
const CERTIFICATE_DEPTH: usize = 32;

pub(super) fn certifies_entity_document<T>(
    envelope: &PersistentFileEnvelope<T>,
    placements: &[SceneEntityPlacement],
    entities: &[SceneEntityRecord],
    inputs: Option<&std::collections::BTreeSet<ReferenceInputId>>,
    limits: SerdeShapeLimits,
) -> bool {
    Certificate {
        limits,
        nodes: 0,
        string_bytes: 0,
    }
    .envelope(envelope, placements, entities, inputs)
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
        self.string_length(depth, value.len())
    }

    fn string_length(&mut self, depth: usize, length: usize) -> Option<()> {
        self.node(depth)?;
        if length > self.limits.string_bytes().get() {
            return None;
        }
        self.string_bytes = self.string_bytes.checked_add(length)?;
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
        placements: &[SceneEntityPlacement],
        entities: &[SceneEntityRecord],
        inputs: Option<&std::collections::BTreeSet<ReferenceInputId>>,
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
        if let Some(inputs) = inputs.filter(|inputs| !inputs.is_empty()) {
            self.structure(2, &["inputs", "placements", "entities"])?;
            self.container(3, inputs.len())?;
            for input in inputs {
                self.string(4, input.as_str())?;
            }
        } else {
            self.structure(2, &["placements", "entities"])?;
        }
        self.container(3, placements.len())?;
        for declaration in placements {
            self.structure(4, &["target", "placement"])?;
            self.entity_target(5, &declaration.target)?;
            self.structure(5, &["parent", "slot"])?;
            match &declaration.placement.parent {
                ScenePlacementParent::Root => {
                    self.structure(6, &["kind"])?;
                    self.string(7, "root")?;
                }
                ScenePlacementParent::Entity(target) => {
                    self.structure(6, &["kind", "target"])?;
                    self.string(7, "entity")?;
                    self.entity_target(7, target)?;
                }
            }
            match &declaration.placement.slot {
                ScenePlacementSlot::First | ScenePlacementSlot::Last => {
                    self.structure(6, &["kind"])?;
                    self.string(
                        7,
                        match &declaration.placement.slot {
                            ScenePlacementSlot::First => "first",
                            _ => "last",
                        },
                    )?;
                }
                ScenePlacementSlot::Before(target) | ScenePlacementSlot::After(target) => {
                    self.structure(6, &["kind", "target"])?;
                    self.string(
                        7,
                        match &declaration.placement.slot {
                            ScenePlacementSlot::Before(_) => "before",
                            _ => "after",
                        },
                    )?;
                    self.entity_target(7, target)?;
                }
            }
        }
        self.container(3, entities.len())?;
        for entity in entities {
            self.entity(4, entity)?;
        }
        Some(())
    }

    fn entity_target(&mut self, depth: usize, target: &PrefabEntityTarget) -> Option<()> {
        self.structure(depth, &["through", "entity"])?;
        self.container(depth + 1, target.through.len())?;
        for hop in &target.through {
            self.structure(depth + 2, &["anchor", "expected_base"])?;
            self.entity_origin(depth + 3, &hop.anchor)?;
            self.source_key(depth + 3, &hop.expected_base)?;
        }
        self.entity_origin(depth + 1, &target.entity)
    }

    fn entity(&mut self, depth: usize, entity: &SceneEntityRecord) -> Option<()> {
        // SceneEntityRecord omits prefab=None. RON Some adds its own wrapper node.
        self.structure(
            depth,
            if entity.prefab.is_some() {
                &["id", "components", "prefab"]
            } else {
                &["id", "components"]
            },
        )?;
        self.string(depth + 1, entity.id.as_str())?;
        self.container(depth + 1, entity.components.len())?;
        for (component, record) in &entity.components {
            self.string(depth + 2, component.as_str())?;
            self.record(depth + 2, record)?;
        }
        if let Some(instance) = &entity.prefab {
            self.node(depth + 1)?;
            self.instance(depth + 2, instance)?;
        }
        Some(())
    }

    fn instance(&mut self, depth: usize, instance: &PrefabInstance) -> Option<()> {
        if instance.overrides.is_some() {
            return None;
        }
        self.structure(
            depth,
            if instance.arguments.is_some() {
                &["source", "binding", "arguments", "overrides"]
            } else {
                &["source", "binding", "overrides"]
            },
        )?;
        self.structure(depth + 1, &["kind", "value"])?;
        match &instance.source {
            nara_asset::AssetRef::Path(path) => {
                self.string(depth + 2, "path")?;
                self.string(depth + 2, path.as_str())?;
            }
            nara_asset::AssetRef::StableId(_) => {
                self.string(depth + 2, "stable_id")?;
                self.string_length(depth + 2, 36)?;
            }
            nara_asset::AssetRef::Product(_) => return None,
        }
        match &instance.binding {
            PrefabInstanceBinding::Forest => {
                self.structure(depth + 1, &["kind"])?;
                self.string(depth + 2, "forest")?;
            }
            PrefabInstanceBinding::Root { base, root } => {
                self.structure(depth + 1, &["kind", "base", "root"])?;
                self.string(depth + 2, "root")?;
                self.source_key(depth + 2, base)?;
                self.entity_target(depth + 2, root)?;
            }
        }
        self.node(depth + 1)?; // overrides=None is retained on the wire.
        if let Some(arguments) = &instance.arguments {
            self.node(depth + 1)?;
            self.structure(depth + 2, &["base", "values"])?;
            self.source_key(depth + 3, &arguments.base)?;
            self.container(depth + 3, arguments.values.len())?;
            for (input, reference) in &arguments.values {
                self.string(depth + 4, input.as_str())?;
                self.entity_reference(depth + 4, reference)?;
            }
        }
        Some(())
    }

    fn record(&mut self, depth: usize, record: &SceneComponentRecord) -> Option<()> {
        let contextual = !record.contextual_defaults.is_empty();
        if let Some(owner) = &record.owner {
            self.structure(
                depth,
                if contextual {
                    &["owner", "version", "value", "contextual_defaults"]
                } else {
                    &["owner", "version", "value"]
                },
            )?;
            // JSON Some is transparent; RON Some adds a node and child depth. Counting
            // the RON wrapper is therefore an upper bound for both formats.
            self.node(depth + 1)?;
            self.string(depth + 2, owner.as_str())?;
        } else {
            self.structure(
                depth,
                if contextual {
                    &["version", "value", "contextual_defaults"]
                } else {
                    &["version", "value"]
                },
            )?;
        }
        self.node(depth + 1)?;
        self.value(depth + 1, &record.value)?;
        if contextual {
            self.container(depth + 1, record.contextual_defaults.len())?;
            for (field, value) in &record.contextual_defaults {
                self.string(depth + 2, field.as_str())?;
                self.value(depth + 2, value)?;
            }
        }
        Some(())
    }

    fn value(&mut self, depth: usize, value: &ComponentValue) -> Option<()> {
        // ComponentValue's adjacent type/value tagging is part of the format contract.
        // Variant tags are explicit literal strings in both JSON and RON, including when
        // an outer value is buffered before its tag. Count their actual string payload.
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
        self.structure(
            depth,
            if matches!(reference, EntityReference::Authored { .. }) {
                &["kind", "reference"]
            } else {
                &["kind", "entity"]
            },
        )?;
        // This internal tag is an explicit string in both formats.
        let kind = match reference {
            EntityReference::SceneLocal { .. } => "scene_local",
            EntityReference::Persistent { .. } => "persistent",
            EntityReference::Authored { .. } => "authored",
        };
        self.string(depth + 1, kind)?;
        match reference {
            EntityReference::SceneLocal { entity } => self.string(depth + 1, entity.as_str()),
            EntityReference::Persistent { entity } => {
                self.structure(depth + 1, &["namespace", "entity"])?;
                self.string(depth + 2, entity.namespace.as_str())?;
                // The ID serializer uses this same Display representation, not its UUID bytes.
                self.string_length(depth + 2, 36)
            }
            EntityReference::Authored { reference } => {
                self.structure(depth + 1, &["through", "target"])?;
                self.container(depth + 2, reference.through.len())?;
                for hop in &reference.through {
                    self.structure(depth + 3, &["anchor", "expected_base"])?;
                    self.entity_origin(depth + 4, &hop.anchor)?;
                    self.source_key(depth + 4, &hop.expected_base)?;
                }
                self.structure(depth + 2, &["kind", "value"])?;
                match &reference.target {
                    AuthoredReferenceTarget::Entity(entity) => {
                        self.string(depth + 3, "entity")?;
                        self.entity_origin(depth + 3, entity)
                    }
                    AuthoredReferenceTarget::Input(input) => {
                        self.string(depth + 3, "input")?;
                        self.string(depth + 3, input.as_str())
                    }
                }
            }
        }
    }

    fn entity_origin(&mut self, depth: usize, origin: &PrefabEntityOrigin) -> Option<()> {
        self.structure(depth, &["kind", "entity"])?;
        self.string(
            depth + 1,
            match origin {
                PrefabEntityOrigin::SourceEntity(_) => "source_entity",
                PrefabEntityOrigin::InstanceAddition(_) => "instance_addition",
            },
        )?;
        self.string(depth + 1, origin.entity().as_str())
    }

    fn source_key(&mut self, depth: usize, key: &SourceDocumentKey) -> Option<()> {
        self.structure(depth, &["kind", "origin"])?;
        self.string(
            depth + 1,
            match key.kind() {
                SourceDocumentKind::Scene => "scene",
                SourceDocumentKind::Prefab => "prefab",
            },
        )?;
        // Both formats use a literal string kind and an internally tagged origin record.
        // UUID wrappers serialize as 36-byte strings; no external variant map is present.
        match key.origin() {
            SourceDocumentOrigin::Named { name, .. } => {
                self.structure(depth + 1, &["kind", "universe", "name"])?;
                self.string(depth + 2, "named")?;
                self.string_length(depth + 2, 36)?;
                self.string(depth + 2, name)
            }
            SourceDocumentOrigin::ProjectLocation { path, .. } => {
                self.structure(depth + 1, &["kind", "universe", "path"])?;
                self.string(depth + 2, "project_location")?;
                self.string_length(depth + 2, 36)?;
                self.string(depth + 2, path.as_str())
            }
            SourceDocumentOrigin::StableAsset { .. } => {
                self.structure(depth + 1, &["kind", "stable_id"])?;
                self.string(depth + 2, "stable_asset")?;
                self.string_length(depth + 2, 36)
            }
        }
    }
}
