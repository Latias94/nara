use nara_project::{
    DEFAULT_MANIFEST_BYTE_LIMIT, ProjectAuthoringRecovery, ProjectAuthoringRecoveryDigest,
    ProjectAuthoringState, ProjectManifest, ProjectPath,
};

use crate::project_content::ProjectContentError;
use crate::project_host::{
    check::{ProjectSourceCheckError, check_project_source_from_manifest_bytes},
    resolve_runtime_configuration,
};

use super::*;

const PENDING: &str = "runtime.configuration.source-authoring-pending";

fn ready_project() -> LazyContentProject {
    let project = LazyContentProject::new();
    // These tests exercise full delivery too, so remove the lazy fixture's unrelated poison.
    fs::remove_file(project.root.join("assets/unused.png.meta")).unwrap();
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        SceneDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    let path = project.root.join("nara.toml");
    let mut manifest = parse_manifest(&fs::read(&path).unwrap());
    manifest.authoring.schema_bundle = Some("schema.bundle.json".to_owned());
    fs::write(path, manifest.to_canonical_toml_bytes().unwrap()).unwrap();
    project
}

fn ready_inputs(
    project: &LazyContentProject,
) -> (ProjectSettingsCandidate, RuntimePlan, ProjectContentLoader) {
    let (candidate, plan, loader) = project.inputs();
    fs::write(
        project.root.join("schema.bundle.json"),
        plan.schema_validation()
            .authoring_schema_bundle()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    (candidate, plan, loader)
}

fn parse_manifest(bytes: &[u8]) -> ProjectManifest {
    let load = ProjectManifest::parse_toml_bytes(bytes);
    assert!(!load.has_errors(), "{:?}", load.diagnostics);
    load.manifest.unwrap()
}

fn pending_manifest(ready: &[u8]) -> Vec<u8> {
    let mut manifest = parse_manifest(ready);
    let digest = ContentDigest::of_bytes(b"unpublished recovery data");
    manifest.authoring.state = ProjectAuthoringState::Pending(
        ProjectAuthoringRecovery::new(
            ProjectPath::new(".nara/upgrades/missing.json").unwrap(),
            ProjectAuthoringRecoveryDigest::from_parts(digest.length(), *digest.as_bytes()),
        )
        .unwrap(),
    );
    manifest.to_canonical_toml_bytes().unwrap()
}

fn load_entry(
    entry: &str,
    loader: &ProjectContentLoader,
    candidate: &ProjectSettingsCandidate,
    plan: &RuntimePlan,
) -> Result<(), ProjectContentError> {
    match entry {
        "load" => loader.load(candidate, plan).map(drop),
        "load_scene" => loader
            .load_scene(
                candidate,
                plan,
                &ProjectPath::new("startup.scene.json").unwrap(),
            )
            .map(drop),
        "load_delivery" => loader.load_delivery(candidate, plan).map(drop),
        _ => unreachable!(),
    }
}

fn assert_refusal(error: &ProjectContentError, code: &str) {
    assert_eq!(error.kind(), ProjectContentErrorKind::HostAuthorityRejected);
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == code),
        "{error:?}",
    );
}

fn assert_released(loader: &ProjectContentLoader) {
    let budget = loader.budget_snapshot();
    assert_eq!(budget.active_reservations(), 0);
    assert_eq!(budget.active_generations(), 0);
    for kind in ProjectContentBudgetKind::ALL {
        assert_eq!(budget.active(*kind), 0, "{kind:?}");
    }
}

#[test]
fn retained_ready_candidate_refuses_actual_pending_at_every_load_entry() {
    let project = ready_project();
    let (candidate, plan, loader) = ready_inputs(&project);
    let path = project.root.join("nara.toml");
    let ready = fs::read(&path).unwrap();
    for entry in ["load", "load_scene", "load_delivery"] {
        load_entry(entry, &loader, &candidate, &plan).unwrap();
        fs::write(&path, pending_manifest(&ready)).unwrap();
        // A Pending reference is enough to refuse execution; this is not package validation.
        assert_refusal(
            &load_entry(entry, &loader, &candidate, &plan).unwrap_err(),
            PENDING,
        );
        assert_released(&loader);
        fs::write(&path, &ready).unwrap();
    }
}

#[test]
fn retained_startup_snapshot_cannot_enter_delivery_after_pending() {
    let project = ready_project();
    let (candidate, plan, loader) = ready_inputs(&project);
    let agreement =
        prepare_runtime_schema_agreement(&loader.project_root(), &candidate, &plan).unwrap();
    let (startup, catalogue) = loader
        .load_startup_for_delivery(&candidate, &plan, None, agreement)
        .unwrap();
    let path = project.root.join("nara.toml");
    fs::write(&path, pending_manifest(&fs::read(&path).unwrap())).unwrap();
    let error = loader
        .load_delivery_with_startup_and_catalogue(&candidate, &plan, startup, catalogue)
        .unwrap_err();
    assert_refusal(&error, PENDING);
    assert_released(&loader);
}

#[test]
fn complete_delivery_union_refuses_pending_after_all_scene_snapshots() {
    for retained_startup in [false, true] {
        let project = ready_project();
        let path = project.root.join("nara.toml");
        let mut manifest = parse_manifest(&fs::read(&path).unwrap());
        manifest.content.scenes = vec!["later.scene.json".to_owned()];
        fs::write(&path, manifest.to_canonical_toml_bytes().unwrap()).unwrap();
        fs::write(
            project.root.join("scenes/later.scene.json"),
            SceneDocument::default().to_json_string().unwrap(),
        )
        .unwrap();
        let (candidate, plan, mut loader) = ready_inputs(&project);
        if !retained_startup {
            let receipt = loader.load_delivery(&candidate, &plan).unwrap();
            assert_eq!(receipt.scene_roots().len(), 2);
            drop(receipt);
            assert_released(&loader);
        }

        let snapshots = Arc::new(AtomicU64::new(0));
        let completed = Arc::clone(&snapshots);
        let host = loader.budget_host();
        loader.before_source_readiness_revalidation = Some(Box::new(move || {
            assert_eq!(
                host.snapshot()
                    .active(ProjectContentBudgetKind::OpenHandles),
                1,
                "Scene loads borrow the delivery entry's single manifest observation",
            );
            completed.fetch_add(1, Ordering::Relaxed);
        }));
        let completed = Arc::clone(&snapshots);
        let host = loader.budget_host();
        let pending = pending_manifest(&fs::read(&path).unwrap());
        loader.before_delivery_readiness_revalidation = Some(Box::new(move || {
            assert_eq!(completed.load(Ordering::Relaxed), 2);
            let budget = host.snapshot();
            assert_eq!(
                budget.active_generations(),
                0,
                "decoded snapshots have retired"
            );
            assert!(budget.active(ProjectContentBudgetKind::RetainedBytes) > 0);
            // All Scene gates succeeded; only the final complete-union gate can see Pending.
            fs::write(&path, &pending).unwrap();
        }));
        let result = if retained_startup {
            let agreement =
                prepare_runtime_schema_agreement(&loader.project_root(), &candidate, &plan)
                    .unwrap();
            let (startup, catalogue) = loader
                .load_startup_for_delivery(&candidate, &plan, None, agreement)
                .unwrap();
            loader.load_delivery_with_startup_and_catalogue(&candidate, &plan, startup, catalogue)
        } else {
            loader.load_delivery(&candidate, &plan)
        };
        assert_refusal(&result.unwrap_err(), PENDING);
        assert_eq!(snapshots.load(Ordering::Relaxed), 2);
        assert_released(&loader);
    }
}

#[test]
fn retained_candidate_refuses_missing_required_readiness_and_invalid_manifest() {
    let project = ready_project();
    let (candidate, plan, loader) = ready_inputs(&project);
    let path = project.root.join("nara.toml");
    let ready = String::from_utf8(fs::read(&path).unwrap()).unwrap();
    let (before, authoring) = ready.split_once("[authoring]\n").unwrap();
    let (_, after) = authoring.split_once("\n[").unwrap();
    let missing_authoring = format!("{before}[{after}");
    for bytes in [
        ready.replace("state = \"ready\"", "").into_bytes(),
        missing_authoring.into_bytes(),
        ready.replace("\"ready\"", "\"unknown\"").into_bytes(),
        vec![0xff],
    ] {
        fs::write(&path, &bytes).unwrap();
        let error = loader.load(&candidate, &plan).unwrap_err();
        assert_eq!(error.kind(), ProjectContentErrorKind::HostAuthorityRejected);
        assert!(error.diagnostics().iter().any(|diagnostic| {
            matches!(
                diagnostic.code().as_str(),
                "project.manifest.parse" | "project.manifest.utf8"
            )
        }));
        assert_released(&loader);
    }
    fs::write(&path, vec![b' '; DEFAULT_MANIFEST_BYTE_LIMIT as usize + 1]).unwrap();
    assert_refusal(
        &loader.load(&candidate, &plan).unwrap_err(),
        "project.manifest.too-large",
    );
    assert_released(&loader);
    fs::remove_file(&path).unwrap();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::HostIo);
    assert_released(&loader);
}

#[test]
fn final_snapshot_gate_refuses_pending_written_during_each_load() {
    for entry in ["load", "load_scene", "load_delivery"] {
        let project = ready_project();
        let (candidate, plan, mut loader) = ready_inputs(&project);
        let path = project.root.join("nara.toml");
        let pending = pending_manifest(&fs::read(&path).unwrap());
        let host = loader.budget_host();
        loader.before_source_readiness_revalidation = Some(Box::new(move || {
            assert!(
                host.snapshot()
                    .active(ProjectContentBudgetKind::RetainedBytes)
                    > 0
            );
            // Deterministic competing disk publication after decoding, before returning content.
            fs::write(&path, &pending).unwrap();
        }));
        let error = load_entry(entry, &loader, &candidate, &plan).unwrap_err();
        assert_refusal(&error, PENDING);
        assert_released(&loader);
    }
}

#[test]
fn final_gate_reopens_manifest_and_refuses_replacement_or_ready_byte_changes() {
    for replace_leaf in [false, true] {
        let project = ready_project();
        let (candidate, plan, mut loader) = ready_inputs(&project);
        let path = project.root.join("nara.toml");
        let original = fs::read(&path).unwrap();
        loader.before_source_readiness_revalidation = Some(Box::new(move || {
            if replace_leaf {
                // Preserve the historical opened leaf to demonstrate actual path reobservation.
                fs::rename(&path, path.with_file_name("old-nara.toml")).unwrap();
                fs::write(&path, &original).unwrap();
            } else {
                let mut changed = original.clone();
                changed.extend_from_slice(b"\n# competing Ready edit\n");
                fs::write(&path, changed).unwrap();
            }
        }));
        assert_refusal(
            &loader.load(&candidate, &plan).unwrap_err(),
            "project.content.readiness-source-changed",
        );
        assert_released(&loader);
    }
}

#[test]
fn manifest_byte_repair_proposal_uses_actual_disk_readiness_without_digest_equality() {
    let project = ready_project();
    let (_, _, loader) = ready_inputs(&project);
    let path = project.root.join("nara.toml");
    let proposal = fs::read(&path).unwrap();
    let mut actual = parse_manifest(&proposal);
    actual
        .replace_startup_scene(None, ProjectPath::new("missing.scene.json").unwrap())
        .unwrap();
    let ready = actual.to_canonical_toml_bytes().unwrap();
    assert_ne!(
        ContentDigest::of_bytes(&proposal),
        ContentDigest::of_bytes(&ready),
    );
    fs::write(&path, &ready).unwrap();
    let checked = check_project_source_from_manifest_bytes(
        loader.project_root(),
        None,
        ProductRecipe::new(),
        &proposal,
    )
    .unwrap();
    let (_, snapshot) = checked.into_parts();
    assert_eq!(
        snapshot.content_closure().manifest_digest(),
        ContentDigest::of_bytes(&proposal),
    );
    assert_eq!(fs::read(&path).unwrap(), ready);
    drop(snapshot);
    fs::write(&path, pending_manifest(&ready)).unwrap();
    let error = match check_project_source_from_manifest_bytes(
        loader.project_root(),
        None,
        ProductRecipe::new(),
        &proposal,
    ) {
        Ok(_) => panic!("a proposed Ready manifest cannot bypass the actual Pending manifest"),
        Err(error) => error,
    };
    let ProjectSourceCheckError::StartupContent { error, .. } = error else {
        panic!("actual disk readiness must refuse at source admission");
    };
    assert_refusal(&error, PENDING);
}

#[test]
fn readiness_read_and_parse_reserve_real_budget_before_invalid_bytes_are_read() {
    let project = ready_project();
    let (candidate, plan, original_loader) = ready_inputs(&project);
    fs::write(project.root.join("nara.toml"), b"invalid manifest").unwrap();
    let manifest_limit = DEFAULT_MANIFEST_BYTE_LIMIT as usize;
    for (kind, limit) in [
        (ProjectContentBudgetKind::EncodedBytes, manifest_limit - 1),
        (ProjectContentBudgetKind::WorkBytes, manifest_limit - 1),
        (
            ProjectContentBudgetKind::AggregateBytes,
            2 * manifest_limit - 1,
        ),
        (ProjectContentBudgetKind::OpenHandles, 1),
    ] {
        let limits = ProjectContentLimits::default()
            .with_limit(kind, limit)
            .unwrap();
        let loader =
            ProjectContentLoader::with_limits(original_loader.project_root(), limits).unwrap();
        let error = loader.load(&candidate, &plan).unwrap_err();
        assert_eq!(
            error.kind(),
            ProjectContentErrorKind::BudgetExceeded,
            "{kind:?}: {error:?}",
        );
        assert!(
            error
                .diagnostics()
                .iter()
                .all(|diagnostic| { diagnostic.code().as_str() != "project.manifest.parse" })
        );
        assert_released(&loader);
    }
}

#[test]
fn final_gate_budget_overlaps_the_retained_snapshot_and_releases_on_refusal() {
    let project = ready_project();
    let (candidate, plan, mut loader) = ready_inputs(&project);
    let host = loader.budget_host();
    let pressure = Arc::new(std::sync::Mutex::new(None));
    let held = Arc::clone(&pressure);
    loader.before_source_readiness_revalidation = Some(Box::new(move || {
        let active = host.snapshot();
        assert!(active.active(ProjectContentBudgetKind::RetainedBytes) > 0);
        let mut ticket = host.reserve();
        // Another consumer can own the remaining capacity. Leave one byte less than the
        // final gate's bounded encoded read, so a gate outside the host budget would bypass it.
        let remaining = DEFAULT_MANIFEST_BYTE_LIMIT as usize - 1;
        let bytes = host
            .limits()
            .limit(ProjectContentBudgetKind::AggregateBytes)
            - active.active(ProjectContentBudgetKind::AggregateBytes)
            - remaining;
        let work = bytes.min(
            host.limits().limit(ProjectContentBudgetKind::WorkBytes)
                - active.active(ProjectContentBudgetKind::WorkBytes),
        );
        ticket
            .set_many(&[
                (ProjectContentBudgetKind::WorkBytes, work),
                (ProjectContentBudgetKind::ArtifactBytes, bytes - work),
            ])
            .unwrap();
        *held.lock().unwrap() = Some(ticket);
    }));
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::BudgetExceeded);
    assert_eq!(loader.budget_snapshot().active_generations(), 0);
    drop(pressure.lock().unwrap().take());
    assert_released(&loader);
}

#[test]
fn source_free_runtime_configuration_stays_independent_of_pending_disk() {
    let project = ready_project();
    let (candidate, _, loader) = ready_inputs(&project);
    let configuration = candidate.settings().runtime_configuration().unwrap();
    let retained = resolve_runtime_configuration(&configuration, ProductRecipe::new()).unwrap();
    let path = project.root.join("nara.toml");
    let pending = pending_manifest(&fs::read(&path).unwrap());
    fs::write(&path, &pending).unwrap();
    let effective = parse_manifest(&pending).resolve_profile(None).unwrap();
    assert_eq!(effective.runtime_configuration().unwrap(), configuration);
    assert_eq!(retained.settings(), &configuration);
    fs::remove_file(&path).unwrap();
    let independent = resolve_runtime_configuration(&configuration, ProductRecipe::new()).unwrap();
    assert_eq!(independent.settings(), retained.settings());
    assert_released(&loader);
}
