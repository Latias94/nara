//! Private flat wire. In particular, no source entity reader can accept `prefab: null` here.

use std::{
    collections::BTreeMap,
    fmt,
    io::{self, Write},
    mem::size_of,
};

use nara_asset::AssetRef;
use nara_core::{
    EngineVersion, FormatGenerator, FormatKind, PersistentFileContract,
    PersistentFileContractError, PersistentFileDecodeError, PersistentFileEnvelope,
    PersistentFileHeader, SerdeShapeError, SerdeShapePreflightError,
    decode_persistent_file_with_preflight, preflight_serde_shape,
};
use nara_reflect::ComponentTypeId;
use serde::{
    Deserialize, Serialize, Serializer,
    de::{DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor},
    ser::SerializeSeq,
};

use super::{
    RuntimeSceneCandidate, RuntimeSceneDocumentCandidate, RuntimeSceneError, RuntimeSceneLimits,
    budget::{ProvenanceBudget, WorkBudget, check, scene_budget_error},
};
use crate::{
    PrefabProjectionPath, PrefabProjectionStep, SceneComponentRecord, SceneDocument, SceneEntityId,
    SceneEntityProvenance, SceneEntityRecord, SourceDocumentKey,
    provenance::{provenance_entry_bytes, provenance_string_bytes},
};

const KIND: &str = "runtime_scene";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PayloadWire {
    source_key: SourceDocumentKey,
    roots: Vec<SceneEntityId>,
    entities: Vec<EntityWire>,
    provenance: Vec<ProvenanceWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntityWire {
    id: SceneEntityId,
    children: Vec<SceneEntityId>,
    components: BTreeMap<ComponentTypeId, SceneComponentRecord>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProvenanceWire {
    entity: SceneEntityId,
    local_entity: SceneEntityId,
    origin: OriginWire,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum OriginWire {
    Local {},
    Projected {
        root_anchor: SceneEntityId,
        steps: Vec<StepWire>,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum StepWire {
    SourceEntity {
        source_document: AssetRef,
        canonical_source: SourceDocumentKey,
        entity: SceneEntityId,
    },
    InstanceAddition {
        entity: SceneEntityId,
    },
}

pub(super) fn decode(
    bytes: &[u8],
    limits: RuntimeSceneLimits,
) -> Result<RuntimeSceneDocumentCandidate, RuntimeSceneError> {
    let mut work = WorkBudget::new(limits);
    let contract =
        PersistentFileContract::canonical_v1(kind()?, version(env!("CARGO_PKG_VERSION"))?);
    let envelope = decode_persistent_file_with_preflight(
        bytes,
        limits.base().encoded_bytes(),
        &contract,
        |bytes| {
            shape(bytes, limits)?;
            // This second walk retains no collections and admits a conservative raw-decode
            // ownership model before serde builds domain strings, Vecs, trees or enum buffers.
            raw_work(bytes, &mut work)
        },
        |bytes| serde_json::from_slice::<PersistentFileHeader>(bytes).map_err(json_error),
        |bytes| {
            serde_json::from_slice::<PersistentFileEnvelope<PayloadWire>>(bytes).map_err(json_error)
        },
    )
    .map_err(|failure| match failure {
        PersistentFileDecodeError::EncodedBytesExceeded { observed, maximum } => {
            RuntimeSceneError::budget("encoded-bytes", observed, maximum)
        }
        PersistentFileDecodeError::Shape(error)
        | PersistentFileDecodeError::Header(error)
        | PersistentFileDecodeError::Payload(error) => error,
        PersistentFileDecodeError::Contract(error) => contract_error(error),
    })?;
    let raw = envelope.into_payload();
    check(
        "entities",
        raw.entities.len(),
        limits.base().entities().get(),
    )?;
    check(
        "entities",
        raw.provenance.len(),
        limits.base().entities().get(),
    )?;
    let mut provenance_budget = ProvenanceBudget::new(&raw.source_key, limits)?;
    for entry in &raw.provenance {
        provenance_budget.add(
            provenance_entry_bytes(&entry.entity, &entry.local_entity),
            1,
        )?;
        if let OriginWire::Projected { root_anchor, steps } = &entry.origin {
            check(
                "provenance-depth",
                steps.len(),
                limits.provenance_depth().get(),
            )?;
            provenance_budget.add(provenance_string_bytes(root_anchor.as_str()), 0)?;
            for step in steps {
                let bytes = match step {
                    StepWire::SourceEntity {
                        source_document,
                        canonical_source,
                        entity,
                    } => PrefabProjectionStep::source_allocation_bytes(
                        source_document,
                        canonical_source,
                        entity,
                    ),
                    StepWire::InstanceAddition { entity } => {
                        PrefabProjectionStep::addition_allocation_bytes(entity)
                    }
                };
                provenance_budget.add(bytes, 1)?;
            }
        }
    }
    // Conversion moves every payload and identity; only the entity Vec and provenance tree
    // need new allocations. Charge their complete requests before constructing either.
    work.reserve(
        raw.entities
            .len()
            .checked_mul(size_of::<SceneEntityRecord>()),
    )?;
    work.reserve(raw.provenance.len().checked_mul(
        size_of::<SceneEntityId>() + size_of::<SceneEntityProvenance>() + 6 * size_of::<usize>(),
    ))?;
    let mut provenance = BTreeMap::new();
    for entry in raw.provenance {
        let origin = match entry.origin {
            OriginWire::Local {} => SceneEntityProvenance::local(entry.local_entity),
            OriginWire::Projected { root_anchor, steps } => {
                work.reserve(steps.len().checked_mul(size_of::<PrefabProjectionStep>()))?;
                let steps = steps
                    .into_iter()
                    .map(|step| match step {
                        StepWire::SourceEntity {
                            source_document,
                            canonical_source,
                            entity,
                        } => PrefabProjectionStep::SourceEntity {
                            source_document,
                            canonical_source,
                            entity,
                        },
                        StepWire::InstanceAddition { entity } => {
                            PrefabProjectionStep::InstanceAddition { entity }
                        }
                    })
                    .collect();
                SceneEntityProvenance::projected(
                    entry.local_entity,
                    PrefabProjectionPath::from_resolved_steps(root_anchor, steps),
                )
            }
        };
        if provenance.insert(entry.entity, origin).is_some() {
            return Err(RuntimeSceneError::refused(
                "scene.runtime-duplicate-provenance",
                "Runtime Scene repeats a provenance entity key",
            ));
        }
    }
    let mut document = SceneDocument {
        roots: raw.roots,
        entities: raw
            .entities
            .into_iter()
            .map(|entity| SceneEntityRecord {
                id: entity.id,
                children: entity.children,
                components: entity.components,
                prefab: None,
            })
            .collect(),
    };
    document
        .entities
        .sort_unstable_by(|left, right| left.id.cmp(&right.id));
    crate::format::validate_scene_value_budget(&document, limits.base())
        .map_err(scene_budget_error)?;
    Ok(RuntimeSceneDocumentCandidate {
        source_key: raw.source_key,
        document,
        provenance,
        work,
    })
}

// Borrowed serialization retains the same concrete grammar without copying component trees,
// request evidence, source keys or projection routes into another raw document.
#[derive(Serialize)]
struct PayloadRef<'a> {
    source_key: &'a SourceDocumentKey,
    roots: &'a [SceneEntityId],
    entities: EntitiesRef<'a>,
    provenance: ProvenanceRef<'a>,
}

struct EntitiesRef<'a>(&'a [SceneEntityRecord]);

impl Serialize for EntitiesRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct EntityRef<'a> {
            id: &'a SceneEntityId,
            children: &'a [SceneEntityId],
            components: &'a BTreeMap<ComponentTypeId, SceneComponentRecord>,
        }
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for entity in self.0 {
            sequence.serialize_element(&EntityRef {
                id: &entity.id,
                children: &entity.children,
                components: &entity.components,
            })?;
        }
        sequence.end()
    }
}

struct ProvenanceRef<'a>(&'a crate::SceneEntityProvenanceMap);

impl Serialize for ProvenanceRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct EntryRef<'a> {
            entity: &'a SceneEntityId,
            local_entity: &'a SceneEntityId,
            origin: OriginRef<'a>,
        }
        #[derive(Serialize)]
        #[serde(tag = "kind", rename_all = "snake_case")]
        enum OriginRef<'a> {
            Local,
            Projected {
                root_anchor: &'a SceneEntityId,
                steps: StepsRef<'a>,
            },
        }
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for (entity, origin) in self.0 {
            let location = match origin.projection_path() {
                None => OriginRef::Local,
                Some(path) => OriginRef::Projected {
                    root_anchor: path.root_anchor(),
                    steps: StepsRef(path.steps()),
                },
            };
            sequence.serialize_element(&EntryRef {
                entity,
                local_entity: origin.local_entity_id(),
                origin: location,
            })?;
        }
        sequence.end()
    }
}

struct StepsRef<'a>(&'a [PrefabProjectionStep]);

impl Serialize for StepsRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(tag = "kind", rename_all = "snake_case")]
        enum StepRef<'a> {
            SourceEntity {
                source_document: &'a AssetRef,
                canonical_source: &'a SourceDocumentKey,
                entity: &'a SceneEntityId,
            },
            InstanceAddition {
                entity: &'a SceneEntityId,
            },
        }
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for step in self.0 {
            let step = match step {
                PrefabProjectionStep::SourceEntity {
                    source_document,
                    canonical_source,
                    entity,
                } => StepRef::SourceEntity {
                    source_document,
                    canonical_source,
                    entity,
                },
                PrefabProjectionStep::InstanceAddition { entity } => {
                    StepRef::InstanceAddition { entity }
                }
            };
            sequence.serialize_element(&step)?;
        }
        sequence.end()
    }
}

pub(super) fn encode(
    candidate: &RuntimeSceneCandidate,
    work: &mut WorkBudget,
) -> Result<Vec<u8>, RuntimeSceneError> {
    let payload = PayloadRef {
        source_key: candidate.source_key(),
        roots: &candidate.document().roots,
        entities: EntitiesRef(&candidate.document().entities),
        provenance: ProvenanceRef(candidate.provenance()),
    };
    let envelope = PersistentFileEnvelope::canonical_v1(
        kind()?,
        version("0.1.0")?,
        FormatGenerator::new("nara", version(env!("CARGO_PKG_VERSION"))?)
            .map_err(|_| metadata_error())?,
        payload,
    );
    let maximum = candidate.limits().base().encoded_bytes().get();
    let mut counter = EncodedWriter {
        bytes: None,
        written: 0,
        maximum,
        exceeded: None,
    };
    if let Err(error) = serde_json::to_writer(&mut counter, &envelope) {
        return Err(counter.failure(error));
    }
    work.reserve(Some(counter.written))?;
    let mut writer = EncodedWriter {
        bytes: Some(Vec::with_capacity(counter.written)),
        written: 0,
        maximum,
        exceeded: None,
    };
    if let Err(error) = serde_json::to_writer(&mut writer, &envelope) {
        return Err(writer.failure(error));
    }
    let bytes = writer.bytes.unwrap_or_default();
    shape(&bytes, candidate.limits())?;
    Ok(bytes)
}

struct EncodedWriter {
    bytes: Option<Vec<u8>>,
    written: usize,
    maximum: usize,
    exceeded: Option<usize>,
}

impl Write for EncodedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let observed = self.written.saturating_add(bytes.len());
        if observed > self.maximum {
            self.exceeded = Some(observed);
            return Err(io::Error::other(
                "Runtime Scene encoded byte limit exceeded",
            ));
        }
        if let Some(output) = &mut self.bytes {
            output.extend_from_slice(bytes);
        }
        self.written = observed;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl EncodedWriter {
    fn failure(&self, error: serde_json::Error) -> RuntimeSceneError {
        match self.exceeded {
            Some(observed) => RuntimeSceneError::budget("encoded-bytes", observed, self.maximum),
            None => json_error(error),
        }
    }
}

fn shape(bytes: &[u8], limits: RuntimeSceneLimits) -> Result<(), RuntimeSceneError> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    preflight_serde_shape(&mut deserializer, limits.base().shape()).map_err(
        |failure| match failure {
            SerdeShapePreflightError::Parse(error) => json_error(error),
            SerdeShapePreflightError::Shape(error) => match error {
                SerdeShapeError::DepthExceeded { maximum } => {
                    RuntimeSceneError::budget("wire-depth", maximum.saturating_add(1), maximum)
                }
                SerdeShapeError::NodeLimitExceeded { maximum } => {
                    RuntimeSceneError::budget("wire-nodes", maximum.saturating_add(1), maximum)
                }
                SerdeShapeError::ContainerItemLimitExceeded { maximum } => {
                    RuntimeSceneError::budget(
                        "wire-container-items",
                        maximum.saturating_add(1),
                        maximum,
                    )
                }
                SerdeShapeError::StringLimitExceeded { maximum } => RuntimeSceneError::budget(
                    "wire-string-bytes",
                    maximum.saturating_add(1),
                    maximum,
                ),
                SerdeShapeError::TotalStringLimitExceeded { maximum } => RuntimeSceneError::budget(
                    "wire-total-string-bytes",
                    maximum.saturating_add(1),
                    maximum,
                ),
                SerdeShapeError::DuplicateMapKey => RuntimeSceneError::refused(
                    "scene.runtime-duplicate-field",
                    "Runtime Scene wire contains a duplicate map field",
                ),
            },
        },
    )?;
    deserializer.end().map_err(json_error)
}

fn kind() -> Result<FormatKind, RuntimeSceneError> {
    FormatKind::new(KIND).map_err(|_| metadata_error())
}

fn version(value: &str) -> Result<EngineVersion, RuntimeSceneError> {
    EngineVersion::parse(value).map_err(|_| metadata_error())
}

fn metadata_error() -> RuntimeSceneError {
    RuntimeSceneError::refused(
        "scene.runtime-format-metadata",
        "Runtime Scene format metadata is invalid",
    )
}

fn contract_error(error: PersistentFileContractError) -> RuntimeSceneError {
    match error {
        PersistentFileContractError::WrongKind { .. } => RuntimeSceneError::refused(
            "scene.runtime-file-kind",
            "Runtime Scene requires the runtime_scene persistent file kind",
        ),
        PersistentFileContractError::UnsupportedFormatVersion {
            actual, supported, ..
        } => RuntimeSceneError::budget(
            "format-version",
            actual.get() as usize,
            supported.get() as usize,
        ),
        PersistentFileContractError::EngineVersionTooOld { .. } => RuntimeSceneError::refused(
            "scene.runtime-engine-version",
            "Runtime Scene requires a newer engine version",
        ),
    }
}

fn json_error(error_value: serde_json::Error) -> RuntimeSceneError {
    use crate::diagnostics::{error, with_public_u64};
    let mut diagnostics = nara_diagnostic::DiagnosticReport::default();
    diagnostics.push(with_public_u64(
        with_public_u64(
            error(
                "scene.runtime-json",
                "Runtime Scene JSON does not match the strict flat wire grammar",
            ),
            "line",
            u64::try_from(error_value.line()).unwrap_or(u64::MAX),
        ),
        "column",
        u64::try_from(error_value.column()).unwrap_or(u64::MAX),
    ));
    RuntimeSceneError { diagnostics }
}

// A generic ownership count, not a second component/provenance grammar. The core preflight
// already bounds depth and verifies unique keys. This walk adds no retained collections.
fn raw_work(bytes: &[u8], work: &mut WorkBudget) -> Result<(), RuntimeSceneError> {
    let mut refusal = None;
    let result = RawWork {
        work,
        refusal: &mut refusal,
    }
    .deserialize(&mut serde_json::Deserializer::from_slice(bytes));
    match refusal {
        Some(error) => Err(error),
        None => result.map_err(json_error),
    }
}

struct RawWork<'a> {
    work: &'a mut WorkBudget,
    refusal: &'a mut Option<RuntimeSceneError>,
}

impl RawWork<'_> {
    fn reserve<E: serde::de::Error>(&mut self, bytes: Option<usize>) -> Result<(), E> {
        self.work.reserve(bytes).map_err(|error| {
            *self.refusal = Some(error);
            E::custom("Runtime Scene raw allocation admission refused")
        })
    }

    fn child(&mut self) -> RawWork<'_> {
        RawWork {
            work: self.work,
            refusal: self.refusal,
        }
    }
}

impl<'de> DeserializeSeed<'de> for RawWork<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(mut self, deserializer: D) -> Result<(), D::Error> {
        // Includes raw node/collection storage and serde's internally tagged enum scratch.
        self.reserve(Some(192))?;
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for RawWork<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bounded Runtime Scene JSON")
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E: serde::de::Error>(mut self, value: &str) -> Result<(), E> {
        self.reserve(value.len().checked_mul(2))
    }

    fn visit_seq<A: SeqAccess<'de>>(mut self, mut sequence: A) -> Result<(), A::Error> {
        while sequence.next_element_seed(self.child())?.is_some() {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(mut self, mut map: A) -> Result<(), A::Error> {
        while map.next_key_seed(self.child())?.is_some() {
            map.next_value_seed(self.child())?;
        }
        Ok(())
    }
}
