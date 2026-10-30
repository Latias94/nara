#![cfg(all(feature = "serde", feature = "runtime-2d"))]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::fs;

use nara::{
    asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetProductRef, AssetSourceKind,
        ImportedProductId, StableAssetId,
    },
    core::ByteLimit,
    diagnostic::DiagnosticValueRef,
    fs::{ContentDigest, FsError, RelativePath},
    input::{
        ActionBinding, ActionContext, ActionId, ActionMapBuilder, ActionMapFileCandidate,
        ActionMapFileLimits, ActionMapLimits, InputBinding, KeyCode, MouseButton,
    },
    project_host::{
        ProjectContentBudgetKind, ProjectContentBudgetSnapshot, ProjectContentError,
        ProjectContentErrorKind, ProjectContentLimits, ProjectContentLoader,
        ProjectContentMemberKind, ProjectInputContent,
    },
};
use project_content_fixture::{TestProject, player_image_meta};

const INPUT_PATH: &str = "input/player.actions.json";
const INPUT_ID: &str = "15e836f6-8c80-4bb9-86a6-c974c2149401";
const SCENE_ID: &str = "4c2a2c0e-25dd-4b3e-9e6b-52f0a04e8de1";
const PREFAB_ID: &str = "5f2f9a19-f6a8-4e4e-9b7d-2f48d2b3bb8b";

fn primary_product(id: &str) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(id).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn action_map_json(product: AssetProductRef, key: KeyCode) -> String {
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    let menu = ActionContext::new("menu").unwrap();
    builder.declare_context(menu.clone(), false).unwrap();
    for binding in [
        ActionBinding::key(ActionId::new("jump").unwrap(), key),
        ActionBinding::mouse(ActionId::new("fire").unwrap(), MouseButton::Left),
        ActionBinding::key(ActionId::new("confirm").unwrap(), KeyCode::Enter).with_context(menu),
    ] {
        builder.bind(binding).unwrap();
    }
    ActionMapFileCandidate::new(product, builder.compile().unwrap())
        .unwrap()
        .to_json_string()
        .unwrap()
}

fn write_project_file(project: &TestProject, path: &str, bytes: &[u8]) {
    // TestProject owns the temporary tree and its cleanup guard. All added files stay inside it.
    RelativePath::new(path).unwrap();
    let target = project.path().join(path);
    assert!(project.path().is_absolute());
    assert!(target.starts_with(project.path()));
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, bytes).unwrap();
}

fn select_input(project: &TestProject, path: &str) {
    let path = nara::project::ProjectPath::new(path).unwrap();
    let manifest_path = project.path().join("nara.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        manifest_path,
        format!("{manifest}\n[input]\naction_map = '{}'\n", path.as_str()),
    )
    .unwrap();
}

fn input_metadata(id: &str, path: &str, kind: AssetSourceKind) -> String {
    AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str(id).unwrap(),
            AssetPath::new(path).unwrap(),
            kind,
        ),
        (),
    )
    .to_json_string()
    .unwrap()
}

fn assert_diagnostic(error: &ProjectContentError, kind: ProjectContentErrorKind, code: &str) {
    assert_eq!(error.kind(), kind, "{error:?}");
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == code),
        "{error:?}"
    );
}

fn budget_value(error: &ProjectContentError, key: &str) -> usize {
    error
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.code().as_str() == "project.content.budget-exceeded")
        .flat_map(|diagnostic| diagnostic.fields())
        .find_map(|field| match field.value() {
            DiagnosticValueRef::Unsigned(value) if field.key().as_str() == key => {
                Some(usize::try_from(value).unwrap())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing budget field {key}: {error:?}"))
}

fn assert_budget(error: &ProjectContentError, kind: ProjectContentBudgetKind) {
    assert_diagnostic(
        error,
        ProjectContentErrorKind::BudgetExceeded,
        "project.content.budget-exceeded",
    );
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.fields().iter().any(|field| {
            field.key().as_str() == "budget"
                && field.value() == DiagnosticValueRef::Identifier(kind.as_str())
        })
    }));
}

fn assert_active_charges_eq(
    actual: ProjectContentBudgetSnapshot,
    expected: ProjectContentBudgetSnapshot,
) {
    assert_eq!(actual.active_reservations(), expected.active_reservations());
    assert_eq!(actual.active_generations(), expected.active_generations());
    for &kind in ProjectContentBudgetKind::ALL {
        assert_eq!(
            actual.active(kind),
            expected.active(kind),
            "{kind:?} charge drifted"
        );
    }
}

#[test]
fn selected_input_captures_real_project_relative_bytes_without_a_sidecar() {
    for path in [
        "assets/input/player.actions.json",
        INPUT_PATH,
        "input/玩家 actions.json",
    ] {
        let project = TestProject::with_prefab_startup();
        let product = primary_product(INPUT_ID);
        let bytes = action_map_json(product, KeyCode::Space).into_bytes();
        write_project_file(&project, path, &bytes);
        select_input(&project, path);
        assert!(!project.path().join(format!("{path}.meta")).exists());
        let (candidate, plan, root) = project.candidate_plan_and_root();
        assert_eq!(candidate.input_requirement().unwrap().as_str(), path);
        assert_eq!(plan.input_requirement(), candidate.input_requirement());
        let loader = ProjectContentLoader::new(root.share()).unwrap();
        let baseline = loader.budget_snapshot();
        let snapshot = loader.load(&candidate, &plan).unwrap();
        let input: &ProjectInputContent = snapshot.input_action_map().unwrap();
        assert_eq!(input.product(), product);
        assert_eq!(input.path().as_str(), path);
        assert_eq!(input.source_digest(), ContentDigest::of_bytes(&bytes));
        let contexts = input.definition().contexts();
        assert_eq!(contexts.len(), 2);
        assert_eq!(contexts[0].context, ActionContext::gameplay());
        assert!(contexts[0].enabled);
        assert_eq!(contexts[1].context.as_str(), "menu");
        assert!(!contexts[1].enabled);
        assert_eq!(input.definition().bindings().len(), 3);
        assert_eq!(
            input.definition().bindings()[0],
            ActionBinding::key(ActionId::new("jump").unwrap(), KeyCode::Space)
        );
        assert_eq!(
            input.definition().bindings()[1].input,
            InputBinding::Mouse(MouseButton::Left)
        );
        assert_eq!(input.definition().bindings()[2].context.as_str(), "menu");
        assert_eq!(snapshot.prefabs().len(), 1);
        assert_eq!(snapshot.images().len(), 1);
        let receipt = snapshot.content_closure();
        let members = receipt
            .members()
            .iter()
            .filter(|member| member.kind() == ProjectContentMemberKind::InputActionMap)
            .collect::<Vec<_>>();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].path().as_str(), path);
        assert_eq!(members[0].digest(), input.source_digest());
        assert_eq!(members[0].byte_len(), u64::try_from(bytes.len()).unwrap());
        assert!(
            !receipt
                .members()
                .iter()
                .any(|member| { member.path().as_str() == format!("{path}.meta") })
        );
        root.open_file_exact(&RelativePath::new(path).unwrap())
            .unwrap()
            .verify_digest(members[0].digest(), members[0].byte_len())
            .unwrap();
        drop(receipt);
        drop(snapshot);
        assert_active_charges_eq(loader.budget_snapshot(), baseline);
    }
}

#[test]
fn unselected_input_files_do_not_enter_the_runtime_content_digest() {
    let project = TestProject::with_prefab_startup();
    write_project_file(&project, INPUT_PATH, b"unselected malformed Input");
    let (candidate, plan, root) = project.candidate_plan_and_root();
    assert!(candidate.input_requirement().is_none());
    assert!(plan.input_requirement().is_none());
    let loader = ProjectContentLoader::new(root).unwrap();
    let baseline = loader.budget_snapshot();
    let before = loader.load(&candidate, &plan).unwrap();
    write_project_file(&project, INPUT_PATH, b"different unselected Input bytes");
    let after = loader.load(&candidate, &plan).unwrap();
    assert!(before.input_action_map().is_none());
    assert!(after.input_action_map().is_none());
    assert_eq!(before.content_digest(), after.content_digest());
    assert_eq!(before.revision(), after.revision());
    assert!(
        after
            .content_closure()
            .members()
            .iter()
            .all(|member| { member.kind() != ProjectContentMemberKind::InputActionMap })
    );
    drop((before, after));
    assert_active_charges_eq(loader.budget_snapshot(), baseline);
}

#[test]
fn matching_optional_sidecar_is_captured_and_its_bytes_affect_the_content_digest() {
    for (path, metadata_path) in [
        ("assets/input/player.actions.json", INPUT_PATH),
        (INPUT_PATH, INPUT_PATH),
    ] {
        let project = TestProject::with_prefab_startup();
        let bytes = action_map_json(primary_product(INPUT_ID), KeyCode::Space);
        write_project_file(&project, path, bytes.as_bytes());
        select_input(&project, path);
        let (candidate, plan, root) = project.candidate_plan_and_root();
        let loader = ProjectContentLoader::new(root.share()).unwrap();
        let baseline = loader.budget_snapshot();
        let without_metadata = loader.load(&candidate, &plan).unwrap();
        let original_digest = without_metadata.content_digest();
        drop(without_metadata);
        let sidecar = format!("{path}.meta");
        let metadata = input_metadata(
            INPUT_ID,
            metadata_path,
            AssetSourceKind::Other("input-action-map".to_owned()),
        );
        write_project_file(&project, &sidecar, metadata.as_bytes());
        let before = loader.load(&candidate, &plan).unwrap();
        assert_ne!(before.content_digest(), original_digest);
        let receipt = before.content_closure();
        let member = receipt
            .members()
            .iter()
            .find(|member| member.path().as_str() == sidecar)
            .unwrap();
        assert_eq!(member.kind(), ProjectContentMemberKind::AssetMetadata);
        assert_eq!(
            member.digest(),
            ContentDigest::of_bytes(metadata.as_bytes())
        );
        root.open_file_exact(&RelativePath::new(&sidecar).unwrap())
            .unwrap()
            .verify_digest(member.digest(), member.byte_len())
            .unwrap();
        let changed_metadata = format!("{metadata}\n");
        write_project_file(&project, &sidecar, changed_metadata.as_bytes());
        let after = loader.load(&candidate, &plan).unwrap();
        assert_ne!(before.content_digest(), after.content_digest());
        let old_input = before.input_action_map().unwrap();
        let new_input = after.input_action_map().unwrap();
        assert_eq!(old_input.source_digest(), new_input.source_digest());
        assert_eq!(
            old_input.definition().bindings(),
            new_input.definition().bindings()
        );
        assert_eq!(old_input.product(), new_input.product());
        assert_eq!(
            after
                .content_closure()
                .members()
                .iter()
                .find(|member| member.path().as_str() == sidecar)
                .unwrap()
                .digest(),
            ContentDigest::of_bytes(changed_metadata.as_bytes())
        );
        drop((before, after, receipt));
        assert_active_charges_eq(loader.budget_snapshot(), baseline);
    }
}

#[test]
fn optional_sidecar_cannot_override_intrinsic_uuid_kind_or_canonical_location() {
    for path in ["assets/input/player.actions.json", INPUT_PATH] {
        for (id, kind, metadata_path) in [
            (
                SCENE_ID,
                AssetSourceKind::Other("input-action-map".to_owned()),
                INPUT_PATH,
            ),
            (INPUT_ID, AssetSourceKind::Image, INPUT_PATH),
            (
                INPUT_ID,
                AssetSourceKind::Other("input-action-map".to_owned()),
                "input/other.actions.json",
            ),
            (
                INPUT_ID,
                AssetSourceKind::Other("input-action-map".to_owned()),
                "input/PLAYER.actions.json",
            ),
        ] {
            let project = TestProject::with_prefab_startup();
            write_project_file(
                &project,
                path,
                action_map_json(primary_product(INPUT_ID), KeyCode::Space).as_bytes(),
            );
            select_input(&project, path);
            write_project_file(
                &project,
                &format!("{path}.meta"),
                input_metadata(id, metadata_path, kind).as_bytes(),
            );
            let (candidate, plan, root) = project.candidate_plan_and_root();
            let loader = ProjectContentLoader::new(root).unwrap();
            let baseline = loader.budget_snapshot();
            let error = loader.load(&candidate, &plan).unwrap_err();
            assert_diagnostic(
                &error,
                ProjectContentErrorKind::AssetMetaMismatch,
                "project.content.asset-identity-conflict",
            );
            assert_active_charges_eq(loader.budget_snapshot(), baseline);
        }
    }
}

#[test]
fn intrinsic_input_uuid_cannot_collide_with_scene_prefab_or_image_sources() {
    let image_id = player_image_meta().identity().stable_id().to_string();
    for conflicting_id in [SCENE_ID, PREFAB_ID, image_id.as_str()] {
        let project = TestProject::with_prefab_startup();
        project.use_stable_native_identity();
        project.write_scene_meta(StableAssetId::parse_str(SCENE_ID).unwrap());
        project.write_prefab_meta(
            "enemy.prefab.json",
            StableAssetId::parse_str(PREFAB_ID).unwrap(),
        );
        write_project_file(
            &project,
            INPUT_PATH,
            action_map_json(primary_product(conflicting_id), KeyCode::Space).as_bytes(),
        );
        select_input(&project, INPUT_PATH);
        let (candidate, plan, root) = project.candidate_plan_and_root();
        let loader = ProjectContentLoader::new(root).unwrap();
        let baseline = loader.budget_snapshot();
        let error = loader.load(&candidate, &plan).unwrap_err();
        assert_diagnostic(
            &error,
            ProjectContentErrorKind::AssetMetaMismatch,
            "project.content.asset-identity-conflict",
        );
        assert_active_charges_eq(loader.budget_snapshot(), baseline);

        write_project_file(
            &project,
            INPUT_PATH,
            action_map_json(primary_product(INPUT_ID), KeyCode::Space).as_bytes(),
        );
        let accepted = loader.load(&candidate, &plan).unwrap();
        assert_eq!(
            accepted.selected_scene_product(),
            Some(primary_product(SCENE_ID))
        );
        assert_eq!(accepted.prefabs().len(), 1);
        assert_eq!(accepted.images().len(), 1);
        assert_eq!(
            accepted.input_action_map().unwrap().product(),
            primary_product(INPUT_ID)
        );
        drop(accepted);
        assert_active_charges_eq(loader.budget_snapshot(), baseline);
    }
}

#[test]
fn selected_input_requires_exact_directory_and_file_spelling() {
    for alias in ["INPUT/player.actions.json", "input/PLAYER.actions.json"] {
        let project = TestProject::with_prefab_startup();
        write_project_file(
            &project,
            INPUT_PATH,
            action_map_json(primary_product(INPUT_ID), KeyCode::Space).as_bytes(),
        );
        select_input(&project, alias);
        let (candidate, plan, root) = project.candidate_plan_and_root();
        let loader = ProjectContentLoader::new(root).unwrap();
        let baseline = loader.budget_snapshot();
        let error = loader.load(&candidate, &plan).unwrap_err();
        assert!(matches!(
            error.kind(),
            ProjectContentErrorKind::HostIo | ProjectContentErrorKind::HostAuthorityRejected
        ));
        assert!(error.diagnostics().has_errors());
        assert_active_charges_eq(loader.budget_snapshot(), baseline);
    }
}

#[test]
fn selected_input_never_falls_back_to_an_assets_relative_alias() {
    let project = TestProject::with_prefab_startup();
    write_project_file(
        &project,
        "assets/input/player.actions.json",
        action_map_json(primary_product(INPUT_ID), KeyCode::Enter).as_bytes(),
    );
    select_input(&project, INPUT_PATH);
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let baseline = loader.budget_snapshot();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_diagnostic(
        &error,
        ProjectContentErrorKind::HostIo,
        "project.content.host-io",
    );
    assert_active_charges_eq(loader.budget_snapshot(), baseline);

    write_project_file(
        &project,
        INPUT_PATH,
        action_map_json(primary_product(INPUT_ID), KeyCode::Space).as_bytes(),
    );
    let snapshot = loader.load(&candidate, &plan).unwrap();
    let input = snapshot.input_action_map().unwrap();
    assert_eq!(input.path().as_str(), INPUT_PATH);
    assert_eq!(
        input.definition().bindings()[0].input,
        InputBinding::Key(KeyCode::Space)
    );
    assert!(
        snapshot
            .content_closure()
            .members()
            .iter()
            .all(|member| { member.path().as_str() != "assets/input/player.actions.json" })
    );
    drop(snapshot);
    assert_active_charges_eq(loader.budget_snapshot(), baseline);
}

#[test]
fn changed_input_bytes_publish_a_distinct_digest_and_leave_old_consumers_unchanged() {
    let project = TestProject::with_prefab_startup();
    let original = action_map_json(primary_product(INPUT_ID), KeyCode::Space);
    write_project_file(&project, INPUT_PATH, original.as_bytes());
    select_input(&project, INPUT_PATH);
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root.share()).unwrap();
    let baseline = loader.budget_snapshot();
    let before = loader.load(&candidate, &plan).unwrap();
    let original_digest = before.input_action_map().unwrap().source_digest();
    let original_bindings = before.input_action_map().unwrap().bindings();
    let changed = action_map_json(primary_product(INPUT_ID), KeyCode::Enter);
    assert_eq!(original.len(), changed.len());
    write_project_file(&project, INPUT_PATH, changed.as_bytes());
    let after = loader.load(&candidate, &plan).unwrap();
    let old_input = before.input_action_map().unwrap();
    let new_input = after.input_action_map().unwrap();
    assert_ne!(before.content_digest(), after.content_digest());
    assert_ne!(before.revision(), after.revision());
    assert_eq!(
        new_input.source_digest(),
        ContentDigest::of_bytes(changed.as_bytes())
    );
    assert_eq!(old_input.source_digest(), original_digest);
    assert_eq!(
        old_input.definition().bindings()[0].input,
        InputBinding::Key(KeyCode::Space)
    );
    assert_eq!(
        new_input.definition().bindings()[0].input,
        InputBinding::Key(KeyCode::Enter)
    );
    assert_eq!(old_input.product(), new_input.product());
    assert!(!old_input.shares_owner_with(new_input));
    assert!(original_bindings.shares_owner_with(&old_input.bindings()));
    assert!(!original_bindings.shares_owner_with(&new_input.bindings()));
    assert_eq!(before.scene(), after.scene());
    assert_eq!(
        before.prefabs()[0].document(),
        after.prefabs()[0].document()
    );
    assert_eq!(
        before.images()[0].image().pixels(),
        after.images()[0].image().pixels()
    );
    let error = root
        .open_file_exact(&RelativePath::new(INPUT_PATH).unwrap())
        .unwrap()
        .verify_digest(original_digest, u64::try_from(original.len()).unwrap())
        .unwrap_err();
    assert!(matches!(error, FsError::DigestMismatch { .. }));
    drop((before, after, original_bindings));
    assert_active_charges_eq(loader.budget_snapshot(), baseline);
}

#[test]
fn malformed_input_rejects_without_changing_a_retained_definition_or_leaking_charges() {
    let project = TestProject::with_prefab_startup();
    let original = action_map_json(primary_product(INPUT_ID), KeyCode::Space);
    write_project_file(&project, INPUT_PATH, original.as_bytes());
    select_input(&project, INPUT_PATH);
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let baseline = loader.budget_snapshot();
    let before = loader.load(&candidate, &plan).unwrap();
    let retained = loader.budget_snapshot();
    let input = before.input_action_map().unwrap();
    let bindings = input.bindings();
    let mut unknown_context: serde_json::Value = serde_json::from_str(&original).unwrap();
    unknown_context["payload"]["bindings"][0]["context"] = serde_json::json!("undeclared");
    for malformed in [
        b"{ malformed selected Input".to_vec(),
        vec![0xff, 0xfe],
        serde_json::to_vec(&unknown_context).unwrap(),
    ] {
        write_project_file(&project, INPUT_PATH, &malformed);
        let error = loader.load(&candidate, &plan).unwrap_err();
        assert_diagnostic(
            &error,
            ProjectContentErrorKind::InputDefinition,
            "project.content.input-definition-invalid",
        );
        assert_active_charges_eq(loader.budget_snapshot(), retained);
        assert_eq!(
            input.source_digest(),
            ContentDigest::of_bytes(original.as_bytes())
        );
        assert!(bindings.shares_owner_with(&input.bindings()));
        assert_eq!(
            input.definition().bindings()[0].input,
            InputBinding::Key(KeyCode::Space)
        );
    }
    write_project_file(&project, INPUT_PATH, original.as_bytes());
    let recovered = loader.load(&candidate, &plan).unwrap();
    assert_eq!(recovered.content_digest(), before.content_digest());
    drop((before, recovered, bindings));
    assert_active_charges_eq(loader.budget_snapshot(), baseline);
}

#[test]
fn input_file_encoded_limit_accepts_exact_bytes_and_rejects_one_byte_short() {
    let project = TestProject::with_prefab_startup();
    let source = action_map_json(primary_product(INPUT_ID), KeyCode::Space);
    write_project_file(&project, INPUT_PATH, source.as_bytes());
    select_input(&project, INPUT_PATH);
    let (candidate, plan, root) = project.candidate_plan_and_root();
    for maximum in [source.len(), source.len() - 1] {
        let limits = ProjectContentLimits::default().with_input_file(
            ActionMapFileLimits::default().with_encoded_bytes(ByteLimit::new(maximum).unwrap()),
        );
        let loader = ProjectContentLoader::with_limits(root.share(), limits).unwrap();
        let baseline = loader.budget_snapshot();
        match loader.load(&candidate, &plan) {
            Ok(snapshot) => {
                assert_eq!(maximum, source.len());
                assert_eq!(
                    snapshot.input_action_map().unwrap().source_digest(),
                    ContentDigest::of_bytes(source.as_bytes())
                );
                drop(snapshot);
            }
            Err(error) => {
                assert_eq!(maximum, source.len() - 1);
                assert_budget(&error, ProjectContentBudgetKind::EncodedBytes);
                assert_eq!(budget_value(&error, "requested"), source.len());
                assert_eq!(budget_value(&error, "limit"), maximum);
            }
        }
        assert_active_charges_eq(loader.budget_snapshot(), baseline);
    }
}

#[test]
fn predecode_work_boundary_rejects_before_parsing_and_releases_every_ticket() {
    let project = TestProject::with_prefab_startup();
    write_project_file(&project, INPUT_PATH, b"{ deliberately malformed Input");
    select_input(&project, INPUT_PATH);
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let file_limits = ActionMapFileLimits::default();
    let predecode = file_limits.predecode_work_bytes().unwrap();
    let limits = ProjectContentLimits::default().with_input_file(file_limits);
    let probe_limits = limits
        .with_limit(ProjectContentBudgetKind::WorkBytes, predecode)
        .unwrap();
    let probe = ProjectContentLoader::with_limits(root.share(), probe_limits).unwrap();
    let baseline = probe.budget_snapshot();
    let error = probe.load(&candidate, &plan).unwrap_err();
    assert_budget(&error, ProjectContentBudgetKind::WorkBytes);
    let requested = budget_value(&error, "requested");
    let active = budget_value(&error, "active");
    assert!(
        requested > predecode,
        "the actual Input owner must also be charged"
    );
    let exact = requested.checked_add(active).unwrap();
    assert_active_charges_eq(probe.budget_snapshot(), baseline);

    // Public refusal fields include actual owner/context costs without duplicating private layout.
    for maximum in [exact - 1, exact] {
        let loader = ProjectContentLoader::with_limits(
            root.share(),
            limits
                .with_limit(ProjectContentBudgetKind::WorkBytes, maximum)
                .unwrap(),
        )
        .unwrap();
        let baseline = loader.budget_snapshot();
        let error = loader.load(&candidate, &plan).unwrap_err();
        if maximum == exact - 1 {
            assert_budget(&error, ProjectContentBudgetKind::WorkBytes);
            assert_eq!(budget_value(&error, "requested"), requested);
            assert_eq!(budget_value(&error, "active"), active);
            assert_eq!(budget_value(&error, "limit"), maximum);
        } else {
            assert_diagnostic(
                &error,
                ProjectContentErrorKind::InputDefinition,
                "project.content.input-definition-invalid",
            );
            assert_eq!(
                loader
                    .budget_snapshot()
                    .high_water(ProjectContentBudgetKind::WorkBytes),
                exact
            );
        }
        assert_active_charges_eq(loader.budget_snapshot(), baseline);
    }
}

#[test]
fn escaped_bindings_keep_the_actual_definition_and_original_lease_until_last_drop() {
    let project = TestProject::with_prefab_startup();
    write_project_file(
        &project,
        INPUT_PATH,
        action_map_json(primary_product(INPUT_ID), KeyCode::Space).as_bytes(),
    );
    select_input(&project, INPUT_PATH);
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let baseline = loader.budget_snapshot();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    let charged = loader.budget_snapshot();
    let snapshot_clone = snapshot.clone();
    let input: ProjectInputContent = snapshot.input_action_map().unwrap().clone();
    let input_clone = input.clone();
    assert!(input.shares_owner_with(&input_clone));
    assert!(input.shares_owner_with(snapshot_clone.input_action_map().unwrap()));
    let bindings = input.bindings();
    let escaped = bindings.clone();
    let definition_address = input.definition().bindings() as *const _;
    let compiled_retained = input.definition().cost().retained_bytes();
    assert!(bindings.shares_owner_with(&escaped));
    assert!(std::ptr::eq(
        bindings.definition().bindings(),
        definition_address
    ));
    assert_active_charges_eq(loader.budget_snapshot(), charged);
    drop(snapshot);
    assert_active_charges_eq(loader.budget_snapshot(), charged);
    drop(snapshot_clone);
    let input_only = loader.budget_snapshot();
    assert_eq!(
        input_only.active_generations(),
        baseline.active_generations()
    );
    assert_eq!(
        input_only.active_reservations(),
        baseline.active_reservations() + 1
    );
    assert!(input_only.active(ProjectContentBudgetKind::RetainedBytes) >= compiled_retained);
    for &kind in ProjectContentBudgetKind::ALL {
        if !matches!(
            kind,
            ProjectContentBudgetKind::RetainedBytes | ProjectContentBudgetKind::AggregateBytes
        ) {
            assert_eq!(
                input_only.active(kind),
                baseline.active(kind),
                "{kind:?} outlived its owner"
            );
        }
    }
    drop((input, input_clone));
    assert_active_charges_eq(loader.budget_snapshot(), input_only);
    drop(bindings);
    assert_active_charges_eq(loader.budget_snapshot(), input_only);
    assert!(std::ptr::eq(
        escaped.definition().bindings(),
        definition_address
    ));
    assert_eq!(
        escaped.definition().bindings()[0].input,
        InputBinding::Key(KeyCode::Space)
    );
    drop(escaped);
    assert_active_charges_eq(loader.budget_snapshot(), baseline);
}

#[cfg(feature = "project-workflow")]
#[test]
fn source_check_retains_the_same_input_file_receipt_and_diagnoses_changed_invalid_bytes() {
    use nara::{
        ProductRecipe,
        project_workflow::{ProjectCheckErrorKind, ProjectCheckRequest, check_project},
    };

    let project = TestProject::with_prefab_startup();
    let source = action_map_json(primary_product(INPUT_ID), KeyCode::Space);
    write_project_file(&project, INPUT_PATH, source.as_bytes());
    select_input(&project, INPUT_PATH);
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root.share()).unwrap();
    let baseline = loader.budget_snapshot();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    let checked =
        check_project(ProjectCheckRequest::new(root.share(), ProductRecipe::new())).unwrap();
    assert_eq!(
        checked.startup_content().content_digest(),
        snapshot.content_digest()
    );
    let receipt = snapshot.content_closure();
    let expected = receipt
        .members()
        .iter()
        .find(|member| member.kind() == ProjectContentMemberKind::InputActionMap)
        .unwrap();
    let actual = checked
        .members()
        .iter()
        .filter(|member| member.kind() == ProjectContentMemberKind::InputActionMap)
        .collect::<Vec<_>>();
    assert_eq!(actual.as_slice(), [expected]);
    drop((snapshot, receipt));
    assert_active_charges_eq(loader.budget_snapshot(), baseline);
    write_project_file(&project, INPUT_PATH, b"changed malformed selected Input");
    let error = check_project(ProjectCheckRequest::new(root, ProductRecipe::new())).unwrap_err();
    assert_eq!(error.kind(), ProjectCheckErrorKind::Content);
    assert_diagnostic(
        error.content_error().unwrap(),
        ProjectContentErrorKind::InputDefinition,
        "project.content.input-definition-invalid",
    );
    assert_eq!(
        actual[0].digest(),
        ContentDigest::of_bytes(source.as_bytes())
    );
}
