//! Host capture follows selected declarations; source inventory remains lossless.

use std::convert::Infallible;

use nara_scene::{
    __private::{
        AuthoringAdapterReceipt, AuthoringPrefabExpansionReport, AuthoringPrefabOmissionReason,
        try_expand_authoring_prefabs_with_loader,
    },
    AuthoringRecordReadiness, UnavailableRecordManifestEntry,
};

use super::*;

fn compile_snapshot(snapshot: &AuthoringProjectSnapshot) -> AuthoringPrefabExpansionReport {
    let sources = snapshot
        .prefabs()
        .iter()
        .map(|prefab| {
            (
                AssetRef::Path(prefab.path().clone()),
                prefab.resolved_source(),
            )
        })
        .collect::<Vec<_>>();
    try_expand_authoring_prefabs_with_loader(
        snapshot.startup_scene().document(),
        &snapshot.startup_scene().schema_basis(),
        Default::default(),
        None,
        |reference, _| {
            Ok::<_, Infallible>(
                sources
                    .iter()
                    .find_map(|(source, resolved)| (source == reference).then(|| resolved.clone())),
            )
        },
    )
    .unwrap()
}

fn entity(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("nara.test.actual-demand").unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.ActualImage")
}

fn image_value(name: &str) -> ComponentValue {
    ComponentValue::map([
        ("kind", ComponentValue::String("path".into())),
        ("value", ComponentValue::String(format!("{name}.png"))),
    ])
}

struct ActualDemandProject {
    project: TestAuthoringProject,
    bundle: AuthoringSchemaBundle,
    scene: SceneDocument,
    p: PrefabDocument,
    r: PrefabDocument,
}

impl ActualDemandProject {
    fn new(bundle: AuthoringSchemaBundle) -> Self {
        let project = TestAuthoringProject::new("selected-authoring-closure", bundle.clone());
        project.write_startup_manifest();
        for directory in ["scenes", "prefabs", "assets"] {
            fs::create_dir_all(project.root.join(directory)).unwrap();
        }
        // A stable reference would index metadata if raw inventory drove dependency capture.
        let suppressed = SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([(
                "image",
                ComponentValue::map([
                    ("kind", ComponentValue::String("stable_id".into())),
                    (
                        "value",
                        ComponentValue::String("88f070af-6f01-4949-b7c4-fb774c339812".into()),
                    ),
                ]),
            )]),
        )
        .with_owner(owner());
        let mut stale = inventory_overlay("q");
        stale.components.push(PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(entity("obsolete-target")),
            component: ComponentTypeId::new("missing.StaleOverlay"),
            version: ComponentSchemaVersion(7),
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("obsolete-field")),
                intent: PrefabPropertyIntent::Set(ComponentValue::String(
                    "preserve opaque intent".into(),
                )),
                bindings: Vec::new(),
            }],
        });
        let mut nested = SceneEntityRecord::new(entity("n"));
        nested.prefab = Some(PrefabInstance {
            source: AssetRef::path("q.prefab.json").unwrap(),
            overrides: Some(stale),
        });
        let p = PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(entity("p-root")).with_component(component(), suppressed),
            nested,
        ]);
        let r = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(entity("r-root"))]);
        let mut outer = inventory_overlay("p");
        outer.instances.push(PrefabInstanceClaim {
            target: PrefabEntityTarget::source(entity("n")),
            value: Some(PrefabInstance {
                source: AssetRef::path("r.prefab.json").unwrap(),
                overrides: None,
            }),
        });
        outer.components.push(PrefabComponentIntent::Suppress {
            target: PrefabEntityTarget::source(entity("p-root")),
            component: component(),
        });
        let scene = SceneDocument::from_ordered_roots(["first", "second"].map(|name| {
            let mut anchor = SceneEntityRecord::new(entity(name));
            anchor.prefab = Some(PrefabInstance {
                source: AssetRef::path("p.prefab.json").unwrap(),
                overrides: Some(outer.clone()),
            });
            anchor
        }));
        fs::write(
            project.root.join("assets/suppressed.png"),
            b"inactive asset",
        )
        .unwrap();
        fs::write(
            project.root.join("assets/suppressed.png.meta"),
            b"invalid metadata canary",
        )
        .unwrap();
        let fixture = Self {
            project,
            bundle,
            scene,
            p,
            r,
        };
        fixture.write_documents();
        fixture
    }

    fn write_documents(&self) {
        for (path, bytes) in [
            (
                "scenes/startup.scene.json",
                self.scene.to_json_string().unwrap(),
            ),
            ("prefabs/p.prefab.json", self.p.to_json_string().unwrap()),
            ("prefabs/r.prefab.json", self.r.to_json_string().unwrap()),
        ] {
            fs::write(self.project.root.join(path), bytes).unwrap();
        }
    }

    fn prepare(
        &self,
        adapters: Box<[AuthoringAdapterReceipt]>,
        limits: AuthoringProjectAdmissionLimits,
    ) -> PreparedAuthoringProject {
        let schema = AuthoringProjectAdmissionHost::new(limits)
            .prepare(self.project.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        PreparedAuthoringProject::new(schema, adapters)
    }

    fn remove_replacements(&mut self) {
        for anchor in &mut self.scene.entities {
            anchor
                .prefab
                .as_mut()
                .unwrap()
                .overrides
                .as_mut()
                .unwrap()
                .instances
                .clear();
        }
        self.write_documents();
    }
}

#[test]
fn selected_repair_captures_p_and_r_once_without_reading_q_or_suppressed_asset_inventory() {
    let fixture = ActualDemandProject::new(asset_schema_bundle(owner(), component()));
    let mut previous_manifest = None;
    for invalid_q in [false, true] {
        if invalid_q {
            fs::write(
                fixture.project.root.join("prefabs/q.prefab.json"),
                b"invalid unselected Prefab",
            )
            .unwrap();
        }
        let receipt = fixture
            .prepare(Box::new([]), Default::default())
            .admit_startup_closure()
            .unwrap()
            .publish();
        let snapshot = receipt.snapshot();
        assert_eq!(snapshot.readiness(), AuthoringProjectReadiness::Degraded);
        assert_eq!(
            snapshot.startup_scene().readiness(),
            nara_scene::AuthoringDocumentReadiness::Degraded,
        );
        assert!(!snapshot.startup_compilation_diagnostics().has_errors());
        assert_eq!(snapshot.startup_scene().document(), &fixture.scene);
        assert_eq!(
            snapshot
                .prefabs()
                .iter()
                .map(|prefab| prefab.path().as_str())
                .collect::<Vec<_>>(),
            ["p.prefab.json", "r.prefab.json"]
        );
        assert_eq!(
            snapshot
                .source_facts()
                .map(|fact| fact.project_path().as_str())
                .collect::<Vec<_>>(),
            [
                "prefabs/p.prefab.json",
                "prefabs/r.prefab.json",
                "scenes/startup.scene.json"
            ]
        );
        assert!(snapshot.assets().is_empty());
        assert!(snapshot.unresolved_dependencies().is_empty());
        let captured = &snapshot.prefabs()[0];
        assert_eq!(captured.document(), &fixture.p);
        assert_eq!(captured.native_source().key(), &inventory_overlay("p").base);
        assert!(std::ptr::eq(
            captured.resolved_source().document(),
            captured.document(),
        ));
        let published = captured.publish_document().unwrap();
        assert_eq!(
            published.schema_basis(),
            snapshot.startup_scene().schema_basis(),
        );
        assert_eq!(
            published.readiness(),
            nara_scene::AuthoringDocumentReadiness::Degraded,
        );
        let manifest = published.unavailable_manifest();
        assert_eq!(manifest.record_count(), 1);
        assert_eq!(manifest.overlay_target_count(), 1);
        assert!(manifest.entries().iter().any(|entry| matches!(
            entry, UnavailableRecordManifestEntry::OverlayTarget(target)
                if target.readiness() == AuthoringRecordReadiness::UnknownSchema
        )));
        assert_eq!(
            manifest.overlay_declarations().next().unwrap().1,
            fixture
                .p
                .entities
                .iter()
                .find(|record| record.id == entity("n"))
                .unwrap()
                .prefab
                .as_ref()
                .unwrap()
        );
        if let Some(previous) = &previous_manifest {
            assert_eq!(manifest, previous);
        }
        previous_manifest = Some(manifest.clone());
        let compiled = compile_snapshot(&snapshot);
        assert!(compiled.candidate.is_some(), "{:?}", compiled.diagnostics);
        assert!(!compiled.diagnostics.has_errors());
        assert!(compiled.omissions.is_empty());
        assert!(compiled.effective_asset_references().is_empty());
        let expanded = compiled.candidate.unwrap();
        assert_eq!(expanded.document().entities.len(), 8);
        assert!(
            expanded
                .document()
                .entities
                .iter()
                .all(|entity| entity.components.is_empty())
        );
        // Save/Reopen keeps raw declarations and reproduces standalone classification on demand.
        fs::write(
            fixture.project.root.join("prefabs/p.prefab.json"),
            captured.document().to_json_string().unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn effective_and_default_assets_are_captured_with_data_adapters_without_native_authority() {
    let original = asset_schema_bundle(owner(), component());
    let mut schema = original.owners()[0].current().components()[0].clone();
    schema.fields[0].default_value = Some(image_value("default"));
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    for use_default in [false, true] {
        let mut fixture = ActualDemandProject::new(bundle.clone());
        let value = if use_default {
            ComponentValue::Map(Default::default())
        } else {
            ComponentValue::map([("image", image_value("superseded"))])
        };
        fixture.r.entities[0].components.insert(
            component(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, value.clone())
                .with_owner(owner()),
        );
        if !use_default {
            for anchor in &mut fixture.scene.entities {
                let selected = anchor
                    .prefab
                    .as_mut()
                    .unwrap()
                    .overrides
                    .as_mut()
                    .unwrap()
                    .instances[0]
                    .value
                    .as_mut()
                    .unwrap();
                let mut overlay = inventory_overlay("r");
                overlay.components.push(PrefabComponentIntent::Properties {
                    target: PrefabEntityTarget::source(entity("r-root")),
                    component: component(),
                    version: ComponentSchemaVersion::ONE,
                    claims: vec![PrefabPropertyClaim {
                        path: ComponentPropertyPath::new(ComponentFieldId::new("image")),
                        intent: PrefabPropertyIntent::Set(image_value("effective")),
                        bindings: Vec::new(),
                    }],
                });
                selected.overrides = Some(overlay);
            }
        }
        fixture.write_documents();
        fs::write(
            fixture.project.root.join("assets/superseded.png.meta"),
            b"invalid inactive metadata",
        )
        .unwrap();
        let name = if use_default { "default" } else { "effective" };
        fs::write(
            fixture.project.root.join(format!("assets/{name}.png")),
            name.as_bytes(),
        )
        .unwrap();
        fs::write(
            fixture.project.root.join(format!("assets/{name}.png.meta")),
            AssetMeta::new(
                AssetMetaIdentity::new(
                    StableAssetId::parse_str("9660f47f-05fc-49c6-9036-657f8d067d97").unwrap(),
                    AssetPath::new(format!("{name}.png")).unwrap(),
                    AssetSourceKind::Image,
                ),
                (),
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
        let adapter = AuthoringAdapterReceipt::from_schema_owner(
            &fixture.bundle.owners()[0],
            &Default::default(),
        )
        .unwrap();
        assert!(adapter.contribution().is_none());
        let receipt = fixture
            .prepare(vec![adapter].into_boxed_slice(), Default::default())
            .admit_startup_closure()
            .unwrap()
            .publish();
        let snapshot = receipt.snapshot();
        assert_eq!(snapshot.readiness(), AuthoringProjectReadiness::Complete);
        assert!(!snapshot.startup_compilation_diagnostics().has_errors());
        assert!(snapshot.unresolved_dependencies().is_empty());
        assert_eq!(snapshot.prefabs().len(), 2);
        assert_eq!(snapshot.assets().len(), 1);
        let asset = &snapshot.assets()[0];
        assert_eq!(asset.path().as_str(), format!("{name}.png"));
        assert_eq!(asset.bytes(), name.as_bytes());
        assert_eq!(snapshot.source_facts().len(), 5);
        for fact in [asset.source(), asset.metadata_source()] {
            assert!(
                snapshot
                    .source_facts()
                    .any(|captured| std::ptr::eq(fact, captured))
            );
            assert_eq!(
                fact.digest(),
                ContentDigest::of_bytes(
                    &fs::read(fixture.project.root.join(fact.project_path().as_str())).unwrap()
                )
            );
        }
        let r = &snapshot.prefabs()[1];
        assert_eq!(r.document(), &fixture.r);
        let published_r = r.publish_document().unwrap();
        assert_eq!(
            published_r.record_facts()[0].readiness(),
            AuthoringRecordReadiness::Available
        );
        assert_eq!(
            r.document().entities[0].components[&component()].value,
            value
        );
        let compiled = compile_snapshot(&snapshot);
        assert!(compiled.candidate.is_some(), "{:?}", compiled.diagnostics);
        assert!(!compiled.diagnostics.has_errors());
        assert_eq!(compiled.effective_asset_references().len(), 2);
        assert!(
            compiled
                .effective_asset_references()
                .iter()
                .all(|reference| {
                    reference.asset_ref() == &AssetRef::path(format!("{name}.png")).unwrap()
                })
        );
        assert_eq!(compiled.omissions.len(), 2);
        assert!(compiled.omissions.iter().all(|omission| {
            omission.component == component()
                && omission.reason == AuthoringPrefabOmissionReason::NativeBindingUnavailable
        }));
        let (native_preview, omissions) = compiled.into_native_preview().unwrap();
        assert_eq!(omissions.len(), 2);
        assert!(
            native_preview
                .document()
                .entities
                .iter()
                .all(|record| record.components.is_empty())
        );
        let preview =
            nara_scene::__private::project_published_authoring_prefab_preview(&published_r);
        assert_eq!(preview.omissions().len(), 1);
        assert_eq!(
            preview.omissions()[0].reason(),
            nara_scene::__private::AuthoringPreviewOmissionReason::NativeBindingUnavailable
        );
        assert!(preview.document().entities[0].components.is_empty());
        // A demanded asset failure is still a Host error, rather than a degraded success.
        drop(snapshot);
        drop(receipt);
        fs::remove_file(fixture.project.root.join(format!("assets/{name}.png"))).unwrap();
        let adapter = AuthoringAdapterReceipt::from_schema_owner(
            &fixture.bundle.owners()[0],
            &Default::default(),
        )
        .unwrap();
        assert!(matches!(
            fixture.prepare(vec![adapter].into_boxed_slice(), Default::default()).admit_startup_closure(),
            Err(AuthoringProjectSourceAdmissionError::SourceHost {
                kind: AuthoringSourceKind::AssetSource, path, ..
            }) if path.as_str() == format!("assets/{name}.png")
        ));
    }
}

#[test]
fn complete_outer_repair_skips_bad_available_source_values_but_standalone_open_stays_strict() {
    use crate::project_host::authoring::document::{
        AuthoringDocumentDataError, admit_prefab_bytes_with_basis,
    };

    let schema = ComponentSchema::new(component(), "Deferred flag", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("flag"),
            "Flag",
            ComponentFieldPath::from_fields(["flag"]),
            ComponentValueKind::Bool,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    for replace in [false, true] {
        let mut fixture = ActualDemandProject::new(bundle.clone());
        let bad = SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([(
                "flag",
                ComponentValue::String("invalid old boolean; preserve exactly".into()),
            )]),
        )
        .with_owner(owner());
        fixture
            .p
            .entities
            .iter_mut()
            .find(|record| record.id == entity("p-root"))
            .unwrap()
            .components
            .insert(component(), bad.clone());
        if replace {
            for anchor in &mut fixture.scene.entities {
                let overlay = anchor.prefab.as_mut().unwrap().overrides.as_mut().unwrap();
                overlay.components.clear();
                overlay.components.push(PrefabComponentIntent::Set {
                    mode: PrefabComponentSetMode::Replace,
                    target: PrefabEntityTarget::source(entity("p-root")),
                    component: component(),
                    record: SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::map([("flag", ComponentValue::Bool(true))]),
                    )
                    .with_owner(owner()),
                    bindings: Vec::new(),
                });
            }
        }
        fixture.write_documents();
        let adapter = AuthoringAdapterReceipt::from_schema_owner(
            &fixture.bundle.owners()[0],
            &Default::default(),
        )
        .unwrap();
        let receipt = fixture
            .prepare(vec![adapter].into_boxed_slice(), Default::default())
            .admit_startup_closure()
            .unwrap()
            .publish();
        let snapshot = receipt.snapshot();
        assert_eq!(
            snapshot.readiness(),
            AuthoringProjectReadiness::Complete,
            "replace={replace}, diagnostics={:?}",
            snapshot.startup_compilation_diagnostics(),
        );
        assert!(!snapshot.startup_compilation_diagnostics().has_errors());
        assert_eq!(snapshot.prefabs().len(), 2);
        assert!(snapshot.assets().is_empty());
        assert!(snapshot.unresolved_dependencies().is_empty());
        let captured = &snapshot.prefabs()[0];
        assert_eq!(captured.document(), &fixture.p);
        assert_eq!(
            captured
                .document()
                .entities
                .iter()
                .find(|record| record.id == entity("p-root"))
                .unwrap()
                .components[&component()],
            bad,
        );
        assert!(matches!(
            captured.publish_document(),
            Err(AuthoringDocumentDataError::Publication(_))
        ));
        let bytes = fs::read(fixture.project.root.join("prefabs/p.prefab.json")).unwrap();
        assert_eq!(captured.source().digest(), ContentDigest::of_bytes(&bytes));
        assert!(matches!(
            admit_prefab_bytes_with_basis(
                captured.source().project_path(),
                &bytes,
                Default::default(),
                &snapshot.startup_scene().schema_basis(),
            ),
            Err(AuthoringDocumentDataError::Publication(_))
        ));
        let compiled = compile_snapshot(&snapshot);
        assert!(compiled.candidate.is_some(), "{:?}", compiled.diagnostics);
        assert!(!compiled.diagnostics.has_errors());
        let expanded = compiled.candidate.unwrap();
        let values = expanded
            .document()
            .entities
            .iter()
            .filter_map(|record| record.components.get(&component()))
            .collect::<Vec<_>>();
        assert_eq!(values.len(), if replace { 2 } else { 0 });
        assert!(values.iter().all(|record| {
            record.value == ComponentValue::map([("flag", ComponentValue::Bool(true))])
        }));
        drop(snapshot);
        let parts = receipt.into_session_parts().unwrap();
        assert!(matches!(
            parts
                .snapshot
                .fork_prefab(&AssetPath::new("p.prefab.json").unwrap()),
            Err(AuthoringDocumentDataError::Publication(_))
        ));
        assert!(
            parts
                .snapshot
                .fork_prefab(&AssetPath::new("uncaptured.prefab.json").unwrap())
                .unwrap()
                .is_none()
        );
        let standalone_r = parts
            .snapshot
            .fork_prefab(&AssetPath::new("r.prefab.json").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(standalone_r.document(), &fixture.r);
        assert_eq!(
            standalone_r.schema_basis(),
            parts.startup_scene.schema_basis()
        );
        drop(parts);

        // Removing the repair changes effective demand, without changing source admission.
        for anchor in &mut fixture.scene.entities {
            anchor
                .prefab
                .as_mut()
                .unwrap()
                .overrides
                .as_mut()
                .unwrap()
                .components
                .clear();
        }
        fixture.write_documents();
        let adapter = AuthoringAdapterReceipt::from_schema_owner(
            &fixture.bundle.owners()[0],
            &Default::default(),
        )
        .unwrap();
        let uncorrected = fixture
            .prepare(vec![adapter].into_boxed_slice(), Default::default())
            .admit_startup_closure()
            .unwrap()
            .publish();
        let snapshot = uncorrected.snapshot();
        assert_eq!(snapshot.readiness(), AuthoringProjectReadiness::Degraded);
        assert!(snapshot.startup_compilation_diagnostics().has_errors());
        assert_eq!(snapshot.prefabs()[0].document(), &fixture.p);
        assert!(compile_snapshot(&snapshot).candidate.is_none());
    }
}

#[test]
fn raw_dependency_capture_rejects_invalid_local_hierarchy_before_value_selection() {
    let mut fixture = ActualDemandProject::new(asset_schema_bundle(owner(), component()));
    fixture.p.roots[0] = entity("missing-local-root");
    fixture.write_documents();
    let error = match fixture
        .prepare(Box::new([]), Default::default())
        .admit_startup_closure()
    {
        Err(error) => error,
        Ok(_) => panic!("an invalid local source hierarchy must not enter composition"),
    };
    let AuthoringProjectSourceAdmissionError::SourceStructure { kind, path, source } = error else {
        panic!("local hierarchy rejection must retain its typed source diagnostics");
    };
    assert_eq!(kind, AuthoringSourceKind::Prefab);
    assert_eq!(path.as_str(), "prefabs/p.prefab.json");
    assert!(
        source
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "scene.dangling-root")
    );
}

#[test]
fn removing_replacements_requests_q_and_preserves_raw_sources_on_compile_failure() {
    let mut fixture = ActualDemandProject::new(asset_schema_bundle(owner(), component()));
    fixture.remove_replacements();
    let receipt = fixture
        .prepare(Box::new([]), Default::default())
        .admit_startup_closure()
        .unwrap()
        .publish();
    let snapshot = receipt.snapshot();
    assert_eq!(snapshot.readiness(), AuthoringProjectReadiness::Degraded);
    assert!(
        snapshot
            .startup_compilation_diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.prefab-source-missing" })
    );
    assert_eq!(snapshot.startup_scene().document(), &fixture.scene);
    assert_eq!(snapshot.prefabs().len(), 1);
    assert_eq!(snapshot.prefabs()[0].document(), &fixture.p);
    let published_p = snapshot.prefabs()[0].publish_document().unwrap();
    assert_eq!(published_p.unavailable_manifest().overlay_target_count(), 1);
    assert_eq!(
        snapshot
            .source_facts()
            .map(|fact| fact.project_path().as_str())
            .collect::<Vec<_>>(),
        ["prefabs/p.prefab.json", "scenes/startup.scene.json"]
    );
    assert!(snapshot.assets().is_empty());
    assert!(!snapshot.unresolved_dependencies().is_empty());
    assert!(snapshot.unresolved_dependencies().iter().all(|dependency| {
        dependency.kind() == project::AuthoringUnresolvedDependencyKind::Prefab
            && dependency.reference() == &AssetRef::path("q.prefab.json").unwrap()
            && dependency.dependency_depth() == 2
    }));
    let compiled = compile_snapshot(&snapshot);
    assert!(compiled.candidate.is_none());
    assert!(
        compiled
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.prefab-source-missing" })
    );
    assert!(compiled.effective_asset_references().is_empty());
    let manifest = published_p.unavailable_manifest().clone();
    // A readable Q is requested and retained, but the old unknown declaration cannot activate.
    let q = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(entity("q-root"))]);
    fs::write(
        fixture.project.root.join("prefabs/q.prefab.json"),
        q.to_json_string().unwrap(),
    )
    .unwrap();
    let conflicted = fixture
        .prepare(Box::new([]), Default::default())
        .admit_startup_closure()
        .unwrap()
        .publish();
    let conflicted = conflicted.snapshot();
    assert_eq!(conflicted.readiness(), AuthoringProjectReadiness::Degraded);
    assert_eq!(conflicted.startup_scene().document(), &fixture.scene);
    assert_eq!(
        conflicted
            .prefabs()
            .iter()
            .map(|prefab| prefab.path().as_str())
            .collect::<Vec<_>>(),
        ["p.prefab.json", "q.prefab.json"]
    );
    assert_eq!(
        conflicted.prefabs()[0]
            .publish_document()
            .unwrap()
            .unavailable_manifest(),
        &manifest
    );
    assert_eq!(conflicted.prefabs()[1].document(), &q);
    assert!(
        conflicted
            .startup_compilation_diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.prefab-overlay-conflict" })
    );
    assert!(conflicted.unresolved_dependencies().is_empty());
    assert!(conflicted.assets().is_empty());
    // Once Q is demanded, malformed bytes remain a typed format error.
    fs::write(
        fixture.project.root.join("prefabs/q.prefab.json"),
        b"invalid demanded Prefab",
    )
    .unwrap();
    assert!(matches!(
        fixture.prepare(Box::new([]), Default::default()).admit_startup_closure(),
        Err(AuthoringProjectSourceAdmissionError::SourceFormat {
            kind: AuthoringSourceKind::Prefab, path, ..
        }) if path.as_str() == "prefabs/q.prefab.json"
    ));
}

#[test]
fn selected_source_identity_and_dependency_budget_failures_remain_typed() {
    let fixture = ActualDemandProject::new(asset_schema_bundle(owner(), component()));
    let limits = AuthoringProjectAdmissionLimits {
        dependency_edges: ItemLimit::ONE,
        ..Default::default()
    };
    assert!(matches!(
        fixture
            .prepare(Box::new([]), limits)
            .admit_startup_closure(),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::DependencyEdgeLimitExceeded { maximum: 1 }
        ))
    ));
    fs::remove_file(fixture.project.root.join("prefabs/r.prefab.json")).unwrap();
    fs::hard_link(
        fixture.project.root.join(BUNDLE_PATH),
        fixture.project.root.join("prefabs/r.prefab.json"),
    )
    .unwrap();
    assert!(matches!(
        fixture.prepare(Box::new([]), Default::default()).admit_startup_closure(),
        Err(AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict { path })
            if path.as_str() == "prefabs/r.prefab.json"
    ));
}

#[test]
fn captured_source_fact_revalidation_checks_bytes_identity_and_bounded_read_work() {
    let fixture = ActualDemandProject::new(asset_schema_bundle(owner(), component()));
    let receipt = fixture
        .prepare(Box::new([]), Default::default())
        .admit_startup_closure()
        .unwrap()
        .publish();
    let snapshot = receipt.snapshot();
    let fact = snapshot
        .source_facts()
        .find(|fact| fact.project_path().as_str() == "prefabs/p.prefab.json")
        .unwrap();
    let root = snapshot.project_root();
    let relative = RelativePath::new(fact.project_path().as_str()).unwrap();
    let path = fixture.project.root.join(fact.project_path().as_str());
    let original = fs::read(&path).unwrap();
    let manifest = snapshot.prefabs()[0]
        .publish_document()
        .unwrap()
        .unavailable_manifest()
        .clone();
    // An exact open charges each path component plus its spelling; the final bounded read
    // reserves one byte beyond the observed length to detect growth without an unbounded read.
    let expected_work = fact.project_path().as_str().split('/').count() * 65_536
        + 2 * fact.project_path().as_str().len()
        + original.len()
        + 1;
    let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
    project::revalidate_source_fact(&root, fact, &mut budget).unwrap();
    assert_eq!(budget.usage().decode_work(), expected_work);
    assert_eq!(budget.usage().aggregate_encoded_bytes(), 0);
    assert_eq!(budget.usage().retained_candidate_bytes(), 0);
    assert_eq!(budget.usage().unique_files(), 0);
    for (limits, expected) in [
        (
            AuthoringProjectAdmissionLimits {
                decode_work: ItemLimit::new(expected_work - 1).unwrap(),
                ..Default::default()
            },
            AuthoringProjectAdmissionError::DecodeWorkExceeded {
                maximum: expected_work - 1,
            },
        ),
        (
            AuthoringProjectAdmissionLimits {
                retained_candidate_bytes: ByteLimit::new(original.len()).unwrap(),
                ..Default::default()
            },
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
                maximum: original.len(),
            },
        ),
    ] {
        let mut budget = AuthoringProjectAdmissionBudget::new(limits);
        let error = project::revalidate_source_fact(&root, fact, &mut budget).unwrap_err();
        let AuthoringProjectSourceAdmissionError::Budget(error) = error else {
            panic!("revalidation admission must preserve its typed budget error");
        };
        match (error, expected) {
            (
                AuthoringProjectAdmissionError::DecodeWorkExceeded { maximum },
                AuthoringProjectAdmissionError::DecodeWorkExceeded { maximum: expected },
            )
            | (
                AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { maximum },
                AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
                    maximum: expected,
                },
            ) => assert_eq!(maximum, expected),
            (error, expected) => panic!("expected {expected:?}, received {error:?}"),
        }
    }
    let mut edited = original.clone();
    let index = edited
        .windows(b"preserve opaque intent".len())
        .position(|bytes| bytes == b"preserve opaque intent")
        .unwrap();
    edited[index] = b'P';
    fs::write(&path, &edited).unwrap();
    assert_eq!(
        root.open_file(&relative).unwrap().identity(),
        fact.identity()
    );
    assert_eq!(edited.len(), fact.encoded_bytes());
    let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
    assert!(matches!(
        project::revalidate_source_fact(&root, fact, &mut budget),
        Err(AuthoringProjectSourceAdmissionError::SourceChanged {
            kind: AuthoringSourceKind::Prefab, path,
        }) if path.as_str() == "prefabs/p.prefab.json"
    ));
    fs::write(&path, &original).unwrap();
    let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
    project::revalidate_source_fact(&root, fact, &mut budget).unwrap();
    // Keep the original file alive under another name so a new file cannot reuse its identity.
    fs::rename(
        &path,
        fixture.project.root.join("prefabs/p.previous.prefab.json"),
    )
    .unwrap();
    fs::write(&path, &original).unwrap();
    assert_ne!(
        root.open_file(&relative).unwrap().identity(),
        fact.identity()
    );
    let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
    assert!(matches!(
        project::revalidate_source_fact(&root, fact, &mut budget),
        Err(AuthoringProjectSourceAdmissionError::SourceChanged {
            kind: AuthoringSourceKind::Prefab, path,
        }) if path.as_str() == "prefabs/p.prefab.json"
    ));
    fs::remove_file(&path).unwrap();
    let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
    assert!(matches!(
        project::revalidate_source_fact(&root, fact, &mut budget),
        Err(AuthoringProjectSourceAdmissionError::SourceHost {
            kind: AuthoringSourceKind::Prefab, path,
            source: FsError::Io { source, .. },
        }) if path.as_str() == "prefabs/p.prefab.json" && source.kind() == io::ErrorKind::NotFound
    ));
    // Failed revalidation never rewrites raw capture or its independent classification.
    assert_eq!(snapshot.prefabs()[0].document(), &fixture.p);
    assert_eq!(
        snapshot.prefabs()[0]
            .publish_document()
            .unwrap()
            .unavailable_manifest(),
        &manifest
    );
    assert_eq!(
        snapshot
            .source_facts()
            .find(|source| source.project_path() == fact.project_path()),
        Some(fact)
    );
}

#[cfg(windows)]
#[test]
fn selected_source_access_failure_is_not_reclassified_as_missing_or_degraded() {
    use std::os::windows::fs::OpenOptionsExt;

    let fixture = ActualDemandProject::new(asset_schema_bundle(owner(), component()));
    let _exclusive = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(fixture.project.root.join("prefabs/r.prefab.json"))
        .unwrap();
    let expected = fs::File::open(fixture.project.root.join("prefabs/r.prefab.json")).unwrap_err();
    assert_ne!(expected.kind(), io::ErrorKind::NotFound);
    let error = match fixture
        .prepare(Box::new([]), Default::default())
        .admit_startup_closure()
    {
        Err(error) => error,
        Ok(_) => panic!("exclusive source access must reject source admission"),
    };
    assert!(
        matches!(
            &error,
            AuthoringProjectSourceAdmissionError::SourceHost {
                kind: AuthoringSourceKind::Prefab, path,
                source: FsError::Io { source, .. },
            } if path.as_str() == "prefabs/r.prefab.json"
                && source.kind() == expected.kind()
                && source.raw_os_error() == expected.raw_os_error()
        ),
        "{error:?}"
    );
}

#[test]
fn manifest_and_schema_bundle_changes_reject_the_capture_against_its_original_basis() {
    for (path, kind) in [
        (PROJECT_MANIFEST_PATH, AuthoringSourceKind::ProjectManifest),
        (BUNDLE_PATH, AuthoringSourceKind::SchemaBundle),
    ] {
        let fixture = ActualDemandProject::new(asset_schema_bundle(owner(), component()));
        let prepared = fixture.prepare(Box::new([]), Default::default());
        let path_on_disk = fixture.project.root.join(path);
        let mut bytes = fs::read(&path_on_disk).unwrap();
        bytes.push(b'\n');
        fs::write(&path_on_disk, bytes).unwrap();
        assert!(matches!(
            prepared.admit_startup_closure(),
            Err(AuthoringProjectSourceAdmissionError::SourceChanged {
                kind: changed, path: changed_path,
            }) if changed == kind && changed_path.as_str() == path
        ));
    }
}
