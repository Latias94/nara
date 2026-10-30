//! Host observation custody across worker completion, accepted output and validation.

use super::prefab_acquisition::open_prefab;
use super::scene_dependencies::{fixture, open_scene};
use super::*;

#[test]
fn scene_capture_transfers_host_retention_and_releases_only_finished_worker_inputs() {
    let project = fixture(&[2], &[], &[], &[]);
    let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("outer.prefab.json").unwrap(),
        overrides: None,
    });
    fs::write(
        project.root.join("scenes/b.scene.json"),
        SceneDocument::from_ordered_roots([anchor])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let (snapshot, source, session) = open_second_scene(&project);
    let (b_source, b) = open_scene(&snapshot, "scenes/b.scene.json");
    let (c_source, c) = open_scene(&snapshot, "scenes/c.scene.json");
    let (prefab_source, prefab) = open_prefab(&snapshot, "prefabs/outer.prefab.json");
    let (unused_source, unused) = open_prefab(&snapshot, "prefabs/alternate.prefab.json");
    let capture = |host, with_unused| {
        let mut worker = checked_sum([
            source.checked_retained_bytes().unwrap(),
            AuthoringSceneSessionSnapshot::capture_retained_bytes(&b_source, &b).unwrap(),
            AuthoringPrefabSessionSnapshot::capture_retained_bytes(&prefab_source, &prefab)
                .unwrap(),
        ])
        .unwrap();
        if with_unused {
            worker = checked_sum([
                worker,
                AuthoringSceneSessionSnapshot::capture_retained_bytes(&c_source, &c).unwrap(),
                AuthoringPrefabSessionSnapshot::capture_retained_bytes(&unused_source, &unused)
                    .unwrap(),
                211,
            ])
            .unwrap();
        }
        let inputs = snapshot.admit_source_capture_inputs(host, worker).unwrap();
        let mut scenes = vec![AuthoringSceneSessionSnapshot::capture(&b_source, &b)];
        let mut prefabs = vec![AuthoringPrefabSessionSnapshot::capture(
            &prefab_source,
            &prefab,
        )];
        if with_unused {
            scenes.push(AuthoringSceneSessionSnapshot::capture(&c_source, &c));
            prefabs.push(AuthoringPrefabSessionSnapshot::capture(
                &unused_source,
                &unused,
            ));
        }
        snapshot
            .capture_scene_request(&source, &session, inputs, prefabs.into_boxed_slice())
            .unwrap()
            .with_open_scenes(scenes.into_boxed_slice())
            .prepare()
            .unwrap()
    };
    let baseline = capture(0, false);
    let host = 137;
    let captured = capture(host, true);
    let mut expected = baseline.usage();
    expected.retained_candidate_bytes += host;
    assert_eq!(baseline.host_retained_bytes, 0);
    assert_eq!(captured.host_retained_bytes, host);
    assert_eq!(captured.usage(), expected);
    assert_eq!(captured.readiness(), AuthoringProjectReadiness::Complete);
    assert_eq!(captured.scenes().len(), 2);
    let selected = captured
        .scenes()
        .iter()
        .find(|scene| scene.source().project_path().as_str() == "scenes/b.scene.json")
        .unwrap();
    assert!(Arc::ptr_eq(&selected.document, &b.shared_document()));
    let selected = captured
        .prefabs()
        .iter()
        .find(|source| source.source().project_path().as_str() == "prefabs/outer.prefab.json")
        .unwrap();
    assert_eq!(selected.session_revision(), Some(prefab.revision()));
    assert!(std::ptr::eq(selected.document(), prefab.document()));
    assert!(captured.prefabs().iter().all(|source| {
        source.source().project_path().as_str() != "prefabs/alternate.prefab.json"
    }));
    assert_eq!(
        captured.source_validation().retained_capture_bytes,
        captured.usage().retained_candidate_bytes()
    );
    captured.source_validation().validate().unwrap();
}

#[test]
fn input_admission_checks_the_combined_limit_without_lending_host_bytes_to_the_worker() {
    let project = independent_project();
    let (snapshot, source, session) = open_second_scene(&project);
    let maximum = snapshot
        .schema
        .budget
        .limits
        .retained_candidate_bytes()
        .get();
    let host = 137;
    let worker = maximum - snapshot.schema.budget.retained_candidate_bytes - host;
    let inputs = snapshot.admit_source_capture_inputs(host, worker).unwrap();
    assert_eq!(inputs.host_retained_bytes, host);
    assert_eq!(inputs.worker_retained_bytes, worker);
    assert!(matches!(
        snapshot.admit_source_capture_inputs(host, worker + 1),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { maximum: observed }
        )) if observed == maximum
    ));
    assert!(matches!(
        snapshot.admit_source_capture_inputs(usize::MAX, 1),
        Err(AuthoringProjectSourceAdmissionError::AccountingOverflow)
    ));
    let (prefab_source, prefab) = open_prefab(&snapshot, "prefabs/outer.prefab.json");
    let snapshot_bytes =
        AuthoringPrefabSessionSnapshot::capture_retained_bytes(&prefab_source, &prefab).unwrap();
    let inputs = snapshot
        .admit_source_capture_inputs(snapshot_bytes, 0)
        .unwrap();
    let request = snapshot
        .capture_scene_request(
            &source,
            &session,
            inputs,
            vec![AuthoringPrefabSessionSnapshot::capture(
                &prefab_source,
                &prefab,
            )]
            .into_boxed_slice(),
        )
        .unwrap();
    assert!(matches!(
        request.prepare(),
        Err(AuthoringProjectSourceAdmissionError::AccountingOverflow)
    ));
}

#[test]
fn host_replacement_admits_exact_overlap_and_rejects_one_short_or_overflow_atomically() {
    let project = independent_project();
    let (snapshot, source, session) = open_second_scene(&project);
    let host = 137;
    let output = 251;
    let inputs = snapshot
        .admit_source_capture_inputs(host, source.checked_retained_bytes().unwrap())
        .unwrap();
    let mut capture = snapshot
        .capture_scene_request(&source, &session, inputs, Box::new([]))
        .unwrap()
        .prepare()
        .unwrap();
    let before = capture.usage();
    let source_retained = before.retained_candidate_bytes() - host;
    let source_facts = Arc::clone(&capture.source_facts);
    let stable_lookups = Arc::clone(&capture.stable_lookups);
    let source_document = std::ptr::from_ref(capture.scenes()[0].document());
    let maximum = before.retained_candidate_bytes() + output;
    drop(snapshot);
    Arc::get_mut(&mut capture.authority)
        .expect("the capture must exclusively own its Schema receipt")
        .budget
        .limits
        .retained_candidate_bytes = ByteLimit::new(maximum - 1).unwrap();
    assert!(matches!(
        capture.replace_host_retained_bytes(host + output),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { maximum: observed }
        )) if observed == maximum - 1
    ));
    assert_eq!(capture.usage(), before);
    assert_eq!(capture.host_retained_bytes, host);
    assert!(matches!(
        capture.replace_host_retained_bytes(usize::MAX),
        Err(AuthoringProjectSourceAdmissionError::AccountingOverflow)
    ));
    assert_eq!(capture.usage(), before);
    assert_eq!(capture.host_retained_bytes, host);
    Arc::get_mut(&mut capture.authority)
        .unwrap()
        .budget
        .limits
        .retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
    capture.replace_host_retained_bytes(host + output).unwrap();
    let mut expected = before;
    expected.retained_candidate_bytes = maximum;
    assert_eq!(capture.usage(), expected);
    assert_eq!(capture.host_retained_bytes, host + output);
    capture.replace_host_retained_bytes(host + output).unwrap();
    assert_eq!(capture.usage(), expected);
    capture.replace_host_retained_bytes(output).unwrap();
    expected.retained_candidate_bytes = source_retained + output;
    assert_eq!(capture.usage(), expected);
    assert_eq!(capture.host_retained_bytes, output);
    assert_eq!(
        capture.source_validation().retained_capture_bytes,
        expected.retained_candidate_bytes()
    );
    capture.replace_host_retained_bytes(0).unwrap();
    expected.retained_candidate_bytes = source_retained;
    assert_eq!(capture.usage(), expected);
    assert_eq!(capture.host_retained_bytes, 0);
    assert!(Arc::ptr_eq(&capture.source_facts, &source_facts));
    assert!(Arc::ptr_eq(&capture.stable_lookups, &stable_lookups));
    assert_eq!(
        std::ptr::from_ref(capture.scenes()[0].document()),
        source_document
    );
}

#[test]
fn startup_validation_addition_uses_project_retention_and_survives_into_validation() {
    let project = independent_project();
    let (snapshot, _, _) = open_second_scene(&project);
    let mut validation = snapshot.startup_source_validation();
    let retained = snapshot.usage().retained_candidate_bytes();
    assert_eq!(validation.retained_capture_bytes, retained);
    let additional = 1024 * 1024;
    let maximum = retained + additional;
    drop(snapshot);
    Arc::get_mut(&mut validation.schema)
        .expect("the validation must exclusively own its Schema receipt")
        .budget
        .limits
        .retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
    assert!(matches!(
        validation
            .clone()
            .with_additional_retained_bytes(additional + 1),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { maximum: observed }
        )) if observed == maximum
    ));
    assert!(matches!(
        validation
            .clone()
            .with_additional_retained_bytes(usize::MAX),
        Err(AuthoringProjectSourceAdmissionError::AccountingOverflow)
    ));
    assert_eq!(validation.retained_capture_bytes, retained);
    let admitted = validation
        .clone()
        .with_additional_retained_bytes(additional)
        .unwrap();
    assert_eq!(admitted.retained_capture_bytes, maximum);
    assert!(Arc::ptr_eq(&admitted.sources, &validation.sources));
    assert!(Arc::ptr_eq(
        &admitted.stable_lookups,
        &validation.stable_lookups
    ));
    validation.validate().unwrap();
    // The extra observations fill the retained allowance, so even the first file-read
    // reservation must reject rather than silently borrowing their admitted bytes.
    assert!(matches!(
        admitted.validate(),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { maximum: observed }
        )) if observed == maximum
    ));
}
