use super::*;
use crate::levels::LevelId;

fn cue(sequence: u64, phase: AudioPhase) -> AudioRecord {
    AudioRecord::Cue {
        runtime: 1,
        instance: 1,
        sequence,
        clip: AudioClip::Travel,
        cause: AudioCause::Travel {
            level: LevelKey::main(LevelId::Cavern),
        },
        frame: sequence,
        phase,
    }
}

#[test]
fn scope_observation_exhaustion_preserves_every_reserved_cue_terminal() {
    let mut journal = AudioJournal::default();
    for sequence in 1..MAX_CAUSES as u64 {
        for phase in [
            AudioPhase::Accepted,
            AudioPhase::Started,
            AudioPhase::Completed,
        ] {
            journal.push(cue(sequence, phase)).unwrap();
        }
    }
    for instance in 1..=MAX_SCOPE_RECORDS as u64 {
        journal
            .push(AudioRecord::ScopeApplied {
                runtime: 1,
                instance,
                frame: instance,
            })
            .unwrap();
    }
    assert!(
        journal
            .push(AudioRecord::ScopeApplied {
                runtime: 1,
                instance: 4097,
                frame: 4097
            })
            .is_err()
    );
    for phase in [
        AudioPhase::Accepted,
        AudioPhase::Started,
        AudioPhase::Completed,
    ] {
        journal.push(cue(MAX_CAUSES as u64, phase)).unwrap();
    }
    assert_eq!(journal.cues, MAX_CUE_RECORDS);
    assert_eq!(journal.scopes, MAX_SCOPE_RECORDS);
    assert_eq!(journal.records.len(), MAX_CUE_RECORDS + MAX_SCOPE_RECORDS);
    assert!(matches!(
        journal.records.last(),
        Some(AudioRecord::Cue {
            sequence: 4096,
            phase: AudioPhase::Completed,
            ..
        })
    ));
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
