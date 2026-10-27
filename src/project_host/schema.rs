use std::io;

use nara_fs::{ContentDigest, DirectoryCapability, FsError, RelativePath};
use nara_reflect::AuthoringSchemaBundleLimits;

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
    let bytes = plan
        .schema_validation()
        .authoring_schema_bundle()
        .to_json_string()
        .map_err(|_| SchemaRefreshError::Encode)?
        .into_bytes();
    let limit = u64::try_from(AuthoringSchemaBundleLimits::default().encoded_bytes().get())
        .expect("the authoring bundle limit fits a filesystem byte limit");
    let current = match root.open_file(&relative) {
        Ok(file) => {
            let bytes = file
                .read_to_end_bounded(limit)
                .map_err(|_| SchemaRefreshError::BundleRead)?;
            Some((file.identity(), bytes))
        }
        Err(FsError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => None,
        Err(_) => return Err(SchemaRefreshError::BundleOpen),
    };
    if current.as_ref().is_some_and(|(_, actual)| *actual == bytes) {
        return Ok(false);
    }
    if !write {
        return Err(SchemaRefreshError::Stale);
    }
    let mut publication = match current {
        Some((identity, current)) => FilePublicationHost::bind_existing(
            root,
            path.as_str(),
            identity,
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
    use nara_reflect::AuthoringSchemaBundle;

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

    #[test]
    fn schema_check_does_not_write_missing_or_stale_bundle() {
        let project = TestProject::new();
        let root = host_fs::readable_directory(&project.0).unwrap();
        assert_eq!(root.options().rights(), CapabilityRights::ReadOnly);
        let error = execute(&root, None, ProductRecipe::new(), false).unwrap_err();
        assert_eq!(error, SchemaRefreshError::Stale);
        assert!(!project.bundle().exists());

        fs::write(project.bundle(), b"prior stale bundle").unwrap();
        let error = execute(&root, None, ProductRecipe::new(), false).unwrap_err();
        assert_eq!(error, SchemaRefreshError::Stale);
        assert_eq!(fs::read(project.bundle()).unwrap(), b"prior stale bundle");
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
            original_identity,
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
