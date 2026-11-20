use super::*;
use crate::levels::LevelId;

#[test]
fn overflowing_scope_history_evicts_the_chronological_prefix_across_record_kinds() {
    let mut journal = AudioJournal::default();
    let scope = |instance| AudioRecord::ScopeApplied {
        runtime: 1,
        instance,
        frame: instance * 2,
    };
    journal.push(scope(1));
    for (phase, frame) in [
        (AudioPhase::Accepted, 2),
        (AudioPhase::Started, 2),
        (AudioPhase::Completed, 3),
    ] {
        journal.push(AudioRecord::Cue {
            runtime: 1,
            instance: 1,
            sequence: 1,
            clip: AudioClip::Travel,
            cause: AudioCause::Travel {
                level: LevelKey::main(LevelId::Cavern),
            },
            frame,
            phase,
        });
    }
    let last = MAX_SCOPE_RECORDS as u64 + 2;
    for instance in 2..=last {
        journal.push(scope(instance));
    }
    assert_eq!(journal.evicted_records, 5);
    assert_eq!(journal.cues, 0);
    assert_eq!(journal.scopes, MAX_SCOPE_RECORDS);
    assert!(journal.records.into_iter().eq((3..=last).map(scope)));
}

#[test]
fn development_reset_is_fingerprinted_without_changing_ordinary_audio_options() {
    let ordinary = recipe(RunOptions::default(), AudioOptions::default());
    let development = development_recipe(RunOptions::default(), AudioOptions::default());
    let observed = development_recipe(
        RunOptions::default(),
        AudioOptions {
            capture_output: true,
        },
    );
    let fingerprint = |recipe: &ProductRecipe| {
        recipe
            .entries()
            .find(|entry| entry.plugin_id() == AUDIO_PLUGIN_ID)
            .unwrap()
            .configuration_fingerprint()
    };
    assert_eq!(ordinary.len(), development.len());
    assert_ne!(fingerprint(&ordinary), fingerprint(&development));
    assert_ne!(fingerprint(&development), fingerprint(&observed));
    assert_eq!(
        fingerprint(&development),
        fingerprint(&development_recipe(
            RunOptions::default(),
            AudioOptions::default()
        ))
    );
    for entry in ordinary
        .entries()
        .filter(|entry| entry.plugin_id() != AUDIO_PLUGIN_ID)
    {
        assert_eq!(
            entry.configuration_fingerprint(),
            development
                .entries()
                .find(|other| other.plugin_id() == entry.plugin_id())
                .unwrap()
                .configuration_fingerprint()
        );
    }
    for capture_output in [false, true] {
        let mut canonical = Vec::new();
        AudioOptions { capture_output }.write_canonical(&mut canonical);
        let mut expected = b"two-level-cpal-f32-stereo-48000/v1".to_vec();
        expected.push(u8::from(capture_output));
        assert_eq!(canonical, expected);
    }
}
