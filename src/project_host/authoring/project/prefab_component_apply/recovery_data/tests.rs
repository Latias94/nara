use nara_asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetSourceKind, AssetSourcePath};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentSchemaCatalog,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{PrefabDocument, SceneDocument, SceneEntityId, SceneEntityRecord};
use serde_json::{Value, json};

use super::*;
use crate::project_host::authoring::AuthoringProjectAdmissionLimits;

const UNIVERSE: &str = "421112bd-693a-4e6b-85bc-318d567b2f21";
const DESTINATION: &str = "prefabs/crate.prefab.ron";
const CONTAINING: &str = "scenes/startup.scene.json";
const GUARD: &str = "scripts/control.scene.json";
const GUARD_SIDECAR: &str = "prefabs/crate.prefab.ron.meta";
const MISSING_SIDECAR: &str = "scenes/startup.scene.json.meta";

struct Fixture {
    manifest: String,
    bundle: String,
    keys: [SourceDocumentKey; 3],
    original: [String; 3],
    destination_output: String,
    metadata: String,
}

impl Fixture {
    fn new() -> Self {
        let root = |id| SceneEntityRecord::new(SceneEntityId::new(id).unwrap());
        let prefab = |id| {
            PrefabDocument::from_ordered_roots([root(id)])
                .to_ron_string()
                .unwrap()
        };
        let scene = |id| {
            SceneDocument::from_ordered_roots([root(id)])
                .to_json_string()
                .unwrap()
        };
        let universe: SourceUniverseId = UNIVERSE.parse().unwrap();
        Self {
            manifest: format!(
                "# Original Ready bytes\r\nschema_version = 1\r\n\
                 [project]\r\nname = \"Component Apply recovery\"\r\nsource_universe = \"{universe}\"\r\n\
                 native_document_identity = \"project_location\"\r\n\
                 [authoring]\r\nstate = \"ready\"\r\nschema_bundle = \"schema.json\"\r\n\
                 [profiles.desktop.paths]\r\nprefabs = \"prefabs\"\r\n\
                 [profiles.preview.paths]\r\nscenes = \"archive/preview\"\r\n"
            ),
            bundle: AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
                PersistentComponentSchemaOwnerId::try_new("trial.recovery").unwrap(),
                ComponentSchemaCatalog::default(),
                None,
            )])
            .unwrap()
            .to_json_string()
            .unwrap(),
            keys: [
                (DESTINATION, SourceDocumentKind::Prefab),
                (CONTAINING, SourceDocumentKind::Scene),
                (GUARD, SourceDocumentKind::Scene),
            ]
            .map(|(path, kind)| {
                SourceDocumentKey::project_location(
                    universe,
                    AssetSourcePath::new(path).unwrap(),
                    kind,
                )
            }),
            original: [
                format!("{}\r\n", prefab("original")),
                scene("containing"),
                scene("guard"),
            ],
            destination_output: format!("{}\r\n", prefab("replacement")),
            metadata: AssetMeta::new(
                AssetMetaIdentity::new(
                    "ad322a15-01a2-4c04-847f-000000000001".parse().unwrap(),
                    AssetPath::new("crate.prefab.ron").unwrap(),
                    AssetSourceKind::Prefab,
                ),
                (),
            )
            .to_json_string()
            .unwrap(),
        }
    }

    fn create(
        &self,
        change: impl FnOnce(&mut PrefabComponentApplyRecoveryInput<'_>),
    ) -> Result<Arc<PrefabComponentApplyRecoveryData>, PrefabComponentApplyRecoveryError> {
        let mut sources = [
            (DESTINATION, SourceEncoding::Ron, false),
            (CONTAINING, SourceEncoding::Json, false),
            (GUARD, SourceEncoding::Json, true),
        ]
        .into_iter()
        .zip(self.keys.iter().zip(&self.original))
        .map(
            |((path, encoding, explicit_native), (key, bytes))| RecoverySourceInput {
                path,
                key,
                encoding,
                explicit_native,
                original_disk: bytes.as_bytes(),
            },
        )
        .collect::<Vec<_>>();
        sources.swap(0, 1);
        let metadata_guards = [RecoveryMetadataGuardInput {
            path: GUARD_SIDECAR,
            observed: self.metadata.as_bytes(),
        }];
        let roots = [
            RecoveryRootInput {
                path: "archive/preview",
                missing_ancestor: Some("archive"),
            },
            RecoveryRootInput {
                path: "prefabs",
                missing_ancestor: None,
            },
            RecoveryRootInput {
                path: "scenes",
                missing_ancestor: None,
            },
        ];
        let mut input = PrefabComponentApplyRecoveryInput {
            ready_manifest: self.manifest.as_bytes(),
            bundle_path: "schema.json",
            bundle_bytes: self.bundle.as_bytes(),
            source_universe: UNIVERSE.parse().unwrap(),
            selected_profile: Some("desktop"),
            complete_original_sources: &sources,
            containing: RecoveryOutputInput {
                key: &self.keys[1],
                output: self.original[1].as_bytes(),
            },
            destination: RecoveryOutputInput {
                key: &self.keys[0],
                output: self.destination_output.as_bytes(),
            },
            metadata_guards: &metadata_guards,
            roots: &roots,
            missing_sidecars: &[MISSING_SIDECAR],
        };
        change(&mut input);
        PrefabComponentApplyRecoveryData::create(
            input,
            PrefabComponentApplyRecoveryLimits::default(),
            &mut budget(),
        )
    }
}

fn budget() -> AuthoringProjectAdmissionBudget {
    AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default())
}

fn decode_wire(
    wire: &Value,
) -> Result<Arc<PrefabComponentApplyRecoveryData>, PrefabComponentApplyRecoveryError> {
    PrefabComponentApplyRecoveryData::decode(
        &serde_json::to_vec(wire).unwrap(),
        PrefabComponentApplyRecoveryLimits::default(),
        &mut budget(),
    )
}

#[test]
fn exact_commitment_round_trips_with_pair_order_independent_of_path_sorting() {
    let fixture = Fixture::new();
    let package = fixture.create(|_| {}).unwrap();
    let wire: Value = serde_json::from_slice(package.encoded_bytes()).unwrap();
    assert_eq!(wire["kind"], KIND);
    assert_eq!(wire["format_version"], 1);
    let different_layout = serde_json::to_vec_pretty(&wire).unwrap();
    let decoded = PrefabComponentApplyRecoveryData::decode(
        &different_layout,
        PrefabComponentApplyRecoveryLimits::default(),
        &mut budget(),
    )
    .unwrap();
    assert_eq!(decoded.encoded_bytes(), different_layout);
    assert_eq!(decoded.digest(), recovery_digest(&different_layout));
    assert_ne!(package.digest(), decoded.digest());
    assert_eq!(decoded.old_ready_manifest(), fixture.manifest.as_bytes());
    assert_eq!(decoded.final_ready_manifest(), decoded.old_ready_manifest());
    assert_eq!(decoded.source_universe(), UNIVERSE.parse().unwrap());
    assert_eq!(decoded.selected_profile(), Some("desktop"));
    assert_eq!(decoded.unchanged_bundle().path(), "schema.json");
    assert_eq!(
        decoded.unchanged_bundle().blob().bytes(),
        fixture.bundle.as_bytes()
    );

    let [destination, containing, guard] = decoded.complete_original_sources() else {
        panic!("the complete original inventory must survive decoding");
    };
    for (index, source) in [destination, containing, guard].into_iter().enumerate() {
        assert_eq!(source.key(), &fixture.keys[index]);
        assert_eq!(
            source.original_disk().bytes(),
            fixture.original[index].as_bytes()
        );
    }
    assert_eq!(destination.encoding(), SourceEncoding::Ron);
    assert!(guard.explicit_native());
    let [
        (containing_source, containing_output),
        (destination_source, destination_output),
    ] = decoded.outputs();
    assert_eq!(containing_source.path(), CONTAINING);
    assert_eq!(
        containing_output.bytes(),
        containing.original_disk().bytes()
    );
    assert_eq!(destination_source.path(), DESTINATION);
    assert_eq!(
        destination_output.bytes(),
        fixture.destination_output.as_bytes()
    );
    assert_eq!(
        destination_output.digest(),
        ContentDigest::of_bytes(destination_output.bytes())
    );
    assert_eq!(decoded.metadata_guards()[0].path(), GUARD_SIDECAR);
    assert_eq!(
        decoded.metadata_guards()[0].observed().bytes(),
        fixture.metadata.as_bytes()
    );
    assert_eq!(decoded.roots()[0].missing_ancestor(), Some("archive"));
    assert_eq!(decoded.roots()[1].missing_ancestor(), None);
    assert_eq!(decoded.missing_sidecars(), &[MISSING_SIDECAR]);
    assert!(
        decoded
            .protected_paths()
            .any(|path| path == "archive/preview")
    );
    assert!(
        decoded
            .protected_paths()
            .any(|path| path == MISSING_SIDECAR)
    );
}

#[test]
fn both_existing_outputs_may_have_the_original_bytes() {
    let fixture = Fixture::new();
    let package = fixture
        .create(|input| input.destination.output = input.complete_original_sources[1].original_disk)
        .unwrap();
    let decoded = PrefabComponentApplyRecoveryData::decode(
        package.encoded_bytes(),
        PrefabComponentApplyRecoveryLimits::default(),
        &mut budget(),
    )
    .unwrap();
    for (source, output) in decoded.outputs() {
        assert_eq!(source.original_disk().digest(), output.digest());
        assert_eq!(source.original_disk().bytes(), output.bytes());
    }
}

#[test]
fn rejects_false_commitments_unknown_fields_and_unsupported_headers() {
    use PrefabComponentApplyRecoveryError as Error;

    let package = Fixture::new().create(|_| {}).unwrap();
    let original: Value = serde_json::from_slice(package.encoded_bytes()).unwrap();
    let mut changed = original.clone();
    changed["payload"]["complete_original_sources"][2]["original_disk"]["bytes"] = json!("other");
    assert!(matches!(
        decode_wire(&changed),
        Err(Error::BlobDigestMismatch)
    ));
    changed = original.clone();
    changed["payload"]["metadata_guards"][0]["observed"]["bytes"] = json!("replaced");
    assert!(matches!(
        decode_wire(&changed),
        Err(Error::BlobDigestMismatch)
    ));
    changed = original.clone();
    changed["payload"]["containing"]["file_identity"] = json!(42);
    assert!(matches!(decode_wire(&changed), Err(Error::Syntax)));
    changed = original.clone();
    changed["payload"]
        .as_object_mut()
        .unwrap()
        .remove("selected_profile");
    assert!(matches!(decode_wire(&changed), Err(Error::Syntax)));
    changed = original.clone();
    changed["kind"] = json!("prefab_extraction_recovery");
    assert!(matches!(decode_wire(&changed), Err(Error::Contract(_))));
    changed = original;
    changed["format_version"] = json!(2);
    assert!(matches!(decode_wire(&changed), Err(Error::Contract(_))));
}

#[test]
fn rejects_invalid_pair_and_noncanonical_inventory() {
    use PrefabComponentApplyRecoveryError as Error;

    let fixture = Fixture::new();
    assert!(matches!(
        fixture.create(|input| input.destination.key = input.containing.key),
        Err(Error::InvalidPair)
    ));
    assert!(matches!(
        fixture.create(|input| input.destination.key = input.complete_original_sources[2].key),
        Err(Error::InvalidPair)
    ));
    let package = fixture.create(|_| {}).unwrap();
    let original: Value = serde_json::from_slice(package.encoded_bytes()).unwrap();
    let mut changed = original.clone();
    changed["payload"]["containing"]["source_index"] = json!(3);
    assert!(matches!(decode_wire(&changed), Err(Error::InvalidPair)));
    changed = original.clone();
    changed["payload"]["complete_original_sources"]
        .as_array_mut()
        .unwrap()
        .swap(1, 2);
    assert!(matches!(decode_wire(&changed), Err(Error::InvalidPath)));
    changed = original.clone();
    changed["payload"]["complete_original_sources"][1]["key"] =
        original["payload"]["complete_original_sources"][2]["key"].clone();
    assert!(matches!(decode_wire(&changed), Err(Error::SourceKey)));
    changed = original;
    changed["payload"]["missing_sidecars"][0] = json!(GUARD_SIDECAR);
    assert!(matches!(decode_wire(&changed), Err(Error::InvalidPath)));
}

#[test]
fn count_and_size_refusals_precede_owned_payload_and_preserve_incumbent_storage() {
    use PrefabComponentApplyRecoveryError as Error;

    let package = Fixture::new().create(|_| {}).unwrap();
    let mut encoded: Value = serde_json::from_slice(package.encoded_bytes()).unwrap();
    encoded["payload"]["destination"]["file_identity"] = json!(42);
    let encoded = serde_json::to_vec(&encoded).unwrap();
    let defaults = PrefabComponentApplyRecoveryLimits::default();
    let mut budget = budget();
    budget.charge_source_bytes(0, 0, 0, 2_048).unwrap();
    assert!(matches!(
        PrefabComponentApplyRecoveryData::decode(
            &encoded,
            PrefabComponentApplyRecoveryLimits {
                sources: ItemLimit::new(2).unwrap(),
                ..defaults
            },
            &mut budget,
        ),
        Err(Error::SourceCount)
    ));
    assert_eq!(budget.retained_candidate_bytes, 2_048);
    assert!(matches!(
        PrefabComponentApplyRecoveryData::decode(
            &encoded,
            PrefabComponentApplyRecoveryLimits {
                aggregate_bytes: ByteLimit::new(1).unwrap(),
                ..defaults
            },
            &mut budget,
        ),
        Err(Error::AggregateBytesExceeded)
    ));
    assert_eq!(budget.retained_candidate_bytes, 2_048);
    assert!(matches!(
        PrefabComponentApplyRecoveryData::decode(
            &encoded,
            PrefabComponentApplyRecoveryLimits {
                scene: defaults
                    .scene
                    .with_encoded_bytes(ByteLimit::new(1).unwrap()),
                ..defaults
            },
            &mut budget,
        ),
        Err(Error::BlobBytesExceeded)
    ));
    assert_eq!(budget.retained_candidate_bytes, 2_048);
    assert!(matches!(
        PrefabComponentApplyRecoveryData::decode(
            &encoded,
            PrefabComponentApplyRecoveryLimits {
                shape: SerdeShapeLimits::new(
                    defaults.shape.depth(),
                    ItemLimit::new(1).unwrap(),
                    defaults.shape.container_items(),
                    defaults.shape.string_bytes(),
                    defaults.shape.total_string_bytes(),
                ),
                ..defaults
            },
            &mut budget,
        ),
        Err(Error::Shape(_))
    ));
    assert_eq!(budget.retained_candidate_bytes, 2_048);
    assert!(budget.usage().decode_work() > 0);
}
