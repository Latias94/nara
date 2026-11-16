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
