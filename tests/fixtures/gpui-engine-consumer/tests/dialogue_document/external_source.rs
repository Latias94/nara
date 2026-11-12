use jellyflow_core::{CanvasPoint, CanvasRect, CanvasSize, NodeId as CoreNodeId};
use jellyflow_runtime::{
    commands::GraphCommand,
    geometry::{ViewPoint, ViewRect, ViewSize},
    graph::{ExternalGraphDocument, GraphRuntimePolicy, GraphSnapshot, RuntimePatchKind},
    view::{
        AdapterFrame, FeedbackSubmissionError, FrameRequest, LayoutNodeStatus, RendererStamp,
        ViewCloseReason, ViewCloseRequest, ViewSession, ViewSessionError, ViewSessionOptions,
    },
    viewport::ViewportTransform,
};
use nara_gpui_engine_consumer::dialogue::jellyflow::DialogueFacts;

use super::*;

fn capture(editor: &EditorProjectSession, document: EditorDocumentId) -> Result<DialogueFacts> {
    Ok(DialogueFacts::capture(
        document,
        facts(editor, document).revision,
        view(editor, document),
    )?)
}

fn core_node(projection: &DialogueFacts, node: NodeId) -> CoreNodeId {
    projection
        .graph()
        .nodes()
        .keys()
        .copied()
        .find(|candidate| {
            projection.domain_node(projection.graph().graph_id(), *candidate) == Some(node)
        })
        .unwrap()
}

fn assert_projection(projection: &DialogueFacts, snapshot: &GraphSnapshot) {
    assert!(std::ptr::eq(projection.graph(), snapshot.graph()));
    assert_eq!(snapshot.graph().graph_id(), projection.graph().graph_id());
    assert_eq!(
        snapshot.graph().nodes().len(),
        projection.source().nodes().count()
    );
    assert_eq!(
        snapshot.graph().edges().len(),
        projection.source().links().count()
    );
    for (node, record) in snapshot.graph().nodes() {
        let original = projection
            .source()
            .node(
                projection
                    .domain_node(snapshot.graph().graph_id(), *node)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            record.data,
            serde_json::to_value(original.body.as_ref()).unwrap()
        );
        assert_eq!(record.pos.x, original.position[0]);
        assert_eq!(record.pos.y, original.position[1]);
        assert!(record.size.is_none());
        assert!(snapshot.node_lifetime_key(*node).is_some());
    }
    for port in snapshot.graph().ports().keys() {
        assert!(snapshot.port_instance_key(*port).is_some());
    }
    for edge in snapshot.graph().edges().keys() {
        assert!(snapshot.edge_instance_key(*edge).is_some());
    }
}

fn materialize(session: &ViewSession) -> Result<AdapterFrame> {
    Ok(session.frame(FrameRequest::new(
        CanvasRect {
            origin: CanvasPoint {
                x: -100.0,
                y: -100.0,
            },
            size: CanvasSize {
                width: 2_000.0,
                height: 1_000.0,
            },
        },
        RendererStamp::new("nara.dialogue.headless", 1),
    ))?)
}

fn collect_layout(frame: &AdapterFrame, height: f32) -> Result<()> {
    let collector = frame.collector();
    for node in frame.nodes() {
        let mut layout = node.begin_layout(ViewRect::new(
            ViewPoint::new(node.bounds().origin.x, node.bounds().origin.y),
            ViewSize::new(180.0, height),
        ));
        for port in node.ports() {
            layout.port_hidden(port.layout_target())?;
        }
        collector.layout(layout.finish()?)?;
    }
    Ok(())
}

#[test]
fn original_receipts_publish_to_two_views_without_sharing_local_state() -> Result<()> {
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let (line, _, ending) = seed_conversation(editor, document);
    let initial = capture(editor, document)?;
    let external_facts = initial.external_facts()?;
    assert!(std::ptr::eq(
        initial.graph(),
        external_facts.graph().as_ref()
    ));
    assert!(Arc::ptr_eq(initial.source(), &view(editor, document)));
    let external = ExternalGraphDocument::new(
        Default::default(),
        GraphRuntimePolicy::default(),
        external_facts,
    )?;
    let first = external.attach_view(ViewSessionOptions::default())?;
    let second = external.attach_view(ViewSessionOptions::default())?;
    assert_eq!(external.live_view_count(), 2);
    let original = external.snapshot()?;
    assert_projection(&initial, &original);
    let line_node = core_node(&initial, line);
    let ending_node = core_node(&initial, ending);
    let line_key = original.node_lifetime_key(line_node).unwrap();
    let ending_key = original.node_lifetime_key(ending_node).unwrap();
    first.set_selection(vec![line_key], Vec::new(), Vec::new())?;
    second.set_selection(vec![ending_key], Vec::new(), Vec::new())?;
    let first_transform = ViewportTransform::new(CanvasPoint { x: 18.0, y: -9.0 }, 1.5).unwrap();
    let second_transform = ViewportTransform::new(CanvasPoint { x: -42.0, y: 27.0 }, 0.75).unwrap();
    first.set_viewport(first_transform)?;
    second.set_viewport(second_transform)?;
    for _ in 0..3 {
        assert!(external.snapshot()?.ptr_eq(&original));
        assert!(first.graph_snapshot()?.ptr_eq(&original));
        assert!(second.graph_snapshot()?.ptr_eq(&original));
        assert_eq!(first.node_lifetime_key(line_node)?, Some(line_key));
        assert_eq!(second.node_lifetime_key(ending_node)?, Some(ending_key));
    }
    let before = facts(editor, document);
    let outcomes = dispatch(
        editor,
        [action(
            document,
            before.revision,
            DialogueCommand::SetLine {
                node: line,
                speaker: "Guide 向导".to_owned(),
                text: "The original Session owns this edit.\n第二行".to_owned(),
            },
        )],
    );
    assert_eq!(outcomes.len(), 1);
    assert_applied(&outcomes[0], document, before.revision.next());
    assert!(external.snapshot()?.ptr_eq(&original));
    assert!(first.graph_snapshot()?.ptr_eq(&original));
    let changed = capture(editor, document)?;
    assert_eq!(changed.basis().revision, before.revision.next());
    assert!(Arc::ptr_eq(changed.source(), &view(editor, document)));
    let patch = external.publish(original.version(), changed.external_facts()?)?;
    assert_eq!(patch.kind(), RuntimePatchKind::Incremental);
    assert_eq!(patch.from_version(), original.version());
    assert_ne!(patch.to_version(), original.version());
    let published = external.snapshot()?;
    assert!(published.ptr_eq(patch.snapshot()));
    assert_projection(&changed, &published);
    assert!(first.graph_snapshot()?.ptr_eq(&published));
    assert!(second.graph_snapshot()?.ptr_eq(&published));
    assert_eq!(first.selection()?.nodes(), &[line_node]);
    assert_eq!(second.selection()?.nodes(), &[ending_node]);
    assert_eq!(first.viewport_transform()?, first_transform);
    assert_eq!(second.viewport_transform()?, second_transform);
    assert_eq!(published.node_lifetime_key(line_node), Some(line_key));
    assert_eq!(
        facts(editor, document).history.undo_depth,
        before.history.undo_depth + 1
    );

    let before_no_op = facts(editor, document);
    let source_before_no_op = view(editor, document);
    let stale_command = action(
        document,
        before_no_op.revision,
        DialogueCommand::SetEnd {
            node: ending,
            text: "This old basis must remain stale after View publication.".to_owned(),
        },
    );
    let NodeBody::Line { speaker, text } = source_before_no_op.node(line).unwrap().body.as_ref()
    else {
        panic!("expected the original line");
    };
    let outcomes = dispatch(
        editor,
        [action(
            document,
            before_no_op.revision,
            DialogueCommand::SetLine {
                node: line,
                speaker: speaker.clone(),
                text: text.clone(),
            },
        )],
    );
    assert_eq!(outcomes.len(), 1);
    assert_applied(&outcomes[0], document, before_no_op.revision.next());
    let after_no_op = facts(editor, document);
    assert_eq!(after_no_op.revision, before_no_op.revision.next());
    assert_eq!(after_no_op.history, before_no_op.history);
    assert_eq!(after_no_op.saved_revision, before_no_op.saved_revision);
    assert!(Arc::ptr_eq(&source_before_no_op, &view(editor, document)));
    assert!(external.snapshot()?.ptr_eq(&published));
    let no_op = capture(editor, document)?;
    assert_eq!(no_op.basis().revision, after_no_op.revision);
    assert!(Arc::ptr_eq(no_op.source(), changed.source()));
    let patch = external.publish(published.version(), no_op.external_facts()?)?;
    let after_publication = external.snapshot()?;
    assert_projection(&no_op, &after_publication);
    assert_eq!(after_publication.version(), patch.to_version());
    assert_ne!(after_publication.version(), published.version());
    for node in published.graph().nodes().keys() {
        assert_eq!(
            after_publication.node_lifetime_key(*node),
            published.node_lifetime_key(*node)
        );
    }
    for port in published.graph().ports().keys() {
        assert_eq!(
            after_publication.port_instance_key(*port),
            published.port_instance_key(*port)
        );
    }
    for edge in published.graph().edges().keys() {
        assert_eq!(
            after_publication.edge_instance_key(*edge),
            published.edge_instance_key(*edge)
        );
    }
    assert!(first.graph_snapshot()?.ptr_eq(&after_publication));
    assert!(second.graph_snapshot()?.ptr_eq(&after_publication));
    assert_eq!(
        dispatch(editor, [stale_command]),
        [EditorActionOutcome::Rejected(
            EditorActionRejection::StaleDocumentFact {
                document,
                expected: before_no_op.revision,
                actual: Some(after_no_op.revision),
            }
        )]
    );
    assert_eq!(facts(editor, document), after_no_op);
    assert_eq!(first.close()?.reason(), ViewCloseReason::HostClosed);
    assert_eq!(external.live_view_count(), 1);
    assert!(first.close_reason().is_some());
    assert!(second.close_reason().is_none());
    assert!(second.graph_snapshot()?.ptr_eq(&after_publication));
    assert_eq!(second.selection()?.nodes(), &[ending_node]);
    assert_eq!(facts(editor, document), after_no_op);
    second.close()?;
    assert_eq!(external.live_view_count(), 0);
    assert!(external.snapshot()?.ptr_eq(&after_publication));
    assert_eq!(facts(editor, document), after_no_op);
    Ok(())
}

#[test]
fn coalesced_remove_and_undo_retire_only_the_restored_node_ports_and_links() -> Result<()> {
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let (line, choice, _) = seed_conversation(editor, document);
    let initial = capture(editor, document)?;
    let external = ExternalGraphDocument::new(
        Default::default(),
        GraphRuntimePolicy::default(),
        initial.external_facts()?,
    )?;
    let first = external.attach_view(ViewSessionOptions::default())?;
    let second = external.attach_view(ViewSessionOptions::default())?;
    let old = external.snapshot()?;
    let line_node = core_node(&initial, line);
    let choice_node = core_node(&initial, choice);
    let old_line = old.node_lifetime_key(line_node).unwrap();
    first.set_selection(vec![old_line], Vec::new(), Vec::new())?;
    second.set_selection(
        vec![old.node_lifetime_key(choice_node).unwrap()],
        Vec::new(),
        Vec::new(),
    )?;
    let pending = materialize(&first)?;
    collect_layout(&pending, 160.0)?;
    let before = facts(editor, document);
    let before_content = encoded_content(initial.source());
    edit(editor, document, DialogueCommand::Remove(vec![line]));
    assert!(view(editor, document).node(line).is_none());
    assert!(external.snapshot()?.ptr_eq(&old));
    history(editor, document, false);
    let restored = capture(editor, document)?;
    assert_eq!(encoded_content(restored.source()), before_content);
    assert_eq!(
        facts(editor, document).history.undo_depth,
        before.history.undo_depth
    );
    assert_eq!(facts(editor, document).history.redo_depth, 1);
    assert_ne!(
        restored.source().node_incarnation(line),
        initial.source().node_incarnation(line)
    );
    let patch = external.publish(old.version(), restored.external_facts()?)?;
    let current = external.snapshot()?;
    assert_projection(&restored, &current);
    assert_eq!(patch.from_version(), old.version());
    assert_eq!(patch.to_version(), current.version());
    assert_eq!(current.document_epoch(), old.document_epoch());
    for node in old.graph().nodes().keys() {
        let key = old.node_lifetime_key(*node).unwrap();
        assert_eq!(current.node_lifetime_is_current(key), *node != line_node);
        assert_eq!(
            current.node_lifetime_key(*node) == Some(key),
            *node != line_node
        );
    }
    for (port, record) in old.graph().ports() {
        let key = old.port_instance_key(*port).unwrap();
        assert_eq!(
            current.port_instance_is_current(key),
            record.node != line_node
        );
        assert_eq!(
            current.port_instance_key(*port) == Some(key),
            record.node != line_node
        );
    }
    for edge in old.graph().edges().keys() {
        let key = old.edge_instance_key(*edge).unwrap();
        let removed = initial.edge_output(initial.graph().graph_id(), *edge)
            == Some(OutputPort::Next { node: line });
        assert_eq!(current.edge_instance_is_current(key), !removed);
        assert_eq!(current.edge_instance_key(*edge) == Some(key), !removed);
    }
    assert!(first.graph_snapshot()?.ptr_eq(&current));
    assert!(second.graph_snapshot()?.ptr_eq(&current));
    assert!(first.selection()?.nodes().is_empty());
    assert_eq!(second.selection()?.nodes(), &[choice_node]);
    let before_feedback = facts(editor, document);
    let stale_receipt = first.submit_feedback(pending.collector().finish()?)?;
    assert_eq!(stale_receipt.command_completions().count(), 0);
    assert_eq!(
        stale_receipt.layout_receipts().count(),
        pending.nodes().len()
    );
    assert_eq!(
        stale_receipt
            .layout_receipts()
            .find(|receipt| receipt.node() == line_node)
            .unwrap()
            .status(),
        Some(LayoutNodeStatus::Stale)
    );
    assert!(first.geometry_snapshot()?.node(line_node).is_none());
    assert!(second.geometry_snapshot()?.node(line_node).is_none());
    assert_eq!(facts(editor, document), before_feedback);
    let restored_line = current.node_lifetime_key(line_node).unwrap();
    assert_ne!(restored_line, old_line);
    let fresh = materialize(&first)?;
    collect_layout(&fresh, 164.0)?;
    let fresh_receipt = first.submit_feedback(fresh.collector().finish()?)?;
    assert!(
        fresh_receipt
            .layout_receipts()
            .all(|receipt| receipt.status() == Some(LayoutNodeStatus::Published))
    );
    let geometry = first.geometry_snapshot()?;
    let restored_geometry = geometry.node(line_node).unwrap();
    assert!(restored_geometry.is_fresh());
    assert_eq!(restored_geometry.lifetime(), restored_line);
    assert_eq!(restored_geometry.bounds().size.height, 164.0);
    assert!(second.geometry_snapshot()?.node(line_node).is_none());
    assert_eq!(facts(editor, document), before_feedback);
    first.set_selection(vec![restored_line], Vec::new(), Vec::new())?;
    assert_eq!(first.selection()?.nodes(), &[line_node]);
    assert_eq!(second.selection()?.nodes(), &[choice_node]);
    let before_close = facts(editor, document);
    first.close()?;
    assert_eq!(external.live_view_count(), 1);
    assert!(second.graph_snapshot()?.ptr_eq(&current));
    assert_eq!(facts(editor, document), before_close);
    second.close()?;
    Ok(())
}

#[test]
fn saved_equal_content_reopen_resets_every_key_and_preserves_the_original_owner() -> Result<()> {
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let (line, _, ending) = seed_conversation(editor, document);
    let saved_bytes = save(editor, &project, document)?;
    let saved = facts(editor, document);
    let initial = capture(editor, document)?;
    let external = ExternalGraphDocument::new(
        Default::default(),
        GraphRuntimePolicy::default(),
        initial.external_facts()?,
    )?;
    let first = external.attach_view(ViewSessionOptions::default())?;
    let second = external.attach_view(ViewSessionOptions::default())?;
    let old = external.snapshot()?;
    let line_node = core_node(&initial, line);
    let stale_command = action(
        document,
        saved.revision,
        DialogueCommand::SetEnd {
            node: ending,
            text: "This command belongs to the source before Reopen.".to_owned(),
        },
    );
    let result = persist(
        editor,
        EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: saved.revision,
            },
        },
    );
    let reopened = facts(editor, document);
    assert!(matches!(
        result,
        EditorPersistenceResult::Opened {
            document: actual_document,
            revision,
            ..
        } if actual_document == document && revision == reopened.revision
    ));
    assert_ne!(reopened.revision.source_id(), saved.revision.source_id());
    assert_eq!(reopened.revision.generation(), 0);
    assert_eq!(reopened.history, Default::default());
    assert!(!reopened.dirty);
    assert_eq!(fs::read(project.path().join(SOURCE))?, saved_bytes);
    acknowledge(editor);
    let replacement = capture(editor, document)?;
    assert_eq!(replacement.basis().document, initial.basis().document);
    assert_eq!(replacement.basis().revision, reopened.revision);
    assert!(!Arc::ptr_eq(replacement.source(), initial.source()));
    assert_eq!(
        encoded_content(replacement.source()),
        encoded_content(initial.source())
    );
    assert_eq!(replacement.graph().graph_id(), initial.graph().graph_id());
    assert_eq!(
        replacement.graph().nodes().keys().collect::<Vec<_>>(),
        initial.graph().nodes().keys().collect::<Vec<_>>()
    );
    assert_eq!(
        replacement.graph().ports().keys().collect::<Vec<_>>(),
        initial.graph().ports().keys().collect::<Vec<_>>()
    );
    assert_eq!(
        replacement.graph().edges().keys().collect::<Vec<_>>(),
        initial.graph().edges().keys().collect::<Vec<_>>()
    );
    assert!(external.snapshot()?.ptr_eq(&old));
    let patch = external.reset(old.version(), replacement.external_facts()?)?;
    assert_eq!(patch.kind(), RuntimePatchKind::DocumentReset);
    let current = external.snapshot()?;
    assert_projection(&replacement, &current);
    assert_eq!(current.provenance(), old.provenance());
    assert_eq!(current.document_epoch(), old.document_epoch() + 1);
    assert_eq!(current.version(), patch.to_version());
    for node in old.graph().nodes().keys() {
        let key = old.node_lifetime_key(*node).unwrap();
        assert!(!current.node_lifetime_is_current(key));
        assert_ne!(current.node_lifetime_key(*node), Some(key));
    }
    for port in old.graph().ports().keys() {
        let key = old.port_instance_key(*port).unwrap();
        assert!(!current.port_instance_is_current(key));
        assert_ne!(current.port_instance_key(*port), Some(key));
    }
    for edge in old.graph().edges().keys() {
        let key = old.edge_instance_key(*edge).unwrap();
        assert!(!current.edge_instance_is_current(key));
        assert_ne!(current.edge_instance_key(*edge), Some(key));
    }
    assert!(first.graph_snapshot()?.ptr_eq(&current));
    assert!(second.graph_snapshot()?.ptr_eq(&current));
    assert_eq!(
        dispatch(editor, [stale_command]),
        [EditorActionOutcome::Rejected(
            EditorActionRejection::StaleDocumentFact {
                document,
                expected: saved.revision,
                actual: Some(reopened.revision),
            }
        )]
    );
    assert_eq!(facts(editor, document), reopened);
    first.close()?;
    assert_eq!(external.live_view_count(), 1);
    second.set_selection(
        vec![current.node_lifetime_key(line_node).unwrap()],
        Vec::new(),
        Vec::new(),
    )?;
    let changed = edit(
        editor,
        document,
        DialogueCommand::SetEnd {
            node: ending,
            text: "The remaining View observes the same original document.".to_owned(),
        },
    );
    assert_eq!(changed.history.undo_depth, 1);
    assert!(changed.dirty);
    let successor = capture(editor, document)?;
    let patch = external.publish(current.version(), successor.external_facts()?)?;
    let remaining = second.graph_snapshot()?;
    assert_projection(&successor, &remaining);
    assert_eq!(remaining.version(), patch.to_version());
    assert_eq!(second.selection()?.nodes(), &[line_node]);
    second.close()?;
    assert_eq!(external.live_view_count(), 0);
    assert_eq!(facts(editor, document), changed);
    let later_bytes = save(editor, &project, document)?;
    assert_ne!(later_bytes, saved_bytes);
    assert_eq!(
        encoded_content(&DialogueGraph::decode(&later_bytes)?.view()),
        encoded_content(successor.source())
    );
    assert_eq!(facts(editor, document).history, changed.history);
    Ok(())
}

#[test]
fn original_frame_layout_survives_command_refusal_and_stays_local_to_each_view() -> Result<()> {
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let (line, _, _) = seed_conversation(editor, document);
    let projection = capture(editor, document)?;
    let external = ExternalGraphDocument::new(
        Default::default(),
        GraphRuntimePolicy::default(),
        projection.external_facts()?,
    )?;
    let first = external.attach_view(ViewSessionOptions::default())?;
    let second = external.attach_view(ViewSessionOptions::default())?;
    let original = external.snapshot()?;
    let original_document = facts(editor, document);
    let original_source = view(editor, document);
    let line_node = core_node(&projection, line);
    let frame = materialize(&first)?;
    assert_eq!(frame.nodes().len(), projection.graph().nodes().len());
    assert_eq!(frame.graph_version(), original.version());
    let refusal = first.execute(GraphCommand::set_node_data(
        original.version(),
        line_node,
        serde_json::json!({ "must_not_write": true }),
    ));
    assert!(matches!(
        refusal,
        Err(ViewSessionError::ExternalMutationRequired)
    ));
    assert!(first.close_reason().is_none());
    collect_layout(&frame, 140.0)?;
    let receipt = first.submit_feedback(frame.collector().finish()?)?;
    assert_eq!(receipt.command_completions().count(), 0);
    assert_eq!(receipt.layout_receipts().count(), frame.nodes().len());
    assert!(
        receipt
            .layout_receipts()
            .all(|layout| layout.status() == Some(LayoutNodeStatus::Published))
    );
    let first_geometry = first.geometry_snapshot()?;
    assert_eq!(
        first_geometry.node(line_node).unwrap().bounds().size.height,
        140.0
    );
    assert!(second.geometry_snapshot()?.node(line_node).is_none());
    let second_frame = materialize(&second)?;
    collect_layout(&second_frame, 232.0)?;
    second.submit_feedback(second_frame.collector().finish()?)?;
    assert_eq!(
        second
            .geometry_snapshot()?
            .node(line_node)
            .unwrap()
            .bounds()
            .size
            .height,
        232.0
    );
    assert_eq!(
        first
            .geometry_snapshot()?
            .node(line_node)
            .unwrap()
            .bounds()
            .size
            .height,
        140.0
    );
    let command_frame = materialize(&first)?;
    collect_layout(&command_frame, 333.0)?;
    let command = GraphCommand::set_node_data(
        original.version(),
        line_node,
        serde_json::json!({ "feedback_must_not_write": true }),
    );
    let command_identity = command.identity().clone();
    command_frame.collector().command(command)?;
    let failure = first
        .submit_feedback(command_frame.collector().finish()?)
        .unwrap_err();
    assert!(matches!(
        failure.error(),
        FeedbackSubmissionError::Session(ViewSessionError::ExternalMutationRequired)
    ));
    assert!(first.close_reason().is_none());
    let after_refusal = first.geometry_snapshot()?;
    assert_eq!(
        after_refusal.layout_revision(),
        first_geometry.layout_revision()
    );
    assert_eq!(
        after_refusal.node(line_node).unwrap().bounds().size.height,
        140.0
    );
    assert!(external.snapshot()?.ptr_eq(&original));
    assert_eq!(facts(editor, document), original_document);
    assert!(Arc::ptr_eq(&view(editor, document), &original_source));
    let (_, unsubmitted) = failure.into_parts();
    let (layouts, commands) = unsubmitted.unwrap().split_commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].identity(), &command_identity);
    let receipt = first.submit_feedback(layouts)?;
    assert!(
        receipt
            .layout_receipts()
            .all(|layout| layout.status() == Some(LayoutNodeStatus::Published))
    );
    assert_eq!(
        first
            .geometry_snapshot()?
            .node(line_node)
            .unwrap()
            .bounds()
            .size
            .height,
        333.0
    );
    assert!(external.snapshot()?.ptr_eq(&original));
    assert_eq!(facts(editor, document), original_document);
    let next_frame = materialize(&first)?;
    collect_layout(&next_frame, 155.0)?;
    let receipt = first.submit_feedback(next_frame.collector().finish()?)?;
    assert!(
        receipt
            .layout_receipts()
            .all(|layout| layout.status() == Some(LayoutNodeStatus::Published))
    );
    assert_eq!(
        first
            .geometry_snapshot()?
            .node(line_node)
            .unwrap()
            .bounds()
            .size
            .height,
        155.0
    );
    assert_eq!(
        second
            .geometry_snapshot()?
            .node(line_node)
            .unwrap()
            .bounds()
            .size
            .height,
        232.0
    );
    assert!(matches!(
        first.close_with_request(ViewCloseRequest::new().with_command(
            GraphCommand::set_node_data(
                original.version(),
                line_node,
                serde_json::json!({ "close_must_not_write": true }),
            )
        )),
        Err(ViewSessionError::ExternalMutationRequired)
    ));
    assert!(first.close_reason().is_none());
    assert_eq!(external.live_view_count(), 2);
    assert!(first.graph_snapshot()?.ptr_eq(&original));
    assert!(second.graph_snapshot()?.ptr_eq(&original));
    assert_eq!(facts(editor, document), original_document);
    assert!(Arc::ptr_eq(&view(editor, document), &original_source));
    assert!(
        external
            .snapshot()?
            .graph()
            .nodes()
            .values()
            .all(|node| node.size.is_none())
    );
    first.close()?;
    assert_eq!(external.live_view_count(), 1);
    assert!(second.graph_snapshot()?.ptr_eq(&original));
    second.close()?;
    assert_eq!(external.live_view_count(), 0);
    assert_eq!(facts(editor, document), original_document);
    Ok(())
}
