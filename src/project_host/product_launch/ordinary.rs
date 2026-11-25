//! Ordinary desktop launch owns bootstrap, source authority and process cleanup policy.

use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::fs::FsError;

use super::{
    PRODUCT_CLEANUP_TIMEOUT, ProductSupervisedDesktopBootstrapError,
    ProductSupervisedDesktopLaunchCandidate, SupervisedDesktopProductIdentity,
    SupervisedDesktopWireError, open_explicit_product_project_root, open_product_project_root,
};
use crate::{
    ProductRecipe,
    project_host::{DesktopRun, DesktopRunIntent, DesktopRunOutcome, DesktopRunReport},
};

/// The ordinary process entrypoint for one Cargo desktop product.
///
/// Game code supplies its development root, Cargo identity and recipe. This owner selects packaged
/// or supervised source, verifies inherited source facts, drives the first-party desktop Host and
/// completes retained cleanup. Use [`DesktopRun`] directly for an embedded Host that owns its own
/// event-loop or incomplete-cleanup policy.
#[derive(Debug)]
pub struct DesktopProductLaunch {
    development_root: PathBuf,
    identity: SupervisedDesktopProductIdentity,
    recipe: ProductRecipe,
    standalone_profile: String,
    verify_first_present: bool,
}

impl DesktopProductLaunch {
    /// Binds the product identity shared by Cargo delivery and supervised Editor Play.
    pub fn from_cargo_product(
        development_root: impl AsRef<Path>,
        package_name: &str,
        package_version: &str,
        binary_target_name: &str,
        recipe: ProductRecipe,
    ) -> Result<Self, DesktopProductLaunchError> {
        let identity = SupervisedDesktopProductIdentity::from_cargo_product(
            package_name,
            package_version,
            binary_target_name,
        )
        .map_err(DesktopProductLaunchError::ProductIdentity)?;
        Ok(Self {
            development_root: development_root.as_ref().to_path_buf(),
            identity,
            recipe,
            standalone_profile: "desktop".into(),
            verify_first_present: false,
        })
    }

    /// Selects the manifest profile for a standalone launch, defaulting to `desktop`.
    ///
    /// An inherited supervised launch retains its original Host-selected profile and source
    /// agreement. This setting does not override that authority or the Cargo binary identity.
    #[must_use]
    pub fn with_standalone_profile(mut self, profile: impl Into<String>) -> Self {
        self.standalone_profile = profile.into();
        self
    }

    /// Requests bounded first-presentation verification for a standalone verification run.
    ///
    /// Supervised Editor Play already has its own first-presentation attestation and rejects this
    /// additional mode before opening source or starting the runtime.
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
    pub fn run(self) -> Result<DesktopRunReport, DesktopProductLaunchError> {
        let candidate = ProductSupervisedDesktopLaunchCandidate::read_from_inherited_bootstrap()
            .map_err(DesktopProductLaunchError::SupervisedBootstrap)?;
        Ok(finish_product_run(
            self.prepare(candidate)?,
            PRODUCT_CLEANUP_TIMEOUT,
        ))
    }

    fn prepare(
        self,
        candidate: Option<ProductSupervisedDesktopLaunchCandidate>,
    ) -> Result<DesktopRun, DesktopProductLaunchError> {
        if self.verify_first_present && candidate.is_some() {
            return Err(DesktopProductLaunchError::ConflictingPresentationModes);
        }
        let project_root = match candidate.as_ref() {
            Some(candidate) => {
                open_explicit_product_project_root(candidate.context().project_root())
            }
            None => open_product_project_root(&self.development_root),
        }
        .map_err(DesktopProductLaunchError::ProjectSource)?;
        let session = candidate
            .map(|candidate| candidate.verify_project_source(&project_root))
            .transpose()
            .map_err(DesktopProductLaunchError::SupervisedBootstrap)?;
        let profile = session
            .as_ref()
            .map(|session| session.context().profile_name())
            .unwrap_or(&self.standalone_profile);
        let intent = DesktopRunIntent::new()
            .with_profile(profile)
            .with_recipe(self.recipe);
        let intent = match session {
            Some(session) => intent.with_supervised_desktop_activation(session, self.identity),
            None if self.verify_first_present => intent.with_first_present_verification(),
            None => intent,
        };
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

/// Failure before an ordinary desktop runtime is created.
#[derive(Debug)]
pub enum DesktopProductLaunchError {
    ProductIdentity(SupervisedDesktopWireError),
    ProjectSource(FsError),
    SupervisedBootstrap(ProductSupervisedDesktopBootstrapError),
    ConflictingPresentationModes,
}

impl fmt::Display for DesktopProductLaunchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProductIdentity(source) => {
                write!(formatter, "desktop product identity: {source}")
            }
            Self::ProjectSource(source) => write!(formatter, "desktop product source: {source}"),
            Self::SupervisedBootstrap(source) => {
                write!(formatter, "supervised desktop launch: {source}")
            }
            Self::ConflictingPresentationModes => formatter.write_str(
                "first-presentation verification cannot be combined with supervised desktop launch",
            ),
        }
    }
}

impl Error for DesktopProductLaunchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ProductIdentity(source) => Some(source),
            Self::ProjectSource(source) => Some(source),
            Self::SupervisedBootstrap(source) => Some(source),
            Self::ConflictingPresentationModes => None,
        }
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
    fn ordinary_product_rejects_invalid_cargo_identity_before_source_open() {
        let error = DesktopProductLaunch::from_cargo_product(
            "not-an-absolute-root",
            "invalid package",
            "0.1.0",
            "desktop",
            ProductRecipe::new(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            DesktopProductLaunchError::ProductIdentity(_)
        ));
    }

    #[test]
    fn ordinary_product_rejects_missing_development_source_without_runtime_start() {
        let root = std::env::temp_dir().join(format!(
            "nara-missing-desktop-product-{}",
            std::process::id()
        ));
        let launch = DesktopProductLaunch::from_cargo_product(
            root,
            "game",
            "0.1.0",
            "desktop",
            ProductRecipe::new(),
        )
        .unwrap();
        assert!(matches!(
            launch.prepare(None),
            Err(DesktopProductLaunchError::ProjectSource(_))
        ));
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
            let launch =
                DesktopProductLaunch::from_cargo_product(&root, "game", "0.1.0", "desktop", recipe)
                    .unwrap();
            let report = finish_product_run(launch.prepare(None).unwrap(), Duration::ZERO);
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
