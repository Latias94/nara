//! Review selection and outstanding publication evidence remain separate proof obligations.

use super::*;

fn captured_disk(
    stage: &mut PreparedEditorWorkspaceExtractionPersistence,
) -> Vec<CapturedExtractionSource> {
    // These controls use the fixture's original captured allocations as its disk observation.
    // Only Review selection and publication evidence change between attempts.
    let bytes = size_of::<Vec<CapturedExtractionSource>>()
        + stage
            .sources
            .prepared
            .corpus
            .original_sources()
            .iter()
            .map(|source| {
                size_of::<CapturedExtractionSource>()
                    + source.key().checked_retained_bytes().unwrap()
            })
            .sum::<usize>();
    stage.admit_persistence_storage(Some(bytes)).unwrap();
    stage
        .sources
        .prepared
        .corpus
        .original_sources()
        .iter()
        .map(|source| match source {
            CapturedExtractionSource::Scene {
                key,
                document,
                basis,
            } => CapturedExtractionSource::Scene {
                key: key.clone(),
                document: Arc::clone(document),
                basis: basis.clone(),
            },
            CapturedExtractionSource::Prefab { source } => CapturedExtractionSource::Prefab {
                source: source.clone(),
            },
        })
        .collect()
}

#[test]
fn revised_save_current_mask_rejects_old_proof_even_with_identical_source_allocations() {
    let basis = basis();
    let (workspace, _, documents) = fixture(&basis);
    let before = snapshot(&workspace);
    let mut original = stage(&workspace, &basis, documents);
    let disk = captured_disk(&mut original);
    let proof_a = original.prepare_saved_generation(&disk).unwrap();
    original.require_saved_generation(&proof_a).unwrap();
    assert_eq!(proof_a.published(), [true, true, false, true]);
    let source_table = original.sources.sources.as_ptr();
    let scene = Arc::clone(original.saved_scene(documents[0]).unwrap());
    let prefab = Arc::clone(original.saved_prefab(documents[3]).unwrap());
    let spent = original.usage();

    let sources = original.into_sources_before_publication().unwrap();
    assert_eq!(sources.sources.as_ptr(), source_table);
    assert_eq!(sources.usage(), spent);
    let mut revised = sources.prepare_persistence(vec![documents[2]]).unwrap();
    assert_eq!(revised.sources.sources.as_ptr(), source_table);
    assert!(Arc::ptr_eq(
        revised.saved_scene(documents[0]).unwrap(),
        &scene
    ));
    assert!(Arc::ptr_eq(
        revised.saved_prefab(documents[3]).unwrap(),
        &prefab
    ));
    assert!(revised.saved_scene(documents[1]).is_none());
    assert!(revised.saved_scene(documents[2]).is_some());

    // Nominal Work and Q still match, so the Tooling refusal must concern the revised mask.
    revised
        .sources
        .prepared
        .corpus
        .require_saved_generation(&proof_a)
        .unwrap();
    let failure = revised.require_saved_generation(&proof_a).unwrap_err();
    assert!(!failure.applied);
    assert!(failure.diagnostics.has_errors());
    let proof_b = revised.prepare_saved_generation(&disk).unwrap();
    assert_eq!(proof_b.published(), [true, false, true, true]);
    for (old, new) in proof_a.sources().iter().zip(proof_b.sources()) {
        match (old, new) {
            (
                CapturedExtractionSource::Scene { document: old, .. },
                CapturedExtractionSource::Scene { document: new, .. },
            ) => assert!(Arc::ptr_eq(old, new)),
            (
                CapturedExtractionSource::Prefab { source: old },
                CapturedExtractionSource::Prefab { source: new },
            ) => assert!(std::ptr::eq(old.document(), new.document())),
            _ => panic!("a revised Review must preserve every physical source kind"),
        }
    }
    assert!(std::ptr::eq(
        proof_a.extracted().document(),
        proof_b.extracted().document(),
    ));
    revised.require_saved_generation(&proof_b).unwrap();
    assert!(revised.usage().work_bytes > spent.work_bytes);
    assert!(revised.usage().work_nodes > spent.work_nodes);
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn outstanding_encoded_checkpoint_blocks_revision_and_returns_to_the_same_owner() {
    let basis = basis();
    let (workspace, _, documents) = fixture(&basis);
    let before = snapshot(&workspace);
    let mut prepared = stage(&workspace, &basis, documents);
    let disk = captured_disk(&mut prepared);
    let proof = prepared.prepare_saved_generation(&disk).unwrap();
    let encoded = prepared
        .encode_checkpoint(documents[1], SceneFileEncoding::Json)
        .unwrap();
    let capture = Arc::clone(&encoded.checkpoint().capture);
    let output = encoded.bytes().as_ptr();
    let digest = encoded.digest();
    let entry_table = prepared.entries.as_ptr();
    let source_table = prepared.sources.sources.as_ptr();
    let spent = prepared.usage();

    let mut retained = prepared.into_sources_before_publication().unwrap_err();
    assert_eq!(retained.entries.as_ptr(), entry_table);
    assert_eq!(retained.sources.sources.as_ptr(), source_table);
    assert_eq!(retained.usage(), spent);
    let save = retained.entries[retained.index(documents[1]).unwrap()]
        .save
        .as_ref()
        .unwrap();
    assert!(matches!(&save.evidence, SavedEvidence::Taken));
    assert!(Arc::ptr_eq(&save.capture, &capture));
    retained.require_saved_generation(&proof).unwrap();
    assert_eq!(encoded.bytes().as_ptr(), output);
    assert_eq!(encoded.digest(), digest);
    assert!(Arc::ptr_eq(&encoded.checkpoint().capture, &capture));

    retained
        .return_checkpoint(encoded.into_checkpoint())
        .unwrap();
    let save = retained.entries[retained.index(documents[1]).unwrap()]
        .save
        .as_ref()
        .unwrap();
    let SavedEvidence::Available(checkpoint) = &save.evidence else {
        panic!("the exact outstanding checkpoint must return to its original stage");
    };
    assert!(Arc::ptr_eq(&checkpoint.capture, &capture));
    let spent = retained.usage();
    let sources = retained.into_sources_before_publication().unwrap();
    assert_eq!(sources.sources.as_ptr(), source_table);
    assert_eq!(sources.usage(), spent);
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn published_commits_block_revision_and_survive_for_the_original_workspace_commit() {
    let basis = basis();
    let (mut workspace, authority, documents) = fixture(&basis);
    let before = snapshot(&workspace);
    let mut prepared = stage(&workspace, &basis, documents);
    let disk = captured_disk(&mut prepared);
    let proof = prepared.prepare_saved_generation(&disk).unwrap();
    publish_all(&mut prepared);
    let commits: Vec<_> = prepared
        .entries
        .iter()
        .filter_map(|entry| {
            entry.save.as_ref().map(|save| {
                let SavedEvidence::Published(commit) = &save.evidence else {
                    panic!("every selected output must have real publication evidence");
                };
                (
                    entry.document,
                    Arc::clone(&commit.checkpoint.capture),
                    commit.digest(),
                )
            })
        })
        .collect();
    assert_eq!(commits.len(), 3);
    let entry_table = prepared.entries.as_ptr();
    let source_table = prepared.sources.sources.as_ptr();
    let spent = prepared.usage();

    let mut retained = prepared.into_sources_before_publication().unwrap_err();
    assert_eq!(retained.entries.as_ptr(), entry_table);
    assert_eq!(retained.sources.sources.as_ptr(), source_table);
    assert_eq!(retained.usage(), spent);
    for (document, capture, digest) in commits {
        let save = retained.entries[retained.index(document).unwrap()]
            .save
            .as_ref()
            .unwrap();
        let SavedEvidence::Published(commit) = &save.evidence else {
            panic!("revision refusal must retain every accepted filesystem commit");
        };
        assert!(Arc::ptr_eq(&commit.checkpoint.capture, &capture));
        assert_eq!(commit.digest(), digest);
    }
    retained.require_saved_generation(&proof).unwrap();
    assert_eq!(snapshot(&workspace), before);
    authority
        .bind_prefab_extraction_persistence(&mut workspace, retained)
        .unwrap()
        .commit();
    let after = snapshot(&workspace);
    assert_eq!(after[2], before[2]);
    assert_eq!(after[1].source, before[1].source);
    assert_eq!(after[1].classification, before[1].classification);
    assert_eq!(after[1].tab.history, before[1].tab.history);
    assert_eq!(after[1].tab.revision, before[1].tab.revision);
    for index in [0, 1, 3] {
        assert!(!after[index].tab.dirty);
        assert!(after[index].digest.is_some());
    }
}
