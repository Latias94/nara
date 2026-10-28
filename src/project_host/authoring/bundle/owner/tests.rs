#![cfg(any(windows, target_os = "linux"))]

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use nara_asset::AssetRef;
use nara_core::ByteLimit;
use nara_fs::{CapabilityRights, DirectoryCapability, FileIdentity, RelativePath};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    AuthoringDocumentSource, AuthoringRecordReadiness, SceneAuthoringSession, SceneComponentRecord,
    SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord,
    SchemaReclassificationWork,
};

use super::*;
use crate::{
    project_host::{
        authoring::{AuthoringProjectAdmissionHost, AuthoringProjectAdmissionLimits},
        file_publication::FilePublicationOutcome,
    },
    project_tooling::host_fs,
};

static NEXT_PROJECT: AtomicU64 = AtomicU64::new(1);
const BUNDLE: &str = "schema/authoring.bundle.json";
const NEW_OWNER: &str = "test.owner.new";
const NEW_COMPONENT: &str = "test.owner.NewValue";

struct Project(PathBuf, DirectoryCapability);

impl Project {
    fn new(bundle: &AuthoringSchemaBundle) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "nara-additive-owner-{}-{stamp}-{}",
            std::process::id(),
            NEXT_PROJECT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("schema")).unwrap();
        fs::write(
            root.join("nara.toml"),
            r#"schema_version = 1
[project]
source_universe = 'e6896809-3ac2-5a42-bfb7-cddc44b3f2ee'
native_document_identity = 'project_location'
name = "additive-owner-test"
[authoring]
schema_bundle = "schema/authoring.bundle.json"
[runtime]
preset = "local-headless"
[window]
enabled = false
[capabilities]
requested = ["runtime-2d"]
"#,
        )
        .unwrap();
        fs::write(root.join(BUNDLE), bundle.to_json_string().unwrap()).unwrap();
        let capability = host_fs::writable_directory(&root).unwrap();
        assert_eq!(capability.options().rights(), CapabilityRights::ReadWrite);
        Self(root, capability)
    }

    fn admit(&self, limits: AuthoringProjectAdmissionLimits) -> Arc<AuthoringSchemaReceipt> {
        Arc::new(
            AuthoringProjectAdmissionHost::new(limits)
                .prepare(self.1.share(), None)
                .unwrap()
                .admit_bundle()
                .unwrap()
                .publish(),
        )
    }

    fn disk(&self) -> (FileIdentity, Vec<u8>) {
        let file = self
            .1
            .open_file(&RelativePath::new(BUNDLE).unwrap())
            .unwrap();
        (
            file.identity(),
            file.read_to_end_bounded(bundle_byte_limit()).unwrap(),
        )
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        // Every leaf is beneath the directory exclusively created by this fixture. Avoid
        // recursive cleanup so a failed test cannot traverse an unexpected replacement tree.
        let mut failures = Vec::new();
        for directory in ["schema", "schema-original"] {
            for leaf in ["authoring.bundle.json", "held.bundle.json", "other.json"] {
                let path = self.0.join(directory).join(leaf);
                if let Err(error) = fs::remove_file(&path)
                    && error.kind() != std::io::ErrorKind::NotFound
                {
                    failures.push(format!("{}: {error}", path.display()));
                }
            }
            let path = self.0.join(directory);
            if let Err(error) = fs::remove_dir(&path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                failures.push(format!("{}: {error}", path.display()));
            }
        }
        for leaf in ["nara.toml", "manifest-original.toml"] {
            let path = self.0.join(leaf);
            if let Err(error) = fs::remove_file(&path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                failures.push(format!("{}: {error}", path.display()));
            }
        }
        if let Err(error) = fs::remove_dir(&self.0) {
            failures.push(format!("{}: {error}", self.0.display()));
        }
        if !failures.is_empty() {
            let details = failures.join("\n");
            if std::thread::panicking() {
                eprintln!("Fixture cleanup failed while preserving the original panic:\n{details}");
            } else {
                panic!("Fixture cleanup failed:\n{details}");
            }
        }
    }
}

fn owner_id(name: &str) -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(name).unwrap()
}

fn schema(component: &str) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new(component),
        "Value",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValueKind::U64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
}

fn generation_one(name: &str, component: &str) -> ComponentSchemaOwnerSnapshot {
    ComponentSchemaOwnerSnapshot::admit(
        owner_id(name),
        ComponentSchemaCatalog {
            components: vec![schema(component)],
            ..Default::default()
        },
        None,
    )
    .unwrap()
}

fn evolved(name: &str) -> ComponentSchemaOwnerSnapshot {
    let active = schema(&format!("{name}.Active"));
    let retired = ComponentTypeId::new(format!("{name}.Retired"));
    let predecessor = ComponentSchemaCatalog {
        components: vec![active.clone(), schema(retired.as_str())],
        type_tombstones: vec![ComponentTypeId::new(format!("{name}.Ancient"))],
        ..Default::default()
    };
    let mut current = ComponentSchemaCatalog::successor_of(&predecessor).unwrap();
    current.components.push(active);
    current.type_tombstones.push(retired);
    ComponentSchemaOwnerSnapshot::admit(owner_id(name), current, Some(predecessor)).unwrap()
}

fn initial_bundle() -> AuthoringSchemaBundle {
    AuthoringSchemaBundle::from_owner_snapshots([
        generation_one("test.owner.existing", "test.owner.Existing"),
        evolved("test.owner.evolved-a"),
        evolved("test.owner.evolved-b"),
    ])
    .unwrap()
}

fn prepare(
    incumbent: &Arc<AuthoringSchemaReceipt>,
) -> (PreparedAuthoringSchemaOwner, FilePublicationHost) {
    match incumbent
        .prepare_owner(&generation_one(NEW_OWNER, NEW_COMPONENT))
        .unwrap()
    {
        AuthoringSchemaOwnerPreparation::Prepared { owner, publication } => (owner, publication),
        AuthoringSchemaOwnerPreparation::AlreadyAdmitted => panic!("owner must be absent"),
    }
}

fn error(
    incumbent: &Arc<AuthoringSchemaReceipt>,
    incoming: &ComponentSchemaOwnerSnapshot,
) -> AuthoringSchemaOwnerError {
    match incumbent.prepare_owner(incoming) {
        Err(error) => error,
        Ok(_) => panic!("owner preparation unexpectedly succeeded"),
    }
}

fn publish(host: &mut FilePublicationHost, bytes: &[u8]) -> AcceptedFilePublication {
    match host.publish(bytes) {
        FilePublicationOutcome::Published(accepted) => accepted,
        FilePublicationOutcome::Rejected(reason) => panic!("publication rejected: {reason:?}"),
        FilePublicationOutcome::Failed(stage) => panic!("publication failed: {stage:?}"),
        FilePublicationOutcome::Uncertain(evidence) => {
            panic!("publication uncertain: {evidence:?}")
        }
    }
}

#[test]
fn generation_one_addition_preserves_all_lineage_and_attaches_the_actual_leaf_without_readmission()
{
    let project = Project::new(&initial_bundle());
    let incumbent = project.admit(Default::default());
    let weak_incumbent = Arc::downgrade(&incumbent);
    let before = project.disk();
    let previous_owners = incumbent.bundle().owners().to_vec();
    let previous_attempt = incumbent.attempt_generation();
    let previous_manifest = incumbent.manifest_identity();
    let previous_root = incumbent.root_identity();
    let previous_basis = AuthoringSchemaBasis::admit(
        incumbent.bundle(),
        &project_authoring_adapters(incumbent.bundle()).unwrap(),
    )
    .unwrap();
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("root").unwrap(),
    )
    .with_component(
        ComponentTypeId::new(NEW_COMPONENT),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("value", ComponentValue::U64(7))]),
        )
        .with_owner(owner_id(NEW_OWNER)),
    )]);
    let mut source = SceneAuthoringSession::from_published_authoring_document(
        SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path("scenes/independent.json").unwrap()),
                &previous_basis,
            )
            .unwrap()
            .publish(),
    );
    let source_document = source.shared_document();
    let source_revision = source.revision();
    let source_content = source.content_state();
    let (prepared, mut publisher) = prepare(&incumbent);
    assert!(Arc::ptr_eq(prepared.incumbent(), &incumbent));
    assert_eq!(prepared.bundle().owners().len(), previous_owners.len() + 1);
    for previous in &previous_owners {
        let retained = prepared.bundle().owner(previous.owner_id()).unwrap();
        assert!(previous.ptr_eq(retained));
        assert_eq!(retained.current(), previous.current());
        assert_eq!(retained.predecessor(), previous.predecessor());
    }
    prepared.validate().unwrap();
    let basis = prepared.schema_basis().clone();
    let bytes = prepared.bytes();
    assert!(Arc::ptr_eq(&bytes, &prepared.bytes()));
    assert_eq!(
        AuthoringSchemaBundle::from_json_bytes(&bytes).unwrap(),
        *prepared.bundle()
    );
    let reviewed_bundle = Arc::clone(&prepared.bundle);
    let source_candidate = source
        .prepare_schema_reclassification(&basis, &mut SchemaReclassificationWork::default())
        .unwrap();
    assert_eq!(project.disk(), before);
    assert_eq!(incumbent.bundle_identity(), before.0);
    assert_eq!(incumbent.bundle_bytes(), before.1);
    assert!(Arc::ptr_eq(&source_document, &source.shared_document()));
    assert_eq!(source.revision(), source_revision);
    assert_eq!(source.authoring_schema_basis(), Some(previous_basis));

    let accepted = publish(&mut publisher, &bytes);
    let accepted_identity = accepted.source().identity();
    assert_ne!(accepted_identity, before.0);
    let admitted = prepared.attach(accepted).unwrap();
    assert_eq!(admitted.basis, basis);
    assert!(Arc::ptr_eq(&bytes, &admitted.schema.bundle.bytes));
    assert!(Arc::ptr_eq(
        &reviewed_bundle,
        &admitted.schema.bundle.bundle
    ));
    assert!(!Arc::ptr_eq(&incumbent, &admitted.schema));
    assert_eq!(admitted.schema.bundle_identity(), accepted_identity);
    assert_eq!(
        admitted.schema.bundle_digest(),
        ContentDigest::of_bytes(&bytes)
    );
    assert_eq!(admitted.schema.attempt_generation(), previous_attempt);
    assert_eq!(admitted.schema.manifest_identity(), previous_manifest);
    assert_eq!(admitted.schema.root_identity(), previous_root);
    assert_eq!(
        admitted.publication.receipt().previous_identity(),
        Some(before.0)
    );
    assert_eq!(
        admitted.publication.receipt().published_identity(),
        Some(accepted_identity)
    );
    let independently_admitted =
        AuthoringSchemaBasis::admit(admitted.schema.bundle(), &admitted.adapters).unwrap();
    assert_ne!(independently_admitted, admitted.basis);
    drop(incumbent);
    assert!(weak_incumbent.upgrade().is_none());
    drop(publisher);
    fs::rename(
        project.0.join(BUNDLE),
        project.0.join("schema/held.bundle.json"),
    )
    .unwrap();
    fs::remove_file(project.0.join("schema/held.bundle.json")).unwrap();
    assert_eq!(
        admitted
            .schema
            .bundle
            .capability
            .read_to_end_bounded(bundle_byte_limit())
            .unwrap(),
        bytes.as_ref(),
    );
    admitted
        .schema
        .bundle
        .capability
        .verify_digest(ContentDigest::of_bytes(&bytes), bundle_byte_limit())
        .unwrap();
    // This is an independent source consumer, not integrated project/Workspace adoption.
    source
        .commit_schema_reclassification(source_candidate)
        .unwrap();
    assert_eq!(source.authoring_schema_basis(), Some(admitted.basis));
    let recovered = &source.authoring_record_facts().unwrap()[0];
    assert_eq!(recovered.readiness(), AuthoringRecordReadiness::Available);
    assert!(!recovered.adapter().unwrap().supports_native_preview());
    assert!(Arc::ptr_eq(&source_document, &source.shared_document()));
    assert_eq!(source.revision(), source_revision);
    assert_eq!(source.content_state(), source_content);
}

#[test]
fn identical_owner_returns_already_admitted_without_rewriting_bytes_or_leaf() {
    let project = Project::new(&initial_bundle());
    let incumbent = project.admit(Default::default());
    let before = project.disk();
    let existing = generation_one("test.owner.existing", "test.owner.Existing");
    assert!(matches!(
        incumbent.prepare_owner(&existing),
        Ok(AuthoringSchemaOwnerPreparation::AlreadyAdmitted)
    ));
    assert_eq!(project.disk(), before);
    assert_eq!(incumbent.bundle_identity(), before.0);
    assert_eq!(incumbent.bundle_bytes(), before.1);
    assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
}

#[test]
fn identical_evolved_owner_is_a_noop_but_changed_current_or_predecessor_is_rejected() {
    let project = Project::new(&initial_bundle());
    let incumbent = project.admit(Default::default());
    let before = project.disk();
    let existing = incumbent
        .bundle()
        .owner(&owner_id("test.owner.evolved-a"))
        .unwrap();
    let equal = ComponentSchemaOwnerSnapshot::admit(
        existing.owner_id().clone(),
        existing.current().clone(),
        existing.predecessor().cloned(),
    )
    .unwrap();
    assert_eq!(equal.current().generation(), 2);
    assert!(equal.predecessor().is_some());
    assert!(!equal.ptr_eq(existing));
    assert!(matches!(
        incumbent.prepare_owner(&equal),
        Ok(AuthoringSchemaOwnerPreparation::AlreadyAdmitted)
    ));
    assert_eq!(project.disk(), before);

    let mut changed_current = existing.current().clone();
    changed_current.components[0].authoring_initial_value =
        Some(ComponentValue::map([("value", ComponentValue::U64(9))]));
    let changed_current = ComponentSchemaOwnerSnapshot::admit(
        existing.owner_id().clone(),
        changed_current,
        existing.predecessor().cloned(),
    )
    .unwrap();
    assert_eq!(changed_current.predecessor(), existing.predecessor());

    let mut changed_predecessor = existing.predecessor().unwrap().clone();
    changed_predecessor
        .components
        .iter_mut()
        .find(|component| component.id() == &ComponentTypeId::new("test.owner.evolved-a.Retired"))
        .unwrap()
        .authoring_initial_value = Some(ComponentValue::map([("value", ComponentValue::U64(19))]));
    let mut relinked_current = existing.current().clone();
    relinked_current.predecessor = Some(changed_predecessor.fingerprint());
    let changed_predecessor = ComponentSchemaOwnerSnapshot::admit(
        existing.owner_id().clone(),
        relinked_current,
        Some(changed_predecessor),
    )
    .unwrap();
    assert_eq!(
        changed_predecessor.current().components(),
        existing.current().components()
    );
    assert_ne!(changed_predecessor.predecessor(), existing.predecessor());
    for changed in [&changed_current, &changed_predecessor] {
        assert!(matches!(
            error(&incumbent, changed),
            AuthoringSchemaOwnerError::ExistingOwnerChanged
        ));
        assert_eq!(project.disk(), before);
        assert_eq!(incumbent.bundle_identity(), before.0);
        assert_eq!(incumbent.bundle_bytes(), before.1);
    }
    assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
}

#[test]
fn conflicting_existing_owner_evolved_incoming_and_all_historical_claim_collisions_refuse() {
    let project = Project::new(&initial_bundle());
    let incumbent = project.admit(Default::default());
    let before = project.disk();
    let conflicting = generation_one("test.owner.existing", "test.owner.Different");
    assert!(matches!(
        error(&incumbent, &conflicting),
        AuthoringSchemaOwnerError::ExistingOwnerChanged
    ));
    assert!(matches!(
        error(&incumbent, &evolved(NEW_OWNER)),
        AuthoringSchemaOwnerError::UnsupportedLineage
    ));
    for component in [
        "test.owner.Existing",
        "test.owner.evolved-a.Active",
        "test.owner.evolved-a.Retired",
        "test.owner.evolved-a.Ancient",
    ] {
        let incoming = generation_one(NEW_OWNER, component);
        assert!(matches!(
            error(&incumbent, &incoming),
            AuthoringSchemaOwnerError::Composition(_)
        ));
    }
    assert_eq!(project.disk(), before);
    assert_eq!(incumbent.bundle_bytes(), before.1);
}

#[test]
fn external_bundle_rewrite_and_disk_successor_with_live_predecessor_never_return_already_admitted()
{
    for successor_on_disk in [false, true] {
        let project = Project::new(&initial_bundle());
        let incumbent = project.admit(Default::default());
        let previous_identity = incumbent.bundle_identity();
        let previous_bytes = incumbent.bundle_bytes().to_vec();
        let incoming = if successor_on_disk {
            let (prepared, mut publisher) = prepare(&incumbent);
            let accepted = publish(&mut publisher, &prepared.bytes());
            assert_ne!(accepted.source().identity(), previous_identity);
            drop(accepted);
            generation_one(NEW_OWNER, NEW_COMPONENT)
        } else {
            fs::write(project.0.join(BUNDLE), b"external bundle rewrite").unwrap();
            generation_one("test.owner.existing", "test.owner.Existing")
        };
        assert!(matches!(
            error(&incumbent, &incoming),
            AuthoringSchemaOwnerError::BundleChanged
        ));
        assert_eq!(incumbent.bundle_identity(), previous_identity);
        assert_eq!(incumbent.bundle_bytes(), previous_bytes);
    }
}

#[test]
fn manifest_change_or_replaced_bundle_parent_invalidates_review_before_write() {
    for change in 0..4 {
        let project = Project::new(&initial_bundle());
        let incumbent = project.admit(Default::default());
        let before = project.disk();
        let (prepared, publisher) = prepare(&incumbent);
        match change {
            0 => fs::write(project.0.join("nara.toml"), b"changed manifest").unwrap(),
            1 => {
                let bytes = fs::read(project.0.join("nara.toml")).unwrap();
                fs::rename(
                    project.0.join("nara.toml"),
                    project.0.join("manifest-original.toml"),
                )
                .unwrap();
                fs::write(project.0.join("nara.toml"), bytes).unwrap();
            }
            2 => fs::write(project.0.join(BUNDLE), b"changed bundle after review").unwrap(),
            3 => {
                match fs::rename(project.0.join("schema"), project.0.join("schema-original")) {
                    Ok(()) => {}
                    #[cfg(windows)]
                    Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                        // Retained parent/leaf custody can prevent directory renaming on
                        // Windows. This refusal leaves the original observation valid.
                        assert!(project.0.join("schema").is_dir());
                        assert!(!project.0.join("schema-original").exists());
                        assert_eq!(project.disk(), before);
                        assert_eq!(incumbent.bundle_identity(), before.0);
                        assert_eq!(incumbent.bundle_bytes(), before.1);
                        assert_eq!(publisher.test_accepted_source_bytes().unwrap(), before.1);
                        prepared.validate().unwrap();
                        drop(publisher);
                        drop(prepared);
                        drop(incumbent);
                        // Prove that releasing the actual custody permits the intended
                        // rename; unrelated permission failures must still fail this test.
                        fs::rename(project.0.join("schema"), project.0.join("schema-original"))
                            .unwrap();
                        assert!(!project.0.join("schema").exists());
                        let moved = project
                            .1
                            .open_file(
                                &RelativePath::new("schema-original/authoring.bundle.json")
                                    .unwrap(),
                            )
                            .unwrap();
                        assert_eq!(moved.identity(), before.0);
                        assert_eq!(
                            moved.read_to_end_bounded(bundle_byte_limit()).unwrap(),
                            before.1
                        );
                        continue;
                    }
                    Err(error) => panic!("bundle parent rename failed: {error}"),
                }
                fs::create_dir(project.0.join("schema")).unwrap();
                // Move the identical leaf into a different parent, isolating parent identity.
                fs::rename(
                    project.0.join("schema-original/authoring.bundle.json"),
                    project.0.join(BUNDLE),
                )
                .unwrap();
                assert_eq!(project.disk(), before);
            }
            _ => unreachable!(),
        }
        let incoming = generation_one("test.owner.existing", "test.owner.Existing");
        if change < 2 {
            assert!(matches!(
                prepared.validate(),
                Err(AuthoringSchemaOwnerError::ManifestChanged)
            ));
            assert!(matches!(
                error(&incumbent, &incoming),
                AuthoringSchemaOwnerError::ManifestChanged
            ));
            assert_eq!(project.disk(), before);
        } else {
            assert!(matches!(
                prepared.validate(),
                Err(AuthoringSchemaOwnerError::BundleChanged)
            ));
            assert!(matches!(
                error(&incumbent, &incoming),
                AuthoringSchemaOwnerError::BundleChanged
            ));
        }
        assert_eq!(incumbent.bundle_identity(), before.0);
        assert_eq!(incumbent.bundle_bytes(), before.1);
        assert_eq!(
            publisher.test_accepted_source_bytes().unwrap(),
            fs::read(project.0.join(BUNDLE)).unwrap()
        );
        drop(publisher);
    }
}

#[test]
fn attachment_refuses_wrong_accepted_leaf_and_retains_exact_publication_evidence() {
    let project = Project::new(&initial_bundle());
    let incumbent = project.admit(Default::default());
    let before = project.disk();
    let (prepared, _publisher) = prepare(&incumbent);
    let basis = prepared.schema_basis().clone();
    let bytes = prepared.bytes();
    let mut wrong = FilePublicationHost::bind_missing(
        &incumbent.project_root(),
        "schema/other.json",
        bundle_byte_limit(),
    )
    .unwrap();
    let accepted = publish(&mut wrong, &bytes);
    let identity = accepted.source().identity();
    let digest = accepted.source().digest();
    let failure = match prepared.attach(accepted) {
        Err(failure) => failure,
        Ok(_) => panic!("an unrelated accepted publication must not attach"),
    };
    assert_eq!(failure.prepared.schema_basis(), &basis);
    assert!(Arc::ptr_eq(&failure.prepared.bytes(), &bytes));
    failure.prepared.validate().unwrap();
    assert_eq!(failure.publication.source().identity(), identity);
    assert_eq!(failure.publication.source().digest(), digest);
    assert_eq!(
        failure
            .publication
            .evidence()
            .receipt()
            .published_identity(),
        Some(identity)
    );
    assert_eq!(project.disk(), before);
    fs::remove_file(project.0.join("schema/other.json")).unwrap();
    let (_, source) = failure.publication.into_parts();
    let (file, observed_digest) = source.into_parts();
    assert_eq!(observed_digest, digest);
    assert_eq!(file.identity(), identity);
    assert_eq!(
        file.read_to_end_bounded(bundle_byte_limit()).unwrap(),
        bytes.as_ref()
    );
}

#[test]
fn renamed_incumbent_leaf_cannot_attach_when_publication_targets_another_name() {
    let project = Project::new(&initial_bundle());
    let incumbent = project.admit(Default::default());
    let before = project.disk();
    let (prepared, original_publisher) = prepare(&incumbent);
    let basis = prepared.schema_basis().clone();
    let bytes = prepared.bytes();
    prepared.validate().unwrap();
    drop(original_publisher);
    fs::rename(project.0.join(BUNDLE), project.0.join("schema/other.json")).unwrap();
    assert!(!project.0.join(BUNDLE).exists());
    assert_eq!(
        incumbent
            .bundle
            .capability()
            .read_to_end_bounded(bundle_byte_limit())
            .unwrap(),
        before.1
    );
    let mut wrong = FilePublicationHost::bind_existing(
        &incumbent.project_root(),
        "schema/other.json",
        incumbent.bundle.capability(),
        incumbent.bundle_digest(),
        bundle_byte_limit(),
    )
    .unwrap();
    let accepted = publish(&mut wrong, &bytes);
    let identity = accepted.source().identity();
    let digest = ContentDigest::of_bytes(&bytes);
    // All pre-existing attachment checks match. Only the actual published target differs.
    assert_eq!(
        accepted.evidence().receipt().previous_identity(),
        Some(before.0)
    );
    assert_eq!(accepted.evidence().receipt().candidate_identity(), identity);
    assert_eq!(
        accepted.evidence().receipt().published_identity(),
        Some(identity)
    );
    assert_eq!(
        accepted.evidence().receipt().verified_content_digest(),
        Some(digest)
    );
    assert_eq!(accepted.source().digest(), digest);
    assert_eq!(
        accepted.source().parent_identity(),
        incumbent.bundle_parent_identity()
    );
    assert_eq!(accepted.target().to_str(), Some("other.json"));
    let failure = match prepared.attach(accepted) {
        Err(failure) => failure,
        Ok(_) => panic!("a renamed predecessor does not authorize publication at another leaf"),
    };
    assert!(Arc::ptr_eq(failure.prepared.incumbent(), &incumbent));
    assert_eq!(failure.prepared.schema_basis(), &basis);
    assert!(Arc::ptr_eq(&failure.prepared.bytes(), &bytes));
    assert_eq!(failure.publication.target().to_str(), Some("other.json"));
    assert_eq!(failure.publication.source().identity(), identity);
    assert_eq!(failure.publication.source().digest(), digest);
    assert_eq!(
        failure.publication.evidence().receipt().previous_identity(),
        Some(before.0)
    );
    assert_eq!(incumbent.bundle_identity(), before.0);
    assert_eq!(incumbent.bundle_bytes(), before.1);
    fs::remove_file(project.0.join("schema/other.json")).unwrap();
    let (_, source) = failure.publication.into_parts();
    let (file, observed_digest) = source.into_parts();
    assert_eq!(observed_digest, digest);
    assert_eq!(file.identity(), identity);
    assert_eq!(
        file.read_to_end_bounded(bundle_byte_limit()).unwrap(),
        bytes.as_ref()
    );
}

#[test]
fn correct_target_publication_with_different_payload_retains_evidence_for_reconciliation() {
    let project = Project::new(&initial_bundle());
    let incumbent = project.admit(Default::default());
    let before = project.disk();
    let (prepared, mut publisher) = prepare(&incumbent);
    let basis = prepared.schema_basis().clone();
    let reviewed_bytes = prepared.bytes();
    let different_bytes: &[u8] = b"published payload differs from the reviewed bundle";
    let accepted = publish(&mut publisher, different_bytes);
    let identity = accepted.source().identity();
    assert_eq!(
        accepted.evidence().receipt().previous_identity(),
        Some(before.0)
    );
    let failure = match prepared.attach(accepted) {
        Err(failure) => failure,
        Ok(_) => panic!("a different accepted payload must not attach"),
    };
    assert_eq!(failure.prepared.schema_basis(), &basis);
    assert!(Arc::ptr_eq(&failure.prepared.bytes(), &reviewed_bytes));
    assert_eq!(failure.publication.source().identity(), identity);
    assert_eq!(
        failure.publication.source().digest(),
        ContentDigest::of_bytes(different_bytes)
    );
    assert_eq!(project.disk().1, different_bytes);
    assert_eq!(incumbent.bundle_identity(), before.0);
    assert_eq!(incumbent.bundle_bytes(), before.1);
    assert!(matches!(
        failure.prepared.validate(),
        Err(AuthoringSchemaOwnerError::BundleChanged)
    ));
    fs::remove_file(project.0.join(BUNDLE)).unwrap();
    let (_, source) = failure.publication.into_parts();
    let (file, digest) = source.into_parts();
    assert_eq!(file.identity(), identity);
    file.verify_digest(digest, bundle_byte_limit()).unwrap();
    assert_eq!(
        file.read_to_end_bounded(bundle_byte_limit()).unwrap(),
        different_bytes
    );
}

#[test]
fn real_publisher_known_failure_leaves_preparation_and_incumbent_available_for_retry() {
    let project = Project::new(&initial_bundle());
    let incumbent = project.admit(Default::default());
    let before = project.disk();
    let (prepared, mut publisher) = prepare(&incumbent);
    let bytes = prepared.bytes();
    let basis = prepared.schema_basis().clone();
    let cleanup = publisher.test_fail_after_write();
    assert!(matches!(
        publisher.publish(&bytes),
        FilePublicationOutcome::Failed(FilePublicationFailureStage::WriteTemporary)
    ));
    assert_eq!(project.disk(), before);
    assert!(!publisher.uncertain());
    assert!(publisher.has_cleanup());
    prepared.validate().unwrap();
    cleanup.release();
    assert!(publisher.retry_cleanup());
    let accepted = publish(&mut publisher, &bytes);
    let admitted = prepared.attach(accepted).unwrap();
    assert_eq!(admitted.basis, basis);
    assert_eq!(project.disk().1, bytes.as_ref());
}

#[test]
fn failed_later_budget_charge_blocks_validation_and_attachment_without_losing_actual_proof() {
    let project = Project::new(&initial_bundle());
    let incumbent = project.admit(Default::default());
    let before = project.disk();
    let (mut prepared, mut publisher) = prepare(&incumbent);
    let basis = prepared.schema_basis().clone();
    let bytes = prepared.bytes();
    prepared.validate().unwrap();

    // An earlier successful charge cannot authorize reuse after a later capture exceeds
    // the saved allowance. Use the owning accounting method, which charges before refusal.
    prepared.budget.charge_source_bytes(0, 0, 0, 16).unwrap();
    let retained_before = prepared.budget.usage().retained_candidate_bytes();
    let maximum = retained_before + 32;
    prepared.budget.limits.retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
    prepared.validate().unwrap();
    assert!(matches!(
        prepared.budget.charge_source_bytes(0, 0, 0, 64),
        Err(AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
            maximum: observed,
        }) if observed == maximum
    ));
    assert_eq!(
        prepared.budget.usage().retained_candidate_bytes(),
        retained_before + 64
    );
    assert!(matches!(
        prepared.validate(),
        Err(AuthoringSchemaOwnerError::Budget(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
                maximum: observed,
            }
        )) if observed == maximum
    ));
    assert_eq!(project.disk(), before);

    // Deliberately bypass validation: even otherwise matching accepted evidence cannot
    // attach an owner whose saved budget is already over its finite limit.
    let accepted = publish(&mut publisher, &bytes);
    let identity = accepted.source().identity();
    let digest = ContentDigest::of_bytes(&bytes);
    assert_eq!(
        accepted.evidence().receipt().previous_identity(),
        Some(before.0)
    );
    assert_eq!(accepted.evidence().receipt().candidate_identity(), identity);
    assert_eq!(
        accepted.evidence().receipt().published_identity(),
        Some(identity)
    );
    assert_eq!(
        accepted.evidence().receipt().verified_content_digest(),
        Some(digest)
    );
    assert_eq!(accepted.source().digest(), digest);
    assert_eq!(
        accepted.source().parent_identity(),
        incumbent.bundle_parent_identity()
    );
    let failure = match prepared.attach(accepted) {
        Err(failure) => failure,
        Ok(_) => panic!("an over-budget owner must not attach even after actual publication"),
    };
    assert_eq!(failure.prepared.schema_basis(), &basis);
    assert!(Arc::ptr_eq(&failure.prepared.bytes(), &bytes));
    assert_eq!(
        failure.prepared.budget.usage().retained_candidate_bytes(),
        retained_before + 64
    );
    assert!(matches!(
        failure.prepared.validate(),
        Err(AuthoringSchemaOwnerError::Budget(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
                maximum: observed,
            }
        )) if observed == maximum
    ));
    assert_eq!(failure.publication.source().identity(), identity);
    assert_eq!(failure.publication.source().digest(), digest);
    assert_eq!(incumbent.bundle_identity(), before.0);
    assert_eq!(incumbent.bundle_bytes(), before.1);
    assert_eq!(project.disk(), (identity, bytes.to_vec()));
    fs::remove_file(project.0.join(BUNDLE)).unwrap();
    let (_, source) = failure.publication.into_parts();
    let (file, retained_digest) = source.into_parts();
    assert_eq!(retained_digest, digest);
    assert_eq!(file.identity(), identity);
    assert_eq!(
        file.read_to_end_bounded(bundle_byte_limit()).unwrap(),
        bytes.as_ref()
    );
}

#[test]
fn incumbent_encoded_budget_can_admit_existing_bundle_but_refuse_the_addition() {
    let project = Project::new(&initial_bundle());
    let before = project.disk();
    let manifest_bytes = fs::read(project.0.join("nara.toml")).unwrap().len();
    let limit = manifest_bytes + before.1.len();
    let incumbent = project.admit(AuthoringProjectAdmissionLimits::new(
        ByteLimit::new(limit).unwrap(),
    ));
    assert!(matches!(
        error(&incumbent, &generation_one(NEW_OWNER, NEW_COMPONENT)),
        AuthoringSchemaOwnerError::Budget(
            AuthoringProjectAdmissionError::AggregateEncodedBytesExceeded { maximum }
        ) if maximum == limit
    ));
    assert_eq!(project.disk(), before);
}
