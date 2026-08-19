use std::sync::Arc;

use super::*;

fn entry(path: &str, kind: EditorProjectSourceKind) -> EditorProjectBrowserEntry {
    EditorProjectBrowserEntry::new(
        Arc::from(path),
        kind,
        match kind {
            EditorProjectSourceKind::Scene => EditorProjectSourceOpenCapability::Scene,
            EditorProjectSourceKind::Prefab => EditorProjectSourceOpenCapability::Prefab,
            _ => EditorProjectSourceOpenCapability::InspectOnly(
                EditorProjectSourceInspectReason::ReadOnlySource,
            ),
        },
    )
}

#[test]
fn browser_query_is_deterministic_and_local_state_is_revision_keyed() {
    let snapshot = Arc::new(EditorProjectBrowserSnapshot::new(
        7,
        [
            entry("scenes/z.scene.json", EditorProjectSourceKind::Scene),
            entry("assets", EditorProjectSourceKind::Directory),
            entry("scenes", EditorProjectSourceKind::Directory),
            entry("assets/icon.png", EditorProjectSourceKind::AssetSource),
            entry("scenes/a.scene.json", EditorProjectSourceKind::Scene),
        ],
        [],
        EditorProjectBrowserBudgetUsage::default(),
    ));
    let mut browser = EditorProjectBrowserState::new();
    browser.synchronize(
        EditorProjectBrowserStatus::Ready,
        Some(snapshot),
        None,
        None,
        false,
        false,
    );

    while browser.advance_query(2).pending() {}
    assert_eq!(
        browser
            .view()
            .rows()
            .iter()
            .map(EditorProjectBrowserRow::relative_path)
            .collect::<Vec<_>>(),
        ["assets", "scenes"]
    );

    assert!(
        browser.apply_local_action(EditorProjectBrowserLocalAction::ToggleExpanded {
            relative_path: Arc::from("scenes"),
        })
    );
    assert!(
        browser.apply_local_action(EditorProjectBrowserLocalAction::SetPathFilter {
            value: Arc::from("a.scene"),
        })
    );
    while browser.advance_query(1).pending() {}
    let view = browser.view();
    assert_eq!(
        view.rows()
            .iter()
            .map(EditorProjectBrowserRow::relative_path)
            .collect::<Vec<_>>(),
        ["scenes/a.scene.json"]
    );
    assert_eq!(
        view.path_filter(),
        "a.scene",
        "filter state remains UI-neutral and outside the Host action algebra"
    );

    assert!(
        browser.apply_local_action(EditorProjectBrowserLocalAction::SetPathFilter {
            value: Arc::from(""),
        })
    );
    assert!(
        browser.apply_local_action(EditorProjectBrowserLocalAction::SetKindFilter {
            filter: EditorProjectBrowserKindFilter::Documents,
        })
    );
    while browser.advance_query(2).pending() {}
    assert_eq!(
        browser
            .view()
            .rows()
            .iter()
            .map(EditorProjectBrowserRow::relative_path)
            .collect::<Vec<_>>(),
        ["scenes/a.scene.json", "scenes/z.scene.json"]
    );
}

#[test]
fn maximum_query_work_is_bounded_per_advance() {
    let entries = (0..600)
        .map(|index| {
            entry(
                &format!("scenes/{index:04}.scene.json"),
                EditorProjectSourceKind::Scene,
            )
        })
        .collect::<Vec<_>>();
    let snapshot = Arc::new(EditorProjectBrowserSnapshot::new(
        11,
        entries,
        [],
        EditorProjectBrowserBudgetUsage::default(),
    ));
    let mut browser = EditorProjectBrowserState::new();
    browser.synchronize(
        EditorProjectBrowserStatus::Ready,
        Some(Arc::clone(&snapshot)),
        None,
        None,
        false,
        false,
    );
    browser.apply_local_action(EditorProjectBrowserLocalAction::SetPathFilter {
        value: Arc::from(".scene.json"),
    });

    let first = browser.advance_query(128);
    assert_eq!(first.processed_entries(), 128);
    assert!(first.pending());
    assert!(browser.view().rows().is_empty());

    let mut observed = first.processed_entries();
    while browser.query_work_pending() {
        let advance = browser.advance_query(128);
        assert!(advance.processed_entries() <= 128);
        observed += advance.processed_entries();
    }
    assert_eq!(observed, 600);
    let view = browser.view();
    assert_eq!(view.rows().len(), 600);
    assert!(
        view.work_observed(),
        "the final completed query slice must schedule one last redraw"
    );
    browser.synchronize(
        EditorProjectBrowserStatus::Ready,
        Some(snapshot),
        None,
        None,
        false,
        false,
    );
    assert!(
        !browser.view().work_observed(),
        "work observation is a single-frame redraw pulse"
    );
}
