use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, NodeId as CoreNodeId, NodeKindKey, PortCapacity,
    PortDirection, PortId, PortKey, PortKind, interaction::NodeGraphConnectionMode,
};
use jellyflow_runtime::{
    geometry::{HandlePosition, ViewPoint, ViewRect, ViewSize},
    graph::{
        EdgeEndpoint, ExternalGraphDocument, ExternalGraphError, GraphRuntimePolicy, GraphSnapshot,
    },
    interaction::{
        BoundInteractionSubmission, CanvasInteractionConfig, CanvasInteractionEffect,
        CanvasInteractionPhase, CanvasInteractionSession, CanvasInteractionUpdate,
        CanvasPressTarget, ConnectionInteractionStart, ExternalGraphCompletion,
        ExternalGraphEffect, GraphConnectionTarget, GraphMutationIntent,
        InteractionSubmissionSettlementEvidence, InteractionSubmissionSettlementResult,
        InteractionTerminalReason, NodeNudgeDirection, SelectionMode,
    },
    schema::{NodeCatalog, NodeKindSpec, NodePortSpec},
    view::{FrameRequest, RendererStamp, ViewSession, ViewSessionOptions},
};
use nara_gpui_engine_consumer::dialogue::jellyflow::{
    DialogueFacts,
    requests::{DialogueCanvasError, DialogueCanvasRequest},
};

use super::*;

fn capture(editor: &EditorProjectSession, document: EditorDocumentId) -> Result<DialogueFacts> {
    Ok(DialogueFacts::capture(
        document,
        facts(editor, document).revision,
        view(editor, document),
    )?)
}

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    for (kind, output) in [
        ("nara.dialogue.line", Some("next")),
        ("nara.dialogue.choice", Some("choice")),
        ("nara.dialogue.end", None),
    ] {
        let mut spec = NodeKindSpec::builder(kind).port(NodePortSpec::new(
            "input",
            PortDirection::In,
            PortKind::Exec,
            PortCapacity::Multi,
        ));
        if let Some(output) = output {
            spec = spec.port(NodePortSpec::new(
                output,
                PortDirection::Out,
                PortKind::Exec,
                PortCapacity::Single,
            ));
        }
        catalog.register(spec.build()).unwrap();
    }
    catalog
}

fn external(
    editor: &EditorProjectSession,
    document: EditorDocumentId,
) -> Result<(ExternalGraphDocument, ViewSession, ViewSession)> {
    let external = ExternalGraphDocument::new(
        catalog(),
        GraphRuntimePolicy::default(),
        capture(editor, document)?.external_facts()?,
    )?;
    let first = external.attach_view(ViewSessionOptions::default())?;
    let second = external.attach_view(ViewSessionOptions::default())?;
    Ok((external, first, second))
}

fn interaction(view: &ViewSession) -> CanvasInteractionSession {
    CanvasInteractionSession::attach(view, CanvasInteractionConfig::default()).unwrap()
}

fn core(node: NodeId) -> CoreNodeId {
    CoreNodeId::from_u128(u128::from(node.0))
}

fn input(snapshot: &GraphSnapshot, node: NodeId) -> PortId {
    snapshot
        .graph()
        .ports()
        .iter()
        .find_map(|(id, port)| {
            (port.node == core(node) && port.dir == PortDirection::In).then_some(*id)
        })
        .unwrap()
}

fn output(snapshot: &GraphSnapshot, node: NodeId) -> PortId {
    snapshot
        .graph()
        .ports()
        .iter()
        .find_map(|(id, port)| {
            (port.node == core(node) && port.dir == PortDirection::Out).then_some(*id)
        })
        .unwrap()
}

fn bind(
    interaction: &mut CanvasInteractionSession,
    update: CanvasInteractionUpdate,
) -> (BoundInteractionSubmission, DialogueCanvasRequest) {
    let mut effects = update.into_dispatch().effects.into_iter();
    let CanvasInteractionEffect::SubmitGraph { submission } = effects.next().unwrap() else {
        panic!("expected one original semantic submission");
    };
    assert!(effects.next().is_none());
    let mut bound = interaction.bind_external_submission(submission).unwrap();
    assert!(bound.take_command().is_none());
    let request = bound.take_external_request().unwrap();
    let identity = request.identity().clone();
    let pending = DialogueCanvasRequest::lower(request).unwrap();
    assert_eq!(pending.request().identity(), &identity);
    (bound, pending)
}

fn nudge(
    interaction: &mut CanvasInteractionSession,
    view: &ViewSession,
    node: NodeId,
) -> (BoundInteractionSubmission, DialogueCanvasRequest) {
    let snapshot = view.graph_snapshot().unwrap();
    view.set_selection(
        vec![snapshot.node_lifetime_key(core(node)).unwrap()],
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let update = interaction
        .nudge_selection(NodeNudgeDirection::Right, false)
        .unwrap();
    bind(interaction, update)
}

fn drag(
    interaction: &mut CanvasInteractionSession,
    view: &ViewSession,
    node: NodeId,
) -> (BoundInteractionSubmission, DialogueCanvasRequest) {
    let snapshot = view.graph_snapshot().unwrap();
    let start = view
        .node_drag_start(
            snapshot.node_lifetime_key(core(node)).unwrap(),
            SelectionMode::Replace,
        )
        .unwrap()
        .unwrap();
    let press = view
        .viewport_pointer_sample(ViewPoint::new(20.0, 20.0))
        .unwrap();
    let release = view
        .viewport_pointer_sample(ViewPoint::new(44.0, 32.0))
        .unwrap();
    interaction
        .press(CanvasPressTarget::NodeDrag(start), press)
        .unwrap();
    interaction.move_to(release).unwrap();
    let update = interaction.release(release).unwrap();
    bind(interaction, update)
}

fn connect(
    interaction: &mut CanvasInteractionSession,
    view: &ViewSession,
    source: PortId,
    target: PortId,
    proposal: EdgeId,
) -> (BoundInteractionSubmission, DialogueCanvasRequest) {
    let snapshot = view.graph_snapshot().unwrap();
    let source = view
        .keyboard_connection_port(snapshot.port_instance_key(source).unwrap())
        .unwrap();
    let target = view
        .keyboard_connection_port(snapshot.port_instance_key(target).unwrap())
        .unwrap();
    interaction
        .activate_keyboard_connection(source, proposal, NodeGraphConnectionMode::Strict)
        .unwrap();
    let update = interaction
        .activate_keyboard_connection_target(target)
        .unwrap();
    bind(interaction, update)
}

fn create_from_wire(
    interaction: &mut CanvasInteractionSession,
    view: &ViewSession,
    catalog: &NodeCatalog,
    source: PortId,
    kind: &str,
    target: &str,
) -> (
    BoundInteractionSubmission,
    DialogueCanvasRequest,
    CoreNodeId,
    PortId,
) {
    let snapshot = view.graph_snapshot().unwrap();
    let drag = view
        .port_connection_drag_intent(snapshot.port_instance_key(source).unwrap())
        .unwrap()
        .into_outcome()
        .ready()
        .unwrap();
    let start = view
        .connection_start_from_port_drag_intent(
            drag,
            EdgeId::from_u128(0xf00d),
            NodeGraphConnectionMode::Strict,
            CanvasPoint { x: 30.0, y: 40.0 },
            HandlePosition::Right,
            HandlePosition::Left,
            true,
        )
        .unwrap()
        .unwrap();
    create_from_start(interaction, view, catalog, start, kind, target)
}

fn create_from_start(
    interaction: &mut CanvasInteractionSession,
    view: &ViewSession,
    catalog: &NodeCatalog,
    start: ConnectionInteractionStart,
    kind: &str,
    target: &str,
) -> (
    BoundInteractionSubmission,
    DialogueCanvasRequest,
    CoreNodeId,
    PortId,
) {
    let position = view
        .viewport_pointer_sample(ViewPoint::new(320.0, 180.0))
        .unwrap();
    interaction
        .activate_connection_search(start, position)
        .unwrap();
    let search = interaction.search_request().unwrap();
    interaction.search_ready(&search, 1).unwrap();
    let creation = catalog
        .instantiate_node(
            &NodeKindKey::new(kind),
            CanvasRect {
                origin: CanvasPoint { x: 320.0, y: 180.0 },
                size: CanvasSize {
                    width: 180.0,
                    height: 90.0,
                },
            },
        )
        .unwrap();
    let proposed_node = creation.node_id();
    let proposed_port = creation.port_id(&PortKey::new(target)).unwrap();
    let selection = interaction
        .create_search_selection(&search, proposed_port, creation)
        .unwrap()
        .unwrap();
    let update = interaction.select_search_result(selection).unwrap();
    let (bound, pending) = bind(interaction, update);
    (bound, pending, proposed_node, proposed_port)
}

fn publish_geometry(view: &ViewSession) {
    let frame = view
        .frame(FrameRequest::new(
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
            RendererStamp::new("nara.dialogue.cpu", 1),
        ))
        .unwrap();
    let collector = frame.collector();
    for node in frame.nodes() {
        let position = node.node().unwrap().pos;
        let mut layout = node.begin_layout(ViewRect::new(
            ViewPoint::new(position.x, position.y),
            ViewSize::new(180.0, 40.0 + node.ports().len() as f32 * 24.0),
        ));
        for (index, port) in node.ports().iter().enumerate() {
            let (x, outward) = match port.port().unwrap().dir {
                PortDirection::In => (position.x, HandlePosition::Left),
                PortDirection::Out => (position.x + 180.0, HandlePosition::Right),
            };
            let y = position.y + 20.0 + index as f32 * 24.0;
            let bounds = ViewRect::new(ViewPoint::new(x - 6.0, y - 6.0), ViewSize::new(12.0, 12.0));
            layout
                .port_rendered(
                    port.layout_target(),
                    bounds,
                    bounds,
                    ViewPoint::new(x, y),
                    outward,
                )
                .unwrap();
        }
        collector.layout(layout.finish().unwrap()).unwrap();
    }
    view.submit_feedback(collector.finish().unwrap()).unwrap();
}

fn advance_probes(
    interaction: &mut CanvasInteractionSession,
    mut update: CanvasInteractionUpdate,
) -> CanvasInteractionUpdate {
    for _ in 0..16 {
        if !update.query_work_pending() {
            return update;
        }
        assert!(update.effects().is_empty());
        let (next, _, _, terminal_error) = interaction.advance_query_work(1).into_parts();
        assert!(terminal_error.is_none(), "{terminal_error:?}");
        update = next;
    }
    panic!("the finite fixture connection probe did not settle");
}

fn reconnect_from_route(
    interaction: &mut CanvasInteractionSession,
    view: &ViewSession,
    edge: EdgeId,
    endpoint: EdgeEndpoint,
    target: PortId,
) -> (BoundInteractionSubmission, DialogueCanvasRequest) {
    publish_geometry(view);
    let geometry = view.geometry_snapshot().unwrap();
    let route = geometry.route(edge).unwrap();
    let (fixed, moving) = match endpoint {
        EdgeEndpoint::From => (
            route.facts().endpoints.target,
            route.facts().endpoints.source,
        ),
        EdgeEndpoint::To => (
            route.facts().endpoints.source,
            route.facts().endpoints.target,
        ),
    };
    let start = view
        .connection_start_from_route(
            route,
            endpoint,
            fixed,
            NodeGraphConnectionMode::Strict,
            false,
        )
        .unwrap()
        .unwrap();
    let anchor = geometry.port(target).unwrap().route_anchor();
    let press = view
        .viewport_pointer_sample(ViewPoint::new(moving.point.x, moving.point.y))
        .unwrap();
    let release = view
        .viewport_pointer_sample(ViewPoint::new(anchor.x, anchor.y))
        .unwrap();
    interaction
        .press(CanvasPressTarget::Connection(start), press)
        .unwrap();
    let moved = interaction.move_to(release).unwrap();
    let moved = advance_probes(interaction, moved);
    assert!(moved.effects().is_empty());
    let released = interaction.release(release).unwrap();
    let update = advance_probes(interaction, released);
    bind(interaction, update)
}

fn execute(
    editor: &mut EditorProjectSession,
    pending: &DialogueCanvasRequest,
) -> EditorExtensionCommandOutcome {
    let command = pending.command().clone();
    let outcomes = dispatch(editor, [EditorAction::Extension(command.clone())]);
    assert_eq!(outcomes.len(), 1);
    assert_applied(
        &outcomes[0],
        command.document(),
        command.base_revision().next(),
    );
    let EditorActionOutcome::Workspace(report) = &outcomes[0] else {
        unreachable!()
    };
    let outcome = report.extension_outcome.clone().unwrap();
    assert_eq!(outcome.command(), &command);
    assert_eq!(outcome.revision(), command.base_revision().next());
    outcome
}

fn finish(
    interaction: &mut CanvasInteractionSession,
    bound: BoundInteractionSubmission,
    completion: ExternalGraphCompletion,
) {
    assert!(matches!(
        interaction.settle_submission(
            bound,
            InteractionSubmissionSettlementEvidence::External(completion),
        ),
        InteractionSubmissionSettlementResult::Settled(_)
    ));
}

fn publish_current(
    external: &ExternalGraphDocument,
    editor: &EditorProjectSession,
    document: EditorDocumentId,
) -> Result<()> {
    external.publish(
        external.snapshot()?.version(),
        capture(editor, document)?.external_facts()?,
    )?;
    Ok(())
}

#[test]
fn exact_move_results_repair_wrong_evidence_and_settle_late_without_projection_rollback()
-> Result<()> {
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let line = add(editor, document, NodeRecipe::Line, [24.0, 24.0]);
    let ending = add(editor, document, NodeRecipe::End, [480.0, 24.0]);
    let baseline = facts(editor, document);
    let (external, first, second) = external(editor, document)?;
    let mut first_interaction = interaction(&first);
    let mut second_interaction = interaction(&second);
    let (first_bound, first_request) = drag(&mut first_interaction, &first, line);
    let original_identity = first_request.request().identity().clone();
    let original_basis = first_request
        .request()
        .owner_basis::<DialogueFacts>()
        .unwrap();
    assert_eq!(original_basis.basis().revision, baseline.revision);
    assert!(Arc::ptr_eq(
        original_basis.source(),
        &view(editor, document)
    ));
    let GraphMutationIntent::MoveNodes(edits) = first_request.request().intent() else {
        panic!("pointer movement must resolve through typed geometry");
    };
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].node(), core(line));
    assert_eq!(edits[0].position(), Some(CanvasPoint { x: 48.0, y: 36.0 }));
    let first_result = execute(editor, &first_request);
    let first_publication = first_request.publish(&external, first_result.clone())?;
    assert_eq!(first_publication.outcome(), &first_result);
    assert!(Arc::ptr_eq(
        first_publication.facts().source(),
        first_result.get::<DialogueGraph>().unwrap().view(),
    ));
    let first_snapshot = first_publication.patch().snapshot().clone();
    let (second_bound, second_request) = nudge(&mut second_interaction, &second, ending);
    let second_identity = second_request.request().identity().clone();
    let second_result = execute(editor, &second_request);
    let before_wrong_result = external.snapshot()?;
    let wrong = first_request
        .publish(&external, second_result.clone())
        .unwrap_err();
    assert!(matches!(wrong.error(), DialogueCanvasError::ResultMismatch));
    let (returned_result, _) = wrong.into_parts();
    assert_eq!(returned_result, second_result);
    assert!(external.snapshot()?.ptr_eq(&before_wrong_result));
    let second_publication = second_request.publish(&external, returned_result)?;
    let latest = external.snapshot()?;
    assert!(latest.ptr_eq(second_publication.patch().snapshot()));
    assert_eq!(
        latest.graph().node(&core(line)).unwrap().pos,
        CanvasPoint { x: 48.0, y: 36.0 }
    );
    assert_eq!(latest.graph().node(&core(ending)).unwrap().pos.x, 490.0);
    first_interaction.reconcile_authoritative_state()?;
    second_interaction.reconcile_authoritative_state()?;
    assert_eq!(
        first_interaction.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
    assert_eq!(
        second_interaction.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
    let wrong = first_request.complete(second_publication).unwrap_err();
    assert!(matches!(wrong.error(), DialogueCanvasError::ResultMismatch));
    let (first_request, second_publication, _) = wrong.into_parts();
    assert_eq!(first_request.request().identity(), &original_identity);
    assert_eq!(second_publication.outcome(), &second_result);
    let second_completion = second_request.complete(second_publication)?;
    assert_eq!(second_completion.identity(), &second_identity);
    finish(&mut second_interaction, second_bound, second_completion);
    let first_completion = first_request.complete(first_publication)?;
    assert_eq!(first_completion.identity(), &original_identity);
    assert!(
        first_completion
            .publication()
            .unwrap()
            .snapshot()
            .ptr_eq(&first_snapshot)
    );
    finish(&mut first_interaction, first_bound, first_completion);
    assert!(external.snapshot()?.ptr_eq(&latest));
    assert!(first.graph_snapshot()?.ptr_eq(&latest));
    assert!(second.graph_snapshot()?.ptr_eq(&latest));
    assert_eq!(
        first_interaction.presentation().terminal(),
        Some(InteractionTerminalReason::Applied)
    );
    assert_eq!(
        second_interaction.presentation().terminal(),
        Some(InteractionTerminalReason::Applied)
    );
    assert_eq!(
        facts(editor, document).history.undo_depth,
        baseline.history.undo_depth + 2
    );
    assert_eq!(first.selection()?.nodes(), &[core(line)]);
    assert_eq!(second.selection()?.nodes(), &[core(ending)]);
    history(editor, document, false);
    assert_eq!(
        view(editor, document).node(ending).unwrap().position,
        [480.0, 24.0]
    );
    assert_eq!(
        view(editor, document).node(line).unwrap().position,
        [48.0, 36.0]
    );
    history(editor, document, false);
    assert_eq!(
        view(editor, document).node(line).unwrap().position,
        [24.0, 24.0]
    );
    Ok(())
}

#[test]
fn connection_and_both_wire_creations_adopt_domain_allocations_and_single_history_entries()
-> Result<()> {
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let line = add(editor, document, NodeRecipe::Line, [24.0, 24.0]);
    let ending = add(editor, document, NodeRecipe::End, [600.0, 24.0]);
    let (external, first, second) = external(editor, document)?;
    let snapshot = external.snapshot()?;
    let line_output = output(&snapshot, line);
    let ending_input = input(&snapshot, ending);
    let proposed_edge = EdgeId::from_u128(0xdead);
    let mut connection = interaction(&first);
    let (bound, request) = connect(
        &mut connection,
        &first,
        line_output,
        ending_input,
        proposed_edge,
    );
    let before = facts(editor, document);
    let result = execute(editor, &request);
    assert_eq!(
        result.get::<DialogueGraph>().unwrap().outcome(),
        &DialogueCommandOutcome::Changed
    );
    let publication = request.publish(&external, result)?;
    let canonical_edge = EdgeId(line_output.0);
    assert_ne!(canonical_edge, proposed_edge);
    assert!(
        publication
            .patch()
            .snapshot()
            .edge_instance_key(canonical_edge)
            .is_some()
    );
    assert!(
        publication
            .patch()
            .snapshot()
            .edge_instance_key(proposed_edge)
            .is_none()
    );
    finish(&mut connection, bound, request.complete(publication)?);
    assert_eq!(
        facts(editor, document).history.undo_depth,
        before.history.undo_depth + 1
    );
    assert_eq!(
        view(editor, document).target(OutputPort::Next { node: line }),
        Some(ending)
    );
    connection.close();
    history(editor, document, false);
    assert_eq!(
        view(editor, document).target(OutputPort::Next { node: line }),
        None
    );
    publish_current(&external, editor, document)?;
    for (source, kind, target, expected_recipe) in [
        (
            line_output,
            "nara.dialogue.choice",
            "input",
            NodeRecipe::Choice,
        ),
        (ending_input, "nara.dialogue.line", "next", NodeRecipe::Line),
    ] {
        let mut creation = interaction(&second);
        let (bound, request, proposed_node, proposed_port) = create_from_wire(
            &mut creation,
            &second,
            external.catalog(),
            source,
            kind,
            target,
        );
        let GraphMutationIntent::Connection {
            target:
                GraphConnectionTarget::CreateFromWire {
                    creation: proposal, ..
                },
            ..
        } = request.request().intent()
        else {
            panic!("search must retain its actual proposed creation");
        };
        assert_eq!(proposal.node().kind, NodeKindKey::new(kind));
        let baseline_content = encoded_content(&view(editor, document));
        let before = facts(editor, document);
        let result = execute(editor, &request);
        let DialogueCommandOutcome::Created {
            node,
            outputs,
            link: Some(link),
        } = result.get::<DialogueGraph>().unwrap().outcome()
        else {
            panic!("creation must return the original allocator identities");
        };
        let canonical_node = core(*node);
        assert_ne!(canonical_node, proposed_node);
        assert_eq!(
            match result
                .get::<DialogueGraph>()
                .unwrap()
                .view()
                .node(*node)
                .unwrap()
                .body
                .as_ref()
            {
                NodeBody::Line { .. } => NodeRecipe::Line,
                NodeBody::Choice { .. } => NodeRecipe::Choice,
                NodeBody::End { .. } => NodeRecipe::End,
            },
            expected_recipe,
        );
        assert_eq!(outputs.len(), 1);
        let publication = request.publish(&external, result.clone())?;
        let accepted = publication.patch().snapshot().clone();
        let opposite = if target == "input" {
            input(&accepted, *node)
        } else {
            output(&accepted, *node)
        };
        assert_ne!(opposite, proposed_port);
        if expected_recipe == NodeRecipe::Choice {
            assert_ne!(
                accepted
                    .graph()
                    .port(&output(&accepted, *node))
                    .unwrap()
                    .key,
                PortKey::new("choice")
            );
        }
        assert!(accepted.graph().node(&proposed_node).is_none());
        assert!(accepted.graph().port(&proposed_port).is_none());
        assert_eq!(
            publication.facts().edge_output(
                accepted.graph().graph_id(),
                EdgeId(output(&accepted, link.from.node()).0)
            ),
            Some(link.from)
        );
        let completion = request.complete(publication)?;
        assert_eq!(completion.effect(), Some(ExternalGraphEffect::Applied));
        finish(&mut creation, bound, completion);
        assert!(
            creation
                .required_connection_nodes(&accepted)
                .contains(&canonical_node)
        );
        assert!(
            !creation
                .required_connection_nodes(&accepted)
                .contains(&proposed_node)
        );
        publish_geometry(&second);
        creation.reconcile_authoritative_state()?;
        assert_eq!(
            creation.presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert_eq!(
            facts(editor, document).history.undo_depth,
            before.history.undo_depth + 1
        );
        assert!(first.graph_snapshot()?.ptr_eq(&accepted));
        assert!(second.graph_snapshot()?.ptr_eq(&accepted));
        creation.close();
        history(editor, document, false);
        assert_eq!(encoded_content(&view(editor, document)), baseline_content);
        publish_current(&external, editor, document)?;
    }
    Ok(())
}

#[test]
fn source_reconnection_and_mixed_delete_use_one_original_atomic_operation() -> Result<()> {
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let (line, choice, ending) = seed_conversation(editor, document);
    let replacement = add(editor, document, NodeRecipe::Line, [50.0, 220.0]);
    let (external, first, second) = external(editor, document)?;
    let initial = capture(editor, document)?;
    let snapshot = external.snapshot()?;
    let original_output = output(&snapshot, line);
    let replacement_output = output(&snapshot, replacement);
    let old_edge = snapshot
        .edge_instance_key(EdgeId(original_output.0))
        .unwrap();
    let mut reconnect = interaction(&first);
    let (bound, request) = reconnect_from_route(
        &mut reconnect,
        &first,
        old_edge.edge(),
        EdgeEndpoint::From,
        replacement_output,
    );
    let GraphMutationIntent::Connection { binding, .. } = request.request().intent() else {
        panic!("expected typed reconnection");
    };
    assert_eq!(binding.reconnect_edge(), Some(old_edge));
    let before = facts(editor, document);
    let result = execute(editor, &request);
    let publication = request.publish(&external, result)?;
    assert!(
        publication
            .patch()
            .snapshot()
            .edge_instance_key(old_edge.edge())
            .is_none()
    );
    assert!(
        publication
            .patch()
            .snapshot()
            .edge_instance_key(EdgeId(replacement_output.0))
            .is_some()
    );
    reconnect.reconcile_authoritative_state()?;
    assert_eq!(
        reconnect.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
    finish(&mut reconnect, bound, request.complete(publication)?);
    assert_eq!(
        view(editor, document).target(OutputPort::Next { node: line }),
        None
    );
    assert_eq!(
        view(editor, document).target(OutputPort::Next { node: replacement }),
        Some(choice)
    );
    assert_eq!(
        facts(editor, document).history.undo_depth,
        before.history.undo_depth + 1
    );
    reconnect.close();
    history(editor, document, false);
    assert_eq!(
        encoded_content(&view(editor, document)),
        encoded_content(initial.source())
    );
    publish_current(&external, editor, document)?;
    let mut retarget = interaction(&second);
    let snapshot = external.snapshot()?;
    let original_key = snapshot
        .edge_instance_key(EdgeId(original_output.0))
        .unwrap();
    let (bound, request) = reconnect_from_route(
        &mut retarget,
        &second,
        original_key.edge(),
        EdgeEndpoint::To,
        input(&snapshot, ending),
    );
    let before = facts(editor, document);
    let result = execute(editor, &request);
    let publication = request.publish(&external, result)?;
    assert_eq!(
        publication
            .patch()
            .snapshot()
            .edge_instance_key(original_key.edge()),
        Some(original_key)
    );
    finish(&mut retarget, bound, request.complete(publication)?);
    assert_eq!(
        view(editor, document).target(OutputPort::Next { node: line }),
        Some(ending)
    );
    assert_eq!(
        facts(editor, document).history.undo_depth,
        before.history.undo_depth + 1
    );
    retarget.close();
    history(editor, document, false);
    assert_eq!(
        encoded_content(&view(editor, document)),
        encoded_content(initial.source())
    );
    publish_current(&external, editor, document)?;
    let snapshot = external.snapshot()?;
    let ending_key = snapshot.node_lifetime_key(core(ending)).unwrap();
    let incident = snapshot
        .graph()
        .edges()
        .iter()
        .find_map(|(edge, model)| {
            (snapshot.graph().port(&model.to).unwrap().node == core(ending))
                .then_some(snapshot.edge_instance_key(*edge).unwrap())
        })
        .unwrap();
    let unrelated = snapshot
        .edge_instance_key(EdgeId(original_output.0))
        .unwrap();
    first.set_selection(vec![ending_key], vec![incident, unrelated], Vec::new())?;
    let mut deletion = interaction(&first);
    let update = deletion.delete_selection()?;
    let (bound, request) = bind(&mut deletion, update);
    let GraphMutationIntent::DeleteSelection { nodes, edges } = request.request().intent() else {
        panic!("mixed deletion must remain one semantic request");
    };
    assert_eq!(nodes, &[ending_key]);
    assert_eq!(edges, &[unrelated]);
    let before = facts(editor, document);
    let result = execute(editor, &request);
    let publication = request.publish(&external, result)?;
    finish(&mut deletion, bound, request.complete(publication)?);
    assert!(view(editor, document).node(ending).is_none());
    assert_eq!(
        view(editor, document).target(OutputPort::Next { node: line }),
        None
    );
    assert_eq!(
        facts(editor, document).history.undo_depth,
        before.history.undo_depth + 1
    );
    let deleted = external.snapshot()?;
    assert!(second.graph_snapshot()?.ptr_eq(&deleted));
    history(editor, document, false);
    assert_eq!(
        encoded_content(&view(editor, document)),
        encoded_content(initial.source())
    );
    publish_current(&external, editor, document)?;
    assert_ne!(
        external.snapshot()?.node_lifetime_key(core(ending)),
        Some(ending_key)
    );
    assert_ne!(
        external.snapshot()?.edge_instance_key(incident.edge()),
        Some(incident)
    );
    Ok(())
}

#[test]
fn creation_during_either_endpoint_reconnection_uses_one_session_edit_and_survives_reopen()
-> Result<()> {
    for (endpoint, kind, target) in [
        (EdgeEndpoint::From, "nara.dialogue.choice", "choice"),
        (EdgeEndpoint::To, "nara.dialogue.end", "input"),
    ] {
        let project = create_project()?;
        let (mut owner, document) = open(&project)?;
        let editor = &mut owner.0;
        let (line, choice, _) = seed_conversation(editor, document);
        let baseline_bytes = save(editor, &project, document)?;
        let baseline = view(editor, document);
        let before = facts(editor, document);
        let (external, first, second) = external(editor, document)?;
        publish_geometry(&first);
        let geometry = first.geometry_snapshot()?;
        let snapshot = external.snapshot()?;
        let line_output = output(&snapshot, line);
        let old_edge = snapshot.edge_instance_key(EdgeId(line_output.0)).unwrap();
        let route = geometry.route(old_edge.edge()).unwrap();
        let fixed = match endpoint {
            EdgeEndpoint::From => route.facts().endpoints.target,
            EdgeEndpoint::To => route.facts().endpoints.source,
        };
        let start = first
            .connection_start_from_route(
                route,
                endpoint,
                fixed,
                NodeGraphConnectionMode::Strict,
                true,
            )?
            .unwrap();
        let mut creation = interaction(&first);
        let (bound, request, proposed_node, proposed_port) = create_from_start(
            &mut creation,
            &first,
            external.catalog(),
            start,
            kind,
            target,
        );
        let GraphMutationIntent::Connection { binding, .. } = request.request().intent() else {
            panic!("expected original route reconnection authority");
        };
        assert_eq!(binding.reconnect_edge(), Some(old_edge));
        let result = execute(editor, &request);
        let DialogueCommandOutcome::Created {
            node,
            outputs,
            link: Some(link),
        } = result.get::<DialogueGraph>().unwrap().outcome()
        else {
            panic!("expected the original atomic domain allocation");
        };
        let created = *node;
        assert_ne!(core(created), proposed_node);
        let changed = view(editor, document);
        match endpoint {
            EdgeEndpoint::From => {
                assert_eq!(outputs.len(), 1);
                assert_eq!(changed.target(OutputPort::Next { node: line }), None);
                assert_eq!(link.from, outputs[0]);
                assert_eq!(link.to, choice);
                assert_eq!(changed.target(outputs[0]), Some(choice));
            }
            EdgeEndpoint::To => {
                assert!(outputs.is_empty());
                assert_eq!(link.from, OutputPort::Next { node: line });
                assert_eq!(link.to, created);
                assert_eq!(changed.target(link.from), Some(created));
            }
        }
        assert_eq!(
            facts(editor, document).history.undo_depth,
            before.history.undo_depth + 1
        );
        let changed_content = encoded_content(&changed);
        let publication = request.publish(&external, result.clone())?;
        let accepted = publication.patch().snapshot().clone();
        let opposite = match endpoint {
            EdgeEndpoint::From => output(&accepted, created),
            EdgeEndpoint::To => input(&accepted, created),
        };
        assert_ne!(opposite, proposed_port);
        assert!(accepted.graph().port(&proposed_port).is_none());
        match endpoint {
            EdgeEndpoint::From => assert!(accepted.edge_instance_key(old_edge.edge()).is_none()),
            EdgeEndpoint::To => {
                assert_eq!(accepted.edge_instance_key(old_edge.edge()), Some(old_edge))
            }
        }
        creation.reconcile_authoritative_state()?;
        assert_eq!(
            creation.presentation().phase(),
            CanvasInteractionPhase::Committing
        );
        finish(&mut creation, bound, request.complete(publication)?);
        assert!(
            creation
                .required_connection_nodes(&accepted)
                .contains(&core(created))
        );
        assert!(
            !creation
                .required_connection_nodes(&accepted)
                .contains(&proposed_node)
        );
        assert!(second.graph_snapshot()?.ptr_eq(&accepted));
        publish_geometry(&first);
        creation.reconcile_authoritative_state()?;
        assert_eq!(
            creation.presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert_eq!(fs::read(project.path().join(SOURCE))?, baseline_bytes);
        history(editor, document, false);
        assert_eq!(
            encoded_content(&view(editor, document)),
            encoded_content(&baseline)
        );
        assert!(!facts(editor, document).dirty);
        history(editor, document, true);
        assert_eq!(encoded_content(&view(editor, document)), changed_content);
        assert_ne!(
            view(editor, document).node_incarnation(created),
            changed.node_incarnation(created)
        );
        publish_current(&external, editor, document)?;
        let saved = save(editor, &project, document)?;
        let before_reopen = facts(editor, document);
        let reopened = persist(
            editor,
            EditorPersistenceAction::Reopen {
                target: EditorReopenTarget::OpenDocument {
                    document,
                    base_revision: before_reopen.revision,
                },
            },
        );
        let current = facts(editor, document);
        assert!(
            matches!(reopened, EditorPersistenceResult::Opened { document: actual, revision, .. }
            if actual == document && revision == current.revision)
        );
        acknowledge(editor);
        assert_ne!(
            current.revision.source_id(),
            before_reopen.revision.source_id()
        );
        assert_eq!(current.history, Default::default());
        assert_eq!(encoded_content(&view(editor, document)), changed_content);
        assert_eq!(fs::read(project.path().join(SOURCE))?, saved);
        external.reset(
            external.snapshot()?.version(),
            capture(editor, document)?.external_facts()?,
        )?;
        let replacement = external.snapshot()?;
        assert_ne!(
            replacement.node_lifetime_key(core(created)),
            accepted.node_lifetime_key(core(created))
        );
        assert!(first.graph_snapshot()?.ptr_eq(&replacement));
        assert!(second.graph_snapshot()?.ptr_eq(&replacement));
        owner.close()?;
        assert_eq!(
            encoded_content(result.get::<DialogueGraph>().unwrap().view()),
            changed_content
        );
    }
    Ok(())
}

#[test]
fn published_edge_deletion_uses_its_original_instance_and_one_domain_undo() -> Result<()> {
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let (line, choice, _) = seed_conversation(editor, document);
    let original = view(editor, document);
    let before = facts(editor, document);
    let (external, first, second) = external(editor, document)?;
    publish_geometry(&first);
    let snapshot = external.snapshot()?;
    let edge = EdgeId(output(&snapshot, line).0);
    let original_instance = snapshot.edge_instance_key(edge).unwrap();
    let geometry = first.geometry_snapshot()?;
    let mut deletion = interaction(&first);
    let update = deletion.delete_edge(geometry.route(edge).unwrap())?;
    let (bound, request) = bind(&mut deletion, update);
    assert!(matches!(
        request.request().intent(),
        GraphMutationIntent::DeleteEdge(instance) if *instance == original_instance,
    ));
    let result = execute(editor, &request);
    let publication = request.publish(&external, result)?;
    finish(&mut deletion, bound, request.complete(publication)?);
    assert_eq!(
        view(editor, document).target(OutputPort::Next { node: line }),
        None
    );
    assert!(view(editor, document).node(line).is_some());
    assert!(view(editor, document).node(choice).is_some());
    assert_eq!(
        facts(editor, document).history.undo_depth,
        before.history.undo_depth + 1
    );
    assert!(second.graph_snapshot()?.graph().edge(&edge).is_none());
    history(editor, document, false);
    assert_eq!(
        encoded_content(&view(editor, document)),
        encoded_content(&original)
    );
    publish_current(&external, editor, document)?;
    assert_ne!(
        external.snapshot()?.edge_instance_key(edge),
        Some(original_instance)
    );
    Ok(())
}

#[test]
fn domain_canvas_refusal_retains_original_source_and_does_not_create_history() -> Result<()> {
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let line = add(editor, document, NodeRecipe::Line, [1_000_000.0, 24.0]);
    let (external, first, _) = external(editor, document)?;
    let snapshot = external.snapshot()?;
    let mut movement = interaction(&first);
    let (bound, request) = nudge(&mut movement, &first, line);
    let before = facts(editor, document);
    let original = view(editor, document);
    let command = request.command().clone();
    let mut outcomes = dispatch(editor, [EditorAction::Extension(command.clone())]);
    let outcome = outcomes.remove(0);
    let EditorActionOutcome::Workspace(report) = &outcome else {
        panic!("expected the original domain's exact refusal: {outcome:?}");
    };
    assert!(!report.applied);
    assert!(report.extension_outcome.is_none());
    assert_eq!(report.document, Some(document));
    assert!(!report.diagnostics.is_empty());
    let completion = request.reject(command, outcome)?;
    assert!(
        completion
            .rejection()
            .unwrap()
            .contains("dialogue.position")
    );
    finish(&mut movement, bound, completion);
    assert_eq!(
        movement.presentation().terminal(),
        Some(InteractionTerminalReason::Rejected)
    );
    assert_eq!(facts(editor, document).revision, before.revision);
    assert_eq!(facts(editor, document).history, before.history);
    assert!(Arc::ptr_eq(&view(editor, document), &original));
    assert!(external.snapshot()?.ptr_eq(&snapshot));
    Ok(())
}

#[test]
fn successful_noop_keeps_source_history_and_has_an_exact_accepted_operation_revision() -> Result<()>
{
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let line = add(editor, document, NodeRecipe::Line, [24.0, 24.0]);
    let ending = add(editor, document, NodeRecipe::End, [480.0, 24.0]);
    edit(
        editor,
        document,
        DialogueCommand::SetTarget {
            port: OutputPort::Next { node: line },
            target: Some(ending),
        },
    );
    let before = facts(editor, document);
    let source = view(editor, document);
    let (external, first, _) = external(editor, document)?;
    let snapshot = external.snapshot()?;
    let mut connection = interaction(&first);
    let (bound, request) = connect(
        &mut connection,
        &first,
        output(&snapshot, line),
        input(&snapshot, ending),
        EdgeId::from_u128(0xcafe),
    );
    let result = execute(editor, &request);
    let receipt = result.get::<DialogueGraph>().unwrap();
    assert_eq!(receipt.outcome(), &DialogueCommandOutcome::Noop);
    assert!(Arc::ptr_eq(receipt.view(), &source));
    assert!(Arc::ptr_eq(&view(editor, document), &source));
    assert_eq!(facts(editor, document).history, before.history);
    assert_eq!(facts(editor, document).revision, before.revision.next());
    let publication = request.publish(&external, result)?;
    assert_ne!(publication.patch().to_version(), snapshot.version());
    let completion = request.complete(publication)?;
    assert_eq!(completion.effect(), Some(ExternalGraphEffect::Noop));
    finish(&mut connection, bound, completion);
    assert_eq!(facts(editor, document).history, before.history);
    Ok(())
}

#[test]
fn stale_domain_basis_uses_the_original_rejection_slot_without_execution_or_history() -> Result<()>
{
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let line = add(editor, document, NodeRecipe::Line, [24.0, 24.0]);
    let (external, first, _) = external(editor, document)?;
    let mut movement = interaction(&first);
    let (bound, request) = nudge(&mut movement, &first, line);
    let command = request.command().clone();
    let captured = request.request().snapshot().clone();
    edit(
        editor,
        document,
        DialogueCommand::SetLine {
            node: line,
            speaker: "Guide".into(),
            text: "A later edit".into(),
        },
    );
    let before = facts(editor, document);
    let source = view(editor, document);
    let mut outcomes = dispatch(editor, [EditorAction::Extension(command.clone())]);
    let outcome = outcomes.remove(0);
    assert_eq!(
        outcome,
        EditorActionOutcome::Rejected(EditorActionRejection::StaleDocumentFact {
            document,
            expected: command.base_revision(),
            actual: Some(before.revision),
        }),
    );
    let unrelated = EditorExtensionCommand::new::<DialogueGraph>(
        document,
        command.base_revision(),
        DialogueCommand::Move(Vec::new()),
    );
    let wrong = request.reject(unrelated, outcome).unwrap_err();
    assert!(matches!(wrong.error(), DialogueCanvasError::ResultMismatch));
    let (request, _, outcome, _) = wrong.into_parts();
    let completion = request.reject(command, outcome)?;
    assert!(completion.rejection().is_some());
    finish(&mut movement, bound, completion);
    assert_eq!(
        movement.presentation().terminal(),
        Some(InteractionTerminalReason::Rejected)
    );
    assert_eq!(
        movement.presentation().phase(),
        CanvasInteractionPhase::Idle
    );
    assert_eq!(facts(editor, document).revision, before.revision);
    assert_eq!(facts(editor, document).history, before.history);
    assert!(Arc::ptr_eq(&view(editor, document), &source));
    assert!(external.snapshot()?.ptr_eq(&captured));
    Ok(())
}

#[test]
fn reopen_resets_authority_and_preserves_the_original_refused_request_basis() -> Result<()> {
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let line = add(editor, document, NodeRecipe::Line, [24.0, 24.0]);
    let saved = save(editor, &project, document)?;
    let before = facts(editor, document);
    let (external, first, second) = external(editor, document)?;
    let mut movement = interaction(&first);
    let (bound, request) = nudge(&mut movement, &first, line);
    let original = request.request().snapshot().clone();
    let command = request.command().clone();
    let reopened = persist(
        editor,
        EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: before.revision,
            },
        },
    );
    assert!(
        matches!(reopened, EditorPersistenceResult::Opened { document: current, .. } if current == document)
    );
    acknowledge(editor);
    let after = facts(editor, document);
    assert_ne!(after.revision.source_id(), before.revision.source_id());
    assert_eq!(after.history.undo_depth, 0);
    let current = capture(editor, document)?;
    external.reset(external.snapshot()?.version(), current.external_facts()?)?;
    assert_eq!(
        request
            .request()
            .owner_basis::<DialogueFacts>()
            .unwrap()
            .basis()
            .revision,
        before.revision
    );
    let mut outcomes = dispatch(editor, [EditorAction::Extension(command.clone())]);
    let outcome = outcomes.remove(0);
    assert!(matches!(
        outcome,
        EditorActionOutcome::Rejected(EditorActionRejection::StaleDocumentFact { .. })
    ));
    let completion = request.reject(command, outcome)?;
    assert!(matches!(
        movement.settle_submission(
            bound,
            InteractionSubmissionSettlementEvidence::External(completion)
        ),
        InteractionSubmissionSettlementResult::Stale(_),
    ));
    assert_eq!(
        movement.presentation().phase(),
        CanvasInteractionPhase::Idle
    );
    assert_ne!(
        external.snapshot()?.node_lifetime_key(core(line)),
        original.node_lifetime_key(core(line))
    );
    assert!(first.graph_snapshot()?.ptr_eq(&external.snapshot()?));
    assert!(second.graph_snapshot()?.ptr_eq(&external.snapshot()?));
    assert_eq!(facts(editor, document).revision, after.revision);
    assert_eq!(facts(editor, document).history, after.history);
    assert_eq!(fs::read(project.path().join(SOURCE))?, saved);
    Ok(())
}

#[test]
fn accepted_receipt_stays_historical_after_reset_view_close_and_session_retirement() -> Result<()> {
    for reset in [false, true] {
        let project = create_project()?;
        let (mut owner, document) = open(&project)?;
        let editor = &mut owner.0;
        let line = add(editor, document, NodeRecipe::Line, [24.0, 24.0]);
        let (external, first, second) = external(editor, document)?;
        let mut movement = interaction(&first);
        let (bound, request) = nudge(&mut movement, &first, line);
        let result = execute(editor, &request);
        let publication = request.publish(&external, result.clone())?;
        let accepted = publication.patch().snapshot().clone();
        if reset {
            external.reset(
                external.snapshot()?.version(),
                capture(editor, document)?.external_facts()?,
            )?;
        } else {
            movement.close();
            first.close()?;
            assert_eq!(external.live_view_count(), 1);
        }
        let current = external.snapshot()?;
        let completion = request.complete(publication)?;
        assert!(
            completion
                .publication()
                .unwrap()
                .snapshot()
                .ptr_eq(&accepted)
        );
        assert!(matches!(
            movement.settle_submission(
                bound,
                InteractionSubmissionSettlementEvidence::External(completion)
            ),
            InteractionSubmissionSettlementResult::Stale(_),
        ));
        assert!(second.graph_snapshot()?.ptr_eq(&current));
        owner.close()?;
        assert_eq!(
            result
                .get::<DialogueGraph>()
                .unwrap()
                .view()
                .node(line)
                .unwrap()
                .position,
            [34.0, 24.0]
        );
        assert!(external.snapshot()?.ptr_eq(&current));
    }
    Ok(())
}

#[test]
fn stale_publication_returns_the_accepted_outcome_without_redispatch() -> Result<()> {
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let line = add(editor, document, NodeRecipe::Line, [24.0, 24.0]);
    let (external, first, _) = external(editor, document)?;
    let mut movement = interaction(&first);
    let (bound, request) = nudge(&mut movement, &first, line);
    let result = execute(editor, &request);
    let before = facts(editor, document);
    publish_current(&external, editor, document)?;
    let latest = external.snapshot()?;
    let failure = request.publish(&external, result.clone()).unwrap_err();
    assert!(matches!(
        failure.error(),
        DialogueCanvasError::Publication(ExternalGraphError::StalePublication { .. })
    ));
    let (retained, _) = failure.into_parts();
    assert_eq!(retained, result);
    assert_eq!(facts(editor, document).revision, before.revision);
    assert_eq!(facts(editor, document).history, before.history);
    assert!(external.snapshot()?.ptr_eq(&latest));
    assert!(matches!(
        movement.settle_submission(
            bound,
            InteractionSubmissionSettlementEvidence::OutcomeUnknown
        ),
        InteractionSubmissionSettlementResult::Settled(_),
    ));
    assert_eq!(
        movement.presentation().terminal(),
        Some(InteractionTerminalReason::Uncertain)
    );
    Ok(())
}
