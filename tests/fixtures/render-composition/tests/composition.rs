//! Private resource model checks; native workloads join the same graph in this comparison.

#![forbid(unsafe_code)]

#[path = "support/graph.rs"]
mod graph;
#[path = "support/graph_tests.rs"]
mod graph_tests;
#[path = "support/history.rs"]
mod history;
#[path = "support/history_tests.rs"]
mod history_tests;
#[path = "support/workload.rs"]
mod workload;

#[test]
fn shared_image_and_buffer_workload_has_the_same_dependencies_under_declaration_permutations() {
    let mut reverse = workload::OPERATION_KEYS;
    reverse.reverse();
    let shuffled = [
        workload::HISTOGRAM,
        workload::HUD,
        workload::NORMALIZE,
        workload::BLUR_VERTICAL,
        workload::LUMINANCE,
        workload::COMPLEMENT,
        workload::BLUR_HORIZONTAL,
    ];
    let mut admitted = Vec::new();
    for order in [&workload::OPERATION_KEYS, &reverse, &shuffled] {
        let workload = workload::build(order).unwrap();
        let keys: Vec<_> = workload
            .graph
            .operations()
            .map(|operation| operation.key)
            .collect();
        let copies: Vec<_> = workload.graph.copy_dependencies().collect();
        assert_eq!(copies.len(), 1);
        assert_eq!(copies[0].source, workload.versions.raw);
        assert_eq!(copies[0].destination, workload.versions.normalized);
        assert_eq!(copies[0].before, workload::NORMALIZE);
        assert_eq!(workload.graph.outputs().len(), 8);
        admitted.push(keys);
    }
    assert_eq!(admitted[0], admitted[1]);
    assert_eq!(admitted[0], admitted[2]);
}
