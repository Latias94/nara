use std::{
    fs,
    time::{Duration, Instant},
};

#[cfg(feature = "project-workflow")]
use nara::project_workflow::{ProjectCheckRequest, check_project};
use nara::{
    ProductRecipe,
    app::{FixedTime, RuntimeFault, RuntimeFaultKind, RuntimeFaultReporter, RuntimeState},
    asset::Assets,
    diagnostic::{DiagnosticReport, DiagnosticValueRef},
    image::ImageAsset,
    project_host::{HeadlessSession, HeadlessSessionClose, HeadlessSessionError},
    scene::SceneEntitySource,
    sprite::Sprite,
};

use crate::TestProject;

#[test]
fn checked_minimal_and_headless_projects_start_and_publish_their_scene() {
    for preset in [None, Some("minimal"), Some("local-headless")] {
        let project = TestProject::with_prefab_startup();
        if let Some(preset) = preset {
            let manifest = project.path().join("nara.toml");
            let source = fs::read_to_string(&manifest).unwrap();
            fs::write(
                manifest,
                format!("{source}\n[runtime]\npreset = '{preset}'\n"),
            )
            .unwrap();
        }
        let recipe = ProductRecipe::new();
        #[cfg(feature = "project-workflow")]
        let checked = check_project(ProjectCheckRequest::new(
            project.root_capability(),
            recipe.clone(),
        ))
        .unwrap();
        let mut session = HeadlessSession::new(project.root_capability(), recipe);
        session
            .start()
            .unwrap_or_else(|error| panic!("preset {preset:?}: {error:?}"));
        assert_eq!(session.state(), Some(RuntimeState::Running));
        #[cfg(feature = "project-workflow")]
        assert_eq!(
            session.startup_content_closure().unwrap().content_digest(),
            checked.startup_content().content_digest(),
        );
        let timestep = session
            .observe(|world| world.resource::<FixedTime>().timestep())
            .unwrap();
        assert!(session.frame(timestep).unwrap().is_none());
        session
            .observe(|world| {
                assert_eq!(world.resource::<FixedTime>().tick(), 1);
                let sprite = world
                    .iter_entities()
                    .find_map(|entity| {
                        let source = entity.get::<SceneEntitySource>()?;
                        (source.provenance().local_entity_id().as_str() == "enemy")
                            .then(|| entity.get::<Sprite>().unwrap())
                    })
                    .expect("the authored Prefab must be instantiated");
                let image = world
                    .resource::<Assets<ImageAsset>>()
                    .get(sprite.material.image.unwrap())
                    .expect("the live Sprite must resolve its Image");
                assert_eq!(
                    (
                        image.extent().width,
                        image.extent().height,
                        image.pixels().len()
                    ),
                    (1, 1, 4)
                );
            })
            .unwrap();
        close(&mut session, HeadlessSessionClose::Complete);
    }
}

#[test]
fn paused_capture_returns_the_original_fault_instead_of_the_checkpoint_value() {
    const SOURCE: &str = "test.paused-capture.required-service";

    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let mut session = HeadlessSession::new(project.root_capability(), ProductRecipe::new());
    session.start().unwrap();
    session.pause().unwrap();
    assert_eq!(session.capture(|_| 17).unwrap(), 17);
    let reporter = session
        .observe(|world| world.resource::<RuntimeFaultReporter>().clone())
        .unwrap();
    let expected = RuntimeFault::engine(RuntimeFaultKind::RequiredService, SOURCE);
    let worker_reporter = reporter.clone();
    let worker_fault = expected.clone();
    let result = session.capture(|_| {
        assert!(
            std::thread::spawn(move || worker_reporter.report(worker_fault))
                .join()
                .unwrap()
        );
        42
    });
    assert_eq!(session.state(), Some(RuntimeState::Faulted));
    assert_eq!(reporter.fault(), Some(expected));
    let Err(HeadlessSessionError::Runtime(diagnostics)) = result else {
        panic!("a fault during paused capture must discard its value: {result:?}");
    };
    assert_fault(&diagnostics, SOURCE);
    let Err(HeadlessSessionError::Runtime(diagnostics)) =
        session.capture(|_| panic!("must not run after a fault"))
    else {
        panic!("capture must retain the original fault");
    };
    assert_fault(&diagnostics, SOURCE);
    close(&mut session, HeadlessSessionClose::Failed);
    assert_fault(session.diagnostics(), SOURCE);
}

fn assert_fault(report: &DiagnosticReport, source: &str) {
    let diagnostic = report
        .iter()
        .find(|entry| entry.code().as_str() == "project.run.runtime-faulted")
        .unwrap();
    assert!(
        diagnostic.fields().iter().any(|field| {
            field.key().as_str() == "fault-source"
                && matches!(field.value(), DiagnosticValueRef::Identifier(value) if value == source)
        }),
        "{diagnostic:?}"
    );
}

fn close(session: &mut HeadlessSession, expected: HeadlessSessionClose) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let outcome = session.close();
        if outcome != HeadlessSessionClose::Pending {
            assert_eq!(outcome, expected, "{:?}", session.diagnostics());
            return;
        }
        assert!(Instant::now() < deadline, "Headless session did not retire");
        std::thread::yield_now();
    }
}
