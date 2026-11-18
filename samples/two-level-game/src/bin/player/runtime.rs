use std::{
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    process::ExitCode,
    time::{Duration, Instant},
};

use nara::{
    fs::{ContentDigest, RelativePath},
    project_host::{HeadlessSession, HeadlessSessionClose, open_executable_relative_runtime_pack},
    runtime_content::{
        RuntimeContentExpectation, RuntimeContentLimits, RuntimeContentMemberKind,
        RuntimeContentTarget,
    },
};
use serde_json::{Value, json};
use two_level_game::{
    GamePhase, RunOptions, Scenario,
    animation::{atlas_product, clips_product},
    input::input_product,
    levels::{LevelId, LevelKey},
    markers::marker_product,
    save::{CURRENT_FORMAT_VERSION, io::SaveSlot},
    side_room,
};

use super::{evidence::Evidence, generated, output};

pub(super) fn run(options: RunOptions) -> ExitCode {
    let mut session = None;
    let mut evidence = Evidence::default();
    let mut save = None;
    let outcome = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(60);
        let expected = RuntimeContentExpectation::new(
            ContentDigest::from_parts(generated::CATALOG_LENGTH, generated::CATALOG_HASH),
            RuntimeContentTarget::new(generated::CONTENT_TARGET, generated::CONTENT_PROFILE)
                .map_err(|error| error.to_string())?,
        );
        let base = open_executable_relative_runtime_pack(
            &RelativePath::new(generated::PACK_FILE).map_err(|error| error.to_string())?,
            &expected,
            two_level_game::cooked_recipe(),
            RuntimeContentLimits::default(),
        )
        .map_err(|error| error.to_string())?;
        let catalog = base.catalog();
        if catalog.startup_scene() != LevelId::STARTUP.scene_product()
            || catalog.scenes().len() != LevelId::ALL.len()
            || catalog.members().len() != 13
            || catalog
                .members()
                .iter()
                .filter(|member| matches!(member.kind(), RuntimeContentMemberKind::Scene { .. }))
                .count()
                != 5
            || [
                marker_product(),
                side_room::scene_product(),
                side_room::lookout_scene_product(),
            ]
            .into_iter()
            .any(|product| {
                !base.member(product).is_some_and(|member| {
                    matches!(member.kind(), RuntimeContentMemberKind::Scene { .. })
                })
            })
            || !catalog.addressable_roots().is_empty()
            || base.plan().settings().input().action_map != Some(input_product())
            || base.member(input_product()).is_none()
            || base.member(atlas_product()).is_none()
            || base.member(clips_product()).is_none()
            || base.member(side_room::image_product()).is_none()
            || base
                .member(two_level_game::sound_bank::bank_product())
                .is_none()
            || LevelId::ALL.into_iter().any(|level| {
                !catalog.scenes().contains(&level.scene_product())
                    || base.member(level.image_product()).is_none()
            })
        {
            return Err(
                "compiled pack does not contain the game's exact two-level Product catalog".into(),
            );
        }
        let input = base
            .acquire_startup_input()
            .map_err(|error| error.to_string())?
            .ok_or("compiled pack has no selected global Input")?;
        evidence.expect_input(&input)?;
        let original = session.insert(HeadlessSession::from_runtime_content(base));
        original.start().map_err(|error| error.to_string())?;
        evidence.observe(original)?;
        if let Some(command) = two_level_game::player_launch(options.clone())? {
            original
                .submit(command)
                .map_err(|error| error.to_string())?;
        }
        drive(original, &mut evidence, options.scenario, deadline)?;
        save = verify_save(&options)?;
        Ok(())
    }));
    let close = session.as_mut().map(close_original);
    let failed_diagnostics = session
        .as_ref()
        .is_some_and(|original| original.diagnostics().has_errors());
    if let Some(original) = &session
        && failed_diagnostics
    {
        let _ = output::diagnostic_to(
            &mut std::io::stderr().lock(),
            &format!("{:?}", original.diagnostics()),
        );
    }
    match outcome {
        Ok(result) => {
            let message = match result {
                Err(message) => Some(message),
                Ok(()) if close == Some(HeadlessSessionClose::Failed) || failed_diagnostics => {
                    Some(
                        "original Headless session physically closed with failure diagnostics"
                            .into(),
                    )
                }
                Ok(()) => None,
            };
            output::emit(evidence.result(options.scenario, message.as_deref(), save))
        }
        Err(panic) => {
            let detail = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("non-string panic payload");
            output::emit(evidence.result(
                options.scenario,
                Some(&format!("Player panicked: {}", output::bounded(detail))),
                save,
            ));
            resume_unwind(panic)
        }
    }
}

fn drive(
    session: &mut HeadlessSession,
    evidence: &mut Evidence,
    scenario: Scenario,
    deadline: Instant,
) -> Result<(), String> {
    for _ in 0..200_000 {
        if Instant::now() >= deadline {
            return Err("Player exceeded its 60-second execution deadline".into());
        }
        let exit = session
            .frame(Duration::from_millis(20))
            .map_err(|error| error.to_string())?;
        evidence.observe(session)?;
        if Instant::now() >= deadline {
            return Err("Player exceeded its 60-second execution deadline".into());
        }
        let snapshot = evidence
            .snapshot
            .as_ref()
            .ok_or("gameplay snapshot is missing")?;
        if let Some(error) = &snapshot.error {
            if snapshot.travel_count == 0 && evidence.current != evidence.startup {
                return Err(format!(
                    "failed travel did not preserve the actual startup Scene instance: {}",
                    output::bounded(error)
                ));
            }
            return Err(format!("gameplay failed: {}", output::bounded(error)));
        }
        if exit.is_some() {
            return Err("Player exited before its finite scenario completed".into());
        }
        let scripted = scenario != Scenario::Interactive;
        if snapshot.initialized && (!scripted || snapshot.verified) {
            if snapshot.phase != GamePhase::Running || snapshot.restoring {
                return Err("completed Player scenario is not a running reconstructed game".into());
            }
            if scripted {
                let level = if scenario == Scenario::Save {
                    LevelId::Cavern
                } else {
                    LevelId::Meadow
                };
                let marker = if scenario == Scenario::Save { 2 } else { 1 };
                if evidence.visited.len() != LevelId::ALL.len()
                    || snapshot.level != Some(LevelKey::main(level))
                    || snapshot.score != 30
                    || snapshot.collected != 3
                    || snapshot
                        .markers
                        .iter()
                        .map(|id| id.get())
                        .collect::<Vec<_>>()
                        != [marker]
                    || (scenario == Scenario::Save && snapshot.saves != 1)
                    || (scenario == Scenario::Restore && snapshot.restores != 1)
                {
                    return Err("completed Player scenario disagrees with actual two-level progress or Image residency".into());
                }
            }
            return Ok(());
        }
        std::thread::yield_now();
    }
    Err("Player exhausted its finite 200000-frame budget".into())
}

fn verify_save(options: &RunOptions) -> Result<Option<Value>, String> {
    if options.scenario != Scenario::Save {
        return Ok(None);
    }
    let path = options
        .save_path
        .as_deref()
        .ok_or("Save requires an explicit path")?;
    let mut slot = SaveSlot::open(path).map_err(|error| format!("save readback: {error}"))?;
    let decoded = slot
        .load()
        .map_err(|error| format!("save decode: {error}"))?;
    let saved = &decoded.checkpoint;
    if decoded.migrated_from.is_some()
        || saved.score != 30
        || saved.collected.len() != 3
        || saved
            .markers
            .iter()
            .map(|marker| marker.id.get())
            .collect::<Vec<_>>()
            != [1, 2, 3]
        || saved.next_marker_id.get() != 4
        || saved.location.level != LevelKey::main(LevelId::Cavern)
    {
        return Err("published checkpoint disagrees with the actual Save trajectory".into());
    }
    Ok(Some(json!({
        "format_version": CURRENT_FORMAT_VERSION,
        "migrated_from": decoded.migrated_from,
        "load_verified": true,
        "level": saved.location.level,
        "score": saved.score,
        "collected": saved.collected.len(),
        "markers": saved.markers,
        "next_marker_id": saved.next_marker_id.get(),
    })))
}

fn close_original(session: &mut HeadlessSession) -> HeadlessSessionClose {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match catch_unwind(AssertUnwindSafe(|| session.close())) {
            Ok(HeadlessSessionClose::Complete) => return HeadlessSessionClose::Complete,
            Ok(HeadlessSessionClose::Failed) => return HeadlessSessionClose::Failed,
            Ok(HeadlessSessionClose::Pending) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(HeadlessSessionClose::Pending) => {
                let _ = output::diagnostic_to(
                    &mut std::io::stderr().lock(),
                    &format!(
                        "original Headless session cleanup exceeded 10 seconds; retaining the original owner until process exit: {:?}",
                        session.diagnostics()
                    ),
                );
                std::process::exit(1);
            }
            Err(_) => {
                let _ = output::diagnostic_to(
                    &mut std::io::stderr().lock(),
                    "original Headless session cleanup panicked; retaining the original owner until process exit",
                );
                std::process::exit(1);
            }
        }
    }
}
