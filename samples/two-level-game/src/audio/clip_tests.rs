use super::*;

#[test]
fn direct_samples_are_bounded_nonempty_and_finite_for_both_clips() {
    for kind in [ClipKind::Pickup, ClipKind::Travel] {
        for samples in [
            vec![],
            vec![0.0; MAX_CLIP_FRAMES + 1],
            vec![0.0, f32::NAN],
            vec![0.0, f32::INFINITY],
            vec![0.0, f32::NEG_INFINITY],
            vec![0.0, 1.01],
            vec![0.0, -1.01],
        ] {
            let valid: Arc<[f32]> = Arc::from([0.0]);
            let supplied: Arc<[f32]> = samples.into();
            let result = match kind {
                ClipKind::Pickup => PreparedClips::new(supplied, valid),
                ClipKind::Travel => PreparedClips::new(valid, supplied),
            };
            match result.unwrap_err() {
                ClipError::Empty { clip }
                | ClipError::TooLong { clip, .. }
                | ClipError::InvalidSample { clip, .. } => assert_eq!(clip, kind),
            }
        }
    }
}

#[test]
fn prepared_clips_keep_the_original_immutable_arc_allocations() {
    let pickup: Arc<[f32]> = Arc::from([-1.0, 0.0, 1.0]);
    let travel: Arc<[f32]> = Arc::from([0.25, -0.25]);
    let clips = PreparedClips::new(pickup.clone(), travel.clone()).unwrap();
    assert!(Arc::ptr_eq(&clips.pickup, &pickup));
    assert!(Arc::ptr_eq(&clips.travel, &travel));
}
