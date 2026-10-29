#[path = "support/text_project.rs"]
mod text_project;

use std::{
    fs,
    time::{Duration, Instant},
};

use nara::project_host::EditorSourceAssetDomain;
use nara_asset::{AssetMetaIdentityCandidate, ImportedProductId};
use nara_asset_domain_consumer::bundle::{
    TextBundleDomain, TextBundleIdentityDecision, TextBundleRebindAssignment, TextBundleSettings,
};
use nara_diagnostic::Diagnostic;
use nara_editor::{EditorApplication, EditorNativeComposition};
use nara_tooling::{EditorAction, EditorActionBatch, EditorSourceAssetImportAttempt};

const FIRST: &str = "10a5c635-e71b-478d-86c4-ef933956a425";
const SECOND: &str = "f319f4c7-d809-40d9-8c97-4abb6c68b6da";
const RAW: &[u8] = br#"{"suffix":"suffix.txt","entries":[{"name":"Opening","text":"Hello"},{"name":"Ending","text":"Goodbye"}]}"#;
const REDUCED: &[u8] = br#"{"suffix":"suffix.txt","entries":[{"name":"Opening","text":"Hello"}]}"#;

struct BundleImport;

impl EditorSourceAssetDomain for BundleImport {
    type Domain = TextBundleDomain;

    fn default_settings() -> TextBundleSettings {
        TextBundleSettings::initial(RAW, 64, [id(FIRST), id(SECOND)]).unwrap()
    }

    fn validate_settings_successor(
        previous: &TextBundleSettings,
        candidate: &TextBundleSettings,
    ) -> Result<(), Diagnostic> {
        candidate
            .validate_successor(previous)
            .map_err(|error| error.into_diagnostic())
    }
}

fn id(value: &str) -> ImportedProductId {
    ImportedProductId::parse_subproduct(value).unwrap()
}

fn open(project: &text_project::TextProject) -> EditorApplication {
    let mut composition = EditorNativeComposition::new();
    composition
        .register_source_asset_domain::<BundleImport>()
        .unwrap();
    let (mut application, _frontend) = composition
        .build(nara_editor::EditorLaunchOptions::new(&project.root))
        .unwrap();
    application.native_shell_ready();
    wait(&mut application, |frame| {
        frame
            .project_browser()
            .entry("assets/bundle.json")
            .is_some()
            && !frame.image_import_work_pending()
    });
    application
}

fn drive(application: &mut EditorApplication) {
    application.advance(Duration::ZERO);
    std::thread::sleep(Duration::from_millis(1));
}

fn wait(
    application: &mut EditorApplication,
    condition: impl Fn(&nara_tooling::EditorFrameModel) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !application
        .current_frame()
        .is_some_and(|frame| condition(&frame))
    {
        assert!(Instant::now() < deadline, "compound import did not settle");
        drive(application);
    }
}

fn import(application: &mut EditorApplication, settings: &TextBundleSettings) {
    let frame = application.current_frame().unwrap();
    let submitted = application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::SourceAssetImport(
                BundleImport::import_action("assets/bundle.json", settings).unwrap(),
            )],
        ),
        Duration::ZERO,
    );
    assert!(
        matches!(
            submitted.actions(),
            [nara_editor::EditorApplicationActionDispatch::Dispatched(
                nara_tooling::EditorActionOutcome::SourceAssetImport(
                    nara_tooling::EditorSourceAssetImportActionOutcome::Started,
                ),
            ),]
        ),
        "{submitted:?}"
    );
    wait(application, |frame| {
        !frame.source_asset_import_work_pending()
    });
}

fn close(application: &mut EditorApplication) {
    let mut ready = application.request_exit().exit_ready();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !ready {
        assert!(
            Instant::now() < deadline,
            "compound import Close did not settle"
        );
        ready = application.advance(Duration::ZERO).exit_ready();
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn editor_reimport_preserves_retired_product_lineage_and_rejects_forged_settings() {
    let project = text_project::TextProject::new();
    fs::write(project.root.join("assets/bundle.json"), RAW).unwrap();
    let path = project.root.join("assets/bundle.json.meta");
    let initial = BundleImport::default_settings();
    let mut application = open(&project);
    import(&mut application, &initial);
    wait(&mut application, |frame| !frame.image_import_work_pending());
    let admitted =
        AssetMetaIdentityCandidate::decode_json_bytes(&fs::read(&path).unwrap()).unwrap();
    let source = admitted.identity().stable_id();
    close(&mut application);
    drop(application);

    fs::write(project.root.join("assets/bundle.json"), REDUCED).unwrap();
    let reduced = initial
        .rebind(
            REDUCED,
            &[TextBundleRebindAssignment {
                row: 0,
                identity: TextBundleIdentityDecision::Continue(id(FIRST)),
            }],
        )
        .unwrap();
    let mut application = open(&project);
    import(&mut application, &reduced);
    wait(&mut application, |frame| !frame.image_import_work_pending());
    let retained = fs::read(&path).unwrap();
    let admitted = AssetMetaIdentityCandidate::decode_json_bytes(&retained).unwrap();
    assert_eq!(admitted.identity().stable_id(), source);
    let settings = admitted.admit::<TextBundleSettings>().unwrap();
    assert_eq!(settings.settings().continuity.retired(), [id(SECOND)]);
    close(&mut application);
    drop(application);

    // Standalone candidate settings are valid, but publishing them would erase known retirement.
    let forged = TextBundleSettings::initial(REDUCED, 64, [id(FIRST)]).unwrap();
    let mut application = open(&project);
    import(&mut application, &forged);
    assert_eq!(fs::read(&path).unwrap(), retained);
    let frame = application.current_frame().unwrap();
    let Some(EditorSourceAssetImportAttempt::Failed { diagnostics }) =
        frame.source_asset_import().map(|view| &view.attempt)
    else {
        panic!("the authored successor must reject the lost retirement");
    };
    assert!(
        diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == "consumer.text-bundle.transition-invalid")
    );
    close(&mut application);
}
