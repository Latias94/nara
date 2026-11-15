use serde_json::{Value, json};
use two_level_game::{
    GamePhase, RunOptions, Scenario,
    levels::{LevelId, LevelKey},
    save::{CURRENT_FORMAT_VERSION, io::SaveSlot},
};

use super::evidence::Evidence;

pub(super) fn completed(evidence: &Evidence, scenario: Scenario) -> Result<bool, String> {
    let Some(snapshot) = &evidence.snapshot else {
        return Ok(false);
    };
    if !snapshot.initialized || !snapshot.verified {
        return Ok(false);
    }
    if snapshot.phase != GamePhase::Running || snapshot.restoring {
        return Err("completed desktop scenario is not a running reconstructed game".into());
    }
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
        return Err("completed desktop scenario disagrees with actual two-level progress or Image residency".into());
    }
    Ok(true)
}

pub(super) fn save(options: &RunOptions) -> Result<Option<Value>, String> {
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
        return Err(
            "published checkpoint disagrees with the actual desktop Save trajectory".into(),
        );
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
