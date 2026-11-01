use super::*;
use crate::{WgpuStockBindings, WgpuStockCapture, WgpuStockStages, WgpuWorldColor};

struct ComposingPolicy(WgpuStockStages);

#[derive(Clone, Copy)]
enum Construction {
    Branch,
    MissingProducer,
    WrongExtent,
    MissingCanvas,
    ForeignStages,
    OutputLimit,
    DisabledEffect,
}

struct Capture {
    stock: WgpuStockCapture,
    construction: Construction,
}

impl WgpuRenderPolicy for ComposingPolicy {
    type Capture = Capture;
    type Bindings = WgpuStockBindings;

    fn device_requirements(&self) -> WgpuDeviceRequirements {
        self.0.device_requirements()
    }

    fn bind(
        &self,
        _: &Capture,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Self::Bindings, WgpuRenderProgramError> {
        self.0.bind(context)
    }

    fn build(
        &self,
        bindings: &Self::Bindings,
        capture: &Capture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        let mut limits = GraphLimits::default();
        if matches!(
            capture.construction,
            Construction::OutputLimit | Construction::DisabledEffect
        ) {
            limits.max_outputs = 1;
        }
        let foreign = WgpuStockStages::new();
        let stages = if matches!(capture.construction, Construction::ForeignStages) {
            &foreign
        } else {
            &self.0
        };
        let mut stock = stages.begin(bindings, &capture.stock, context, limits)?;
        let original = stock.world(90)?;
        if matches!(capture.construction, Construction::DisabledEffect) {
            return stock.finish(100, &original, &[original.output()]);
        }
        if matches!(capture.construction, Construction::MissingCanvas) {
            stock.resolve_canvas(91, &original)?;
        }
        let mut descriptor = original.descriptor().clone();
        if matches!(capture.construction, Construction::WrongExtent) {
            descriptor.native.size.width += 1;
        }
        let selected = stock.graph().texture::<WgpuWorldColor>(descriptor)?;
        if !matches!(
            capture.construction,
            Construction::MissingProducer | Construction::WrongExtent
        ) {
            stock.graph().initialize(
                10,
                &[selected.preserve(&original, AccessRole::CopyDestination)],
            )?;
        }
        stock.finish(100, &selected, &[original.output()])
    }
}

fn input(construction: Construction) -> WgpuRenderInput {
    let installation = WgpuPolicyDefinition::new(ComposingPolicy(WgpuStockStages::new()))
        .install()
        .unwrap();
    installation
        .capture(
            image_packet(1),
            Arc::new(Capture {
                stock: WgpuStockCapture::new(),
                construction,
            }),
        )
        .unwrap()
}

#[test]
fn shared_stock_world_branches_to_exact_original_and_selected_outputs() {
    let input = input(Construction::Branch);
    let mut cpu = CpuInstanceSet::new(1);
    let built = cpu.build(&input).unwrap();
    let graph = &built.output.program.0.graph;
    assert_eq!(graph.outputs().len(), 2);
    assert_ne!(graph.outputs()[0], graph.outputs()[1]);
    assert_eq!(built.output.final_version, graph.outputs()[1]);
    let order: Vec<_> = graph.operations().map(|operation| operation.key).collect();
    assert_eq!(order, [90, 10]);
    assert_eq!(built.output.facts.frame.draw_calls, 0);
}

#[test]
fn shared_stock_finish_keeps_graph_producer_and_output_limits() {
    let mut cpu = CpuInstanceSet::new(1);
    assert!(matches!(
        cpu.build(&input(Construction::MissingProducer)),
        Err(WgpuRenderProgramError::Graph(
            crate::GraphError::MissingProducer(_)
        ))
    ));
    assert!(matches!(
        cpu.build(&input(Construction::OutputLimit)),
        Err(WgpuRenderProgramError::Graph(
            crate::GraphError::LimitExceeded(crate::LimitKind::Outputs)
        ))
    ));
}

#[test]
fn disabling_an_effect_keeps_one_exact_before_after_version() {
    let mut cpu = CpuInstanceSet::new(1);
    let built = cpu.build(&input(Construction::DisabledEffect)).unwrap();
    assert_eq!(built.output.program.0.graph.outputs().len(), 1);
    assert_eq!(
        built.output.program.0.graph.outputs()[0],
        built.output.final_version
    );
}

#[test]
fn shared_stock_does_not_invent_a_canvas_or_silently_resize_the_selected_scene() {
    let mut cpu = CpuInstanceSet::new(1);
    for construction in [
        Construction::MissingCanvas,
        Construction::WrongExtent,
        Construction::ForeignStages,
    ] {
        assert!(matches!(
            cpu.build(&input(construction)),
            Err(WgpuRenderProgramError::Feature { .. })
        ));
    }
}

#[test]
fn stock_adapter_keeps_the_original_custom_capture_after_its_owner_retires() {
    let installation = WgpuPolicyDefinition::new(ComposingPolicy(WgpuStockStages::new()))
        .install()
        .unwrap();
    let adapter = crate::WgpuStockCaptureAdapter::new(installation, |stock| {
        Ok(Arc::new(Capture {
            stock,
            construction: Construction::Branch,
        }))
    });
    let original = adapter
        .capture(image_packet(7), WgpuStockCapture::new())
        .unwrap();
    assert!(original.captured::<ComposingPolicy>().is_some());
    assert!(original.captured::<crate::WgpuStockPolicy>().is_none());
    drop(adapter);
    let mut cpu = CpuInstanceSet::new(1);
    assert_eq!(
        cpu.build(&original).unwrap().output.program.frame_index(),
        7
    );
}

#[test]
fn stock_adapter_reports_mapper_failure_and_does_not_call_it_after_close() {
    let definition = WgpuPolicyDefinition::new(ComposingPolicy(WgpuStockStages::new()));
    let mut installation = definition.install().unwrap();
    installation.close();
    let closed = crate::WgpuStockCaptureAdapter::new(installation, |_| {
        panic!("closed admission cannot call the capture mapper")
    });
    assert!(matches!(
        closed.capture(image_packet(1), WgpuStockCapture::new()),
        Err(WgpuRenderProgramError::PolicyClosed)
    ));
    let panicked = crate::WgpuStockCaptureAdapter::new(definition.install().unwrap(), |_| {
        panic!("requested capture mapper panic")
    });
    assert!(matches!(
        panicked.capture(image_packet(1), WgpuStockCapture::new()),
        Err(WgpuRenderProgramError::PolicyPanicked)
    ));
}
