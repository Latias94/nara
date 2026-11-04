use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringError, UiAuthoringSnapshot, UiAuthoringSnapshotBuilder, UiColor, UiCompileError,
    UiCompiler, UiEdges, UiEnvironment, UiInteraction, UiKey, UiLayer, UiLimits, UiLocalKey,
    UiMesh, UiMeshError, UiMeshVertex, UiNode, UiOverflow, UiPaintPrimitive, UiPoint, UiRect,
    UiRoot, UiRootKey, UiRootStatus, UiSafeAreaInsets, UiSemanticRole, UiSemantics, UiSize,
    UiStyle, UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId, UiText, UiTheme,
    UiVisual,
};

const COLOR: UiColor = UiColor::rgba(0.25, 0.5, 0.75, 0.5);

fn vertex(position_x: f32, position_y: f32) -> UiMeshVertex {
    UiMeshVertex::new(UiPoint::new(position_x, position_y), COLOR)
}

fn triangle() -> UiMesh {
    UiMesh::new(
        vec![vertex(0.0, 0.0), vertex(30.0, 0.0), vertex(0.0, 20.0)],
        vec![0, 1, 2],
    )
    .unwrap()
}

fn square() -> UiMesh {
    UiMesh::new(
        vec![
            vertex(0.0, 0.0),
            vertex(30.0, 0.0),
            vertex(30.0, 20.0),
            vertex(0.0, 20.0),
        ],
        vec![0, 1, 2, 0, 2, 3],
    )
    .unwrap()
}

fn root_key(name: &str) -> UiRootKey {
    UiRootKey::runtime("mesh", name).unwrap()
}

fn key(name: &str) -> UiLocalKey {
    UiLocalKey::runtime("mesh", name).unwrap()
}

fn scoped(root: &str, name: &str) -> UiKey {
    UiKey::scoped(root_key(root), key(name)).unwrap()
}

fn root(name: &str) -> UiRoot {
    UiRoot::new(root_key(name), UiLayer::Base, UiStyle::fill())
}

fn invalid_root(name: &str) -> UiRoot {
    UiRoot::new(
        root_key(name),
        UiLayer::Base,
        UiStyle::fill().with_gap(f32::NAN),
    )
}

fn snapshot(
    revision: u64,
    limits: UiLimits,
    roots: impl IntoIterator<Item = UiRoot>,
) -> UiAuthoringSnapshot {
    roots
        .into_iter()
        .fold(
            UiAuthoringSnapshotBuilder::new(revision).with_limits(limits),
            |builder, root| builder.with_root(root),
        )
        .build()
        .unwrap()
}

fn environment() -> UiEnvironment {
    UiEnvironment::new(UiSurfaceId::new(90).unwrap(), 100.0, 80.0, 1.25, 1, 1, 1, 1).unwrap()
}

fn compiler(limits: UiLimits) -> UiCompiler {
    UiCompiler::new(bundled_font_face().unwrap(), limits).unwrap()
}

fn rect(origin_x: f32, origin_y: f32, width: f32, height: f32) -> UiRect {
    UiRect::from_origin_size(origin_x, origin_y, width, height).unwrap()
}

#[test]
fn immutable_mesh_preserves_all_vertices_colors_indices_and_local_bounds() {
    let vertices = vec![
        UiMeshVertex::new(
            UiPoint::new(-16_384.0, -16.0),
            UiColor::rgba(0.0, 0.0, 0.0, 0.0),
        ),
        UiMeshVertex::new(UiPoint::new(100.0, 0.0), UiColor::rgba(1.0, 1.0, 1.0, 1.0)),
        vertex(0.0, 16_384.0),
        vertex(16_384.0, -16_384.0),
    ];
    let mesh = UiMesh::new(vertices.clone(), vec![0, 1, 2]).unwrap();
    assert_eq!(mesh.vertices(), vertices);
    assert_eq!(mesh.vertices()[2].position(), UiPoint::new(0.0, 16_384.0));
    assert_eq!(mesh.vertices()[2].color(), COLOR);
    assert_eq!(mesh.indices(), &[0, 1, 2]);
    assert_eq!(
        mesh.bounds(),
        rect(-16_384.0, -16_384.0, 32_768.0, 32_768.0)
    );

    let shared = mesh.clone();
    assert!(std::ptr::eq(mesh.vertices(), shared.vertices()));
    assert!(std::ptr::eq(mesh.indices(), shared.indices()));
    drop(mesh);
    assert_eq!(shared.vertices(), vertices);
    assert_eq!(shared.indices(), &[0, 1, 2]);
}

#[test]
fn mesh_rejects_empty_incomplete_and_out_of_range_triangles() {
    assert_eq!(
        UiMesh::new(Vec::new(), vec![0, 0, 0]),
        Err(UiMeshError::EmptyVertices)
    );
    let vertices = triangle().vertices().to_vec();
    assert_eq!(
        UiMesh::new(vertices.clone(), Vec::new()),
        Err(UiMeshError::EmptyTriangles)
    );
    for indices in [vec![0], vec![0, 1], vec![0, 1, 2, 0]] {
        let index_count = indices.len();
        assert_eq!(
            UiMesh::new(vertices.clone(), indices),
            Err(UiMeshError::IncompleteTriangle { index_count })
        );
    }
    for index in [3, u32::MAX] {
        assert_eq!(
            UiMesh::new(vertices.clone(), vec![0, index, 2]),
            Err(UiMeshError::IndexOutOfRange {
                offset: 1,
                index,
                vertex_count: 3
            }),
        );
    }
}

#[test]
fn mesh_rejects_nonfinite_and_out_of_range_coordinates_on_both_axes() {
    for coordinate in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        -16_384.5,
        16_384.5,
    ] {
        for position in [UiPoint::new(coordinate, 0.0), UiPoint::new(0.0, coordinate)] {
            let mut vertices = triangle().vertices().to_vec();
            vertices[1] = UiMeshVertex::new(position, COLOR);
            assert!(matches!(
                UiMesh::new(vertices, vec![0, 1, 2]),
                Err(UiMeshError::InvalidPosition { vertex: 1, .. })
            ));
        }
    }
}

#[test]
fn mesh_validates_each_straight_alpha_channel_including_unreferenced_vertices() {
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.001, 1.001] {
        for channel in 0..4 {
            let mut channels = [0.5; 4];
            channels[channel] = invalid;
            let mut vertices = triangle().vertices().to_vec();
            vertices.push(UiMeshVertex::new(
                UiPoint::new(10.0, 10.0),
                UiColor::rgba(channels[0], channels[1], channels[2], channels[3]),
            ));
            assert_eq!(
                UiMesh::new(vertices, vec![0, 1, 2]),
                Err(UiMeshError::InvalidColor { vertex: 3 }),
            );
        }
    }
}

#[test]
fn mesh_constructor_and_owner_profiles_keep_the_hard_ceilings() {
    let limits = UiLimits::default();
    assert_eq!(limits.max_mesh_vertices(), 65_536);
    assert_eq!(limits.max_mesh_indices(), 196_608);
    assert!(limits.with_max_mesh_vertices(0).is_err());
    assert!(limits.with_max_mesh_vertices(65_537).is_err());
    assert!(limits.with_max_mesh_indices(0).is_err());
    assert!(limits.with_max_mesh_indices(196_609).is_err());
    let mesh = UiMesh::new(vec![vertex(0.0, 0.0); 65_536], vec![65_535; 196_608]).unwrap();
    assert_eq!(mesh.vertices().len(), 65_536);
    assert_eq!(mesh.indices().len(), 196_608);
    assert_eq!(mesh.bounds(), rect(0.0, 0.0, 0.0, 0.0));
    assert_eq!(
        UiMesh::new(vec![vertex(0.0, 0.0); 65_537], vec![0; 3]),
        Err(UiMeshError::VertexLimitExceeded {
            actual: 65_537,
            maximum: 65_536
        }),
    );
    assert_eq!(
        UiMesh::new(vec![vertex(0.0, 0.0)], vec![0; 196_611]),
        Err(UiMeshError::IndexLimitExceeded {
            actual: 196_611,
            maximum: 196_608
        }),
    );
}

#[test]
fn mesh_uses_content_origin_padding_and_mandatory_clip_at_fractional_scale() {
    let transform = UiSurfaceContentTransform::new(
        UiSize::new(100.0, 80.0).unwrap(),
        UiSize::new(256.0, 220.0).unwrap(),
        UiSafeAreaInsets::new(3.0, 5.0, 3.0, 5.0).unwrap(),
        1.25,
    )
    .unwrap();
    assert_eq!(transform.content_scale_factor(), 2.5);
    assert_eq!(transform.physical_content_origin(), UiPoint::new(3.0, 10.0));
    let environment = UiEnvironment::for_surface(
        UiSurfaceId::new(90).unwrap(),
        transform,
        UiSurfaceGenerations::new(1, 1, 1, 1).unwrap(),
        1,
        1,
    )
    .unwrap();
    let mesh = UiMesh::new(
        vec![vertex(-8.0, -5.0), vertex(48.0, 0.0), vertex(0.0, 35.0)],
        vec![0, 1, 2],
    )
    .unwrap();
    let source = snapshot(
        1,
        UiLimits::default(),
        [root("mapped").with_child(
            UiNode::new(
                UiStyle::absolute(10.25, 12.5, 30.0, 22.75).with_overflow(UiOverflow::Clip),
            )
            .with_child(
                UiNode::new(
                    UiStyle::absolute(5.5, 4.25, 40.5, 28.25)
                        .with_padding(UiEdges::new(2.25, 3.75, 1.5, 2.5).unwrap()),
                )
                .with_key(key("curve"))
                .with_mesh(mesh.clone())
                .with_visual(UiVisual::solid(COLOR))
                .with_text(UiText::new("Curve", 10.0).unwrap())
                .with_interaction(UiInteraction::focus_target())
                .with_semantics(UiSemantics::new(UiSemanticRole::Button, "Curve").unwrap()),
            ),
        )],
    );
    let mut compiler = compiler(UiLimits::default());
    let frame = compiler
        .compile(&source, environment, UiTheme::default())
        .unwrap();
    assert_eq!(
        frame.root_outcomes()[0].status(),
        UiRootStatus::Published,
        "{:?}",
        frame.diagnostics()
    );
    let key = scoped("mapped", "curve");
    let outer = rect(15.75, 16.75, 40.5, 28.25);
    let content = rect(18.0, 18.25, 34.5, 24.25);
    let inherited_clip = rect(10.25, 12.5, 30.0, 22.75);
    let clipped_content = rect(18.0, 18.25, 22.25, 17.0);
    let UiPaintPrimitive::Mesh {
        key: paint_key,
        rect,
        physical_rect,
        clip_rect,
        mesh: captured,
    } = &frame.paint()[1]
    else {
        panic!("background, mesh and text must retain their order")
    };
    assert_eq!(paint_key.as_ref(), Some(&key));
    assert_eq!(*rect, content);
    assert_eq!(*clip_rect, Some(clipped_content));
    assert_eq!(*physical_rect, environment.physical_rect(content).unwrap());
    assert_eq!(
        (
            physical_rect.min_x(),
            physical_rect.min_y(),
            physical_rect.max_x(),
            physical_rect.max_y()
        ),
        (48, 56, 134, 116)
    );
    assert_eq!(captured, &mesh);
    assert!(std::ptr::eq(captured.vertices(), mesh.vertices()));
    assert_eq!(captured.vertices()[0].position(), UiPoint::new(-8.0, -5.0));
    assert!(
        matches!(frame.paint()[0], UiPaintPrimitive::Rect { rect, clip_rect: Some(clip), .. } if rect == outer && clip == inherited_clip)
    );
    assert!(matches!(frame.paint()[2], UiPaintPrimitive::Text { rect, .. } if rect == content));
    assert_eq!(
        frame.hit_test(UiPoint::new(16.0, 17.0)).unwrap().key(),
        &key
    );
    assert_eq!(frame.focus_order()[0].rect(), outer);
    assert_eq!(frame.semantics()[0].rect(), outer);
    assert_eq!(frame.semantics()[0].clip_rect(), Some(inherited_clip));
}

#[test]
fn mesh_never_expands_intrinsic_layout_or_implicitly_enables_input() {
    let roots = |with_mesh| {
        let child = UiNode::new(UiStyle::flex());
        root("intrinsic").with_child(if with_mesh {
            child.with_mesh(triangle())
        } else {
            child
        })
    };
    let limits = UiLimits::default();
    let mut compiler = compiler(limits);
    let plain = compiler
        .compile(
            &snapshot(1, limits, [roots(false)]),
            environment(),
            UiTheme::default(),
        )
        .unwrap();
    let drawn = compiler
        .compile(
            &snapshot(2, limits, [roots(true)]),
            environment(),
            UiTheme::default(),
        )
        .unwrap();
    assert_eq!(drawn.layouts(), plain.layouts());
    assert!(drawn.paint().is_empty());
    assert!(drawn.hit_regions().is_empty());
    assert!(drawn.focus_order().is_empty());
    assert!(drawn.semantics().is_empty());
}

#[test]
fn same_node_fragments_and_other_nodes_follow_existing_paint_order() {
    let decorated = |name, z_index| {
        UiNode::new(UiStyle::absolute(5.0, 5.0, 60.0, 30.0).with_z_index(z_index))
            .with_key(key(name))
            .with_visual(UiVisual::solid(COLOR))
            .with_mesh(triangle())
            .with_text(UiText::new(name, 10.0).unwrap())
    };
    let source = snapshot(
        1,
        UiLimits::default(),
        [root("order")
            .with_visual(UiVisual::solid(COLOR))
            .with_mesh(triangle())
            .with_text(UiText::new("root", 10.0).unwrap())
            .with_children([decorated("foreground", 1), decorated("underlay", 0)])],
    );
    let frame = compiler(UiLimits::default())
        .compile(&source, environment(), UiTheme::default())
        .unwrap();
    let fragments = frame
        .paint()
        .iter()
        .map(|paint| {
            let kind = match paint {
                UiPaintPrimitive::Rect { .. } => "rect",
                UiPaintPrimitive::Mesh { .. } => "mesh",
                UiPaintPrimitive::Text { .. } => "text",
            };
            (paint.key().unwrap().clone(), kind)
        })
        .collect::<Vec<_>>();
    let root_key = UiKey::root(root_key("order")).unwrap();
    assert_eq!(
        fragments,
        [
            (root_key.clone(), "rect"),
            (root_key.clone(), "mesh"),
            (root_key, "text"),
            (scoped("order", "underlay"), "rect"),
            (scoped("order", "underlay"), "mesh"),
            (scoped("order", "underlay"), "text"),
            (scoped("order", "foreground"), "rect"),
            (scoped("order", "foreground"), "mesh"),
            (scoped("order", "foreground"), "text"),
        ]
    );
}

#[test]
fn source_admission_counts_repeated_hidden_and_rejected_root_meshes() {
    let mesh = triangle();
    let repeated = || {
        root("repeated").with_mesh(mesh.clone()).with_child(
            UiNode::new(UiStyle::absolute(0.0, 0.0, 20.0, 20.0)).with_mesh(mesh.clone()),
        )
    };
    let hidden = || {
        UiRoot::new(
            root_key("hidden"),
            UiLayer::Base,
            UiStyle::fill().with_visible(false),
        )
        .with_mesh(mesh.clone())
        .with_child(UiNode::new(UiStyle::fill()).with_mesh(mesh.clone()))
    };
    let rejected = || {
        invalid_root("rejected").with_mesh(mesh.clone()).with_child(
            UiNode::new(UiStyle::absolute(0.0, 0.0, 20.0, 20.0)).with_mesh(mesh.clone()),
        )
    };
    for root in [repeated(), hidden(), rejected()] {
        assert!(matches!(
            UiAuthoringSnapshotBuilder::new(1)
                .with_limits(UiLimits::default().with_max_mesh_vertices(5).unwrap())
                .with_root(root.clone())
                .build(),
            Err(UiAuthoringError::MeshVertexLimitExceeded {
                actual: 6,
                maximum: 5
            }),
        ));
        assert!(matches!(
            UiAuthoringSnapshotBuilder::new(1)
                .with_limits(UiLimits::default().with_max_mesh_indices(5).unwrap())
                .with_root(root)
                .build(),
            Err(UiAuthoringError::MeshIndexLimitExceeded {
                actual: 6,
                maximum: 5
            }),
        ));
    }
    let source = snapshot(2, UiLimits::default(), [hidden(), rejected()]);
    assert_eq!(source.roots().len(), 1);
    let frame = compiler(UiLimits::default())
        .compile(&source, environment(), UiTheme::default())
        .unwrap();
    assert_eq!(frame.root_outcomes()[0].status(), UiRootStatus::Published);
    assert_eq!(frame.root_outcomes()[1].status(), UiRootStatus::Absent);
    assert!(frame.paint().is_empty());
}

#[test]
fn source_mesh_budgets_are_aggregate_across_roots_even_with_shared_allocations() {
    let mesh = triangle();
    for (limits, vertices) in [
        (UiLimits::default().with_max_mesh_vertices(5).unwrap(), true),
        (UiLimits::default().with_max_mesh_indices(5).unwrap(), false),
    ] {
        let result = UiAuthoringSnapshotBuilder::new(1)
            .with_limits(limits)
            .with_root(root("one").with_mesh(mesh.clone()))
            .with_root(invalid_root("two").with_mesh(mesh.clone()))
            .build();
        assert!(matches!(
            (result, vertices),
            (
                Err(UiAuthoringError::MeshVertexLimitExceeded {
                    actual: 6,
                    maximum: 5
                }),
                true
            ) | (
                Err(UiAuthoringError::MeshIndexLimitExceeded {
                    actual: 6,
                    maximum: 5
                }),
                false
            )
        ));
    }
}

#[test]
fn mesh_admission_limits_travel_with_the_snapshot_profile_and_refusal_uses_no_revision() {
    for strict in [
        UiLimits::default().with_max_mesh_vertices(3).unwrap(),
        UiLimits::default().with_max_mesh_indices(3).unwrap(),
    ] {
        for (authored, compiled) in [(strict, UiLimits::default()), (UiLimits::default(), strict)] {
            let mut compiler = compiler(compiled);
            let foreign = snapshot(1, authored, [root("profile").with_mesh(triangle())]);
            assert!(matches!(
                compiler.compile(&foreign, environment(), UiTheme::default()),
                Err(UiCompileError::SnapshotProfileMismatch)
            ));
            assert_eq!(compiler.work_stats().root_compile_attempts, 0);
            assert_eq!(compiler.work_stats().published_frames, 0);
            let matching = snapshot(2, compiled, [root("profile").with_mesh(triangle())]);
            let frame = compiler
                .compile(&matching, environment(), UiTheme::default())
                .unwrap();
            assert_eq!(frame.revision().get(), 1);
            assert!(matches!(frame.paint()[0], UiPaintPrimitive::Mesh { .. }));
        }
    }
}

#[test]
fn fallback_output_counts_shared_occurrences_and_rejection_preserves_the_old_cache() {
    let shared = triangle();
    for (limits, vertex_refusal) in [
        (
            UiLimits::default()
                .with_max_mesh_vertices(6)
                .unwrap()
                .with_max_mesh_indices(9)
                .unwrap(),
            true,
        ),
        (
            UiLimits::default()
                .with_max_mesh_vertices(7)
                .unwrap()
                .with_max_mesh_indices(6)
                .unwrap(),
            false,
        ),
    ] {
        let mut compiler = compiler(limits);
        let baseline = snapshot(
            1,
            limits,
            [
                root("one")
                    .with_mesh(shared.clone())
                    .with_interaction(UiInteraction::focus_target()),
                root("two")
                    .with_mesh(shared.clone())
                    .with_interaction(UiInteraction::focus_target()),
            ],
        );
        let first = compiler
            .compile(&baseline, environment(), UiTheme::default())
            .unwrap();
        assert_eq!(first.paint().len(), 2);
        assert_eq!(first.hit_regions().len(), 2);
        let too_large = snapshot(
            2,
            limits,
            [invalid_root("one"), root("two").with_mesh(square())],
        );
        let result = compiler.compile(&too_large, environment(), UiTheme::default());
        assert!(matches!(
            (result, vertex_refusal),
            (
                Err(UiCompileError::MeshVertexLimitExceeded {
                    actual: 7,
                    maximum: 6
                }),
                true
            ) | (
                Err(UiCompileError::MeshIndexLimitExceeded {
                    actual: 9,
                    maximum: 6
                }),
                false
            )
        ));
        assert_eq!(compiler.work_stats().published_frames, 1);
        let recovery = snapshot(3, limits, [invalid_root("one"), invalid_root("two")]);
        let retained = compiler
            .compile(&recovery, environment(), UiTheme::default())
            .unwrap();
        assert_eq!(retained.revision().get(), 2);
        assert!(
            retained
                .root_outcomes()
                .iter()
                .all(|outcome| outcome.status() == UiRootStatus::RetainedVisual)
        );
        assert_eq!(retained.paint(), first.paint());
        assert!(retained.hit_regions().is_empty());
        assert!(retained.focus_order().is_empty());
        assert!(retained.semantics().is_empty());
    }
}

#[test]
fn recomposed_geometry_and_content_boxes_replace_old_facts_without_mutating_old_frames() {
    let limits = UiLimits::default();
    let mut compiler = compiler(limits);
    let first_source = snapshot(
        1,
        limits,
        [root("edit").with_child(
            UiNode::new(UiStyle::absolute(2.25, 3.5, 40.0, 30.0))
                .with_key(key("curve"))
                .with_mesh(triangle()),
        )],
    );
    let first = compiler
        .compile(&first_source, environment(), UiTheme::default())
        .unwrap();
    let changed = square();
    let next_source = snapshot(
        2,
        limits,
        [root("edit").with_child(
            UiNode::new(
                UiStyle::absolute(12.75, 7.25, 50.0, 40.0).with_padding(UiEdges::all(2.5).unwrap()),
            )
            .with_key(key("curve"))
            .with_mesh(changed.clone()),
        )],
    );
    let next = compiler
        .compile(&next_source, environment(), UiTheme::default())
        .unwrap();
    let UiPaintPrimitive::Mesh {
        rect: new_rect,
        mesh: captured,
        ..
    } = &next.paint()[0]
    else {
        panic!("expected mesh")
    };
    assert_eq!(*new_rect, rect(15.25, 9.75, 45.0, 35.0));
    assert_eq!(captured, &changed);
    assert_eq!(first.paint()[0].rect(), rect(2.25, 3.5, 40.0, 30.0));
    assert!(
        matches!(&first.paint()[0], UiPaintPrimitive::Mesh { mesh, .. } if mesh.vertices().len() == 3)
    );
    let rejected = snapshot(3, limits, [invalid_root("edit")]);
    let retained = compiler
        .compile(&rejected, environment(), UiTheme::default())
        .unwrap();
    assert_eq!(retained.paint(), next.paint());

    let resized_environment =
        UiEnvironment::new(UiSurfaceId::new(90).unwrap(), 100.0, 80.0, 1.5, 1, 1, 1, 1).unwrap();
    let absent = compiler
        .compile(&rejected, resized_environment, UiTheme::default())
        .unwrap();
    assert_eq!(absent.root_outcomes()[0].status(), UiRootStatus::Absent);
    assert!(absent.paint().is_empty());
}

#[test]
fn mesh_without_an_inherited_clip_still_cannot_paint_padding_or_outside_content() {
    let source = snapshot(
        1,
        UiLimits::default(),
        [root("own-clip").with_child(
            UiNode::new(
                UiStyle::absolute(10.0, 10.0, 50.0, 40.0).with_padding(UiEdges::all(5.0).unwrap()),
            )
            .with_mesh(triangle()),
        )],
    );
    let frame = compiler(UiLimits::default())
        .compile(&source, environment(), UiTheme::default())
        .unwrap();
    assert_eq!(frame.paint()[0].rect(), rect(15.0, 15.0, 40.0, 30.0));
    assert_eq!(
        frame.paint()[0].clip_rect(),
        Some(rect(15.0, 15.0, 40.0, 30.0))
    );
    assert!(frame.hit_regions().is_empty());
}

#[test]
fn public_local_mesh_bounds_enclose_small_edges_after_cancellation() {
    let color = UiColor::rgba(1.0, 0.0, 0.0, 1.0);
    let points = [
        UiPoint::new(-16_384.0, -16_384.0),
        UiPoint::new(0.001, 0.001),
        UiPoint::new(8.0, 0.001),
    ];
    let mesh = UiMesh::new(
        points
            .into_iter()
            .map(|point| UiMeshVertex::new(point, color))
            .collect(),
        vec![0, 1, 2],
    )
    .unwrap();
    let bounds = mesh.bounds();
    for point in points {
        assert!(bounds.origin().x() <= point.x() && bounds.max().x() >= point.x());
        assert!(bounds.origin().y() <= point.y() && bounds.max().y() >= point.y());
    }
}

#[test]
fn fully_clipped_geometry_is_admitted_as_source_but_produces_no_mesh_occurrence() {
    let mesh = triangle();
    let clipped = || {
        root("clipped").with_child(
            UiNode::new(UiStyle::absolute(0.0, 0.0, 20.0, 20.0).with_overflow(UiOverflow::Clip))
                .with_child(
                    UiNode::new(UiStyle::absolute(30.0, 30.0, 20.0, 20.0)).with_mesh(mesh.clone()),
                ),
        )
    };
    let limits = UiLimits::default().with_max_mesh_vertices(5).unwrap();
    assert!(matches!(
        UiAuthoringSnapshotBuilder::new(1)
            .with_limits(limits)
            .with_root(clipped())
            .with_root(root("visible").with_mesh(mesh.clone()))
            .build(),
        Err(UiAuthoringError::MeshVertexLimitExceeded {
            actual: 6,
            maximum: 5
        }),
    ));
    let source = snapshot(2, limits, [clipped()]);
    let frame = compiler(limits)
        .compile(&source, environment(), UiTheme::default())
        .unwrap();
    assert_eq!(frame.root_outcomes()[0].status(), UiRootStatus::Published);
    assert!(frame.paint().is_empty());
}
