use super::*;

fn r_document(name: &str) -> PrefabDocument {
    let marker = format!("{name}_marker");
    let mut document = PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("r")),
            child("r_reader", "r"),
            child(&marker, "r"),
        ],
        [
            SceneEntityRecord::new(id("r")),
            reader("r_reader", input_reference(&[])),
            SceneEntityRecord::new(id(&marker)),
        ],
    );
    document.inputs.insert(input());
    document
}

fn q_document(name: &str) -> PrefabDocument {
    let mut q = SceneEntityRecord::new(id("q"));
    q.prefab = Some(with_argument(
        root_instance("r", "r"),
        "r",
        local("q_socket"),
    ));
    let marker = format!("{name}_marker");
    let mut document = PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("q")),
            child("q_socket", "q"),
            child("q_reader", "q"),
            child(&marker, "q"),
        ],
        [
            q,
            SceneEntityRecord::new(id("q_socket")),
            reader("q_reader", input_reference(&[])),
            SceneEntityRecord::new(id(&marker)),
        ],
    );
    document.inputs.insert(input());
    document
}

fn p_document() -> PrefabDocument {
    let mut k = reader("k", input_reference(&[]));
    k.prefab = Some(with_argument(
        root_instance("q", "q"),
        "q",
        local("p_socket"),
    ));
    let mut document = PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("k")),
            child("p_socket", "k"),
            child("p_reader", "k"),
            child("via_r", "k"),
        ],
        [
            k,
            SceneEntityRecord::new(id("p_socket")),
            reader("p_reader", input_reference(&[])),
            reader("via_r", local("p_socket")),
        ],
    );
    document.inputs.insert(input());
    document
}

fn sources() -> InMemoryPrefabSourceResolver {
    InMemoryPrefabSourceResolver::new()
        .with_prefab(source("p"), key("p"), p_document())
        .with_prefab(source("q"), key("q"), q_document("q"))
        .with_prefab(
            source("q_changed"),
            key("q_changed"),
            q_document("q_changed"),
        )
        .with_prefab(source("r"), key("r"), r_document("r"))
        .with_prefab(
            source("r_changed"),
            key("r_changed"),
            r_document("r_changed"),
        )
}

fn scene(q_source: &str, r_source: &str) -> SceneDocument {
    let mut records = vec![
        SceneEntityRecord::new(id("scene_socket")),
        // Equal local names must not capture an occurrence's input argument.
        SceneEntityRecord::new(id("p_socket")),
        SceneEntityRecord::new(id("q_socket")),
    ];
    for anchor in ["left", "right"] {
        let mut overlay = PrefabOverlay::new(key("p"));
        if q_source != "q" {
            overlay.instances.push(PrefabInstanceClaim {
                target: target(&[], "k"),
                value: Some(with_argument(
                    root_instance(q_source, "q"),
                    q_source,
                    reference(&[(anchor, "p")], "p_socket"),
                )),
            });
        }
        if r_source != "r" {
            overlay.instances.push(PrefabInstanceClaim {
                target: target(&[("k", q_source)], "q"),
                value: Some(with_argument(
                    root_instance(r_source, "r"),
                    r_source,
                    reference(&[(anchor, "p"), ("k", q_source)], "q_socket"),
                )),
            });
        }
        // Targets start in P; payload routes start in the containing Scene. The root
        // component belongs to one final entity, but its explicit Q.u must still enter Q.
        overlay.components.push(pin_link(
            target(&[("k", q_source)], "q"),
            input_reference(&[(anchor, "p"), ("k", q_source)]),
        ));
        overlay.components.push(pin_link(
            target(&[], "via_r"),
            input_reference(&[(anchor, "p"), ("k", q_source), ("q", r_source)]),
        ));
        let mut instance = with_argument(root_instance("p", "k"), "p", local("scene_socket"));
        instance.overrides = Some(overlay);
        let mut record = SceneEntityRecord::new(id(anchor));
        record.prefab = Some(instance);
        records.push(record);
        for (suffix, route, terminal) in [
            ("p_alias", vec![(anchor, "p")], "k"),
            ("q_alias", vec![(anchor, "p"), ("k", q_source)], "q"),
            (
                "r_alias",
                vec![(anchor, "p"), ("k", q_source), ("q", r_source)],
                "r",
            ),
        ] {
            records.push(reader(
                &format!("{anchor}_{suffix}"),
                reference(&route, terminal),
            ));
        }
    }
    SceneDocument::from_ordered_roots(records)
}

fn assert_selected_slots(q_source: &str, r_source: &str) {
    let scene = scene(q_source, r_source);
    let (report, requests) = compile_with_requests(&scene, &sources(), &registry());
    let document = compiled(report);

    assert_eq!(
        requests.len(),
        6,
        "each selected declaration is loaded once per occurrence",
    );
    for name in ["p", "q", "q_changed", "r", "r_changed"] {
        let expected = if ["p", q_source, r_source].contains(&name) {
            2
        } else {
            0
        };
        assert_eq!(
            requests
                .iter()
                .filter(|requested| *requested == &source(name))
                .count(),
            expected,
            "unexpected actual source demand for {name}",
        );
    }
    assert_eq!(
        document.entities().len(),
        27,
        "bound roots must not add wrappers",
    );
    assert_eq!(document.canonical_provenance().len(), 27);
    assert_eq!(
        document.roots(),
        &[
            id("scene_socket"),
            id("p_socket"),
            id("q_socket"),
            id("left"),
            id("left_p_alias"),
            id("left_q_alias"),
            id("left_r_alias"),
            id("right"),
            id("right_p_alias"),
            id("right_q_alias"),
            id("right_r_alias"),
        ],
    );
    let mut previous_sockets = None;
    for anchor in ["left", "right"] {
        let p_socket = projected(&document, anchor, &[("p", "p_socket")]);
        let p_reader = projected(&document, anchor, &[("p", "p_reader")]);
        let via_r = projected(&document, anchor, &[("p", "via_r")]);
        let q_socket = projected(&document, anchor, &[("p", "k"), (q_source, "q_socket")]);
        let q_reader = projected(&document, anchor, &[("p", "k"), (q_source, "q_reader")]);
        let q_marker = projected(
            &document,
            anchor,
            &[("p", "k"), (q_source, &format!("{q_source}_marker"))],
        );
        let r_reader = projected(
            &document,
            anchor,
            &[("p", "k"), (q_source, "q"), (r_source, "r_reader")],
        );
        let r_marker = projected(
            &document,
            anchor,
            &[
                ("p", "k"),
                (q_source, "q"),
                (r_source, &format!("{r_source}_marker")),
            ],
        );
        let children = [
            r_reader.clone(),
            r_marker,
            q_socket.clone(),
            q_reader.clone(),
            q_marker,
            p_socket.clone(),
            p_reader.clone(),
            via_r.clone(),
        ];
        assert_eq!(entity(&document, &id(anchor)).children(), &children);
        for child in &children {
            assert!(entity(&document, child).children().is_empty());
        }
        assert_link(&document, &id(anchor), &p_socket);
        assert_link(&document, &p_reader, &id("scene_socket"));
        assert_link(&document, &q_reader, &p_socket);
        assert_link(&document, &r_reader, &q_socket);
        assert_link(&document, &via_r, &q_socket);
        for suffix in ["p_alias", "q_alias", "r_alias"] {
            assert_link(&document, &id(&format!("{anchor}_{suffix}")), &id(anchor));
        }
        let provenance = &document.canonical_provenance()[&id(anchor)];
        assert!(provenance.projection_path().is_none());
        assert_eq!(provenance.local_entity_id(), &id(anchor));
        assert_ne!(p_socket, id("p_socket"));
        assert_ne!(q_socket, id("q_socket"));
        assert_ne!(p_socket, q_socket);
        if let Some((previous_p, previous_q)) = previous_sockets {
            assert_ne!(p_socket, previous_p);
            assert_ne!(q_socket, previous_q);
        }
        previous_sockets = Some((p_socket, q_socket));
    }
}

#[test]
fn merged_root_facets_preserve_both_inherited_instance_slots() {
    assert_selected_slots("q", "r");
}

#[test]
fn outer_instance_claim_replaces_only_the_containing_root_facet_slot() {
    assert_selected_slots("q_changed", "r");
}

#[test]
fn outer_instance_claim_replaces_only_the_nested_root_facet_slot() {
    assert_selected_slots("q", "r_changed");
}

#[test]
fn outer_instance_claims_can_select_both_aliased_slots_in_one_overlay() {
    assert_selected_slots("q_changed", "r_changed");
}
