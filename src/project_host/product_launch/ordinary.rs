//! Ordinary desktop launch owns source selection and process cleanup policy.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::fs::FsError;

use super::{PRODUCT_CLEANUP_TIMEOUT, open_product_project_root};
use crate::{
    ProductRecipe,
    project_host::{DesktopRun, DesktopRunIntent, DesktopRunOutcome, DesktopRunReport},
};

/// The ordinary process entrypoint for one Cargo desktop product.
///
/// The launch uses the packaged project or opens the game's development root. It drives the
/// desktop host and completes retained cleanup.
/// Use [`DesktopRun`] directly when an embedded Host owns its event loop or cleanup policy.
#[derive(Debug)]
pub struct DesktopProductLaunch {
    development_root: PathBuf,
    recipe: ProductRecipe,
    profile: String,
    verify_first_present: bool,
}

impl DesktopProductLaunch {
    pub fn new(development_root: impl AsRef<Path>, recipe: ProductRecipe) -> Self {
        Self {
            development_root: development_root.as_ref().to_path_buf(),
            recipe,
            profile: "desktop".into(),
            verify_first_present: false,
        }
    }

    /// Selects the manifest profile for a standalone launch, defaulting to `desktop`.
    #[must_use]
    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = profile.into();
        self
    }

    /// Requests bounded first-presentation verification for a standalone verification run.
    #[must_use]
    pub fn with_first_present_verification(mut self) -> Self {
        self.verify_first_present = true;
        self
    }

    /// Runs the product to completion or returns an incomplete cleanup report at the deadline.
    ///
    /// The report includes pending participants and any cleanup failure. An embedded host that
    /// needs to retain ownership for later retries can continue driving a [`DesktopRun`]. The
    /// deadline is checked between cleanup drives, so callbacks must return promptly.
    pub fn run(self) -> Result<DesktopRunReport, FsError> {
        Ok(finish_product_run(self.prepare()?, PRODUCT_CLEANUP_TIMEOUT))
    }

    fn prepare(self) -> Result<DesktopRun, FsError> {
        let project_root = open_product_project_root(&self.development_root)?;
        let mut intent = DesktopRunIntent::new()
            .with_profile(self.profile)
            .with_recipe(self.recipe);
        if self.verify_first_present {
            intent = intent.with_first_present_verification();
        }
        Ok(DesktopRun::new(project_root, intent))
    }
}

fn finish_product_run(mut run: DesktopRun, cleanup_timeout: Duration) -> DesktopRunReport {
    let mut cleanup_deadline = None;
    loop {
        let report = run.execute();
        if report.outcome() != DesktopRunOutcome::CleanupIncomplete {
            return report;
        }
        let now = Instant::now();
        let Some(deadline) = cleanup_deadline else {
            cleanup_deadline = Some(now + cleanup_timeout);
            // Drive the retained owner once before checking its cleanup deadline.
            continue;
        };
        if now >= deadline {
            return run.report_after_cleanup_deadline();
        }
        std::thread::sleep(Duration::from_millis(1).min(deadline.saturating_duration_since(now)));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use nara_app::{
        App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginShutdownObligationId,
        RuntimeCloseContext, RuntimeCloseParticipant, RuntimeCloseParticipantError,
        RuntimeCloseParticipantId, RuntimeCloseProgress, StartupStage,
    };

    use super::*;

    const CLOSE_OBLIGATION: PluginShutdownObligationId =
        PluginShutdownObligationId::new("nara.test.product-expired-cleanup");
    const CLOSE_PARTICIPANT: RuntimeCloseParticipantId =
        RuntimeCloseParticipantId::new("nara.test.product-expired-cleanup");
    const STARTUP_FAILURE: PluginDeclaration = PluginDeclaration::new(
        nara_app::PluginId::new("nara.test.product-startup-failure"),
        PluginCategory::Service,
    )
    .shutdown_obligations(&[CLOSE_OBLIGATION]);

    #[derive(Debug, Default)]
    struct StartupFailurePlugin<const PENDING: bool>;

    impl<const PENDING: bool> Plugin for StartupFailurePlugin<PENDING> {
        fn declaration() -> &'static PluginDeclaration {
            &STARTUP_FAILURE
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            app.register_plugin_runtime_close_participant(
                CLOSE_OBLIGATION,
                CLOSE_PARTICIPANT,
                UnfinishedCloseParticipant::<PENDING>,
            )?;
            app.add_systems(StartupStage::Runtime, || {
                Err::<(), _>(nara_ecs::bevy_ecs::error::BevyError::error(
                    std::io::Error::other("product startup failure"),
                ))
            })?;
            Ok(())
        }
    }

    struct UnfinishedCloseParticipant<const PENDING: bool>;

    static CLOSE_DROPS: AtomicUsize = AtomicUsize::new(0);

    impl<const PENDING: bool> Drop for UnfinishedCloseParticipant<PENDING> {
        fn drop(&mut self) {
            CLOSE_DROPS.fetch_add(1, Ordering::Relaxed);
        }
    }

    impl<const PENDING: bool> RuntimeCloseParticipant for UnfinishedCloseParticipant<PENDING> {
        fn begin_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            if PENDING {
                return Ok(RuntimeCloseProgress::Pending);
            }
            Err(RuntimeCloseParticipantError::terminal(
                "nara.test.product-live-close-failure",
            ))
        }

        fn poll_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            assert!(PENDING);
            Ok(RuntimeCloseProgress::Pending)
        }
    }

    #[test]
    fn ordinary_product_rejects_missing_development_source_without_runtime_start() {
        let root = std::env::temp_dir().join(format!(
            "nara-missing-desktop-product-{}",
            std::process::id()
        ));
        let launch = DesktopProductLaunch::new(root, ProductRecipe::new());
        let error = launch.prepare().err().expect("source must be missing");
        assert!(
            matches!(error, FsError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound)
        );
    }

    #[test]
    fn expired_live_cleanup_reports_participant_evidence_and_releases_the_owner() {
        let root = std::env::temp_dir().join(format!(
            "nara-product-expired-cleanup-{}",
            std::process::id()
        ));
        for path in ["assets", "scenes", "prefabs", "schema"] {
            std::fs::create_dir_all(root.join(path)).unwrap();
        }
        let bundle = nara_reflect::AuthoringSchemaBundle::from_trusted_provider_definitions(
            crate::project_host::advanced::built_in_schema_providers(),
        )
        .unwrap();
        std::fs::write(
            root.join("schema/authoring.bundle.json"),
            bundle.to_json_string().unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("scenes/startup.scene.json"),
            nara_scene::SceneDocument::default()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("nara.toml"),
            r#"schema_version = 1
[project]
source_universe = 'faedd389-ae49-519e-b046-6351fc9eda45'
native_document_identity = 'project_location'
name = "product-expired-cleanup"
[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
[startup]
default_scene = "startup.scene.json"
[authoring]
state = "ready"
schema_bundle = "schema/authoring.bundle.json"
[runtime]
preset = "local-headless"
[capabilities]
requested = ["runtime-2d"]
[profiles.desktop.runtime]
preset = "local-headless"
[profiles.desktop.capabilities]
requested = ["runtime-2d"]
"#,
        )
        .unwrap();
        let drops_before = CLOSE_DROPS.load(Ordering::Relaxed);
        for pending in [false, true] {
            let recipe = if pending {
                ProductRecipe::new().add_plugin::<StartupFailurePlugin<true>>()
            } else {
                ProductRecipe::new().add_plugin::<StartupFailurePlugin<false>>()
            }
            .unwrap();
            let launch = DesktopProductLaunch::new(&root, recipe);
            let report = finish_product_run(launch.prepare().unwrap(), Duration::ZERO);
            assert_eq!(report.outcome(), DesktopRunOutcome::CleanupIncomplete);
            let diagnostics = format!("{:#?}", report.diagnostics());
            assert!(
                diagnostics.contains("nara.test.product-expired-cleanup"),
                "{diagnostics}"
            );
            assert!(
                diagnostics.contains("project.run.cleanup-participant-pending"),
                "{diagnostics}"
            );
            if pending {
                assert!(!diagnostics.contains("failure-code"), "{diagnostics}");
                assert!(diagnostics.contains("poll"), "{diagnostics}");
            } else {
                assert!(
                    diagnostics.contains("nara.test.product-live-close-failure"),
                    "{diagnostics}"
                );
                assert!(diagnostics.contains("begin"), "{diagnostics}");
            }
        }
        assert_eq!(CLOSE_DROPS.load(Ordering::Relaxed), drops_before + 2);
        std::fs::remove_dir_all(root).unwrap();
    }
}
