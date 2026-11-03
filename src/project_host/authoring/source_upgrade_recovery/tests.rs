use nara_asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetSourcePath, StableAssetId};
use nara_project::{ProjectAuthoringRecovery, ProjectPath};
use nara_reflect::{
    AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldPath, ComponentFieldSchema,
    ComponentPropertyStep, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion,
    ComponentShapeMember, ComponentValue, ComponentValueKind, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    PrefabDocument, PrefabDocumentCandidate, SceneComponentRecord, SceneDocument,
    SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, SourceDocumentKind,
};

use super::*;
use crate::project_host::authoring::{
    AuthoringProjectAdmissionLimits,
    recovery_storage::{RecoveryStorageError, validate_pending_bytes, validate_storage_path},
    source_decode::reserve_source_decode,
    source_upgrade_storage::SourceUpgradeStorageError,
};

const UNIVERSE: &str = "421112bd-693a-4e6b-85bc-318d567b2f21";
const OWNER: &str = "trial.enemy";
const COMPONENT: &str = "trial.enemy.EnemyStats";

struct MemberFixture {
    path: String,
    key: SourceDocumentKey,
    encoding: RecoverySourceEncoding,
    old: Vec<u8>,
    working: Vec<u8>,
    next: Vec<u8>,
}

struct Fixture {
    old_manifest: Vec<u8>,
    future_manifest: Vec<u8>,
    old_bundle: Vec<u8>,
    next_bundle: Vec<u8>,
    component: ComponentTypeId,
    field: ComponentFieldId,
    selector: ComponentPropertyPath,
    members: Vec<MemberFixture>,
    metadata: Vec<(String, Vec<u8>)>,
    roots: Vec<(String, Option<String>)>,
    missing_sidecars: Vec<String>,
}

impl Fixture {
    fn new() -> Self {
        let old_manifest = format!(
            "# Keep this exact saved manifest.\nschema_version = 1\n\
             [project]\nname = \"Recovery fixture\"\nsource_universe = \"{UNIVERSE}\"\n\
             native_document_identity = \"project_location\"\n\
             [authoring]\nstate = \"ready\"\nschema_bundle = \"schema.json\"\n"
        )
        .into_bytes();
        let future_manifest = parse_ready(&old_manifest)
            .unwrap()
            .to_canonical_toml_bytes()
            .unwrap();
        let old_catalog = ComponentSchemaCatalog {
            components: vec![schema(false)],
            ..Default::default()
        };
        let mut next_catalog = ComponentSchemaCatalog::successor_of(&old_catalog).unwrap();
        next_catalog.components.push(schema(true));
        let old_bundle = bundle(old_catalog.clone(), None);
        let next_bundle = bundle(next_catalog, Some(old_catalog));
        let members = vec![
            member(
                "scenes/active.scene.json",
                SourceDocumentKind::Scene,
                105,
                109,
            ),
            member(
                "prefabs/enemy.prefab.ron",
                SourceDocumentKind::Prefab,
                7,
                13,
            ),
            MemberFixture {
                path: "scenes/unaffected.scene.json".to_owned(),
                key: key("scenes/unaffected.scene.json", SourceDocumentKind::Scene),
                encoding: RecoverySourceEncoding::Json,
                old: SceneDocument::default()
                    .to_json_string()
                    .unwrap()
                    .into_bytes(),
                working: SceneDocument::default()
                    .to_json_string()
                    .unwrap()
                    .into_bytes(),
                next: SceneDocument::default()
                    .to_json_string()
                    .unwrap()
                    .into_bytes(),
            },
        ];
        let missing_sidecars = members
            .iter()
            .map(|member| format!("{}.meta", member.path))
            .collect();
        Self {
            old_manifest,
            future_manifest,
            old_bundle,
            next_bundle,
            component: ComponentTypeId::new(COMPONENT),
            field: ComponentFieldId::new("health"),
            selector: ComponentPropertyPath::new(ComponentFieldId::new("health")).with_steps([
                ComponentPropertyStep::Member(ComponentFieldId::new("milli")),
            ]),
            members,
            metadata: Vec::new(),
            roots: vec![
                ("assets".to_owned(), Some(String::new())),
                ("scenes".to_owned(), None),
                ("prefabs".to_owned(), None),
            ],
            missing_sidecars,
        }
    }

    fn prepare(
        &self,
        limits: SourceUpgradeRecoveryLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<SourceUpgradeRecovery, SourceUpgradeRecoveryError> {
        let members: Vec<_> = self
            .members
            .iter()
            .map(|member| SourceUpgradeRecoveryMemberInput {
                path: &member.path,
                key: &member.key,
                encoding: member.encoding,
                explicit_native: false,
                old_disk: &member.old,
                reviewed_working: &member.working,
                successor: &member.next,
            })
            .collect();
        let metadata: Vec<_> = self
            .metadata
            .iter()
            .map(|(path, bytes)| SourceUpgradeRecoveryMetadataInput {
                path,
                observed_bytes: bytes,
            })
            .collect();
        let roots: Vec<_> = self
            .roots
            .iter()
            .map(|(path, ancestor)| SourceUpgradeRecoveryRootInput {
                path,
                missing_ancestor: ancestor.as_deref(),
            })
            .collect();
        let missing: Vec<_> = self.missing_sidecars.iter().map(String::as_str).collect();
        SourceUpgradeRecovery::prepare(
            SourceUpgradeRecoveryInput {
                selected_profile: None,
                old_ready_manifest: &self.old_manifest,
                future_ready_manifest: &self.future_manifest,
                bundle_path: "schema.json",
                old_bundle: &self.old_bundle,
                successor_bundle: &self.next_bundle,
                component: &self.component,
                field: &self.field,
                successor_property: &self.selector,
                members: &members,
                metadata: &metadata,
                roots: &roots,
                missing_sidecars: &missing,
            },
            limits,
            budget,
        )
    }

    fn package(&self) -> SourceUpgradeRecovery {
        self.prepare(SourceUpgradeRecoveryLimits::default(), &mut budget())
            .unwrap()
    }

    fn use_stable_metadata(&mut self) {
        let mut manifest = parse_ready(&self.old_manifest).unwrap();
        manifest.project.native_document_identity = NativeDocumentIdentity::StableMetadata;
        self.old_manifest = manifest.to_canonical_toml_bytes().unwrap();
        self.future_manifest = self.old_manifest.clone();
        self.missing_sidecars.clear();
        self.metadata = self
            .members
            .iter_mut()
            .enumerate()
            .map(|(index, member)| {
                let id: StableAssetId = format!("ad322a15-01a2-4c04-847f-{:012}", index + 1)
                    .parse()
                    .unwrap();
                let kind = match member.key.kind() {
                    SourceDocumentKind::Scene => AssetSourceKind::Scene,
                    SourceDocumentKind::Prefab => AssetSourceKind::Prefab,
                };
                member.key = SourceDocumentKey::stable_asset(id, member.key.kind());
                let metadata = AssetMeta::new(
                    AssetMetaIdentity::new(
                        id,
                        AssetPath::new(member.path.split_once('/').unwrap().1).unwrap(),
                        kind,
                    ),
                    (),
                );
                (
                    format!("{}.meta", member.path),
                    metadata.to_json_string().unwrap().into_bytes(),
                )
            })
            .collect();
    }
}

fn budget() -> AuthoringProjectAdmissionBudget {
    AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default())
}

#[test]
fn sharing_admits_exact_control_storage_without_recopying_or_repeating_work() {
    let fixture = Fixture::new();
    let mut shared = budget();
    shared.charge_source_bytes(17, 23, 41, 137).unwrap();
    let package = fixture
        .prepare(SourceUpgradeRecoveryLimits::default(), &mut shared)
        .unwrap();
    let before = shared.usage();
    let encoded = package.encoded_bytes().to_vec();
    let encoded_pointer = package.encoded_bytes().as_ptr();
    let operation = package.digest();
    let control_bytes = 4 * size_of::<usize>();
    assert!(before.retained_candidate_bytes() > 137);
    assert!(before.decode_work() > 41);
    shared.limits.retained_candidate_bytes =
        ByteLimit::new(before.retained_candidate_bytes() + control_bytes).unwrap();

    let package = package.into_shared(&mut shared).unwrap();

    assert_eq!(package.encoded_bytes(), encoded);
    assert_eq!(package.encoded_bytes().as_ptr(), encoded_pointer);
    assert_eq!(package.digest(), operation);
    assert_eq!(Arc::strong_count(&package), 1);
    let mut expected = before;
    expected.retained_candidate_bytes += control_bytes;
    assert_eq!(shared.usage(), expected);
}

#[test]
fn sharing_refusal_returns_the_original_package_and_refunds_only_failed_control_storage() {
    let fixture = Fixture::new();
    let mut shared = budget();
    shared.charge_source_bytes(17, 23, 41, 137).unwrap();
    let mut package = fixture
        .prepare(SourceUpgradeRecoveryLimits::default(), &mut shared)
        .unwrap();
    let before = shared.usage();
    let encoded = package.encoded_bytes().to_vec();
    let encoded_pointer = package.encoded_bytes().as_ptr();
    let payload = serde_json::to_value(&package.payload).unwrap();
    let operation = package.digest();
    let control_bytes = 4 * size_of::<usize>();
    let maximum = before.retained_candidate_bytes() + control_bytes - 1;
    shared.limits.retained_candidate_bytes = ByteLimit::new(maximum).unwrap();

    for _ in 0..2 {
        let failure = package.into_shared(&mut shared).unwrap_err();
        assert!(matches!(
            failure.error,
            SourceUpgradeRecoveryError::Admission(
                AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
                    maximum: actual
                }
            ) if actual == maximum
        ));
        assert_eq!(failure.recovery.encoded_bytes(), encoded);
        assert_eq!(failure.recovery.encoded_bytes().as_ptr(), encoded_pointer);
        assert_eq!(
            serde_json::to_value(&failure.recovery.payload).unwrap(),
            payload
        );
        assert_eq!(failure.recovery.digest(), operation);
        assert_eq!(shared.usage(), before);
        package = failure.recovery;
    }

    shared.limits.retained_candidate_bytes =
        ByteLimit::new(before.retained_candidate_bytes() + control_bytes).unwrap();
    let package = package.into_shared(&mut shared).unwrap();
    assert_eq!(package.encoded_bytes().as_ptr(), encoded_pointer);
    assert_eq!(package.digest(), operation);
    assert_eq!(serde_json::to_value(&package.payload).unwrap(), payload);
    assert_eq!(Arc::strong_count(&package), 1);
    let mut expected = before;
    expected.retained_candidate_bytes += control_bytes;
    assert_eq!(shared.usage(), expected);
}

fn key(path: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        UNIVERSE.parse().unwrap(),
        AssetSourcePath::new(path).unwrap(),
        kind,
    )
}

fn bundle(current: ComponentSchemaCatalog, previous: Option<ComponentSchemaCatalog>) -> Vec<u8> {
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap(),
        current,
        previous,
    )])
    .unwrap()
    .to_json_string()
    .unwrap()
    .into_bytes()
}

fn schema(next: bool) -> ComponentSchema {
    let shape = if next {
        ComponentValueShape::Record {
            members: vec![
                ComponentShapeMember::new("milli", ComponentValueKind::I64)
                    .with_id(ComponentFieldId::new("milli")),
            ],
        }
    } else {
        ComponentValueShape::scalar(ComponentValueKind::I64)
    };
    ComponentSchema::new(
        ComponentTypeId::new(COMPONENT),
        "Enemy stats",
        ComponentSchemaVersion(if next { 2 } else { 1 }),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("health"),
            "Health",
            ComponentFieldPath::from_fields(["wire_health"]),
            shape,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("armor"),
            "Armor",
            ComponentFieldPath::from_fields(["armor"]),
            ComponentValueKind::I64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ])
}

fn document(health: i64, next: bool) -> SceneDocument {
    let mut entity = SceneEntityRecord::new(SceneEntityId::new("enemy").unwrap());
    let health = if next {
        ComponentValue::map([("milli", ComponentValue::I64(health))])
    } else {
        ComponentValue::I64(health)
    };
    entity.components.insert(
        ComponentTypeId::new(COMPONENT),
        SceneComponentRecord::new(
            ComponentSchemaVersion(if next { 2 } else { 1 }),
            ComponentValue::map([("wire_health", health), ("armor", ComponentValue::I64(7))]),
        )
        .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
    );
    SceneDocument::from_ordered_roots([entity])
}

fn member(path: &str, kind: SourceDocumentKind, saved: i64, working: i64) -> MemberFixture {
    let encoding = if path.ends_with(".ron") {
        RecoverySourceEncoding::Ron
    } else {
        RecoverySourceEncoding::Json
    };
    let encode = |health, next| {
        let document = document(health, next);
        match kind {
            SourceDocumentKind::Scene => document.to_json_string().unwrap().into_bytes(),
            SourceDocumentKind::Prefab => {
                PrefabDocument::new(document.placements, document.entities)
                    .to_ron_string()
                    .unwrap()
                    .into_bytes()
            }
        }
    };
    MemberFixture {
        path: path.to_owned(),
        key: key(path, kind),
        encoding,
        old: encode(saved, false),
        working: encode(working, false),
        next: encode(working * 1000, true),
    }
}

fn decode(bytes: &[u8]) -> Result<SourceUpgradeRecovery, SourceUpgradeRecoveryError> {
    SourceUpgradeRecovery::decode(bytes, SourceUpgradeRecoveryLimits::default(), &mut budget())
}

fn rewritten(
    package: &SourceUpgradeRecovery,
    edit: impl FnOnce(&mut serde_json::Value),
) -> Vec<u8> {
    let mut value: serde_json::Value = serde_json::from_slice(package.encoded_bytes()).unwrap();
    edit(&mut value);
    serde_json::to_vec(&value).unwrap()
}

#[test]
fn ordinary_large_json_and_ron_strings_roundtrip_under_default_shared_limits() {
    let mut fixture = Fixture::new();
    for member in &mut fixture.members[..2] {
        let native = |bytes: &[u8]| match member.key.kind() {
            SourceDocumentKind::Scene => SceneDocumentCandidate::decode_json_bytes(bytes)
                .unwrap()
                .into_composition_source()
                .unwrap(),
            SourceDocumentKind::Prefab => {
                let prefab = PrefabDocumentCandidate::decode_ron_bytes(bytes)
                    .unwrap()
                    .into_composition_source()
                    .unwrap();
                SceneDocument {
                    placements: prefab.placements,
                    entities: prefab.entities,
                }
            }
        };
        let encode = |bytes: &[u8]| {
            let mut source = native(bytes);
            source.entities[0].components.insert(
                ComponentTypeId::new("unavailable.Notes"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion(1),
                    ComponentValue::String("n".repeat(128 * 1024)),
                ),
            );
            match member.key.kind() {
                SourceDocumentKind::Scene => source.to_json_string().unwrap().into_bytes(),
                SourceDocumentKind::Prefab => {
                    PrefabDocument::new(source.placements, source.entities)
                        .to_ron_string()
                        .unwrap()
                        .into_bytes()
                }
            }
        };
        let old = encode(&member.old);
        let working = encode(&member.working);
        let next = encode(&member.next);
        member.old = old;
        member.working = working;
        member.next = next;
    }
    let mut shared = budget();
    let package = fixture
        .prepare(SourceUpgradeRecoveryLimits::default(), &mut shared)
        .unwrap();
    assert!(package.encoded_bytes().len() > 768 * 1024);
    let recovered = decode(package.encoded_bytes()).unwrap();
    assert_eq!(recovered.payload, package.payload);
    assert_eq!(recovered.digest(), package.digest());
    let mut padded = package.encoded_bytes().to_vec();
    padded.resize(padded.len() + 192 * 1024, b' ');
    assert_eq!(decode(&padded).unwrap().payload, package.payload);
}

#[test]
fn adaptive_bundle_budget_admits_large_default_text_without_counting_bytes_as_records() {
    let mut previous = ComponentSchemaCatalog {
        components: vec![schema(false)],
        ..Default::default()
    };
    let notes = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("notes"),
        "Notes",
        ComponentFieldPath::from_fields(["notes"]),
        ComponentValueKind::String,
        ComponentValue::String("n".repeat(128 * 1024)),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING);
    previous.components[0].fields.push(notes.clone());
    let mut next = ComponentSchemaCatalog::successor_of(&previous).unwrap();
    next.components.push(schema(true));
    next.components[0].fields.push(notes);
    let mut fixture = Fixture::new();
    fixture.old_bundle = bundle(previous.clone(), None);
    fixture.next_bundle = bundle(next, Some(previous));
    let package = fixture.package();
    assert!(package.bundle().old_disk.bytes().len() > 128 * 1024);
    assert_eq!(
        decode(package.encoded_bytes()).unwrap().payload,
        package.payload
    );
}

#[test]
fn adaptive_shapes_constrain_real_package_bundle_and_source_decoders() {
    let package = Fixture::new().package();
    let limits = SourceUpgradeRecoveryLimits::default();
    let record = [
        size_of::<RecoveryPayload>(),
        size_of::<RecoveryMember>(),
        size_of::<RecoveryMetadata>(),
        size_of::<RecoveryRoot>(),
        size_of::<ComponentPropertyStep>(),
    ]
    .into_iter()
    .max()
    .unwrap();
    let mut shared = budget();
    shared.limits.retained_candidate_bytes = ByteLimit::new(
        package.encoded_bytes().len() * 8 + OWNER_FIXED_BYTES + record + 192 + 32 + 64,
    )
    .unwrap();
    let (_, shape) =
        package_decode_reservation(package.encoded_bytes().len(), limits.shape, &shared).unwrap();
    assert_eq!(shape.nodes().get(), 1);
    assert!(matches!(
        SourceUpgradeRecovery::decode(package.encoded_bytes(), limits, &mut shared),
        Err(SourceUpgradeRecoveryError::Shape(
            SerdeShapeError::NodeLimitExceeded { .. }
        ))
    ));

    let encoded = package.bundle().old_disk.bytes();
    let owner_limits = AuthoringSchemaBundleLimits::default();
    let node = [
        size_of::<ComponentSchema>(),
        size_of::<ComponentFieldSchema>(),
        size_of::<ComponentShapeMember>(),
        size_of::<ComponentValue>(),
        192,
    ]
    .into_iter()
    .sum::<usize>()
        * 6;
    let mut shared = budget();
    shared.limits.retained_candidate_bytes =
        ByteLimit::new(encoded.len() * 24 + OWNER_FIXED_BYTES + node).unwrap();
    let (_, shape) = bundle_decode_reservation(encoded.len(), owner_limits, &shared).unwrap();
    assert_eq!(shape.nodes().get(), 1);
    assert!(
        AuthoringSchemaBundle::from_json_bytes_with_limits(encoded, owner_limits.with_shape(shape))
            .is_err()
    );

    let member = package
        .members()
        .iter()
        .find(|member| member.path.ends_with("active.scene.json"))
        .unwrap();
    let shape = limits.scene.shape();
    let scene = limits.scene.with_shape(SerdeShapeLimits::new(
        shape.depth(),
        ItemLimit::new(1).unwrap(),
        ItemLimit::new(1).unwrap(),
        shape.string_bytes(),
        shape.total_string_bytes(),
    ));
    assert!(matches!(
        validate_source(member, member.old_disk.bytes(), false, scene, &mut budget()),
        Err(SourceUpgradeRecoveryError::SourceInvalid)
    ));
}

#[test]
fn adaptive_source_reservation_uses_remaining_capacity_and_refunds_json_and_ron_scratch() {
    let package = Fixture::new().package();
    let limits = SceneFileLimits::default();
    for member in package
        .members()
        .iter()
        .filter(|member| !member.path.contains("unaffected"))
    {
        let encoded = member.old_disk.bytes();
        let broad = reserve_source_decode(
            encoded.len(),
            limits.encoded_bytes().get(),
            0,
            limits,
            &mut budget(),
        )
        .unwrap();
        let maximum = broad.bytes / 2;
        let mut previous_nodes = broad.limits.shape().nodes().get();
        for resident in [0, maximum / 4] {
            let mut shared = budget();
            shared.limits.retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
            shared.charge_source_bytes(0, 0, 0, resident).unwrap();
            let narrowed = reserve_source_decode(
                encoded.len(),
                limits.encoded_bytes().get(),
                0,
                limits,
                &mut shared,
            )
            .unwrap();
            assert!(narrowed.bytes <= maximum - resident);
            assert!(narrowed.limits.shape().nodes().get() < previous_nodes);
            previous_nodes = narrowed.limits.shape().nodes().get();
            match member.encoding {
                RecoverySourceEncoding::Json => {
                    assert!(
                        SceneDocumentCandidate::decode_json_bytes_with_limits(
                            encoded,
                            narrowed.limits
                        )
                        .is_ok()
                    );
                }
                RecoverySourceEncoding::Ron => {
                    assert!(
                        PrefabDocumentCandidate::decode_ron_bytes_with_limits(
                            encoded,
                            narrowed.limits
                        )
                        .is_ok()
                    );
                }
            }
            shared
                .reconcile_retained_reservation(narrowed.bytes, 0)
                .unwrap();
            assert_eq!(shared.usage().retained_candidate_bytes(), resident);
            let work = shared.usage().decode_work();
            validate_source(member, encoded, false, limits, &mut shared).unwrap();
            assert_eq!(shared.usage().retained_candidate_bytes(), resident);
            assert!(shared.usage().decode_work() > work);
        }
    }
}

#[test]
fn canonical_source_comparison_requires_its_own_exact_work_and_releases_only_temporary_trees() {
    let package = Fixture::new().package();
    let limits = SceneFileLimits::default();
    for member in package
        .members()
        .iter()
        .filter(|member| !member.path.contains("unaffected"))
    {
        let encoded = member.successor.bytes();
        let mut decoded = budget();
        validate_source(member, encoded, false, limits, &mut decoded).unwrap();
        assert_eq!(decoded.usage().retained_candidate_bytes(), 0);
        let mut measured = budget();
        validate_source(member, encoded, true, limits, &mut measured).unwrap();
        assert!(measured.usage().decode_work() > decoded.usage().decode_work());
        assert_eq!(measured.usage().retained_candidate_bytes(), 0);
        assert_eq!(
            measured.usage().decoded_bytes(),
            decoded.usage().decoded_bytes()
        );

        for (maximum, accepted) in [
            (measured.usage().decode_work(), true),
            (measured.usage().decode_work() - 1, false),
            (decoded.usage().decode_work(), false),
        ] {
            let mut shared = budget();
            shared.charge_source_bytes(0, 0, 0, 321).unwrap();
            shared.limits.decode_work = ItemLimit::new(maximum).unwrap();
            let result = validate_source(member, encoded, true, limits, &mut shared);
            if accepted {
                result.unwrap();
            } else {
                assert!(matches!(
                    result,
                    Err(SourceUpgradeRecoveryError::Admission(
                        AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
                    ))
                ));
            }
            assert_eq!(shared.usage().decode_work(), measured.usage().decode_work());
            assert_eq!(shared.usage().retained_candidate_bytes(), 321);
            assert_eq!(shared.usage().retained_open_handles(), 0);
        }

        let mut noncanonical = encoded.to_vec();
        noncanonical.push(b'\n');
        let mut shared = budget();
        shared.charge_source_bytes(0, 0, 0, 321).unwrap();
        for _ in 0..2 {
            let work = shared.usage().decode_work();
            assert!(matches!(
                validate_source(member, &noncanonical, true, limits, &mut shared),
                Err(SourceUpgradeRecoveryError::NonCanonicalSuccessor)
            ));
            assert!(shared.usage().decode_work() > work);
            assert_eq!(shared.usage().retained_candidate_bytes(), 321);
            assert_eq!(shared.usage().retained_open_handles(), 0);
        }
    }
}

#[test]
fn deterministic_roundtrip_retains_saved_dirty_and_native_json_ron_bytes_separately() {
    let mut fixture = Fixture::new();
    let package = fixture.package();
    fixture.members.reverse();
    fixture.roots.reverse();
    fixture.missing_sidecars.reverse();
    let reordered = fixture.package();
    assert_eq!(package.encoded_bytes(), reordered.encoded_bytes());
    assert_eq!(package.digest(), reordered.digest());
    let recovered = decode(package.encoded_bytes()).unwrap();
    assert_eq!(recovered.encoded_bytes(), package.encoded_bytes());
    assert_eq!(recovered.payload, package.payload);
    assert_eq!(recovered.source_universe(), UNIVERSE.parse().unwrap());
    assert_eq!(recovered.manifest().old_disk.bytes(), fixture.old_manifest);
    assert_eq!(
        recovered.manifest().successor.bytes(),
        fixture.future_manifest
    );
    assert_eq!(recovered.bundle().old_disk.bytes(), fixture.old_bundle);
    assert_eq!(recovered.bundle().successor.bytes(), fixture.next_bundle);
    for member in recovered.members() {
        let original = fixture
            .members
            .iter()
            .find(|original| original.path == member.path)
            .unwrap();
        assert_eq!(member.old_disk.bytes(), original.old);
        assert_eq!(member.reviewed_working.bytes(), original.working);
        assert_eq!(member.successor.bytes(), original.next);
        if !member.path.contains("unaffected") {
            assert_ne!(member.old_disk.digest(), member.reviewed_working.digest());
        }
    }
    assert!(recovered.metadata().is_empty());
    assert_eq!(recovered.missing_sidecars().len(), 3);
    assert!(
        recovered
            .roots()
            .iter()
            .any(|root| matches!(&root.observation,
        RecoveryRootObservation::Missing { ancestor_path } if ancestor_path.is_empty()))
    );
    assert_eq!(
        recovered.payload.history_policy,
        RecoveryHistoryPolicy::SaveReviewedWorkingCopiesAndStartNewHistory
    );
}

#[test]
fn explicit_native_admission_is_required_and_roundtrips_both_values() {
    let package = Fixture::new().package();
    for explicit_native in [false, true] {
        let encoded = rewritten(&package, |value| {
            value["payload"]["members"][0]["explicit_native"] = explicit_native.into();
        });
        let recovered = decode(&encoded).unwrap();
        assert_eq!(recovered.members()[0].explicit_native, explicit_native);
        assert!(!recovered.members()[1].explicit_native);
    }
    let missing = rewritten(&package, |value| {
        value["payload"]["members"][0]
            .as_object_mut()
            .unwrap()
            .remove("explicit_native");
    });
    assert!(matches!(
        decode(&missing),
        Err(SourceUpgradeRecoveryError::Syntax),
    ));
}

#[test]
fn selected_profile_roundtrips_once_and_must_resolve_from_the_full_ready_manifest() {
    let mut fixture = Fixture::new();
    fixture
        .old_manifest
        .extend_from_slice(b"\n[profiles.desktop.paths]\nscenes = 'scenes'\n");
    fixture.future_manifest = parse_ready(&fixture.old_manifest)
        .unwrap()
        .to_canonical_toml_bytes()
        .unwrap();
    let package = fixture.package();
    assert_eq!(package.selected_profile(), None);
    let missing = rewritten(&package, |value| {
        value["payload"]
            .as_object_mut()
            .unwrap()
            .remove("selected_profile");
    });
    assert!(matches!(
        decode(&missing),
        Err(SourceUpgradeRecoveryError::Syntax)
    ));
    // Decode both sides through the same owner: its measured Vec capacities can differ
    // from the producer's exact-capacity tables independently of the selected-profile text.
    let baseline = decode(package.encoded_bytes()).unwrap();
    for selected in [None, Some("desktop")] {
        let encoded = rewritten(&package, |value| {
            value["payload"]["selected_profile"] = serde_json::json!(selected);
        });
        let recovered = decode(&encoded).unwrap();
        assert_eq!(recovered.selected_profile(), selected);
        let missing = rewritten(&recovered, |value| {
            value["payload"]
                .as_object_mut()
                .unwrap()
                .remove("selected_profile");
        });
        assert!(matches!(
            decode(&missing),
            Err(SourceUpgradeRecoveryError::Syntax)
        ));
        assert_eq!(
            payload_retained_bytes(&recovered.payload).unwrap(),
            payload_retained_bytes(&baseline.payload).unwrap() + selected.map_or(0, str::len),
        );
        assert_eq!(
            String::from_utf8(encoded)
                .unwrap()
                .matches("\"selected_profile\"")
                .count(),
            1,
        );
    }
    let invalid = rewritten(&package, |value| {
        value["payload"]["selected_profile"] = serde_json::json!("missing");
    });
    let mut shared = budget();
    let before = shared.usage();
    assert!(matches!(
        SourceUpgradeRecovery::decode(
            &invalid,
            SourceUpgradeRecoveryLimits::default(),
            &mut shared,
        ),
        Err(SourceUpgradeRecoveryError::ConfigurationMismatch),
    ));
    assert_eq!(
        shared.usage().retained_candidate_bytes(),
        before.retained_candidate_bytes()
    );
    assert!(shared.usage().decode_work() > before.decode_work());
}

#[test]
fn actual_digest_classification_never_double_converts_already_published_values() {
    let fixture = Fixture::new();
    let package = fixture.package();
    let member = package
        .members()
        .iter()
        .find(|member| member.path.ends_with("active.scene.json"))
        .unwrap();
    assert_eq!(member.classify(None), RecoveryMemberState::Missing);
    assert_eq!(
        member.classify(Some(ContentDigest::of_bytes(member.old_disk.bytes()))),
        RecoveryMemberState::Old
    );
    // An unsaved reviewed working copy is not the original expected disk input.
    assert_eq!(
        member.classify(Some(ContentDigest::of_bytes(
            member.reviewed_working.bytes()
        ))),
        RecoveryMemberState::Conflict
    );
    let new_bytes = member.successor.bytes();
    for _ in 0..3 {
        assert_eq!(
            member.classify(Some(ContentDigest::of_bytes(new_bytes))),
            RecoveryMemberState::New
        );
        let native = SceneDocumentCandidate::decode_json_bytes(new_bytes)
            .unwrap()
            .into_composition_source()
            .unwrap();
        let value = &native.entities[0].components[&fixture.component].value;
        assert_eq!(
            value,
            &ComponentValue::map([
                (
                    "wire_health",
                    ComponentValue::map([("milli", ComponentValue::I64(109000))])
                ),
                ("armor", ComponentValue::I64(7)),
            ])
        );
    }
    assert_eq!(
        member.classify(Some(ContentDigest::of_bytes(b"external edit"))),
        RecoveryMemberState::Conflict
    );
    let unchanged = package
        .members()
        .iter()
        .find(|member| member.path.contains("unaffected"))
        .unwrap();
    assert_eq!(
        unchanged.classify(Some(ContentDigest::of_bytes(unchanged.successor.bytes()))),
        RecoveryMemberState::Unchanged
    );
    assert_eq!(
        package.bundle().classify(Some(ContentDigest::of_bytes(
            package.bundle().successor.bytes()
        ))),
        RecoveryMemberState::New
    );
}

#[test]
fn pending_requires_exact_location_package_bytes_and_digest() {
    let package = Fixture::new().package();
    let reference = ProjectAuthoringRecovery::new(
        ProjectPath::new(".nara/upgrades/one.json").unwrap(),
        package.digest(),
    )
    .unwrap();
    let load = |path, bytes| {
        validate_pending_bytes(
            &reference,
            path,
            bytes,
            SourceUpgradeRecoveryLimits::default().encoded_bytes,
            &mut budget(),
        )
    };
    assert!(matches!(
        load(".nara/upgrades/one.json", None),
        Err(RecoveryStorageError::PackageMissing)
    ));
    assert!(matches!(
        load(".nara/upgrades/other.json", Some(package.encoded_bytes())),
        Err(RecoveryStorageError::PackagePathMismatch)
    ));
    assert!(matches!(
        load(".nara/upgrades/one.json", Some(b"{}")),
        Err(RecoveryStorageError::PackageDigestMismatch)
    ));
    load(".nara/upgrades/one.json", Some(package.encoded_bytes())).unwrap();
    for (maximum, accepted) in [
        (package.encoded_bytes().len(), true),
        (package.encoded_bytes().len() - 1, false),
    ] {
        let mut shared = budget();
        let result = validate_pending_bytes(
            &reference,
            ".nara/upgrades/one.json",
            Some(package.encoded_bytes()),
            ByteLimit::new(maximum).unwrap(),
            &mut shared,
        );
        if accepted {
            result.unwrap();
            assert_eq!(shared.usage().decode_work(), package.encoded_bytes().len());
        } else {
            assert!(matches!(
                result,
                Err(RecoveryStorageError::PackageEncodedBytesExceeded)
            ));
            assert_eq!(shared.usage().decode_work(), 0);
        }
        assert_eq!(shared.usage().retained_candidate_bytes(), 0);
    }
    let mut shared = budget();
    let recovered = SourceUpgradeRecovery::decode(
        package.encoded_bytes(),
        SourceUpgradeRecoveryLimits::default(),
        &mut shared,
    )
    .unwrap();
    validate_storage_path(".nara/upgrades/one.json", &recovered, &mut shared).unwrap();
    assert_eq!(recovered.digest(), package.digest());
    assert!(matches!(
        validate_storage_path("schema.json", &recovered, &mut shared),
        Err(RecoveryStorageError::RecoveryPathConflict)
    ));
}

#[test]
fn pending_paths_share_case_and_ancestor_conflict_rules_with_storage() {
    let package = Fixture::new().package();
    for path in [
        "SCHEMA.json",
        "NARA.toml",
        "schema.json/recovery.json",
        "nara.toml/recovery.json",
        "SCENES/recovery.json",
        "PREFABS",
        "scenes/active.scene.json.meta/recovery.json",
    ] {
        assert!(
            matches!(
                validate_storage_path(path, &package, &mut budget()),
                Err(RecoveryStorageError::RecoveryPathConflict)
            ),
            "{path}"
        );
    }
    let mut shared = budget();
    let retained = shared.usage().retained_candidate_bytes();
    for path in [
        ".nara/upgrades/one.json",
        "schema.json.sibling",
        "scenes-sibling/recovery.json",
    ] {
        validate_storage_path(path, &package, &mut shared).unwrap();
        assert_eq!(shared.usage().retained_candidate_bytes(), retained);
    }
    for path in ["SCHEMA.json", "schema.json/recovery.json"] {
        let work = shared.usage().decode_work();
        assert!(matches!(
            validate_storage_path(path, &package, &mut shared),
            Err(RecoveryStorageError::RecoveryPathConflict)
        ));
        assert_eq!(shared.usage().retained_candidate_bytes(), retained);
        assert!(shared.usage().decode_work() > work);
    }
}

#[test]
fn semantic_decode_and_storage_path_refusals_release_only_temporary_storage() {
    let package = Fixture::new().package();
    let semantic_invalid = rewritten(&package, |value| {
        value["payload"]["manifest"]["path"] = "wrong.toml".into();
    });
    let path = "SCHEMA.json";
    let mut shared = budget();
    shared.charge_source_bytes(0, 0, 0, 321).unwrap();
    let retained = shared.usage().retained_candidate_bytes();
    for _ in 0..3 {
        let work = shared.usage().decode_work();
        assert!(matches!(
            SourceUpgradeRecovery::decode(
                &semantic_invalid,
                SourceUpgradeRecoveryLimits::default(),
                &mut shared
            ),
            Err(SourceUpgradeRecoveryError::ConfigurationMismatch)
        ));
        assert_eq!(shared.usage().retained_candidate_bytes(), retained);
        assert!(shared.usage().decode_work() > work);
        let work = shared.usage().decode_work();
        assert!(matches!(
            validate_storage_path(path, &package, &mut shared),
            Err(RecoveryStorageError::RecoveryPathConflict)
        ));
        assert_eq!(shared.usage().retained_candidate_bytes(), retained);
        assert!(shared.usage().decode_work() > work);
    }
    let mut fixture = Fixture::new();
    fixture.members[0].next = b"invalid source".to_vec();
    let work = shared.usage().decode_work();
    assert!(
        fixture
            .prepare(SourceUpgradeRecoveryLimits::default(), &mut shared)
            .is_err()
    );
    assert_eq!(shared.usage().retained_candidate_bytes(), retained);
    assert!(shared.usage().decode_work() > work);
}

#[test]
fn rejects_unknown_duplicate_fields_versions_and_non_data_members() {
    let package = Fixture::new().package();
    for bytes in [
        rewritten(&package, |value| {
            value["payload"]["unexpected"] = true.into()
        }),
        rewritten(&package, |value| {
            value["payload"]["members"][0]["callback"] = "run_conversion".into()
        }),
        rewritten(&package, |value| {
            value["payload"]["members"][0]["key"]["kind"] = "script".into()
        }),
        rewritten(&package, |value| {
            value["payload"]["history_policy"] = "replay_old_history".into()
        }),
    ] {
        assert!(decode(&bytes).is_err());
    }
    let duplicate = String::from_utf8(package.encoded_bytes().to_vec())
        .unwrap()
        .replacen(
            "\"payload\": {",
            "\"payload\": {\"source_universe\": \"421112bd-693a-4e6b-85bc-318d567b2f21\",",
            1,
        );
    assert!(matches!(
        decode(duplicate.as_bytes()),
        Err(SourceUpgradeRecoveryError::Shape(
            SerdeShapeError::DuplicateMapKey
        ))
    ));
    let future = rewritten(&package, |value| value["format_version"] = 2.into());
    assert!(matches!(
        decode(&future),
        Err(SourceUpgradeRecoveryError::Contract(_))
    ));
    let kind = rewritten(&package, |value| value["kind"] = "scene".into());
    assert!(matches!(
        decode(&kind),
        Err(SourceUpgradeRecoveryError::Contract(_))
    ));
    let engine = rewritten(&package, |value| {
        value["engine_min_version"] = "999.0.0".into()
    });
    assert!(matches!(
        decode(&engine),
        Err(SourceUpgradeRecoveryError::Contract(_))
    ));
}

#[test]
fn paths_keys_kind_and_observation_role_conflicts_refuse() {
    let package = Fixture::new().package();
    for path in [
        "../outside.scene.json",
        "/absolute.scene.json",
        "C:/outside.scene.json",
        "scenes\\outside.scene.json",
        "scenes//outside.scene.json",
    ] {
        let bytes = rewritten(&package, |value| {
            value["payload"]["members"][0]["path"] = path.into()
        });
        assert!(matches!(
            decode(&bytes),
            Err(SourceUpgradeRecoveryError::PathInvalid)
        ));
    }
    let duplicate_path = rewritten(&package, |value| {
        value["payload"]["members"][1]["path"] = value["payload"]["members"][0]["path"].clone()
    });
    assert!(matches!(
        decode(&duplicate_path),
        Err(SourceUpgradeRecoveryError::PathConflict)
    ));
    let duplicate_key = rewritten(&package, |value| {
        value["payload"]["members"][1]["key"] = value["payload"]["members"][0]["key"].clone()
    });
    assert!(matches!(
        decode(&duplicate_key),
        Err(SourceUpgradeRecoveryError::KeyConflict)
    ));
    let wrong_kind = rewritten(&package, |value| {
        value["payload"]["members"][0]["key"]["kind"] = "scene".into()
    });
    assert!(matches!(
        decode(&wrong_kind),
        Err(SourceUpgradeRecoveryError::MemberInvalid { .. })
    ));
    let wrong_universe = rewritten(&package, |value| {
        value["payload"]["source_universe"] = "ad322a15-01a2-4c04-847f-83e4df33f139".into()
    });
    assert!(matches!(
        decode(&wrong_universe),
        Err(SourceUpgradeRecoveryError::KeyConflict)
    ));
    let missing_conflict = rewritten(&package, |value| {
        value["payload"]["missing_sidecars"][0] = value["payload"]["members"][0]["path"].clone()
    });
    assert!(matches!(
        decode(&missing_conflict),
        Err(SourceUpgradeRecoveryError::PathConflict)
    ));
}

#[test]
fn unicode_scene_paths_with_distinct_keys_and_sidecars_follow_inventory_collision_policy() {
    let mut fixture = Fixture::new();
    for (index, path) in [(0, "scenes/Ä.scene.json"), (2, "scenes/ä.scene.json")] {
        fixture.members[index].path = path.to_owned();
        fixture.members[index].key = key(path, SourceDocumentKind::Scene);
    }
    fixture.missing_sidecars = fixture
        .members
        .iter()
        .map(|member| format!("{}.meta", member.path))
        .collect();
    assert_ne!(fixture.members[0].key, fixture.members[2].key);
    assert!(matches!(
        fixture.prepare(SourceUpgradeRecoveryLimits::default(), &mut budget()),
        Err(SourceUpgradeRecoveryError::PathConflict)
    ));

    let package = Fixture::new().package();
    let colliding = rewritten(&package, |value| {
        // The canonical fixture sorts the Prefab first, followed by these two Scenes.
        for (index, path) in [(1, "scenes/Ä.scene.json"), (2, "scenes/ä.scene.json")] {
            value["payload"]["members"][index]["path"] = path.into();
            value["payload"]["members"][index]["key"] =
                serde_json::to_value(key(path, SourceDocumentKind::Scene)).unwrap();
            value["payload"]["missing_sidecars"][index] = format!("{path}.meta").into();
        }
    });
    assert!(matches!(
        decode(&colliding),
        Err(SourceUpgradeRecoveryError::PathConflict)
    ));
}

#[test]
fn unicode_root_collisions_refuse_in_preparation_and_owned_decode() {
    let mut fixture = Fixture::new();
    fixture
        .roots
        .extend([("Ä".to_owned(), None), ("ä".to_owned(), None)]);
    assert!(matches!(
        fixture.prepare(SourceUpgradeRecoveryLimits::default(), &mut budget()),
        Err(SourceUpgradeRecoveryError::RootConflict)
    ));
    let package = Fixture::new().package();
    let colliding = rewritten(&package, |value| {
        let roots = value["payload"]["roots"].as_array_mut().unwrap();
        roots.extend([
            serde_json::json!({"path": "Ä", "observation": "present"}),
            serde_json::json!({"path": "ä", "observation": "present"}),
        ]);
    });
    assert!(matches!(
        decode(&colliding),
        Err(SourceUpgradeRecoveryError::RootConflict)
    ));
}

#[test]
fn expanding_lowercase_paths_and_roots_admit_storage_and_work_before_folding() {
    assert_eq!("İ".to_lowercase(), "i\u{307}");
    assert!("İ".to_lowercase().len() > "İ".len());
    let mut package = Fixture::new().package();
    for (index, path) in [
        (1, "scenes/İ.scene.json"),
        (2, "scenes/i\u{307}.scene.json"),
    ] {
        package.payload.members[index].path = path.to_owned();
        package.payload.members[index].key = key(path, SourceDocumentKind::Scene);
        package.payload.missing_sidecars[index] = format!("{path}.meta");
    }
    let payload = &package.payload;
    let paths = payload.file_paths().count() + payload.missing_sidecars.len();
    let path_bytes: usize = payload
        .file_paths()
        .chain(payload.missing_sidecars.iter().map(String::as_str))
        .map(str::len)
        .sum();
    let path_storage = 3 * path_bytes + 192 * paths;
    let aggregate = aggregate_bytes(payload).unwrap();
    for (maximum, admitted) in [(path_storage, true), (path_storage - 1, false)] {
        let mut shared = budget();
        shared.limits.retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
        let result = validate_payload(payload, SourceUpgradeRecoveryLimits::default(), &mut shared);
        if admitted {
            assert!(matches!(
                result,
                Err(SourceUpgradeRecoveryError::PathConflict)
            ));
            assert_eq!(shared.usage().retained_candidate_bytes(), 0);
        } else {
            assert!(matches!(
                result,
                Err(SourceUpgradeRecoveryError::Admission(
                    AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
                ))
            ));
            assert_eq!(shared.usage().decode_work(), 4 * aggregate + 3 * path_bytes);
        }
    }

    let mut package = Fixture::new().package();
    package.payload.roots.extend([
        RecoveryRoot {
            path: "İ".to_owned(),
            observation: RecoveryRootObservation::Present,
        },
        RecoveryRoot {
            path: "i\u{307}".to_owned(),
            observation: RecoveryRootObservation::Present,
        },
    ]);
    let manifest = parse_ready(package.manifest().old_disk.bytes()).unwrap();
    let root_bytes: usize = package.roots().iter().map(|root| root.path.len()).sum();
    let root_storage = 3 * root_bytes + 192 * package.roots().len();
    for (maximum, admitted) in [(root_storage, true), (root_storage - 1, false)] {
        let mut shared = budget();
        shared.limits.retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
        let result = validate_configurations(&package.payload, &manifest, &mut shared);
        if admitted {
            assert!(matches!(
                result,
                Err(SourceUpgradeRecoveryError::RootConflict)
            ));
            assert_eq!(shared.usage().retained_candidate_bytes(), 0);
        } else {
            assert!(matches!(
                result,
                Err(SourceUpgradeRecoveryError::Admission(
                    AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
                ))
            ));
            assert_eq!(shared.usage().decode_work(), 3 * root_bytes);
        }
    }
}

#[test]
fn source_count_aggregate_encoded_and_shared_budgets_accept_exact_and_refuse_one_short() {
    let fixture = Fixture::new();
    let package = fixture.package();
    let mut limits = SourceUpgradeRecoveryLimits::default();
    limits.encoded_bytes = ByteLimit::new(package.encoded_bytes().len()).unwrap();
    limits.aggregate_bytes = ByteLimit::new(aggregate_bytes(&package.payload).unwrap()).unwrap();
    limits.sources = ItemLimit::new(package.members().len()).unwrap();
    fixture.prepare(limits, &mut budget()).unwrap();
    SourceUpgradeRecovery::decode(package.encoded_bytes(), limits, &mut budget()).unwrap();
    let mut short = limits;
    short.encoded_bytes = ByteLimit::new(limits.encoded_bytes.get() - 1).unwrap();
    assert!(matches!(
        fixture.prepare(short, &mut budget()),
        Err(SourceUpgradeRecoveryError::EncodedBytesExceeded)
    ));
    assert!(matches!(
        SourceUpgradeRecovery::decode(package.encoded_bytes(), short, &mut budget()),
        Err(SourceUpgradeRecoveryError::EncodedBytesExceeded)
    ));
    short = limits;
    short.aggregate_bytes = ByteLimit::new(limits.aggregate_bytes.get() - 1).unwrap();
    assert!(matches!(
        fixture.prepare(short, &mut budget()),
        Err(SourceUpgradeRecoveryError::AggregateBytesExceeded)
    ));
    assert!(matches!(
        SourceUpgradeRecovery::decode(package.encoded_bytes(), short, &mut budget()),
        Err(SourceUpgradeRecoveryError::AggregateBytesExceeded)
    ));
    short = limits;
    short.sources = ItemLimit::new(limits.sources.get() - 1).unwrap();
    assert!(matches!(
        SourceUpgradeRecovery::decode(package.encoded_bytes(), short, &mut budget()),
        Err(SourceUpgradeRecoveryError::SourceCountExceeded)
    ));
    let mut used = budget();
    fixture.prepare(limits, &mut used).unwrap();
    for select in [0, 1, 2] {
        let exact = match select {
            0 => used.usage().aggregate_encoded_bytes(),
            1 => used.decoded_bytes,
            _ => used.decode_work,
        };
        for (maximum, accepted) in [(exact, true), (exact - 1, false)] {
            let mut shared = budget();
            match select {
                0 => shared.limits.aggregate_encoded_bytes = ByteLimit::new(maximum).unwrap(),
                1 => shared.limits.aggregate_decoded_bytes = ByteLimit::new(maximum).unwrap(),
                _ => shared.limits.decode_work = ItemLimit::new(maximum).unwrap(),
            }
            assert_eq!(fixture.prepare(limits, &mut shared).is_ok(), accepted);
        }
    }
}

#[test]
fn raw_shape_admission_precedes_owned_payload_and_source_allocation() {
    let package = Fixture::new().package();
    let mut limits = SourceUpgradeRecoveryLimits::default();
    limits.shape = SerdeShapeLimits::new(
        DepthLimit::new(3).unwrap(),
        limits.shape.nodes(),
        limits.shape.container_items(),
        limits.shape.string_bytes(),
        limits.shape.total_string_bytes(),
    );
    assert!(matches!(
        SourceUpgradeRecovery::decode(package.encoded_bytes(), limits, &mut budget()),
        Err(SourceUpgradeRecoveryError::Shape(
            SerdeShapeError::DepthExceeded { .. }
        ))
    ));
    let invalid = rewritten(&package, |value| {
        value["payload"]["members"][0]["reviewed_working"]["bytes"] = "not a Scene".into();
    });
    limits = SourceUpgradeRecoveryLimits::default();
    limits.sources = ItemLimit::new(1).unwrap();
    // Count admission wins even when the stored source/digest is inadmissible.
    assert!(matches!(
        SourceUpgradeRecovery::decode(&invalid, limits, &mut budget()),
        Err(SourceUpgradeRecoveryError::SourceCountExceeded)
    ));
    limits = SourceUpgradeRecoveryLimits::default();
    limits.aggregate_bytes = ByteLimit::new(1).unwrap();
    assert!(matches!(
        SourceUpgradeRecovery::decode(&invalid, limits, &mut budget()),
        Err(SourceUpgradeRecoveryError::AggregateBytesExceeded)
    ));
}

#[test]
fn actual_bundle_lineage_ready_and_complete_configuration_are_required() {
    let mut fixture = Fixture::new();
    fixture.next_bundle = fixture.old_bundle.clone();
    assert!(matches!(
        fixture.prepare(SourceUpgradeRecoveryLimits::default(), &mut budget()),
        Err(SourceUpgradeRecoveryError::LineageInvalid)
    ));
    let mut fixture = Fixture::new();
    let mut changed = parse_ready(&fixture.future_manifest).unwrap();
    changed.project.name = "Competing project edit".to_owned();
    fixture.future_manifest = changed.to_canonical_toml_bytes().unwrap();
    assert!(matches!(
        fixture.prepare(SourceUpgradeRecoveryLimits::default(), &mut budget()),
        Err(SourceUpgradeRecoveryError::ConfigurationMismatch)
    ));
    let mut fixture = Fixture::new();
    fixture.old_manifest = String::from_utf8(fixture.old_manifest)
        .unwrap()
        .replace("state = \"ready\"\n", "")
        .into_bytes();
    assert!(matches!(
        fixture.prepare(SourceUpgradeRecoveryLimits::default(), &mut budget()),
        Err(SourceUpgradeRecoveryError::ManifestInvalid)
    ));
    let mut fixture = Fixture::new();
    fixture.roots.retain(|(path, _)| path != "assets");
    assert!(matches!(
        fixture.prepare(SourceUpgradeRecoveryLimits::default(), &mut budget()),
        Err(SourceUpgradeRecoveryError::RootConflict)
    ));
    let mut fixture = Fixture::new();
    fixture
        .roots
        .iter_mut()
        .find(|(path, _)| path == "scenes")
        .unwrap()
        .1 = Some(String::new());
    assert!(matches!(
        fixture.prepare(SourceUpgradeRecoveryLimits::default(), &mut budget()),
        Err(SourceUpgradeRecoveryError::RootConflict)
    ));
}

#[test]
fn native_stable_metadata_roundtrips_and_conflicting_identity_or_kind_refuses() {
    let mut fixture = Fixture::new();
    fixture.use_stable_metadata();
    let package = fixture.package();
    let recovered = decode(package.encoded_bytes()).unwrap();
    assert_eq!(recovered.metadata().len(), 3);
    assert!(recovered.missing_sidecars().is_empty());
    for metadata in recovered.metadata() {
        assert_eq!(
            metadata.observed.bytes(),
            fixture
                .metadata
                .iter()
                .find(|(path, _)| path == &metadata.path)
                .unwrap()
                .1
        );
    }
    fixture.members[1].key = SourceDocumentKey::stable_asset(
        match fixture.members[0].key.origin() {
            SourceDocumentOrigin::StableAsset { stable_id } => *stable_id,
            _ => unreachable!(),
        },
        SourceDocumentKind::Prefab,
    );
    assert!(matches!(
        fixture.prepare(SourceUpgradeRecoveryLimits::default(), &mut budget()),
        Err(SourceUpgradeRecoveryError::KeyConflict)
    ));
    let mut fixture = Fixture::new();
    fixture.use_stable_metadata();
    fixture.members[0].key = SourceDocumentKey::stable_asset(
        "fda28046-1e3b-4a26-bd80-bc50c7180fc0".parse().unwrap(),
        SourceDocumentKind::Scene,
    );
    assert!(matches!(
        fixture.prepare(SourceUpgradeRecoveryLimits::default(), &mut budget()),
        Err(SourceUpgradeRecoveryError::MetadataConflict)
    ));
}

#[test]
fn preparation_admits_copy_hash_and_sort_work_before_any_invalid_utf8_input_is_copied() {
    let mut fixture = Fixture::new();
    fixture.members.last_mut().unwrap().working.push(0xff);
    let limits = SourceUpgradeRecoveryLimits::default();
    let mut shared = budget();
    let manifest_work = fixture.old_manifest.len() * 4;
    shared.limits.decode_work = ItemLimit::new(manifest_work).unwrap();
    assert!(matches!(
        fixture.prepare(limits, &mut shared),
        Err(SourceUpgradeRecoveryError::Admission(
            AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
        ))
    ));
    assert!(shared.usage().decode_work() > manifest_work);
    // With sufficient work the same input reaches its actual UTF-8 refusal. No package is
    // returned in either case, and no source or Session is mutated by this data-only owner.
    assert!(matches!(
        fixture.prepare(limits, &mut budget()),
        Err(SourceUpgradeRecoveryError::Utf8)
    ));
}

#[test]
fn failed_preparation_consumes_work_and_exhausts_the_shared_attempt_budget() {
    let mut fixture = Fixture::new();
    fixture.members[0].path = "../outside.scene.json".to_owned();
    let limits = SourceUpgradeRecoveryLimits::default();
    let mut shared = budget();
    assert!(matches!(
        fixture.prepare(limits, &mut shared),
        Err(SourceUpgradeRecoveryError::PathInvalid)
    ));
    let one_attempt = shared.usage().decode_work();
    assert!(
        one_attempt > 0,
        "the old manifest was parsed before the path was refused"
    );
    shared.limits.decode_work = ItemLimit::new(one_attempt * 2).unwrap();
    assert!(matches!(
        fixture.prepare(limits, &mut shared),
        Err(SourceUpgradeRecoveryError::PathInvalid)
    ));
    assert_eq!(shared.usage().decode_work(), one_attempt * 2);
    assert!(matches!(
        fixture.prepare(limits, &mut shared),
        Err(SourceUpgradeRecoveryError::Admission(
            AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
        ))
    ));
    // A valid request cannot obtain a package from the exhausted operation budget either.
    assert!(matches!(
        Fixture::new().prepare(limits, &mut shared),
        Err(SourceUpgradeRecoveryError::Admission(
            AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
        ))
    ));
}

#[test]
fn failed_owned_and_pending_decode_keep_work_charges_and_refund_temporary_storage() {
    let package = Fixture::new().package();
    let invalid = rewritten(&package, |value| {
        value["payload"]["unexpected"] = true.into()
    });
    let path = ".nara/upgrades/invalid.json";
    let reference =
        ProjectAuthoringRecovery::new(ProjectPath::new(path).unwrap(), digest(&invalid)).unwrap();
    let limits = SourceUpgradeRecoveryLimits::default();
    for pending in [false, true] {
        let attempt = |shared: &mut AuthoringProjectAdmissionBudget|
         -> Result<SourceUpgradeRecovery, SourceUpgradeStorageError> {
            if pending {
                validate_pending_bytes(
                    &reference,
                    path,
                    Some(&invalid),
                    limits.encoded_bytes,
                    shared,
                )?;
            }
            SourceUpgradeRecovery::decode(&invalid, limits, shared)
                .map_err(SourceUpgradeStorageError::Recovery)
        };
        let mut shared = budget();
        assert!(matches!(
            attempt(&mut shared),
            Err(SourceUpgradeStorageError::Recovery(
                SourceUpgradeRecoveryError::Syntax
            ))
        ));
        let one_attempt = shared.usage().decode_work();
        assert!(one_attempt > 0);
        assert_eq!(shared.usage().aggregate_encoded_bytes(), invalid.len());
        assert_eq!(shared.usage().retained_candidate_bytes(), 0);
        shared.limits.decode_work = ItemLimit::new(one_attempt * 2).unwrap();
        assert!(matches!(
            attempt(&mut shared),
            Err(SourceUpgradeStorageError::Recovery(
                SourceUpgradeRecoveryError::Syntax
            ))
        ));
        assert_eq!(shared.usage().decode_work(), one_attempt * 2);
        assert_eq!(shared.usage().retained_candidate_bytes(), 0);
        assert!(matches!(
            attempt(&mut shared),
            Err(SourceUpgradeStorageError::Recovery(
                SourceUpgradeRecoveryError::Admission(
                    AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
                )
            )) | Err(SourceUpgradeStorageError::Storage(
                RecoveryStorageError::Admission(
                    AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
                )
            ))
        ));
        assert!(matches!(
            SourceUpgradeRecovery::decode(package.encoded_bytes(), limits, &mut shared),
            Err(SourceUpgradeRecoveryError::Admission(
                AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
            ))
        ));
    }
    let mut shared = budget();
    let wrong =
        ProjectAuthoringRecovery::new(ProjectPath::new(path).unwrap(), package.digest()).unwrap();
    assert!(matches!(
        validate_pending_bytes(
            &wrong,
            path,
            Some(&invalid),
            limits.encoded_bytes,
            &mut shared
        ),
        Err(RecoveryStorageError::PackageDigestMismatch)
    ));
    assert_eq!(
        shared.usage().decode_work(),
        invalid.len(),
        "a refused digest still performed a hash"
    );
}

#[test]
fn noncanonical_outputs_changed_digests_and_refused_budgets_do_not_publish_a_package() {
    let package = Fixture::new().package();
    let changed = rewritten(&package, |value| {
        value["payload"]["members"][0]["old_disk"]["digest"]["length"] = 1.into()
    });
    assert!(matches!(
        decode(&changed),
        Err(SourceUpgradeRecoveryError::MemberDigestMismatch)
    ));
    let mut fixture = Fixture::new();
    fixture.members[0].next.push(b'\n');
    assert!(matches!(
        fixture.prepare(SourceUpgradeRecoveryLimits::default(), &mut budget()),
        Err(SourceUpgradeRecoveryError::NonCanonicalSuccessor)
    ));
    let fixture = Fixture::new();
    let mut shared = budget();
    shared.limits.retained_candidate_bytes = ByteLimit::new(1).unwrap();
    assert!(matches!(
        fixture.prepare(SourceUpgradeRecoveryLimits::default(), &mut shared),
        Err(SourceUpgradeRecoveryError::Admission(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
        ))
    ));
    assert_eq!(
        shared.usage().decode_work(),
        0,
        "storage admission refused before parsing"
    );
    // Derive the exact required temporary peak without changing production accounting APIs.
    let mut low = 1;
    let mut high = budget().limits.retained_candidate_bytes().get();
    while low < high {
        let middle = low + (high - low) / 2;
        let mut shared = budget();
        shared.limits.retained_candidate_bytes = ByteLimit::new(middle).unwrap();
        if fixture
            .prepare(SourceUpgradeRecoveryLimits::default(), &mut shared)
            .is_ok()
        {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    for (maximum, accepted) in [(low, true), (low - 1, false)] {
        let mut shared = budget();
        shared.limits.retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
        assert_eq!(
            fixture
                .prepare(SourceUpgradeRecoveryLimits::default(), &mut shared)
                .is_ok(),
            accepted
        );
    }
}
