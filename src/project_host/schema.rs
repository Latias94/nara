use std::io;

use nara_fs::{ContentDigest, DirectoryCapability, FsError, RelativePath};
use nara_reflect::{AuthoringSchemaBundle, AuthoringSchemaBundleLimits};

use super::{
    file_publication::{FilePublicationHost, FilePublicationOutcome},
    ingest_project_manifest, resolve_product_recipe,
};
use crate::ProductRecipe;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SchemaPublicationFailure {
    Rejected,
    Failed,
    Uncertain,
}

/// Static owner facts only; cleanup state is independent of the publication outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SchemaRefreshError {
    ManifestAccess,
    ManifestAdmission,
    DeclarationMissing,
    InvalidPath,
    Composition,
    Encode,
    BundleRead,
    BundleOpen,
    BundleInvalid,
    BundleComposition,
    Stale,
    Binding,
    Publication {
        outcome: SchemaPublicationFailure,
        cleanup_pending: bool,
    },
}

/// Refreshes only the manifest-declared data bundle; it never admits or starts gameplay.
pub(crate) fn execute(
    root: &DirectoryCapability,
    profile: Option<&str>,
    recipe: ProductRecipe,
    write: bool,
) -> Result<bool, SchemaRefreshError> {
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").expect("the manifest path is valid"))
        .map_err(|_| SchemaRefreshError::ManifestAccess)?;
    let candidate = ingest_project_manifest(&manifest, profile)
        .map_err(|_| SchemaRefreshError::ManifestAdmission)?;
    let path = candidate
        .settings()
        .authoring_schema_bundle
        .as_ref()
        .ok_or(SchemaRefreshError::DeclarationMissing)?;
    let relative = RelativePath::new(path.as_str()).map_err(|_| SchemaRefreshError::InvalidPath)?;
    let plan =
        resolve_product_recipe(&candidate, recipe).map_err(|_| SchemaRefreshError::Composition)?;
    let recipe_bundle = plan.schema_validation().authoring_schema_bundle();
    let limits = AuthoringSchemaBundleLimits::default();
    let limit = u64::try_from(limits.encoded_bytes().get())
        .expect("the authoring bundle limit fits a filesystem byte limit");
    let current = match root.open_file(&relative) {
        Ok(file) => {
            let bytes = file
                .read_to_end_bounded(limit)
                .map_err(|error| match error {
                    FsError::ByteLimitExceeded { .. } => SchemaRefreshError::BundleInvalid,
                    _ => SchemaRefreshError::BundleRead,
                })?;
            let bundle = AuthoringSchemaBundle::from_json_bytes_with_limits(&bytes, limits)
                .map_err(|_| SchemaRefreshError::BundleInvalid)?;
            Some((file, bytes, bundle))
        }
        Err(FsError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => None,
        Err(_) => return Err(SchemaRefreshError::BundleOpen),
    };
    let retained = current
        .as_ref()
        .into_iter()
        .flat_map(|(_, _, bundle)| bundle.owners())
        .filter(|owner| recipe_bundle.owner(owner.owner_id()).is_none());
    let successor = AuthoringSchemaBundle::from_owner_snapshots(
        recipe_bundle.owners().iter().chain(retained).cloned(),
    )
    .map_err(|_| SchemaRefreshError::BundleComposition)?;
    // Reflect admits canonical owner/catalog content. Compare the complete lineage, not raw
    // JSON spelling or just the current fingerprint, so a no-op preserves the observed leaf.
    if current
        .as_ref()
        .is_some_and(|(_, _, actual)| *actual == successor)
    {
        return Ok(false);
    }
    let bytes = successor
        .to_json_string()
        .map_err(|_| SchemaRefreshError::Encode)?
        .into_bytes();
    if !write {
        return Err(SchemaRefreshError::Stale);
    }
    let mut publication = match current {
        Some((file, current, _)) => FilePublicationHost::bind_existing(
            root,
            path.as_str(),
            &file,
            ContentDigest::of_bytes(&current),
            limit,
        )
        .map_err(|_| SchemaRefreshError::Binding)?,
        None => FilePublicationHost::bind_missing(root, path.as_str(), limit)
            .map_err(|_| SchemaRefreshError::Binding)?,
    };
    publish(&mut publication, &bytes)
}

fn publish(
    publication: &mut FilePublicationHost,
    bytes: &[u8],
) -> Result<bool, SchemaRefreshError> {
    let outcome = match publication.publish(bytes) {
        FilePublicationOutcome::Published(accepted) => {
            let (_, binding) = accepted.into_parts();
            publication.commit_published(binding);
            return Ok(true);
        }
        FilePublicationOutcome::Rejected(_) => SchemaPublicationFailure::Rejected,
        FilePublicationOutcome::Failed(_) => SchemaPublicationFailure::Failed,
        FilePublicationOutcome::Uncertain(_) => SchemaPublicationFailure::Uncertain,
    };
    let cleanup_pending = !publication.retry_cleanup();
    Err(SchemaRefreshError::Publication {
        outcome,
        cleanup_pending,
    })
}

#[cfg(test)]
mod custom_game {
    use crate as nara;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/project_tooling_game.rs"
    ));
}

#[cfg(all(
    test,
    any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use nara_fs::CapabilityRights;
    use nara_reflect::{
        AuthoringSchemaBundleOwner, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId,
        PersistentComponentSchemaOwnerId,
    };

    use super::*;
    use crate::project_tooling::host_fs;

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

    struct TestProject(PathBuf);

    impl TestProject {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "nara-schema-{}-{}",
                std::process::id(),
                NEXT_ROOT.fetch_add(1, Ordering::Relaxed),
            ));
            fs::create_dir_all(path.join("schema")).unwrap();
            fs::write(
                path.join("nara.toml"),
                r#"schema_version = 1
[project]
source_universe = 'e6896809-3ac2-5a42-bfb7-cddc44b3f2ee'
native_document_identity = 'project_location'
name = "schema-test"
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
            Self(path)
        }

        fn bundle(&self) -> PathBuf {
            self.0.join("schema/authoring.bundle.json")
        }
    }

    impl Drop for TestProject {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn lineage_owner(owner: &str) -> AuthoringSchemaBundleOwner {
        let active = ComponentSchema::new(
            ComponentTypeId::new(format!("{owner}.Active")),
            "Active",
            ComponentSchemaVersion::ONE,
        );
        let retired = ComponentTypeId::new(format!("{owner}.Retired"));
        let predecessor = ComponentSchemaCatalog {
            components: vec![
                active.clone(),
                ComponentSchema::new(retired.clone(), "Retired", ComponentSchemaVersion::ONE),
            ],
            type_tombstones: vec![ComponentTypeId::new(format!("{owner}.Ancient"))],
            ..Default::default()
        };
        let mut current = ComponentSchemaCatalog::successor_of(&predecessor).unwrap();
        current.components.push(active);
        current.type_tombstones.push(retired);
        AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new(owner).unwrap(),
            current,
            Some(predecessor),
        )
    }

    #[test]
    fn schema_check_does_not_write_missing_or_stale_bundle() {
        let project = TestProject::new();
        let root = host_fs::readable_directory(&project.0).unwrap();
        assert_eq!(root.options().rights(), CapabilityRights::ReadOnly);
        let error = execute(&root, None, ProductRecipe::new(), false).unwrap_err();
        assert_eq!(error, SchemaRefreshError::Stale);
        assert!(!project.bundle().exists());

        let stale = AuthoringSchemaBundle::new([])
            .unwrap()
            .to_json_string()
            .unwrap();
        fs::write(project.bundle(), &stale).unwrap();
        let error = execute(&root, None, ProductRecipe::new(), false).unwrap_err();
        assert_eq!(error, SchemaRefreshError::Stale);
        assert_eq!(fs::read(project.bundle()).unwrap(), stale.as_bytes());
        assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
    }

    #[test]
    fn schema_write_uses_the_custom_recipe_without_admitting_gameplay() {
        let project = TestProject::new();
        fs::create_dir(project.0.join("scenes")).unwrap();
        fs::write(
            project.0.join("scenes/startup.scene.json"),
            b"invalid scene",
        )
        .unwrap();
        let root = host_fs::writable_directory(&project.0).unwrap();
        assert!(execute(&root, None, ProductRecipe::new(), true).unwrap());
        let built_in = fs::read(project.bundle()).unwrap();
        assert_eq!(
            execute(&root, None, custom_game::custom_recipe().unwrap(), false).unwrap_err(),
            SchemaRefreshError::Stale,
        );
        assert_eq!(fs::read(project.bundle()).unwrap(), built_in);
        assert!(execute(&root, None, custom_game::custom_recipe().unwrap(), true).unwrap());
        let actual = fs::read(project.bundle()).unwrap();
        let manifest = root
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let plan =
            resolve_product_recipe(&candidate, custom_game::custom_recipe().unwrap()).unwrap();
        assert_eq!(
            AuthoringSchemaBundle::from_json_bytes(&actual).unwrap(),
            *plan.schema_validation().authoring_schema_bundle(),
        );
        assert!(execute(&root, None, custom_game::custom_recipe().unwrap(), false).is_ok());
        let original_identity = root
            .open_file(&RelativePath::new("schema/authoring.bundle.json").unwrap())
            .unwrap()
            .identity();
        assert!(!execute(&root, None, custom_game::custom_recipe().unwrap(), true).unwrap());
        assert_eq!(
            original_identity,
            root.open_file(&RelativePath::new("schema/authoring.bundle.json").unwrap())
                .unwrap()
                .identity(),
        );
        assert_eq!(fs::read(project.bundle()).unwrap(), actual);
    }

    #[test]
    fn recipe_refresh_preserves_external_lineage_through_addition_and_owner_replacement() {
        let project = TestProject::new();
        let root = host_fs::writable_directory(&project.0).unwrap();
        assert!(execute(&root, None, ProductRecipe::new(), true).unwrap());
        let built_in =
            AuthoringSchemaBundle::from_json_bytes(&fs::read(project.bundle()).unwrap()).unwrap();
        let external = lineage_owner("studio.adopted");
        let existing = AuthoringSchemaBundle::new(
            built_in
                .owners()
                .iter()
                .map(AuthoringSchemaBundleOwner::from)
                .chain([external.clone()]),
        )
        .unwrap();
        let prior = existing.to_json_string().unwrap();
        fs::write(project.bundle(), &prior).unwrap();
        let readonly = host_fs::readable_directory(&project.0).unwrap();
        assert_eq!(
            execute(
                &readonly,
                None,
                custom_game::custom_recipe().unwrap(),
                false
            ),
            Err(SchemaRefreshError::Stale),
        );
        assert_eq!(fs::read(project.bundle()).unwrap(), prior.as_bytes());
        assert!(execute(&root, None, custom_game::custom_recipe().unwrap(), true).unwrap());
        let added =
            AuthoringSchemaBundle::from_json_bytes(&fs::read(project.bundle()).unwrap()).unwrap();
        assert_eq!(
            AuthoringSchemaBundleOwner::from(added.owner(external.owner_id()).unwrap()),
            external,
        );
        let pulse_id = PersistentComponentSchemaOwnerId::try_new("studio.pulse").unwrap();
        assert!(added.owner(&pulse_id).is_some());

        // The recipe replaces its owner's entire declaration, including an existing predecessor
        // and tombstones; it has no authority to drop the independently adopted owner's lineage.
        let divergent = AuthoringSchemaBundle::new(
            added
                .owners()
                .iter()
                .filter(|owner| owner.owner_id() != &pulse_id)
                .map(AuthoringSchemaBundleOwner::from)
                .chain([lineage_owner("studio.pulse")]),
        )
        .unwrap();
        let prior = divergent.to_json_string().unwrap();
        fs::write(project.bundle(), &prior).unwrap();
        assert_eq!(
            execute(
                &readonly,
                None,
                custom_game::custom_recipe().unwrap(),
                false
            ),
            Err(SchemaRefreshError::Stale),
        );
        assert_eq!(fs::read(project.bundle()).unwrap(), prior.as_bytes());
        assert!(execute(&root, None, custom_game::custom_recipe().unwrap(), true).unwrap());
        let replaced =
            AuthoringSchemaBundle::from_json_bytes(&fs::read(project.bundle()).unwrap()).unwrap();
        assert_eq!(replaced, added);
        assert!(replaced.owner(&pulse_id).unwrap().predecessor().is_none());

        // JSON spelling is not the no-change contract. A canonical admitted composition in a
        // compact envelope keeps its exact bytes and leaf in both read-only Check and write mode.
        let value: serde_json::Value =
            serde_json::from_str(&replaced.to_json_string().unwrap()).unwrap();
        let lexical = format!("{}\n", serde_json::to_string(&value).unwrap());
        assert_ne!(lexical, replaced.to_json_string().unwrap());
        fs::write(project.bundle(), &lexical).unwrap();
        let writable_leaf = root
            .open_file(&RelativePath::new("schema/authoring.bundle.json").unwrap())
            .unwrap();
        let readonly_leaf = readonly
            .open_file(&RelativePath::new("schema/authoring.bundle.json").unwrap())
            .unwrap();
        for (capability, write, leaf) in [
            (&readonly, false, &readonly_leaf),
            (&root, true, &writable_leaf),
        ] {
            assert_eq!(
                execute(
                    capability,
                    None,
                    custom_game::custom_recipe().unwrap(),
                    write
                ),
                Ok(false),
            );
            assert_eq!(fs::read(project.bundle()).unwrap(), lexical.as_bytes());
            assert_eq!(
                capability
                    .open_file(&RelativePath::new("schema/authoring.bundle.json").unwrap())
                    .unwrap()
                    .identity(),
                leaf.identity(),
            );
            assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
        }

        // Exercise ordinary content admission with the retained owner present. No App or native
        // Host is started, and the extra metadata does not enter the selected runtime schema.
        for directory in ["scenes", "assets", "prefabs"] {
            fs::create_dir(project.0.join(directory)).unwrap();
        }
        fs::write(
            project.0.join("scenes/startup.scene.json"),
            crate::scene::SceneDocument::from_ordered_roots([])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let manifest_path = project.0.join("nara.toml");
        let manifest = fs::read_to_string(&manifest_path).unwrap();
        fs::write(
            manifest_path,
            format!(
                "{manifest}\n[paths]\nassets = 'assets'\nscenes = 'scenes'\nprefabs = 'prefabs'\n[startup]\ndefault_scene = 'startup.scene.json'\n",
            ),
        )
        .unwrap();
        let checked = crate::project_workflow::check_project(
            crate::project_workflow::ProjectCheckRequest::new(
                readonly.share(),
                custom_game::custom_recipe().unwrap(),
            ),
        )
        .unwrap();
        assert!(checked.members().iter().any(|member| {
            member.kind()
                == crate::project_workflow::ProjectContentMemberKind::AuthoringSchemaBundle
        }));
        assert_eq!(fs::read(project.bundle()).unwrap(), lexical.as_bytes());
        assert_eq!(
            readonly
                .open_file(&RelativePath::new("schema/authoring.bundle.json").unwrap())
                .unwrap()
                .identity(),
            readonly_leaf.identity(),
        );
    }

    #[test]
    fn composed_bundle_shape_rejects_before_replacing_admitted_project_metadata() {
        let owner = |name: &str| {
            let mut schema = ComponentSchema::new(
                ComponentTypeId::new(format!("{name}.Value")),
                "Value",
                ComponentSchemaVersion::ONE,
            );
            schema.aliases = (0..35_000)
                .map(|index| format!("{index:08}{}", "a".repeat(120)))
                .collect();
            ComponentSchemaOwnerSnapshot::admit(
                PersistentComponentSchemaOwnerId::try_new(name).unwrap(),
                ComponentSchemaCatalog {
                    components: vec![schema],
                    ..Default::default()
                },
                None,
            )
            .unwrap()
        };
        let retained = owner("studio.retained");
        let selected = owner("studio.selected");
        let project = TestProject::new();
        let original = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::from(&retained)])
            .unwrap()
            .to_json_string()
            .unwrap();
        fs::write(project.bundle(), &original).unwrap();
        let recipe = ProductRecipe::new()
            .add_contribution(
                custom_game::custom_contribution()
                    .unwrap()
                    .with_data_owners([selected])
                    .unwrap(),
            )
            .unwrap();
        let root = host_fs::writable_directory(&project.0).unwrap();
        let relative = RelativePath::new("schema/authoring.bundle.json").unwrap();
        let leaf = root.open_file(&relative).unwrap();
        assert_eq!(
            execute(&root, None, recipe, true),
            Err(SchemaRefreshError::Encode),
        );
        assert_eq!(fs::read(project.bundle()).unwrap(), original.as_bytes());
        assert_eq!(
            root.open_file(&relative).unwrap().identity(),
            leaf.identity()
        );
        assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
    }

    #[test]
    fn retained_component_and_tombstone_claims_reject_before_check_or_write_publication() {
        let claim = |tombstone| {
            let mut catalog = ComponentSchemaCatalog::default();
            let id = ComponentTypeId::new("studio.claim.Shared");
            if tombstone {
                catalog.type_tombstones.push(id);
            } else {
                catalog.components.push(ComponentSchema::new(
                    id,
                    "Shared",
                    ComponentSchemaVersion::ONE,
                ));
            }
            catalog
        };
        for (retained_tombstone, recipe_tombstone) in
            [(false, false), (true, false), (false, true), (true, true)]
        {
            let project = TestProject::new();
            let retained = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
                PersistentComponentSchemaOwnerId::try_new("studio.retained").unwrap(),
                claim(retained_tombstone),
                None,
            )])
            .unwrap();
            let prior = retained.to_json_string().unwrap();
            fs::write(project.bundle(), &prior).unwrap();
            let selected = ComponentSchemaOwnerSnapshot::admit(
                PersistentComponentSchemaOwnerId::try_new("studio.selected").unwrap(),
                claim(recipe_tombstone),
                None,
            )
            .unwrap();
            let recipe = ProductRecipe::new()
                .add_contribution(
                    custom_game::custom_contribution()
                        .unwrap()
                        .with_data_owners([selected])
                        .unwrap(),
                )
                .unwrap();
            let readonly = host_fs::readable_directory(&project.0).unwrap();
            let root = host_fs::writable_directory(&project.0).unwrap();
            let writable_leaf = root
                .open_file(&RelativePath::new("schema/authoring.bundle.json").unwrap())
                .unwrap();
            let readonly_leaf = readonly
                .open_file(&RelativePath::new("schema/authoring.bundle.json").unwrap())
                .unwrap();
            for (capability, write, leaf) in [
                (&readonly, false, &readonly_leaf),
                (&root, true, &writable_leaf),
            ] {
                assert_eq!(
                    execute(capability, None, recipe.clone(), write),
                    Err(SchemaRefreshError::BundleComposition),
                    "retained tombstone: {retained_tombstone}, recipe tombstone: {recipe_tombstone}",
                );
                assert_eq!(fs::read(project.bundle()).unwrap(), prior.as_bytes());
                assert_eq!(
                    capability
                        .open_file(&RelativePath::new("schema/authoring.bundle.json").unwrap())
                        .unwrap()
                        .identity(),
                    leaf.identity(),
                );
                assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
            }
        }
    }

    #[test]
    fn invalid_existing_bundle_is_not_permission_to_replace_its_bytes() {
        let owner = lineage_owner("studio.adopted");
        let bundle = AuthoringSchemaBundle::new([owner]).unwrap();
        let mut invalid_lineage: serde_json::Value =
            serde_json::from_str(&bundle.to_json_string().unwrap()).unwrap();
        invalid_lineage["payload"]["owners"][0]
            .as_object_mut()
            .unwrap()
            .remove("predecessor");
        for bytes in [
            b"prior stale bundle".to_vec(),
            b"{}".to_vec(),
            serde_json::to_vec(&invalid_lineage).unwrap(),
        ] {
            let project = TestProject::new();
            fs::write(project.bundle(), &bytes).unwrap();
            let readonly = host_fs::readable_directory(&project.0).unwrap();
            let root = host_fs::writable_directory(&project.0).unwrap();
            let writable_leaf = root
                .open_file(&RelativePath::new("schema/authoring.bundle.json").unwrap())
                .unwrap();
            let readonly_leaf = readonly
                .open_file(&RelativePath::new("schema/authoring.bundle.json").unwrap())
                .unwrap();
            for (capability, write, leaf) in [
                (&readonly, false, &readonly_leaf),
                (&root, true, &writable_leaf),
            ] {
                assert_eq!(
                    execute(capability, None, ProductRecipe::new(), write),
                    Err(SchemaRefreshError::BundleInvalid),
                );
                assert_eq!(fs::read(project.bundle()).unwrap(), bytes);
                assert_eq!(
                    capability
                        .open_file(&RelativePath::new("schema/authoring.bundle.json").unwrap())
                        .unwrap()
                        .identity(),
                    leaf.identity(),
                );
                assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
            }
        }
    }

    #[test]
    fn schema_refresh_requires_a_declaration_and_known_profile() {
        let project = TestProject::new();
        let root = host_fs::writable_directory(&project.0).unwrap();
        assert_eq!(
            execute(&root, Some("unknown"), ProductRecipe::new(), true).unwrap_err(),
            SchemaRefreshError::ManifestAdmission,
        );
        assert!(!project.bundle().exists());
        let manifest = fs::read_to_string(project.0.join("nara.toml")).unwrap();
        fs::write(
            project.0.join("nara.toml"),
            manifest.replace(
                "[authoring]\nschema_bundle = \"schema/authoring.bundle.json\"\n",
                "",
            ),
        )
        .unwrap();
        assert_eq!(
            execute(&root, None, ProductRecipe::new(), true).unwrap_err(),
            SchemaRefreshError::DeclarationMissing,
        );
        assert!(!project.bundle().exists());
    }

    #[test]
    fn publication_projection_keeps_outcome_and_cleanup_independent() {
        use SchemaPublicationFailure::{Failed, Rejected, Uncertain};

        // Typed projection fixtures, not reproductions of filesystem publication uncertainty.
        for (outcome, cleanup_pending, code, summary) in [
            (
                Rejected,
                false,
                "project.tool.schema-publication-rejected",
                "Schema bundle publication was rejected; prior bytes were retained",
            ),
            (
                Rejected,
                true,
                "project.tool.schema-publication-rejected-cleanup-pending",
                "Schema bundle publication was rejected; prior bytes were retained; temporary schema bundle cleanup is pending before retrying schema write",
            ),
            (
                Failed,
                false,
                "project.tool.schema-publication-failed",
                "Schema bundle publication failed; prior bytes were retained",
            ),
            (
                Failed,
                true,
                "project.tool.schema-publication-failed-cleanup-pending",
                "Schema bundle publication failed; prior bytes were retained; temporary schema bundle cleanup is pending before retrying schema write",
            ),
            (
                Uncertain,
                false,
                "project.tool.schema-publication-uncertain",
                "Schema bundle publication outcome is uncertain; reopen and check the declared file before retrying",
            ),
            (
                Uncertain,
                true,
                "project.tool.schema-publication-uncertain-cleanup-pending",
                "Schema bundle publication outcome is uncertain; reopen and check the declared file and resolve pending temporary schema bundle cleanup before retrying schema write",
            ),
        ] {
            let failure = crate::project_tooling::test_schema_failure_protocol_roundtrip(
                SchemaRefreshError::Publication {
                    outcome,
                    cleanup_pending,
                },
            );
            assert_eq!(failure.operation(), "schema-write");
            assert_eq!(
                failure.stage(),
                crate::project_tooling::ProjectToolFailureStage::Schema,
            );
            assert_eq!(failure.code(), code);
            assert_eq!(failure.summary(), summary);
            assert!(failure.diagnostics().is_empty());
            assert_eq!(
                failure.loss(),
                crate::project_tooling::ProjectToolDiagnosticLoss::default(),
            );
            assert!(failure.repair_hint().is_none());
        }
    }

    #[cfg(windows)]
    #[test]
    fn failed_schema_write_preserves_prior_bytes_and_reports_pending_cleanup() {
        let project = TestProject::new();
        let prior = b"prior bundle";
        fs::write(project.bundle(), prior).unwrap();
        let root = host_fs::writable_directory(&project.0).unwrap();
        let file = root
            .open_file(&RelativePath::new("schema/authoring.bundle.json").unwrap())
            .unwrap();
        let original_identity = file.identity();
        let mut publication = FilePublicationHost::bind_existing(
            &root,
            "schema/authoring.bundle.json",
            &file,
            ContentDigest::of_bytes(prior),
            1024,
        )
        .unwrap();
        let original_binding = publication.binding();
        drop(file);
        publication.test_fail_after_write();
        let error = publish(&mut publication, b"successor bundle").unwrap_err();
        assert_eq!(
            error,
            SchemaRefreshError::Publication {
                outcome: SchemaPublicationFailure::Failed,
                cleanup_pending: true,
            },
        );
        assert_eq!(publication.binding(), original_binding);
        assert!(!publication.uncertain());
        assert!(publication.has_cleanup());
        assert_eq!(fs::read(project.bundle()).unwrap(), prior);
        assert_eq!(
            root.open_file(&RelativePath::new("schema/authoring.bundle.json").unwrap())
                .unwrap()
                .identity(),
            original_identity,
        );
        assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 2);

        let failure = crate::project_tooling::test_schema_failure_protocol_roundtrip(error);
        assert_eq!(failure.operation(), "schema-write");
        assert_eq!(
            failure.stage(),
            crate::project_tooling::ProjectToolFailureStage::Schema,
        );
        assert_eq!(
            failure.code(),
            "project.tool.schema-publication-failed-cleanup-pending",
        );
        assert_eq!(
            failure.summary(),
            "Schema bundle publication failed; prior bytes were retained; temporary schema bundle cleanup is pending before retrying schema write",
        );
        assert!(failure.diagnostics().is_empty());
        assert_eq!(
            failure.loss(),
            crate::project_tooling::ProjectToolDiagnosticLoss::default(),
        );
        assert!(failure.repair_hint().is_none());
        assert!(publication.has_cleanup());
        assert_eq!(fs::read(project.bundle()).unwrap(), prior);
        assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 2);

        publication.test_allow_cleanup();
        assert!(publication.retry_cleanup());
        assert!(!publication.has_cleanup());
        assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
    }
}
