use nara_asset::{
    AssetMeta, AssetMetaIdentity, AssetPath, AssetSourceKind, AssetSourcePath, StableAssetId,
};
use nara_project::ProjectPath;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentSchemaCatalog,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{PrefabDocument, SceneDocument, SceneEntityRecord, SourceDocumentKind};
use serde_json::Value;

use super::*;
use crate::project_host::authoring::AuthoringProjectAdmissionLimits;

const UNIVERSE: &str = "421112bd-693a-4e6b-85bc-318d567b2f21";
const Q_ID: &str = "ad322a15-01a2-4c04-847f-000000000001";

struct OriginalFixture {
    path: String,
    key: SourceDocumentKey,
    encoding: RecoverySourceEncoding,
    explicit_native: bool,
    original: Vec<u8>,
    working: Vec<u8>,
    disposition: RecoverySourceDisposition,
    output: Option<Vec<u8>>,
}

struct Fixture {
    old_manifest: Vec<u8>,
    final_manifest: Vec<u8>,
    bundle: Vec<u8>,
    profile: Option<String>,
    selected: SourceDocumentKey,
    root: SceneEntityId,
    originals: Vec<OriginalFixture>,
    q_path: String,
    q_key: SourceDocumentKey,
    q: Vec<u8>,
    sidecar_path: String,
    sidecar: Vec<u8>,
    metadata: Vec<(String, Vec<u8>)>,
    roots: Vec<(String, Option<String>)>,
    missing: Vec<String>,
}

fn key(path: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        UNIVERSE.parse().unwrap(),
        AssetSourcePath::new(path).unwrap(),
        kind,
    )
}

fn prefab(ids: &[&str], encoding: RecoverySourceEncoding) -> Vec<u8> {
    let document = PrefabDocument::from_ordered_roots(
        ids.iter()
            .map(|id| SceneEntityRecord::new(SceneEntityId::new(*id).unwrap())),
    );
    match encoding {
        RecoverySourceEncoding::Json => document.to_json_string().unwrap(),
        RecoverySourceEncoding::Ron => document.to_ron_string().unwrap(),
    }
    .into_bytes()
}

fn scene(id: &str) -> Vec<u8> {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(SceneEntityId::new(id).unwrap())])
        .to_json_string()
        .unwrap()
        .into_bytes()
}

fn metadata(id: &str, local: &str, kind: AssetSourceKind) -> Vec<u8> {
    AssetMeta::new(
        AssetMetaIdentity::new(id.parse().unwrap(), AssetPath::new(local).unwrap(), kind),
        (),
    )
    .to_json_string()
    .unwrap()
    .into_bytes()
}

impl Fixture {
    fn new() -> Self {
        let old_manifest = format!(
            "# Exact captured Ready bytes, including this comment.\nschema_version = 1\n\
             [project]\nname = \"Extraction recovery fixture\"\nsource_universe = \"{UNIVERSE}\"\n\
             native_document_identity = \"project_location\"\n\
             [authoring]\nstate = \"ready\"\nschema_bundle = \"schema.json\"\n\
             [profiles.desktop.paths]\nprefabs = \"prefabs\"\n"
        )
        .into_bytes();
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new("trial.recovery").unwrap(),
            ComponentSchemaCatalog::default(),
            None,
        )])
        .unwrap()
        .to_json_string()
        .unwrap()
        .into_bytes();
        let originals = vec![
            OriginalFixture {
                path: "scenes/r.scene.json".to_owned(),
                key: key("scenes/r.scene.json", SourceDocumentKind::Scene),
                encoding: RecoverySourceEncoding::Json,
                explicit_native: true,
                original: scene("disk"),
                working: scene("reviewed"),
                disposition: RecoverySourceDisposition::SaveCurrent,
                output: Some(scene("reviewed")),
            },
            OriginalFixture {
                path: "prefabs/p.prefab.json".to_owned(),
                key: key("prefabs/p.prefab.json", SourceDocumentKind::Prefab),
                encoding: RecoverySourceEncoding::Json,
                explicit_native: true,
                original: prefab(&["branch", "disk"], RecoverySourceEncoding::Json),
                working: prefab(&["branch", "reviewed"], RecoverySourceEncoding::Json),
                disposition: RecoverySourceDisposition::ReplaceSource,
                output: Some(prefab(&["branch", "output"], RecoverySourceEncoding::Json)),
            },
            OriginalFixture {
                path: "prefabs/guard.prefab.ron".to_owned(),
                key: key("prefabs/guard.prefab.ron", SourceDocumentKind::Prefab),
                encoding: RecoverySourceEncoding::Ron,
                explicit_native: false,
                original: prefab(&["guard"], RecoverySourceEncoding::Ron),
                working: prefab(&["dirty"], RecoverySourceEncoding::Ron),
                disposition: RecoverySourceDisposition::Guard,
                output: None,
            },
        ];
        let missing = originals
            .iter()
            .map(|source| format!("{}.meta", source.path))
            .collect();
        Self {
            final_manifest: old_manifest.clone(),
            old_manifest,
            bundle,
            profile: Some("desktop".to_owned()),
            selected: key("prefabs/p.prefab.json", SourceDocumentKind::Prefab),
            root: SceneEntityId::new("branch").unwrap(),
            originals,
            q_path: "prefabs/q.prefab.json".to_owned(),
            q_key: key("prefabs/q.prefab.json", SourceDocumentKind::Prefab),
            q: prefab(&["branch"], RecoverySourceEncoding::Json),
            sidecar_path: "prefabs/q.prefab.json.meta".to_owned(),
            sidecar: metadata(Q_ID, "q.prefab.json", AssetSourceKind::Prefab),
            metadata: vec![(
                "assets/pixel.png.meta".to_owned(),
                metadata(
                    "ad322a15-01a2-4c04-847f-000000000002",
                    "pixel.png",
                    AssetSourceKind::Image,
                ),
            )],
            roots: vec![
                ("scenes".to_owned(), None),
                ("assets".to_owned(), None),
                ("prefabs".to_owned(), None),
            ],
            missing,
        }
    }

    fn create(
        &self,
        limits: PrefabExtractionRecoveryLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<PrefabExtractionRecovery, PrefabExtractionRecoveryError> {
        let originals: Vec<_> = self
            .originals
            .iter()
            .map(|source| RecoveryOriginalSourceInput {
                path: &source.path,
                key: &source.key,
                encoding: source.encoding,
                explicit_native: source.explicit_native,
                original_disk: &source.original,
                reviewed_working: &source.working,
                disposition: source.disposition,
                output: source.output.as_deref(),
            })
            .collect();
        let metadata: Vec<_> = self
            .metadata
            .iter()
            .map(|(path, observed)| RecoveryMetadataGuardInput { path, observed })
            .collect();
        let roots: Vec<_> = self
            .roots
            .iter()
            .map(|(path, ancestor)| RecoveryRootInput {
                path,
                missing_ancestor: ancestor.as_deref(),
            })
            .collect();
        let missing: Vec<_> = self.missing.iter().map(String::as_str).collect();
        PrefabExtractionRecovery::create(
            PrefabExtractionRecoveryInput {
                old_ready_manifest: &self.old_manifest,
                final_ready_manifest: &self.final_manifest,
                unchanged_bundle: RecoveryBundleInput {
                    path: "schema.json",
                    blob: &self.bundle,
                },
                source_universe: UNIVERSE.parse().unwrap(),
                selected_profile: self.profile.as_deref(),
                selection: RecoverySelectionInput {
                    source: &self.selected,
                    root: &self.root,
                },
                complete_original_sources: &originals,
                q: RecoveryNewSourceInput {
                    path: &self.q_path,
                    key: &self.q_key,
                    encoding: RecoverySourceEncoding::Json,
                    output: &self.q,
                },
                q_sidecar: RecoveryOutputInput {
                    path: &self.sidecar_path,
                    output: &self.sidecar,
                },
                metadata_guards: &metadata,
                roots: &roots,
                missing_sidecars: &missing,
            },
            limits,
            budget,
        )
    }

    fn package(&self) -> PrefabExtractionRecovery {
        self.create(PrefabExtractionRecoveryLimits::default(), &mut budget())
            .unwrap()
    }

    fn use_stable_metadata(&mut self) {
        let mut manifest = nara_project::ProjectManifest::parse_toml_bytes(&self.old_manifest)
            .manifest
            .unwrap();
        manifest.project.native_document_identity =
            nara_project::NativeDocumentIdentity::StableMetadata;
        self.old_manifest = manifest.to_canonical_toml_bytes().unwrap();
        self.final_manifest = self.old_manifest.clone();
        self.missing.clear();
        for (index, source) in self.originals.iter_mut().enumerate() {
            let id = format!("ad322a15-01a2-4c04-847f-{:012}", index + 10);
            let kind = source.key.kind();
            source.key = SourceDocumentKey::stable_asset(id.parse().unwrap(), kind);
            self.metadata.push((
                format!("{}.meta", source.path),
                metadata(
                    &id,
                    source.path.split_once('/').unwrap().1,
                    match kind {
                        SourceDocumentKind::Scene => AssetSourceKind::Scene,
                        SourceDocumentKind::Prefab => AssetSourceKind::Prefab,
                    },
                ),
            ));
            if source.path == "prefabs/p.prefab.json" {
                self.selected = source.key.clone();
            }
        }
        self.q_key =
            SourceDocumentKey::stable_asset(Q_ID.parse().unwrap(), SourceDocumentKind::Prefab);
    }
}

fn budget() -> AuthoringProjectAdmissionBudget {
    AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default())
}

fn decode(bytes: &[u8]) -> Result<PrefabExtractionRecovery, PrefabExtractionRecoveryError> {
    PrefabExtractionRecovery::decode(
        bytes,
        PrefabExtractionRecoveryLimits::default(),
        &mut budget(),
    )
}

fn json(package: &PrefabExtractionRecovery) -> Value {
    serde_json::from_slice(package.encoded_bytes()).unwrap()
}

fn decode_json(value: Value) -> Result<PrefabExtractionRecovery, PrefabExtractionRecoveryError> {
    decode(&serde_json::to_vec(&value).unwrap())
}

fn replace_blob(value: &mut Value, bytes: &[u8]) {
    *value = serde_json::to_value(RecoveryBlob::copy(bytes).unwrap()).unwrap();
}

#[test]
fn roundtrip_retains_exact_original_working_outputs_and_commitment() {
    let fixture = Fixture::new();
    let package = fixture.package();
    let decoded = decode(package.encoded_bytes()).unwrap();
    assert!(decoded.payload == package.payload);
    assert_eq!(decoded.encoded_bytes(), package.encoded_bytes());
    assert_eq!(decoded.digest(), digest(package.encoded_bytes()));
    assert_eq!(decoded.old_ready_manifest().bytes(), fixture.old_manifest);
    assert_eq!(decoded.final_ready_manifest().bytes(), fixture.old_manifest);
    assert_eq!(decoded.unchanged_bundle().path(), "schema.json");
    assert_eq!(decoded.unchanged_bundle().blob().bytes(), fixture.bundle);
    assert_eq!(decoded.source_universe(), UNIVERSE.parse().unwrap());
    assert_eq!(decoded.selected_profile(), Some("desktop"));
    assert_eq!(decoded.selection().source(), &fixture.selected);
    assert_eq!(decoded.selection().root(), &fixture.root);
    assert_eq!(decoded.q().key(), &fixture.q_key);
    assert_eq!(decoded.q().encoding(), RecoverySourceEncoding::Json);
    assert_eq!(decoded.q().output().bytes(), fixture.q);
    assert_eq!(decoded.q_sidecar().path(), fixture.sidecar_path);
    assert_eq!(decoded.q_sidecar().output().bytes(), fixture.sidecar);
    assert_eq!(decoded.metadata_guards()[0].path(), "assets/pixel.png.meta");
    assert_eq!(decoded.roots()[0].path(), "assets");
    assert!(matches!(
        decoded.roots()[0].observation(),
        RecoveryRootObservation::Present
    ));
    assert_eq!(decoded.missing_sidecars().len(), 3);
    for original in decoded.complete_original_sources() {
        let input = fixture
            .originals
            .iter()
            .find(|source| source.path == original.path())
            .unwrap();
        assert_eq!(original.key(), &input.key);
        assert_eq!(original.encoding(), input.encoding);
        assert_eq!(original.explicit_native(), input.explicit_native);
        assert_eq!(original.disposition(), input.disposition);
        assert_eq!(original.original_disk().bytes(), input.original);
        assert_eq!(original.reviewed_working().bytes(), input.working);
        assert_eq!(
            original.output().map(RecoveryBlob::bytes),
            input.output.as_deref()
        );
    }
    // This fixture supplies grammar-valid pairs, not a semantic extraction proof.
    assert!(!format!("{decoded:?}").contains("reviewed"));
    assert!(!format!("{:?}", decoded.q().output()).contains("branch"));
}

#[test]
fn selected_profile_requires_a_present_field_with_explicit_null_for_default() {
    for profile in [None, Some("desktop")] {
        let mut fixture = Fixture::new();
        fixture.profile = profile.map(str::to_owned);
        let package = fixture.package();
        assert_eq!(
            decode(package.encoded_bytes()).unwrap().selected_profile(),
            profile
        );
        let mut missing = json(&package);
        missing["payload"]
            .as_object_mut()
            .unwrap()
            .remove("selected_profile");
        assert!(matches!(
            decode_json(missing),
            Err(PrefabExtractionRecoveryError::Syntax)
        ));
    }
}

#[test]
fn final_ready_can_preserve_original_bytes_or_change_only_their_formatting() {
    let mut fixture = Fixture::new();
    fixture.final_manifest = nara_project::ProjectManifest::parse_toml_bytes(&fixture.old_manifest)
        .manifest
        .unwrap()
        .to_canonical_toml_bytes()
        .unwrap();
    assert_ne!(fixture.old_manifest, fixture.final_manifest);
    let package = fixture.package();
    assert_eq!(
        package.classify_manifest(Some(ContentDigest::of_bytes(&fixture.old_manifest))),
        RecoveryFileState::Original
    );
    assert_eq!(
        package.classify_manifest(Some(ContentDigest::of_bytes(&fixture.final_manifest))),
        RecoveryFileState::Output
    );
    assert_eq!(
        package.final_ready_manifest().bytes(),
        fixture.final_manifest
    );
}

#[test]
fn new_roles_and_original_members_have_distinct_missing_states_and_no_receipts() {
    let package = Fixture::new().package();
    let source = package
        .complete_original_sources()
        .iter()
        .find(|source| source.disposition() == RecoverySourceDisposition::ReplaceSource)
        .unwrap();
    assert_eq!(source.classify(None), RecoveryFileState::MissingOriginal);
    assert_eq!(
        source.classify(Some(ContentDigest::of_bytes(
            source.original_disk().bytes()
        ))),
        RecoveryFileState::Original
    );
    assert_eq!(
        source.classify(Some(ContentDigest::of_bytes(
            source.output().unwrap().bytes()
        ))),
        RecoveryFileState::Output
    );
    assert_eq!(
        source.classify(Some(ContentDigest::of_bytes(b"third content"))),
        RecoveryFileState::Conflict
    );
    let guard = package
        .complete_original_sources()
        .iter()
        .find(|source| source.disposition() == RecoverySourceDisposition::Guard)
        .unwrap();
    assert_eq!(
        guard.classify(Some(ContentDigest::of_bytes(guard.original_disk().bytes()))),
        RecoveryFileState::Unchanged
    );
    assert_eq!(
        guard.classify(Some(ContentDigest::of_bytes(
            guard.reviewed_working().bytes()
        ))),
        RecoveryFileState::Conflict
    );
    assert_eq!(package.q().classify(None), RecoveryFileState::NotCreated);
    assert_eq!(
        package.q_sidecar().classify(None),
        RecoveryFileState::NotCreated
    );
    assert_eq!(
        package
            .q()
            .classify(Some(ContentDigest::of_bytes(package.q().output().bytes()))),
        RecoveryFileState::Output
    );
    assert_eq!(
        package
            .q_sidecar()
            .classify(Some(ContentDigest::of_bytes(b""))),
        RecoveryFileState::Conflict
    );
    assert_eq!(
        package
            .unchanged_bundle()
            .classify(Some(ContentDigest::of_bytes(b"changed bundle"))),
        RecoveryFileState::Conflict
    );
    assert_eq!(
        package.unchanged_bundle().classify(None),
        RecoveryFileState::MissingOriginal
    );
    assert_eq!(
        package.metadata_guards()[0].classify(None),
        RecoveryFileState::MissingOriginal
    );
    assert_eq!(
        package.metadata_guards()[0].classify(Some(ContentDigest::of_bytes(
            package.metadata_guards()[0].observed().bytes()
        ))),
        RecoveryFileState::Unchanged
    );
    assert_eq!(
        package.classify_manifest(Some(ContentDigest::of_bytes(
            package.old_ready_manifest().bytes()
        ))),
        RecoveryFileState::Unchanged
    );
}

#[test]
fn pending_checks_length_and_digest_before_any_decode() {
    use crate::project_host::authoring::recovery_storage::{
        RecoveryStorageError, validate_pending_bytes,
    };
    use nara_project::ProjectAuthoringRecovery;

    let package = Fixture::new().package();
    let path = ".nara/extraction.json";
    let limits = PrefabExtractionRecoveryLimits::default();
    let reference =
        ProjectAuthoringRecovery::new(ProjectPath::new(path).unwrap(), package.digest()).unwrap();
    let mut accepted = budget();
    validate_pending_bytes(
        &reference,
        path,
        Some(package.encoded_bytes()),
        limits.encoded_bytes,
        &mut accepted,
    )
    .unwrap();
    let admitted =
        PrefabExtractionRecovery::decode(package.encoded_bytes(), limits, &mut accepted).unwrap();
    assert_eq!(admitted.digest(), package.digest());
    let mut shared = budget();
    assert!(matches!(
        validate_pending_bytes(
            &reference,
            path,
            Some(b"{"),
            limits.encoded_bytes,
            &mut shared
        ),
        Err(RecoveryStorageError::PackageDigestMismatch)
    ));
    // A different length fails the common commitment before concrete grammar admission.
    assert_eq!(shared.decode_work, 1);
    let mut corrupt = package.encoded_bytes().to_vec();
    corrupt[0] = b'!';
    assert!(matches!(
        validate_pending_bytes(
            &reference,
            path,
            Some(&corrupt),
            limits.encoded_bytes,
            &mut shared
        ),
        Err(RecoveryStorageError::PackageDigestMismatch)
    ));
    assert_eq!(shared.decode_work, 1 + corrupt.len());
    assert_eq!(shared.retained_candidate_bytes, 0);

    // Committing malformed bytes is a transport success, followed by a concrete grammar refusal.
    let malformed = b"{";
    let malformed_reference =
        ProjectAuthoringRecovery::new(ProjectPath::new(path).unwrap(), digest(malformed)).unwrap();
    validate_pending_bytes(
        &malformed_reference,
        path,
        Some(malformed),
        limits.encoded_bytes,
        &mut shared,
    )
    .unwrap();
    assert!(matches!(
        PrefabExtractionRecovery::decode(malformed, limits, &mut shared),
        Err(PrefabExtractionRecoveryError::Syntax)
    ));
    assert_eq!(shared.retained_candidate_bytes, 0);
}

#[test]
fn source_disposition_and_save_current_relations_are_checked() {
    let mut fixture = Fixture::new();
    fixture.originals[0].output = Some(fixture.originals[0].original.clone());
    assert!(matches!(
        fixture.create(PrefabExtractionRecoveryLimits::default(), &mut budget()),
        Err(PrefabExtractionRecoveryError::SaveCurrentMismatch)
    ));
    for disposition in [
        RecoverySourceDisposition::SaveCurrent,
        RecoverySourceDisposition::ReplaceSource,
        RecoverySourceDisposition::RewriteClosed,
    ] {
        let mut value = json(&Fixture::new().package());
        value["payload"]["complete_original_sources"][0]["disposition"] =
            serde_json::to_value(disposition).unwrap();
        assert!(matches!(
            decode_json(value),
            Err(PrefabExtractionRecoveryError::DispositionInvalid)
        ));
    }
    let mut value = json(&Fixture::new().package());
    value["payload"]["complete_original_sources"][1]["disposition"] =
        Value::String("guard".to_owned());
    assert!(matches!(
        decode_json(value),
        Err(PrefabExtractionRecoveryError::DispositionInvalid)
    ));
    let mut fixture = Fixture::new();
    fixture.originals[1].disposition = RecoverySourceDisposition::RewriteClosed;
    assert!(
        fixture
            .create(PrefabExtractionRecoveryLimits::default(), &mut budget())
            .is_ok()
    );
}

#[test]
fn selected_source_profile_and_local_root_must_exist() {
    let mut fixture = Fixture::new();
    fixture.profile = Some("missing".to_owned());
    assert!(matches!(
        fixture.create(PrefabExtractionRecoveryLimits::default(), &mut budget()),
        Err(PrefabExtractionRecoveryError::SelectedProfileInvalid)
    ));
    fixture.profile = None;
    assert!(
        fixture
            .create(PrefabExtractionRecoveryLimits::default(), &mut budget())
            .is_ok()
    );
    fixture.selected = fixture.q_key.clone();
    assert!(matches!(
        fixture.create(PrefabExtractionRecoveryLimits::default(), &mut budget()),
        Err(PrefabExtractionRecoveryError::SelectionInvalid)
    ));
    fixture.selected = fixture.originals[0].key.clone();
    assert!(matches!(
        fixture.create(PrefabExtractionRecoveryLimits::default(), &mut budget()),
        Err(PrefabExtractionRecoveryError::SelectionInvalid)
    ));
    fixture.selected = fixture.originals[1].key.clone();
    fixture.root = SceneEntityId::new("absent").unwrap();
    assert!(matches!(
        fixture.create(PrefabExtractionRecoveryLimits::default(), &mut budget()),
        Err(PrefabExtractionRecoveryError::SelectionInvalid)
    ));
    let mut value = json(&Fixture::new().package());
    value["payload"]["selection"]["root"] = Value::String("../invalid".to_owned());
    assert!(matches!(
        decode_json(value),
        Err(PrefabExtractionRecoveryError::Syntax)
    ));
}

#[test]
fn metadata_matches_q_path_kind_stable_identity_and_native_identity_mode() {
    let mut fixture = Fixture::new();
    fixture.use_stable_metadata();
    let package = fixture.package();
    assert!(decode(package.encoded_bytes()).is_ok());
    fixture.sidecar = metadata(
        "ad322a15-01a2-4c04-847f-000000000099",
        "q.prefab.json",
        AssetSourceKind::Prefab,
    );
    assert!(matches!(
        fixture.create(PrefabExtractionRecoveryLimits::default(), &mut budget()),
        Err(PrefabExtractionRecoveryError::DestinationInvalid)
    ));
    for (alias, kind) in [
        ("other.prefab.json", AssetSourceKind::Prefab),
        ("q.prefab.json", AssetSourceKind::Scene),
    ] {
        let mut fixture = Fixture::new();
        fixture.sidecar = metadata(Q_ID, alias, kind);
        assert!(matches!(
            fixture.create(PrefabExtractionRecoveryLimits::default(), &mut budget()),
            Err(PrefabExtractionRecoveryError::DestinationInvalid)
        ));
    }
    let mut fixture = Fixture::new();
    fixture.q_key = SourceDocumentKey::stable_asset(
        Q_ID.parse::<StableAssetId>().unwrap(),
        SourceDocumentKind::Prefab,
    );
    assert!(matches!(
        fixture.create(PrefabExtractionRecoveryLimits::default(), &mut budget()),
        Err(PrefabExtractionRecoveryError::KeyConflict)
    ));
    let mut fixture = Fixture::new();
    fixture.use_stable_metadata();
    fixture.metadata.remove(1);
    assert!(matches!(
        fixture.create(PrefabExtractionRecoveryLimits::default(), &mut budget()),
        Err(PrefabExtractionRecoveryError::MetadataConflict)
    ));
}

#[test]
fn duplicate_roles_paths_case_prefix_and_observation_relationships_refuse() {
    let package = Fixture::new().package();
    let mut value = json(&package);
    value["payload"]["q_sidecar"]["path"] = Value::String("prefabs/p.prefab.json".to_owned());
    assert!(matches!(
        decode_json(value),
        Err(PrefabExtractionRecoveryError::PathConflict)
    ));
    let mut value = json(&package);
    value["payload"]["q_sidecar"]["path"] = Value::String("Prefabs/q.prefab.json.meta".to_owned());
    assert!(matches!(
        decode_json(value),
        Err(PrefabExtractionRecoveryError::PathConflict)
    ));
    let mut value = json(&package);
    value["payload"]["q_sidecar"]["path"] = Value::String("prefabs/q.prefab.json/child".to_owned());
    assert!(matches!(
        decode_json(value),
        Err(PrefabExtractionRecoveryError::PathConflict)
    ));
    let mut value = json(&package);
    value["payload"]["missing_sidecars"][0] = Value::String("other.json.meta".to_owned());
    assert!(decode_json(value).is_err());
    let mut fixture = Fixture::new();
    fixture.missing.pop();
    assert!(matches!(
        fixture.create(PrefabExtractionRecoveryLimits::default(), &mut budget()),
        Err(PrefabExtractionRecoveryError::MetadataConflict)
    ));
    let mut fixture = Fixture::new();
    fixture
        .roots
        .iter_mut()
        .find(|(path, _)| path == "prefabs")
        .unwrap()
        .1 = Some(String::new());
    assert!(matches!(
        fixture.create(PrefabExtractionRecoveryLimits::default(), &mut budget()),
        Err(PrefabExtractionRecoveryError::RootConflict)
    ));
    let mut fixture = Fixture::new();
    fixture
        .roots
        .push(("unused".to_owned(), Some(String::new())));
    assert!(
        fixture
            .create(PrefabExtractionRecoveryLimits::default(), &mut budget())
            .is_ok()
    );
    fixture.roots.last_mut().unwrap().1 = Some("unrelated".to_owned());
    assert!(matches!(
        fixture.create(PrefabExtractionRecoveryLimits::default(), &mut budget()),
        Err(PrefabExtractionRecoveryError::RootConflict)
    ));
    let mut value = json(&package);
    value["payload"]["complete_original_sources"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    assert!(matches!(
        decode_json(value),
        Err(PrefabExtractionRecoveryError::PathConflict)
    ));
}

#[test]
fn missing_root_components_refuse_existing_roots_and_file_parents() {
    for (path, ancestor, existing_parent) in [
        ("unused/unavailable/sub", "", "unused"),
        ("prefabs/unavailable/sub", "", "prefabs"),
        ("assets/nested/unavailable/sub", "assets", "assets/nested"),
        (
            "prefabs/original/unavailable/sub",
            "prefabs",
            "prefabs/original",
        ),
        (
            "prefabs/generated/unavailable/sub",
            "prefabs",
            "prefabs/generated",
        ),
    ] {
        let mut fixture = Fixture::new();
        fixture.roots.push(("unused/present".to_owned(), None));
        fixture.metadata[0] = (
            "assets/nested/pixel.png.meta".to_owned(),
            metadata(
                "ad322a15-01a2-4c04-847f-000000000002",
                "nested/pixel.png",
                AssetSourceKind::Image,
            ),
        );
        fixture.q_path = "prefabs/generated/q.prefab.json".to_owned();
        fixture.q_key = key(&fixture.q_path, SourceDocumentKind::Prefab);
        fixture.sidecar_path = format!("{}.meta", fixture.q_path);
        fixture.sidecar = metadata(Q_ID, "generated/q.prefab.json", AssetSourceKind::Prefab);
        fixture.originals[2].path = "prefabs/original/guard.prefab.ron".to_owned();
        fixture.originals[2].key = key(&fixture.originals[2].path, SourceDocumentKind::Prefab);
        fixture.missing[2] = format!("{}.meta", fixture.originals[2].path);
        let package = fixture.package();
        fixture
            .roots
            .push((path.to_owned(), Some(ancestor.to_owned())));
        assert!(matches!(
            fixture.create(PrefabExtractionRecoveryLimits::default(), &mut budget()),
            Err(PrefabExtractionRecoveryError::RootConflict)
        ));
        let mut value = json(&package);
        value["payload"]["roots"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "path": path,
                "observation": { "missing": { "ancestor": ancestor } },
            }));
        value["payload"]["roots"]
            .as_array_mut()
            .unwrap()
            .sort_unstable_by(|left, right| {
                left["path"]
                    .as_str()
                    .unwrap()
                    .cmp(right["path"].as_str().unwrap())
            });
        assert!(matches!(
            decode_json(value),
            Err(PrefabExtractionRecoveryError::RootConflict)
        ));
        fixture.roots.last_mut().unwrap().1 = Some(existing_parent.to_owned());
        let package = fixture.package();
        assert!(decode(package.encoded_bytes()).is_ok());
    }
}

#[test]
fn missing_root_components_refuse_other_missing_roots_existing_ancestors() {
    for (path, ancestor, other_path, other_ancestor) in [
        ("unused/a", "", "unused/b/sub", "unused"),
        ("unused/a/sub", "unused", "unused/a/peer/sub", "unused/a"),
    ] {
        let mut fixture = Fixture::new();
        fixture.roots.extend([
            (path.to_owned(), Some(ancestor.to_owned())),
            (other_path.to_owned(), Some(other_ancestor.to_owned())),
        ]);
        assert!(matches!(
            fixture.create(PrefabExtractionRecoveryLimits::default(), &mut budget()),
            Err(PrefabExtractionRecoveryError::RootConflict)
        ));
    }
}

#[test]
fn missing_root_components_preserve_exact_ancestors_and_independent_missing_paths() {
    for (ancestor, paths) in [
        ("", ["unused/a", "unused/b"]),
        (
            "prefabs",
            ["prefabs/unavailable/a", "prefabs/unavailable/b"],
        ),
        (
            "prefabs",
            ["prefabs/unavailable", "prefabs/unavailable_peer/sub"],
        ),
        (
            "prefabs/未创建",
            ["prefabs/未创建/缺失/a", "prefabs/未创建/缺失/b"],
        ),
    ] {
        let mut fixture = Fixture::new();
        fixture.roots.extend(
            paths
                .into_iter()
                .map(|path| (path.to_owned(), Some(ancestor.to_owned()))),
        );
        let package = fixture.package();
        assert!(decode(package.encoded_bytes()).is_ok());
    }
    for ancestor in ["prefab", "prefabs/unavail", "prefabs/unavailable/sub"] {
        let mut fixture = Fixture::new();
        fixture.roots.push((
            "prefabs/unavailable/sub".to_owned(),
            Some(ancestor.to_owned()),
        ));
        assert!(matches!(
            fixture.create(PrefabExtractionRecoveryLimits::default(), &mut budget()),
            Err(PrefabExtractionRecoveryError::RootConflict)
        ));
    }
}

#[test]
fn changed_configuration_bundle_digest_and_source_grammar_refuse() {
    let mut fixture = Fixture::new();
    fixture
        .final_manifest
        .extend_from_slice(b"\n[paths]\nprefabs = \"elsewhere\"\n");
    assert!(matches!(
        fixture.create(PrefabExtractionRecoveryLimits::default(), &mut budget()),
        Err(PrefabExtractionRecoveryError::ConfigurationMismatch)
    ));
    let package = Fixture::new().package();
    let mut value = json(&package);
    value["payload"]["unchanged_bundle"]["blob"]["bytes"] = Value::String("changed".to_owned());
    assert!(matches!(
        decode_json(value),
        Err(PrefabExtractionRecoveryError::BlobDigestMismatch)
    ));
    let mut value = json(&package);
    replace_blob(&mut value["payload"]["unchanged_bundle"]["blob"], b"{}");
    assert!(matches!(
        decode_json(value),
        Err(PrefabExtractionRecoveryError::BundleInvalid)
    ));
    let mut value = json(&package);
    replace_blob(&mut value["payload"]["q"]["output"], b"{}");
    assert!(matches!(
        decode_json(value),
        Err(PrefabExtractionRecoveryError::SourceInvalid)
    ));
    let mut fixture = Fixture::new();
    fixture.q.push(b' ');
    assert!(matches!(
        fixture.create(PrefabExtractionRecoveryLimits::default(), &mut budget()),
        Err(PrefabExtractionRecoveryError::NonCanonicalOutput)
    ));
    let mut fixture = Fixture::new();
    fixture.originals[0].original = vec![0xff];
    assert!(matches!(
        fixture.create(PrefabExtractionRecoveryLimits::default(), &mut budget()),
        Err(PrefabExtractionRecoveryError::Utf8)
    ));
}

#[test]
fn envelope_and_nested_unknown_fields_syntax_and_prototype_versions_refuse() {
    let package = Fixture::new().package();
    assert!(decode(b"{").is_err());
    let mut value = json(&package);
    value["payload"]["invented"] = Value::Bool(true);
    assert!(matches!(
        decode_json(value),
        Err(PrefabExtractionRecoveryError::Syntax)
    ));
    let mut value = json(&package);
    value["payload"]["q"]["original_disk"] = Value::Null;
    assert!(matches!(
        decode_json(value),
        Err(PrefabExtractionRecoveryError::Syntax)
    ));
    let mut value = json(&package);
    value["payload"]["unchanged_bundle"]["successor"] = Value::Null;
    assert!(matches!(
        decode_json(value),
        Err(PrefabExtractionRecoveryError::Syntax)
    ));
    let mut value = json(&package);
    value["format_version"] = Value::from(2);
    assert!(matches!(
        decode_json(value),
        Err(PrefabExtractionRecoveryError::Contract(_))
    ));
    let mut padded = package.encoded_bytes().to_vec();
    padded.extend_from_slice(b"\n  ");
    let admitted = decode(&padded).unwrap();
    assert_eq!(admitted.encoded_bytes(), padded);
    assert_ne!(admitted.digest(), package.digest());
}

#[test]
fn encoded_aggregate_and_count_limits_are_exact_for_create_and_decode() {
    let fixture = Fixture::new();
    let package = fixture.package();
    let mut exact = PrefabExtractionRecoveryLimits::default();
    exact.encoded_bytes = ByteLimit::new(package.encoded_bytes().len()).unwrap();
    exact.aggregate_bytes =
        ByteLimit::new(admission::aggregate(&package.payload).unwrap()).unwrap();
    exact.sources = ItemLimit::new(3).unwrap();
    exact.observations = ItemLimit::new(7).unwrap();
    assert!(fixture.create(exact, &mut budget()).is_ok());
    assert!(
        PrefabExtractionRecovery::decode(package.encoded_bytes(), exact, &mut budget()).is_ok()
    );
    let mut short = exact;
    short.encoded_bytes = ByteLimit::new(exact.encoded_bytes.get() - 1).unwrap();
    assert!(matches!(
        fixture.create(short, &mut budget()),
        Err(PrefabExtractionRecoveryError::EncodedBytesExceeded)
    ));
    assert!(matches!(
        PrefabExtractionRecovery::decode(package.encoded_bytes(), short, &mut budget()),
        Err(PrefabExtractionRecoveryError::EncodedBytesExceeded)
    ));
    let mut short = exact;
    short.aggregate_bytes = ByteLimit::new(exact.aggregate_bytes.get() - 1).unwrap();
    assert!(matches!(
        fixture.create(short, &mut budget()),
        Err(PrefabExtractionRecoveryError::AggregateBytesExceeded)
    ));
    assert!(matches!(
        PrefabExtractionRecovery::decode(package.encoded_bytes(), short, &mut budget()),
        Err(PrefabExtractionRecoveryError::AggregateBytesExceeded)
    ));
    let mut short = exact;
    short.sources = ItemLimit::new(2).unwrap();
    assert!(matches!(
        fixture.create(short, &mut budget()),
        Err(PrefabExtractionRecoveryError::SourceCountExceeded)
    ));
    assert!(matches!(
        PrefabExtractionRecovery::decode(package.encoded_bytes(), short, &mut budget()),
        Err(PrefabExtractionRecoveryError::SourceCountExceeded)
    ));
    let mut short = exact;
    short.observations = ItemLimit::new(6).unwrap();
    assert!(matches!(
        fixture.create(short, &mut budget()),
        Err(PrefabExtractionRecoveryError::ObservationCountExceeded)
    ));
    assert!(matches!(
        PrefabExtractionRecovery::decode(package.encoded_bytes(), short, &mut budget()),
        Err(PrefabExtractionRecoveryError::ObservationCountExceeded)
    ));
}

#[test]
fn failed_create_and_decode_refund_only_dropped_storage_and_keep_spent_work() {
    let mut shared = budget();
    shared.charge_source_bytes(0, 0, 17, 4_096).unwrap();
    shared.retain_open_handles(3).unwrap();
    let before_storage = shared.retained_candidate_bytes;
    let before_work = shared.decode_work;
    let mut fixture = Fixture::new();
    fixture.originals[0].output = Some(scene("wrong"));
    assert!(
        fixture
            .create(PrefabExtractionRecoveryLimits::default(), &mut shared)
            .is_err()
    );
    assert_eq!(shared.retained_candidate_bytes, before_storage);
    assert_eq!(shared.retained_open_handles, 3);
    assert!(shared.decode_work > before_work);
    let before_work = shared.decode_work;
    let mut value = json(&Fixture::new().package());
    replace_blob(&mut value["payload"]["q"]["output"], b"{}");
    assert!(
        PrefabExtractionRecovery::decode(
            &serde_json::to_vec(&value).unwrap(),
            PrefabExtractionRecoveryLimits::default(),
            &mut shared
        )
        .is_err()
    );
    assert_eq!(shared.retained_candidate_bytes, before_storage);
    assert_eq!(shared.retained_open_handles, 3);
    assert!(shared.decode_work > before_work);
}

#[test]
fn actual_shape_and_storage_allowance_constrain_decode_without_refunding_work() {
    let package = Fixture::new().package();
    let mut limits = PrefabExtractionRecoveryLimits::default();
    limits.shape = SerdeShapeLimits::new(
        limits.shape.depth(),
        ItemLimit::new(1).unwrap(),
        ItemLimit::new(1).unwrap(),
        limits.shape.string_bytes(),
        limits.shape.total_string_bytes(),
    );
    let mut shared = budget();
    assert!(matches!(
        PrefabExtractionRecovery::decode(package.encoded_bytes(), limits, &mut shared),
        Err(PrefabExtractionRecoveryError::Shape(_))
    ));
    assert_eq!(shared.retained_candidate_bytes, 0);
    assert!(shared.decode_work > 0);
    let mut shared = budget();
    shared.limits.retained_candidate_bytes = ByteLimit::new(1).unwrap();
    assert!(
        Fixture::new()
            .create(PrefabExtractionRecoveryLimits::default(), &mut shared)
            .is_err()
    );
    assert_eq!(shared.retained_candidate_bytes, 0);
    assert!(shared.decode_work > 0);
}
