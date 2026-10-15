//! A second, CPU-backed toolkit adapter using the actual Host preview and presentation owner.
//! It owns target allocation and display completion; it has no native toolkit/backend dependency.

use std::{
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    time::{Duration, Instant},
};

use nara::{
    project_host::{
        EditorAcceptedPreview, EditorPresentationImage, EditorPresentedToken,
        EditorPreviewCandidate, EditorPreviewPresentation, EditorProjectSession,
        TrustedCompiledEditorProjectIntent,
    },
    render::{
        DisplayMapping2d, DisplayRect, DisplayScale, Extent2d, RenderFrameSourceGeneration,
        RenderFrameTarget, RenderImagePacket,
    },
    scene::{SceneAuthoringRevision, SceneEntityId, ScenePlacement, ScenePlacementOwner},
    tooling::{
        EditorAction, EditorActionOutcome, EditorDocumentId, EditorDocumentMutation,
        SceneCompositionCommand, SceneCompositionEntityKind,
    },
};

use super::{TestProject, editor_driver};

struct CpuTarget {
    image: RenderImagePacket,
    generation: u64,
    /// One coverage byte per physical pixel, sufficient for this colored-sprite consumer.
    pixels: Vec<u8>,
}

struct DisplayCommand {
    accepted: EditorAcceptedPreview,
    image: EditorPresentationImage,
    mapping: DisplayMapping2d,
    pixels: Vec<u8>,
}

struct FakeToolkit {
    presentation: EditorPreviewPresentation,
    target: Option<CpuTarget>,
    next_target_generation: u64,
    device_epoch: u64,
    source: Option<RenderFrameSourceGeneration>,
    source_handoff: u64,
    displayed: Option<(
        EditorDocumentId,
        SceneAuthoringRevision,
        EditorPresentationImage,
        DisplayMapping2d,
    )>,
    displayed_pixels: Vec<u8>,
}

impl FakeToolkit {
    fn new() -> Self {
        Self {
            presentation: EditorPreviewPresentation::new(),
            target: None,
            next_target_generation: 0,
            device_epoch: 1,
            source: None,
            source_handoff: 0,
            displayed: None,
            displayed_pixels: Vec::new(),
        }
    }

    fn submit(
        &mut self,
        candidate: EditorPreviewCandidate,
        mapping: DisplayMapping2d,
    ) -> DisplayCommand {
        let (accepted, packet, batches) = self.presentation.accept(candidate).unwrap();
        let RenderFrameTarget::Image(image) = packet.target() else {
            panic!("the Host preview owns an image target")
        };
        if self
            .target
            .as_ref()
            .is_none_or(|target| target.image != image)
        {
            self.next_target_generation += 1;
            let extent = image.extent();
            self.target = Some(CpuTarget {
                image,
                generation: self.next_target_generation,
                pixels: vec![0; extent.width as usize * extent.height as usize],
            });
        }
        if self.source != Some(packet.source_generation()) {
            self.source = Some(packet.source_generation());
            self.source_handoff += 1;
        }
        let target = self.target.as_mut().unwrap();
        target.pixels.fill(0);
        // Draw the immutable projected quads into the target, rather than deriving fake hits.
        for (view_index, view) in packet.views().iter().enumerate() {
            let viewport = view.viewport;
            for batch in batches.for_view(view_index) {
                assert!(
                    batch.image.is_none(),
                    "this consumer deliberately renders colored sprites"
                );
                for sprite in &batch.instances {
                    let determinant = sprite.x_axis.perp_dot(sprite.y_axis);
                    for y in 0..viewport.physical_height {
                        for x in 0..viewport.physical_width {
                            let clip = nara::prelude::Vec2::new(
                                (x as f32 + 0.5) / viewport.physical_width as f32 * 2.0 - 1.0,
                                1.0 - (y as f32 + 0.5) / viewport.physical_height as f32 * 2.0,
                            );
                            let delta = clip - sprite.center;
                            if determinant.abs() > f32::EPSILON
                                && (delta.perp_dot(sprite.y_axis) / determinant).abs() <= 1.0
                                && (sprite.x_axis.perp_dot(delta) / determinant).abs() <= 1.0
                            {
                                let index = (y + viewport.physical_y) as usize
                                    * image.extent().width as usize
                                    + (x + viewport.physical_x) as usize;
                                target.pixels[index] = 1;
                            }
                        }
                    }
                }
            }
        }
        DisplayCommand {
            accepted,
            mapping,
            pixels: target.pixels.clone(),
            image: EditorPresentationImage {
                target: image,
                target_generation: target.generation,
                source_generation: packet.source_generation(),
                source_handoff_generation: self.source_handoff,
                backend_instance_id: 1,
                device_epoch: self.device_epoch,
                frame_index: packet.frame_index(),
            },
        }
    }

    fn finish(&mut self, command: DisplayCommand) -> EditorPresentedToken {
        let context = (
            command.accepted.document(),
            command.accepted.revision(),
            command.image,
            command.mapping,
        );
        let token = self
            .presentation
            .finish(command.accepted, command.image, command.mapping)
            .unwrap();
        self.displayed = Some(context);
        self.displayed_pixels = command.pixels;
        token
    }

    fn current_token(&mut self) -> Option<EditorPresentedToken> {
        let (document, revision, image, mapping) = self.displayed?;
        self.presentation.token(document, revision, image, mapping)
    }
}

fn mapping(extent: Extent2d) -> DisplayMapping2d {
    DisplayMapping2d::fit(
        extent,
        DisplayRect {
            min: [10.0, 20.0],
            max: [510.0, 320.0],
        },
        2.0,
        DisplayScale::IntegerFit,
    )
    .unwrap()
}

fn candidate(editor: &mut EditorProjectSession) -> EditorPreviewCandidate {
    editor.drive_editor_frame(Duration::ZERO);
    editor
        .take_editor_preview_candidate()
        .expect("the real Host produces an immutable preview")
}

fn add_sprite(editor: &mut EditorProjectSession) -> (EditorDocumentId, SceneEntityId) {
    let frame = editor.current_frame();
    let document = frame.workspace().active_document.unwrap();
    let base_revision = frame.workspace().scene(document).unwrap().revision;
    let outcomes = editor_driver::dispatch(
        editor,
        [EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
            document,
            base_revision,
            command: SceneCompositionCommand::CreateEntity {
                kind: SceneCompositionEntityKind::ColoredSprite,
                placement: ScenePlacement::last(ScenePlacementOwner::Root),
            },
        })],
    );
    assert!(matches!(&outcomes[0], EditorActionOutcome::Workspace(report) if report.applied));
    (document, SceneEntityId::new("editor_entity_1").unwrap())
}

fn close_editor(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let progress = editor.advance_close();
        if matches!(
            progress,
            nara::project_host::EditorProjectCloseProgress::Complete(_)
        ) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the fake toolkit fixture did not close physically: {progress:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn fake_toolkit_picks_only_finished_host_preview_and_preserves_last_display() {
    let project = TestProject::new("fake-toolkit-preview");
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.capability(),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let extent = Extent2d::new(320, 180).unwrap();
        editor.configure_editor_preview(Some(extent));
        let (_, entity) = add_sprite(&mut editor);
        let mut toolkit = FakeToolkit::new();
        let mapping = mapping(extent);
        let command = toolkit.submit(candidate(&mut editor), mapping);
        assert_eq!(
            toolkit.current_token(),
            None,
            "submission alone cannot authorize input"
        );
        assert_eq!(toolkit.target.as_ref().unwrap().pixels[90 * 320 + 160], 1);
        let token = toolkit.finish(command);
        assert_eq!(toolkit.displayed_pixels[90 * 320 + 160], 1);
        assert_eq!(
            toolkit.presentation.pick(token, [260.0, 170.0]).unwrap(),
            Some(entity)
        );
        assert!(
            toolkit.presentation.pick(token, [15.0, 170.0]).is_err(),
            "letterbox bars are not image input"
        );

        let unpresented = toolkit.submit(candidate(&mut editor), mapping);
        assert_eq!(
            toolkit.current_token(),
            Some(token),
            "a new submission does not replace the displayed frame"
        );
        drop(unpresented);
        assert_eq!(toolkit.displayed_pixels[90 * 320 + 160], 1);
        assert!(
            toolkit
                .presentation
                .pick(token, [260.0, 170.0])
                .unwrap()
                .is_some()
        );
    }));
    close_editor(&mut editor);
    if let Err(error) = outcome {
        resume_unwind(error);
    }
}

#[test]
fn fake_toolkit_revokes_old_geometry_for_independent_context_changes() {
    let project = TestProject::new("fake-toolkit-invalidation");
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.capability(),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let extent = Extent2d::new(320, 180).unwrap();
        editor.configure_editor_preview(Some(extent));
        add_sprite(&mut editor);
        let mut toolkit = FakeToolkit::new();

        for change in 0..4 {
            let command = toolkit.submit(candidate(&mut editor), mapping(extent));
            let token = toolkit.finish(command);
            let (document, revision, mut image, mut next_mapping) = toolkit.displayed.unwrap();
            let mut next_revision = revision;
            match change {
                0 => {
                    add_sprite(&mut editor);
                    // A dispatch preserves its published immutable frame. Observe the accepted
                    // revision through the next actual Host preview, just as an adapter does.
                    next_revision = candidate(&mut editor).revision();
                    assert_ne!(next_revision, revision);
                }
                1 => {
                    next_mapping = DisplayMapping2d::fit(
                        extent,
                        DisplayRect {
                            min: [20.0, 20.0],
                            max: [520.0, 320.0],
                        },
                        2.0,
                        DisplayScale::IntegerFit,
                    )
                    .unwrap()
                }
                2 => {
                    editor.configure_editor_preview(Extent2d::new(160, 90));
                    let resized = toolkit.submit(
                        candidate(&mut editor),
                        mapping(Extent2d::new(160, 90).unwrap()),
                    );
                    image = resized.image;
                }
                3 => {
                    toolkit.device_epoch += 1;
                    image.device_epoch = toolkit.device_epoch;
                }
                _ => unreachable!(),
            }
            assert!(
                toolkit
                    .presentation
                    .token(document, next_revision, image, next_mapping)
                    .is_none()
            );
            assert!(toolkit.presentation.pick(token, [260.0, 170.0]).is_err());
            // Restore the physical target for the next independent invalidation case.
            editor.configure_editor_preview(Some(extent));
        }
    }));
    close_editor(&mut editor);
    if let Err(error) = outcome {
        resume_unwind(error);
    }
}
