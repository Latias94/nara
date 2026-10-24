use std::io;

use nara_fs::{ContentDigest, DirectoryCapability, FsError, RelativePath};
use nara_reflect::AuthoringSchemaBundleLimits;

use super::{
    file_publication::{FilePublicationHost, FilePublicationOutcome},
    ingest_project_manifest, resolve_product_recipe,
};
use crate::ProductRecipe;

/// Refreshes only the manifest-declared data bundle; it never admits or starts gameplay.
pub(crate) fn execute(
    root: &DirectoryCapability,
    profile: Option<&str>,
    recipe: ProductRecipe,
    write: bool,
) -> Result<bool, String> {
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").expect("the manifest path is valid"))
        .map_err(|error| format!("cannot open the project manifest: {error}"))?;
    let candidate = ingest_project_manifest(&manifest, profile)
        .map_err(|error| format!("cannot resolve the selected project profile: {error}"))?;
    let path = candidate
        .settings()
        .authoring_schema_bundle
        .as_ref()
        .ok_or("the project must declare authoring.schema_bundle before schema refresh")?;
    let relative = RelativePath::new(path.as_str())
        .map_err(|error| format!("invalid declared authoring schema bundle path: {error}"))?;
    let plan = resolve_product_recipe(&candidate, recipe)
        .map_err(|error| format!("cannot compose the project schema: {error}"))?;
    let bytes = plan
        .schema_validation()
        .authoring_schema_bundle()
        .to_json_string()
        .map_err(|error| format!("cannot encode the project schema: {error}"))?
        .into_bytes();
    let limit = u64::try_from(AuthoringSchemaBundleLimits::default().encoded_bytes().get())
        .expect("the authoring bundle limit fits a filesystem byte limit");
    let current = match root.open_file(&relative) {
        Ok(file) => {
            let bytes = file
                .read_to_end_bounded(limit)
                .map_err(|error| format!("cannot read the declared schema bundle: {error}"))?;
            Some((file.identity(), bytes))
        }
        Err(FsError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("cannot open the declared schema bundle: {error}")),
    };
    if current.as_ref().is_some_and(|(_, actual)| *actual == bytes) {
        return Ok(false);
    }
    if !write {
        return Err("the declared authoring schema bundle is missing or stale; run nara schema <project-directory> --write with the same profile".to_owned());
    }
    let mut publication = match current {
        Some((identity, current)) => FilePublicationHost::bind_existing(
            root,
            path.as_str(),
            identity,
            ContentDigest::of_bytes(&current),
            limit,
        )
        .map_err(|error| format!("cannot bind the declared schema bundle: {error:?}"))?,
        None => FilePublicationHost::bind_missing(root, path.as_str(), limit)
            .map_err(|error| format!("cannot bind the declared schema bundle: {error:?}"))?,
    };
    publish(&mut publication, &bytes)
}

fn publish(publication: &mut FilePublicationHost, bytes: &[u8]) -> Result<bool, String> {
    let mut diagnostic = match publication.publish(bytes) {
        FilePublicationOutcome::Published(accepted) => {
            let (_, binding) = accepted.into_parts();
            publication.commit_published(binding);
            return Ok(true);
        }
        FilePublicationOutcome::Rejected(reason) => {
            format!("schema bundle publication was rejected: {reason:?}; prior bytes were retained")
        }
        FilePublicationOutcome::Failed(stage) => {
            format!("schema bundle publication failed at {stage:?}; prior bytes were retained")
        }
        FilePublicationOutcome::Uncertain(_) => {
            "schema bundle publication outcome is uncertain; reopen and check the declared file before retrying".to_owned()
        }
    };
    if !publication.retry_cleanup() {
        let cleanup = publication
            .cleanup()
            .expect("failed cleanup retains its owner");
        diagnostic.push_str(&format!(
            "; cleanup pending: temporary schema bundle {:?}, identity {:?}; remove it before retrying schema write",
            cleanup.name().to_str(),
            cleanup.identity(),
        ));
    }
    Err(diagnostic)
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
        assert!(error.contains("missing or stale"));
        assert!(!project.bundle().exists());

        fs::write(project.bundle(), b"prior stale bundle").unwrap();
        let error = execute(&root, None, ProductRecipe::new(), false).unwrap_err();
        assert!(error.contains("--write"));
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
        assert!(
            execute(&root, None, custom_game::custom_recipe().unwrap(), false)
                .unwrap_err()
                .contains("stale")
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
        assert!(execute(&root, Some("unknown"), ProductRecipe::new(), true).is_err());
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
        assert!(
            execute(&root, None, ProductRecipe::new(), true)
                .unwrap_err()
                .contains("must declare authoring.schema_bundle")
        );
        assert!(!project.bundle().exists());
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
        let mut publication = FilePublicationHost::bind_existing(
            &root,
            "schema/authoring.bundle.json",
            file.identity(),
            ContentDigest::of_bytes(prior),
            1024,
        )
        .unwrap();
        drop(file);
        publication.test_fail_after_write();
        let error = publish(&mut publication, b"successor bundle").unwrap_err();
        assert!(error.contains("prior bytes were retained"));
        assert!(error.contains("cleanup pending"));
        assert_eq!(fs::read(project.bundle()).unwrap(), prior);
        assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 2);
        publication.test_allow_cleanup();
        assert!(publication.retry_cleanup());
        assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
    }
}
