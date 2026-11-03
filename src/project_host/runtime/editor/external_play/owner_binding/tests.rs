use std::{
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    time::{Duration, Instant},
};

use nara_asset::{AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId};
use nara_fs::ContentDigest;
use nara_scene::SceneEntityId;
use nara_tooling::{
    EditorExternalPlayObservation, EditorExternalPlayOutput, EditorExternalPlayState,
    EditorPlayCommand, EditorPlayRequestResult,
};

use crate::project_host::{
    EditorProjectCloseProgress,
    authoring::{
        AuthoringProjectAdmissionHost, PreparedAuthoringProject, project_authoring_adapters,
    },
};

use super::super::super::prefab_extraction::{
    EditorPrefabExtractionError, EditorPrefabExtractionRequest,
};
use super::super::source_fact_tests::SourceReceiptProject;
use super::*;

fn open(project: &SourceReceiptProject) -> EditorProjectSession {
    let schema = AuthoringProjectAdmissionHost::default()
        .prepare(project.capability(), Some("desktop"))
        .unwrap()
        .admit_bundle()
        .unwrap()
        .publish();
    let adapters = project_authoring_adapters(schema.bundle()).unwrap();
    let receipt = PreparedAuthoringProject::new(schema, adapters)
        .admit_startup_closure()
        .unwrap()
        .publish();
    EditorProjectSession::open(receipt, None).unwrap()
}

fn with_sessions(test: impl FnOnce(&mut EditorProjectSession, &mut EditorProjectSession)) {
    with_project_sessions(SourceReceiptProject::new(&[]), test);
}

fn with_project_sessions(
    project: SourceReceiptProject,
    test: impl FnOnce(&mut EditorProjectSession, &mut EditorProjectSession),
) {
    let mut first = open(&project);
    let mut second = open(&project);
    let result = catch_unwind(AssertUnwindSafe(|| test(&mut first, &mut second)));
    for session in [&mut first, &mut second] {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !matches!(
            session.advance_close(),
            EditorProjectCloseProgress::Complete(_)
        ) {
            assert!(
                Instant::now() < deadline,
                "the binding test Session did not close"
            );
            std::thread::yield_now();
        }
    }
    if let Err(payload) = result {
        resume_unwind(payload);
    }
}

fn digest(bytes: &[u8]) -> ProjectAuthoringRecoveryDigest {
    let digest = ContentDigest::of_bytes(bytes);
    ProjectAuthoringRecoveryDigest::from_parts(digest.length(), *digest.as_bytes())
}

fn begin(session: &mut EditorProjectSession) -> EditorAuthoredSchemaUpgradeId {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match session.begin_authored_schema_upgrade() {
            Ok(operation) => return operation,
            Err(crate::project_host::EditorAuthoredSchemaUpgradeError::Busy) => {
                assert!(
                    Instant::now() < deadline,
                    "initial source owners did not settle"
                );
                session.drive_editor_frame(Duration::ZERO);
                std::thread::yield_now();
            }
            Err(error) => panic!("upgrade acceptance failed: {error:?}"),
        }
    }
}

#[test]
fn registration_is_once_only_and_equal_generation_labels_do_not_grant_custody() {
    with_sessions(|session, other| {
        let binding = session.register_supervised_play_owner(13).unwrap();
        let foreign = other.register_supervised_play_owner(13).unwrap();
        assert_eq!(
            binding.published_generation(),
            foreign.published_generation()
        );
        assert!(matches!(
            session.register_supervised_play_owner(13),
            Err(EditorSupervisedPlayOwnerError::AlreadyRegistered)
        ));
        assert_eq!(
            session.request_external_play(EditorPlayCommand::Play),
            EditorPlayRequestResult::Accepted
        );
        assert!(session.take_external_play_request(&foreign).is_none());
        let request = session.take_external_play_request(&binding).unwrap();
        let observation = || {
            EditorExternalPlayObservation::new(
                request.attempt(),
                EditorExternalPlayState::Running,
                Some(1),
                false,
                request.source_receipt().map(ContentDigest::of_bytes),
                EditorExternalPlayOutput::default(),
                None,
            )
            .unwrap()
        };
        assert!(!session.apply_external_play_observation(&foreign, observation()));
        assert!(session.apply_external_play_observation(&binding, observation()));
        drop(binding);
        assert!(matches!(
            session.register_supervised_play_owner(14),
            Err(EditorSupervisedPlayOwnerError::AlreadyRegistered)
        ));
    });
}

#[test]
fn an_unattached_session_cannot_export_a_launch_or_accept_an_external_observation() {
    with_sessions(|session, other| {
        let foreign = other.register_supervised_play_owner(1).unwrap();
        assert_eq!(
            session.request_external_play(EditorPlayCommand::Play),
            EditorPlayRequestResult::Accepted
        );
        let request = session.external_play_request.as_ref().unwrap();
        let observation = EditorExternalPlayObservation::new(
            request.attempt(),
            EditorExternalPlayState::Running,
            Some(1),
            false,
            None,
            EditorExternalPlayOutput::default(),
            None,
        )
        .unwrap();
        assert!(session.take_external_play_request(&foreign).is_none());
        assert!(!session.apply_external_play_observation(&foreign, observation));
        let operation = EditorSourceOperationId::AuthoredSchemaUpgrade(begin(session));
        session
            .begin_supervised_play_retirement(operation, digest(b"unattached"))
            .unwrap();
        assert!(!session.supervised_play_retirement_pending());
        session.finish_supervised_play_retirement().unwrap();
        assert!(matches!(
            session.register_supervised_play_owner(1),
            Err(EditorSupervisedPlayOwnerError::Operation)
        ));
    });
}

#[test]
fn retirement_requires_the_original_request_and_rejects_foreign_and_replayed_attestations() {
    with_sessions(|session, other| {
        let binding = session.register_supervised_play_owner(23).unwrap();
        let foreign = other.register_supervised_play_owner(23).unwrap();
        let operation = EditorSourceOperationId::AuthoredSchemaUpgrade(begin(session));
        let recovery_digest = digest(b"reviewed recovery");
        session
            .begin_supervised_play_retirement(operation, recovery_digest)
            .unwrap();
        assert!(session.supervised_play_retirement_pending());
        assert_eq!(
            session.finish_supervised_play_retirement(),
            Err(EditorSupervisedPlayOwnerError::Pending)
        );
        assert!(matches!(
            session.supervised_play_retirement_request(&foreign),
            Err(EditorSupervisedPlayOwnerError::ForeignOwner)
        ));
        let first = session
            .supervised_play_retirement_request(&binding)
            .unwrap()
            .unwrap();
        let repeated = session
            .supervised_play_retirement_request(&binding)
            .unwrap()
            .unwrap();
        assert!(first.is_same_request(&repeated));
        assert_eq!(first.operation(), operation);
        assert_eq!(first.recovery_digest(), recovery_digest);
        assert_eq!(first.published_generation(), 23);
        assert!(matches!(
            foreign.attest_retirement(repeated),
            Err(EditorSupervisedPlayOwnerError::ForeignOwner)
        ));
        // Only the concrete Editor owner exercises this trust boundary in production. This
        // owner-conformance test issues two proofs to verify Root's replay rejection.
        let replay = binding
            .attest_retirement(
                session
                    .supervised_play_retirement_request(&binding)
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
        let after_finish = binding
            .attest_retirement(
                session
                    .supervised_play_retirement_request(&binding)
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            session.begin_supervised_play_retirement(operation, digest(b"other recovery")),
            Err(EditorSupervisedPlayOwnerError::Request)
        );
        let acknowledgment = binding.attest_retirement(first).unwrap();
        session
            .apply_supervised_play_retirement_ack(&binding, acknowledgment)
            .unwrap();
        assert!(!session.supervised_play_retirement_pending());
        assert!(
            session
                .supervised_play_retirement_request(&binding)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            session.apply_supervised_play_retirement_ack(&binding, replay),
            Err(EditorSupervisedPlayOwnerError::AlreadyAcknowledged)
        );
        session.finish_supervised_play_retirement().unwrap();
        session
            .begin_supervised_play_retirement(operation, recovery_digest)
            .unwrap();
        assert_eq!(
            session.apply_supervised_play_retirement_ack(&binding, after_finish),
            Err(EditorSupervisedPlayOwnerError::Request)
        );
        let acknowledgment = binding
            .attest_retirement(
                session
                    .supervised_play_retirement_request(&binding)
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
        session
            .apply_supervised_play_retirement_ack(&binding, acknowledgment)
            .unwrap();
        session.finish_supervised_play_retirement().unwrap();
    });
}

#[test]
fn equal_source_bytes_with_a_new_nominal_schema_authority_cannot_satisfy_retirement() {
    with_sessions(|session, other| {
        let binding = session.register_supervised_play_owner(31).unwrap();
        let operation = EditorSourceOperationId::AuthoredSchemaUpgrade(begin(session));
        session
            .begin_supervised_play_retirement(operation, digest(b"schema custody"))
            .unwrap();
        let acknowledgment = binding
            .attest_retirement(
                session
                    .supervised_play_retirement_request(&binding)
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            session
                .authoring_project()
                .unwrap()
                .authoring_bundle_digest(),
            other.authoring_project().unwrap().authoring_bundle_digest()
        );
        std::mem::swap(&mut session.execution, &mut other.execution);
        let refused = session.apply_supervised_play_retirement_ack(&binding, acknowledgment);
        std::mem::swap(&mut session.execution, &mut other.execution);
        assert_eq!(refused, Err(EditorSupervisedPlayOwnerError::Stale));
        assert!(session.supervised_play_retirement_pending());
        let acknowledgment = binding
            .attest_retirement(
                session
                    .supervised_play_retirement_request(&binding)
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
        session
            .apply_supervised_play_retirement_ack(&binding, acknowledgment)
            .unwrap();
        session.finish_supervised_play_retirement().unwrap();
    });
}

#[test]
fn a_retirement_cannot_be_reused_for_another_live_operation_or_registration() {
    with_sessions(|session, other| {
        let binding = session.register_supervised_play_owner(41).unwrap();
        let foreign = other.register_supervised_play_owner(41).unwrap();
        let operation = EditorSourceOperationId::AuthoredSchemaUpgrade(begin(session));
        let foreign_operation = EditorSourceOperationId::AuthoredSchemaUpgrade(begin(other));
        let recovery_digest = digest(b"same recovery bytes");
        assert_eq!(
            session.begin_supervised_play_retirement(foreign_operation, recovery_digest),
            Err(EditorSupervisedPlayOwnerError::Operation)
        );
        session
            .begin_supervised_play_retirement(operation, recovery_digest)
            .unwrap();
        other
            .begin_supervised_play_retirement(foreign_operation, recovery_digest)
            .unwrap();
        let foreign_ack = foreign
            .attest_retirement(
                other
                    .supervised_play_retirement_request(&foreign)
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            session.apply_supervised_play_retirement_ack(&binding, foreign_ack),
            Err(EditorSupervisedPlayOwnerError::Request)
        );
        assert!(session.supervised_play_retirement_pending());
        for (session, binding) in [(session, binding), (other, foreign)] {
            let acknowledgment = binding
                .attest_retirement(
                    session
                        .supervised_play_retirement_request(&binding)
                        .unwrap()
                        .unwrap(),
                )
                .unwrap();
            session
                .apply_supervised_play_retirement_ack(&binding, acknowledgment)
                .unwrap();
            session.finish_supervised_play_retirement().unwrap();
        }
    });
}

#[test]
fn authored_upgrade_and_prefab_extraction_retirements_do_not_share_nominal_authority() {
    let project = SourceReceiptProject::new(&[AssetRef::path("shared.prefab.json").unwrap()]);
    with_project_sessions(project, |session, other| {
        let binding = session.register_supervised_play_owner(47).unwrap();
        let other_binding = other.register_supervised_play_owner(47).unwrap();
        let upgrade = EditorSourceOperationId::AuthoredSchemaUpgrade(begin(session));
        let document = other.workspace.active_document().unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let extraction = loop {
            let request = EditorPrefabExtractionRequest::new(
                document,
                SceneEntityId::new("instance-0").unwrap(),
                AssetMetaIdentity::new(
                    StableAssetId::parse_str("812b243a-3d27-4151-a2c0-41710fbcf42b").unwrap(),
                    AssetPath::new("extracted.prefab.json").unwrap(),
                    AssetSourceKind::Prefab,
                ),
            );
            match other.begin_prefab_extraction(request) {
                Ok(operation) => break EditorSourceOperationId::PrefabExtraction(operation),
                Err(EditorPrefabExtractionError::Busy) => {
                    assert!(
                        Instant::now() < deadline,
                        "initial source owners did not settle"
                    );
                    other.drive_editor_frame(Duration::ZERO);
                    std::thread::yield_now();
                }
                Err(error) => panic!("extraction acceptance failed: {error:?}"),
            }
        };
        let recovery_digest = digest(b"same recovery bytes across source operations");
        assert_eq!(
            session.begin_supervised_play_retirement(extraction, recovery_digest),
            Err(EditorSupervisedPlayOwnerError::Operation)
        );
        assert_eq!(
            other.begin_supervised_play_retirement(upgrade, recovery_digest),
            Err(EditorSupervisedPlayOwnerError::Operation)
        );
        session
            .begin_supervised_play_retirement(upgrade, recovery_digest)
            .unwrap();
        other
            .begin_supervised_play_retirement(extraction, recovery_digest)
            .unwrap();
        let request = session
            .supervised_play_retirement_request(&binding)
            .unwrap()
            .unwrap();
        let other_request = other
            .supervised_play_retirement_request(&other_binding)
            .unwrap()
            .unwrap();
        assert_eq!(request.operation(), upgrade);
        assert_eq!(other_request.operation(), extraction);
        assert!(!request.is_same_request(&other_request));
        assert!(
            other_request.is_same_request(
                &other
                    .supervised_play_retirement_request(&other_binding)
                    .unwrap()
                    .unwrap()
            )
        );
        let acknowledgment = binding.attest_retirement(request).unwrap();
        let other_acknowledgment = other_binding.attest_retirement(other_request).unwrap();
        for (session, binding, foreign_acknowledgment, operation) in [
            (session, binding, other_acknowledgment, upgrade),
            (other, other_binding, acknowledgment, extraction),
        ] {
            assert_eq!(
                session.apply_supervised_play_retirement_ack(&binding, foreign_acknowledgment),
                Err(EditorSupervisedPlayOwnerError::Request)
            );
            assert!(session.supervised_play_retirement_pending());
            let acknowledgment = binding
                .attest_retirement(
                    session
                        .supervised_play_retirement_request(&binding)
                        .unwrap()
                        .unwrap(),
                )
                .unwrap();
            session
                .apply_supervised_play_retirement_ack(&binding, acknowledgment)
                .unwrap();
            session.finish_supervised_play_retirement().unwrap();
            assert!(match operation {
                EditorSourceOperationId::AuthoredSchemaUpgrade(_) => {
                    session.authored_schema_upgrade_view().writes_fenced
                }
                EditorSourceOperationId::PrefabExtraction(_) => {
                    session.prefab_extraction_view().writes_fenced
                }
            });
        }
    });
}

#[test]
fn the_original_binding_can_export_a_fresh_launch_after_acknowledged_cancellation() {
    with_sessions(|session, _| {
        let binding = session.register_supervised_play_owner(51).unwrap();
        let operation = begin(session);
        session
            .begin_supervised_play_retirement(
                EditorSourceOperationId::AuthoredSchemaUpgrade(operation),
                digest(b"cancel recovery"),
            )
            .unwrap();
        let acknowledgment = binding
            .attest_retirement(
                session
                    .supervised_play_retirement_request(&binding)
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
        session
            .apply_supervised_play_retirement_ack(&binding, acknowledgment)
            .unwrap();
        session.finish_supervised_play_retirement().unwrap();
        session.cancel_authored_schema_upgrade(operation).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while session.authored_schema_upgrade_view().writes_fenced {
            assert!(Instant::now() < deadline, "cancellation did not settle");
            let _ = session.drive_editor_frame(Duration::ZERO);
            std::thread::yield_now();
        }
        assert!(
            session
                .supervised_play_retirement_request(&binding)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            session.request_external_play(EditorPlayCommand::Play),
            EditorPlayRequestResult::Accepted
        );
        let request = session.take_external_play_request(&binding).unwrap();
        assert_eq!(request.operation(), nara_tooling::EditorPlayOperation::Play);
        assert_eq!(binding.published_generation(), 51);
    });
}
