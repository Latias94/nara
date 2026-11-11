use super::*;
use crate::graph::{Link, Node, NodeId, NodeKind};

fn basis() -> GraphPreviewBasis {
    GraphPreviewBasis {
        document: EditorDocumentId::from_raw(1),
        revision: DocumentRevision::initial(),
    }
}

fn output(value: f64) -> GraphView {
    GraphView {
        nodes: vec![
            Node {
                id: NodeId(1),
                kind: NodeKind::Scalar(value),
                position: [0.0; 2],
            },
            Node {
                id: NodeId(2),
                kind: NodeKind::Output,
                position: [1.0; 2],
            },
        ],
        links: vec![Link {
            from: NodeId(1),
            to: NodeId(2),
            input: 0,
        }],
    }
}

#[test]
fn unbound_requests_leave_the_default_owner_and_counter_unchanged() {
    for mut owner in [GraphPreviewOwner::new(), GraphPreviewOwner::default()] {
        let before = owner.state();
        assert_eq!(before, GraphPreviewState::default());
        assert_eq!(
            owner.request(output(7.0)).unwrap_err(),
            GraphPreviewRequestError::NoCurrentSource
        );
        assert_eq!(owner.state(), before);
        assert_eq!(owner.last_request, 0);
        owner.observe(Some(basis()));
        assert_eq!(owner.request(output(7.0)).unwrap().id().0, 1);
    }
}

#[test]
fn work_captures_an_immutable_view_and_executes_without_mutating_owner_state() {
    let mut owner = GraphPreviewOwner::new();
    let basis = basis();
    owner.observe(Some(basis));
    let mut source = output(7.0);
    let work = owner.request(source.clone()).unwrap();
    let id = work.id();
    assert_eq!(work.basis(), basis);
    assert_eq!(owner.state().pending, Some(id));
    source.nodes[0].kind = NodeKind::Scalar(9.0);
    let before = owner.state();
    let completion = std::thread::spawn(move || work.execute()).join().unwrap();
    assert_eq!(owner.state(), before);
    assert_eq!(
        owner.complete(completion),
        GraphPreviewDisposition::Accepted
    );
    assert_eq!(
        owner.state(),
        GraphPreviewState {
            current: Some(basis),
            pending: None,
            last_good: Some(GraphPreviewValue {
                request: id,
                basis,
                value: Some(7.0),
            }),
            diagnostic: None,
        }
    );
    assert_eq!(evaluate(&source).unwrap(), Some(9.0));
    let mut snapshot = owner.state().clone();
    snapshot.current = None;
    snapshot.last_good.as_mut().unwrap().value = Some(-1.0);
    snapshot.diagnostic = Some("Observation changed by its caller".to_owned());
    assert_ne!(owner.state(), snapshot);
    assert_eq!(owner.state().last_good.unwrap().value, Some(7.0));
}

#[test]
fn only_the_latest_same_basis_request_can_publish_in_either_completion_order() {
    for older_first in [true, false] {
        let mut owner = GraphPreviewOwner::new();
        let basis = basis();
        owner.observe(Some(basis));
        let older = owner.request(output(3.0)).unwrap();
        let latest = owner.request(output(7.0)).unwrap();
        let latest_id = latest.id();
        assert!(older.id().0 < latest_id.0);
        assert_eq!(older.basis(), latest.basis());
        if older_first {
            let before = owner.state();
            assert_eq!(
                owner.complete(older.execute()),
                GraphPreviewDisposition::Stale
            );
            assert_eq!(owner.state(), before);
            assert_eq!(
                owner.complete(latest.execute()),
                GraphPreviewDisposition::Accepted
            );
        } else {
            assert_eq!(
                owner.complete(latest.execute()),
                GraphPreviewDisposition::Accepted
            );
            let before = owner.state();
            assert_eq!(
                owner.complete(older.execute()),
                GraphPreviewDisposition::Stale
            );
            assert_eq!(owner.state(), before);
        }
        assert_eq!(owner.state().pending, None);
        assert_eq!(
            owner.state().last_good,
            Some(GraphPreviewValue {
                request: latest_id,
                basis,
                value: Some(7.0),
            })
        );
    }
}

#[test]
fn observing_the_same_basis_preserves_pending_admission_and_current_diagnostic() {
    let mut owner = GraphPreviewOwner::new();
    let basis = basis();
    owner.observe(Some(basis));
    let work = owner.request(output(f64::NAN)).unwrap();
    let pending = owner.state();
    owner.observe(Some(basis));
    assert_eq!(owner.state(), pending);
    assert_eq!(
        owner.complete(work.execute()),
        GraphPreviewDisposition::Failed
    );
    let failed = owner.state();
    assert!(failed.diagnostic.is_some());
    owner.observe(Some(basis));
    assert_eq!(owner.state(), failed);
}

#[test]
fn newer_edits_retire_pending_work_and_keep_last_good_labeled_with_its_old_basis() {
    let mut owner = GraphPreviewOwner::new();
    let old_basis = basis();
    owner.observe(Some(old_basis));
    let good = owner.request(output(7.0)).unwrap();
    assert_eq!(
        owner.complete(good.execute()),
        GraphPreviewDisposition::Accepted
    );
    let last_good = owner.state().last_good;
    let old = owner.request(output(8.0)).unwrap();
    let new_basis = GraphPreviewBasis {
        revision: old_basis.revision.next(),
        ..old_basis
    };
    owner.observe(Some(new_basis));
    assert_eq!(owner.state().pending, None);
    assert_eq!(owner.state().last_good, last_good);
    let before = owner.state();
    assert_eq!(
        owner.complete(old.execute()),
        GraphPreviewDisposition::Stale
    );
    assert_eq!(owner.state(), before);
    let current = owner.request(output(9.0)).unwrap();
    assert_eq!(
        owner.complete(current.execute()),
        GraphPreviewDisposition::Accepted
    );
    assert_eq!(owner.state().last_good.unwrap().basis, new_basis);
}

#[test]
fn switching_document_slots_at_the_same_revision_retires_prior_admission() {
    let mut owner = GraphPreviewOwner::new();
    let old_basis = basis();
    owner.observe(Some(old_basis));
    let old = owner.request(output(7.0)).unwrap();
    let new_basis = GraphPreviewBasis {
        document: EditorDocumentId::from_raw(2),
        ..old_basis
    };
    owner.observe(Some(new_basis));
    assert_eq!(owner.state().pending, None);
    let current = owner.request(output(9.0)).unwrap();
    let before = owner.state();
    assert_eq!(
        owner.complete(old.execute()),
        GraphPreviewDisposition::Stale
    );
    assert_eq!(owner.state(), before);
    assert_eq!(
        owner.complete(current.execute()),
        GraphPreviewDisposition::Accepted
    );
    assert_eq!(owner.state().last_good.unwrap().basis, new_basis);
}

#[test]
fn reopen_at_the_same_generation_rejects_the_previous_source_incarnation() {
    let mut owner = GraphPreviewOwner::new();
    let old_basis = basis();
    owner.observe(Some(old_basis));
    let old = owner.request(output(7.0)).unwrap();
    let reopened = GraphPreviewBasis {
        revision: DocumentRevision::initial(),
        ..old_basis
    };
    assert_eq!(
        old_basis.revision.generation(),
        reopened.revision.generation()
    );
    assert_ne!(
        old_basis.revision.source_id(),
        reopened.revision.source_id()
    );
    owner.observe(Some(reopened));
    let current = owner.request(output(9.0)).unwrap();
    assert!(current.id().0 > old.id().0);
    let before = owner.state();
    assert_eq!(
        owner.complete(old.execute()),
        GraphPreviewDisposition::Stale
    );
    assert_eq!(owner.state(), before);
    assert_eq!(
        owner.complete(current.execute()),
        GraphPreviewDisposition::Accepted
    );
    assert_eq!(owner.state().last_good.unwrap().basis, reopened);
}

#[test]
fn close_retains_last_good_and_refuses_late_success_and_failure_without_mutation() {
    for value in [9.0, f64::NAN] {
        let mut owner = GraphPreviewOwner::new();
        let basis = basis();
        owner.observe(Some(basis));
        let good = owner.request(output(7.0)).unwrap();
        assert_eq!(
            owner.complete(good.execute()),
            GraphPreviewDisposition::Accepted
        );
        let last_good = owner.state().last_good;
        let old = owner.request(output(value)).unwrap();
        let counter = owner.last_request;
        owner.observe(None);
        assert_eq!(
            owner.state(),
            GraphPreviewState {
                last_good,
                ..GraphPreviewState::default()
            }
        );
        let before = owner.state();
        assert_eq!(
            owner.complete(old.execute()),
            GraphPreviewDisposition::Stale
        );
        assert_eq!(owner.state(), before);
        assert_eq!(
            owner.request(output(10.0)).unwrap_err(),
            GraphPreviewRequestError::NoCurrentSource
        );
        assert_eq!(owner.state(), before);
        assert_eq!(owner.last_request, counter);
    }
}

#[test]
fn closing_and_reobserving_the_same_basis_never_revives_a_retired_request() {
    let mut owner = GraphPreviewOwner::new();
    let basis = basis();
    owner.observe(Some(basis));
    let old = owner.request(output(7.0)).unwrap();
    owner.observe(None);
    owner.observe(Some(basis));
    assert_eq!(owner.state().pending, None);
    let current = owner.request(output(9.0)).unwrap();
    assert!(current.id().0 > old.id().0);
    let before = owner.state();
    assert_eq!(
        owner.complete(old.execute()),
        GraphPreviewDisposition::Stale
    );
    assert_eq!(owner.state(), before);
    assert_eq!(
        owner.complete(current.execute()),
        GraphPreviewDisposition::Accepted
    );
}

#[test]
fn a_stale_failure_cannot_clear_pending_or_clobber_a_new_success() {
    for older_first in [true, false] {
        let mut owner = GraphPreviewOwner::new();
        owner.observe(Some(basis()));
        let old = owner.request(output(f64::NAN)).unwrap();
        let current = owner.request(output(7.0)).unwrap();
        if older_first {
            let before = owner.state();
            assert_eq!(
                owner.complete(old.execute()),
                GraphPreviewDisposition::Stale
            );
            assert_eq!(owner.state(), before);
            assert_eq!(
                owner.complete(current.execute()),
                GraphPreviewDisposition::Accepted
            );
        } else {
            assert_eq!(
                owner.complete(current.execute()),
                GraphPreviewDisposition::Accepted
            );
            let before = owner.state();
            assert_eq!(
                owner.complete(old.execute()),
                GraphPreviewDisposition::Stale
            );
            assert_eq!(owner.state(), before);
        }
        assert_eq!(owner.state().diagnostic, None);
        assert_eq!(owner.state().last_good.unwrap().value, Some(7.0));
    }
}

#[test]
fn stale_success_and_failure_cannot_replace_the_current_diagnostic_or_last_good() {
    for value in [9.0, f64::NAN] {
        let mut owner = GraphPreviewOwner::new();
        owner.observe(Some(basis()));
        let good = owner.request(output(7.0)).unwrap();
        assert_eq!(
            owner.complete(good.execute()),
            GraphPreviewDisposition::Accepted
        );
        let last_good = owner.state().last_good;
        let old = owner.request(output(value)).unwrap();
        let mut invalid = output(10.0);
        invalid.links[0].from = NodeId(99);
        let diagnostic = evaluate(&invalid).unwrap_err().to_string();
        let current = owner.request(invalid).unwrap();
        assert_eq!(
            owner.complete(current.execute()),
            GraphPreviewDisposition::Failed
        );
        assert_eq!(owner.state().diagnostic, Some(diagnostic));
        assert_eq!(owner.state().last_good, last_good);
        let before = owner.state();
        assert_eq!(
            owner.complete(old.execute()),
            GraphPreviewDisposition::Stale
        );
        assert_eq!(owner.state(), before);
    }
}

#[test]
fn admitted_failure_preserves_last_good_until_a_successful_retry_replaces_it() {
    let mut owner = GraphPreviewOwner::new();
    let basis = basis();
    owner.observe(Some(basis));
    let good = owner.request(output(7.0)).unwrap();
    assert_eq!(
        owner.complete(good.execute()),
        GraphPreviewDisposition::Accepted
    );
    let last_good = owner.state().last_good;
    let invalid = output(f64::INFINITY);
    let diagnostic = evaluate(&invalid).unwrap_err().to_string();
    let failed = owner.request(invalid).unwrap();
    assert_eq!(
        owner.complete(failed.execute()),
        GraphPreviewDisposition::Failed
    );
    assert_eq!(
        owner.state(),
        GraphPreviewState {
            current: Some(basis),
            pending: None,
            last_good: last_good.clone(),
            diagnostic: Some(diagnostic),
        }
    );
    let retry = owner.request(output(9.0)).unwrap();
    let request = retry.id();
    assert_eq!(owner.state().diagnostic, None);
    assert_eq!(owner.state().last_good, last_good);
    assert_eq!(
        owner.complete(retry.execute()),
        GraphPreviewDisposition::Accepted
    );
    assert_eq!(
        owner.state().last_good,
        Some(GraphPreviewValue {
            request,
            basis,
            value: Some(9.0),
        })
    );
}

#[test]
fn changed_sources_and_close_clear_diagnostics_without_relabeling_last_good() {
    let mut owner = GraphPreviewOwner::new();
    let original = basis();
    owner.observe(Some(original));
    let good = owner.request(output(7.0)).unwrap();
    assert_eq!(
        owner.complete(good.execute()),
        GraphPreviewDisposition::Accepted
    );
    let last_good = owner.state().last_good;
    let edited = GraphPreviewBasis {
        revision: original.revision.next(),
        ..original
    };
    for next in [Some(edited), None] {
        let failed = owner.request(output(f64::NAN)).unwrap();
        assert_eq!(
            owner.complete(failed.execute()),
            GraphPreviewDisposition::Failed
        );
        assert!(owner.state().diagnostic.is_some());
        owner.observe(next);
        assert_eq!(owner.state().current, next);
        assert_eq!(owner.state().diagnostic, None);
        assert_eq!(owner.state().last_good, last_good);
    }
}

#[test]
fn request_id_exhaustion_is_atomic_with_pending_work_and_with_an_admitted_failure() {
    let mut owner = GraphPreviewOwner::new();
    owner.observe(Some(basis()));
    let good = owner.request(output(7.0)).unwrap();
    assert_eq!(
        owner.complete(good.execute()),
        GraphPreviewDisposition::Accepted
    );
    owner.last_request = u64::MAX - 1;
    let last = owner.request(output(f64::NAN)).unwrap();
    assert_eq!(last.id().0, u64::MAX);
    let pending = owner.state();
    for _ in 0..2 {
        assert_eq!(
            owner.request(output(9.0)).unwrap_err(),
            GraphPreviewRequestError::RequestIdExhausted
        );
        assert_eq!(owner.state(), pending);
        assert_eq!(owner.last_request, u64::MAX);
    }
    assert_eq!(
        owner.complete(last.execute()),
        GraphPreviewDisposition::Failed
    );
    let failed = owner.state();
    assert!(failed.diagnostic.is_some());
    assert_eq!(
        owner.request(output(9.0)).unwrap_err(),
        GraphPreviewRequestError::RequestIdExhausted
    );
    assert_eq!(owner.state(), failed);
    assert_eq!(owner.last_request, u64::MAX);
    owner.observe(None);
    owner.observe(Some(basis()));
    let reopened = owner.state();
    assert_eq!(
        owner.request(output(9.0)).unwrap_err(),
        GraphPreviewRequestError::RequestIdExhausted
    );
    assert_eq!(owner.state(), reopened);
    assert_eq!(owner.last_request, u64::MAX);
}

#[test]
fn incomplete_graphs_are_accepted_values_that_replace_previous_output_and_failure() {
    let empty = GraphView {
        nodes: Vec::new(),
        links: Vec::new(),
    };
    let mut disconnected = output(7.0);
    disconnected.links.clear();
    let mut partial = output(7.0);
    partial.nodes.push(Node {
        id: NodeId(3),
        kind: NodeKind::Add,
        position: [2.0; 2],
    });
    partial.links[0].from = NodeId(3);
    partial.links.push(Link {
        from: NodeId(1),
        to: NodeId(3),
        input: 0,
    });
    for incomplete in [empty, disconnected, partial] {
        let mut owner = GraphPreviewOwner::new();
        let basis = basis();
        owner.observe(Some(basis));
        let good = owner.request(output(7.0)).unwrap();
        assert_eq!(
            owner.complete(good.execute()),
            GraphPreviewDisposition::Accepted
        );
        let failed = owner.request(output(f64::NAN)).unwrap();
        assert_eq!(
            owner.complete(failed.execute()),
            GraphPreviewDisposition::Failed
        );
        let work = owner.request(incomplete).unwrap();
        let request = work.id();
        assert_eq!(
            owner.complete(work.execute()),
            GraphPreviewDisposition::Accepted
        );
        assert_eq!(
            owner.state(),
            GraphPreviewState {
                current: Some(basis),
                pending: None,
                last_good: Some(GraphPreviewValue {
                    request,
                    basis,
                    value: None,
                }),
                diagnostic: None,
            }
        );
    }
}

#[test]
fn foreign_owners_cannot_publish_completions_with_equal_local_ids_and_bases() {
    let mut owner = GraphPreviewOwner::new();
    let mut foreign = GraphPreviewOwner::new();
    let basis = basis();
    owner.observe(Some(basis));
    foreign.observe(Some(basis));
    let current = owner.request(output(7.0)).unwrap();
    let other = foreign.request(output(9.0)).unwrap();
    assert_eq!(current.id(), other.id());
    assert_eq!(current.basis(), other.basis());
    let before = owner.state();
    assert_eq!(
        owner.complete(other.execute()),
        GraphPreviewDisposition::Stale
    );
    assert_eq!(owner.state(), before);
    assert_eq!(
        owner.complete(current.execute()),
        GraphPreviewDisposition::Accepted
    );
    assert_eq!(owner.state().last_good.unwrap().value, Some(7.0));
}

#[test]
fn exact_request_and_basis_are_checked_independently_of_private_owner_provenance() {
    for mismatch in 0..3 {
        let mut owner = GraphPreviewOwner::new();
        let basis = basis();
        owner.observe(Some(basis));
        let mut completion = owner.request(output(7.0)).unwrap().execute();
        match mismatch {
            0 => completion.id = GraphPreviewRequestId(completion.id.0 + 1),
            1 => completion.basis.document = EditorDocumentId::from_raw(2),
            2 => completion.basis.revision = basis.revision.next(),
            _ => unreachable!(),
        }
        let before = owner.state();
        assert_eq!(owner.complete(completion), GraphPreviewDisposition::Stale);
        assert_eq!(owner.state(), before);
    }
}
