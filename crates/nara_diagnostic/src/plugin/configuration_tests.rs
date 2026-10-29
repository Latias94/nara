use std::{collections::BTreeSet, num::NonZeroU64};

use nara_app::{PluginDefinitionId, PluginPlan};
use nara_core::{ByteLimit, ItemLimit};

use crate::{
    DIAGNOSTICS_PLUGIN_DECLARATION, DIAGNOSTICS_PLUGIN_ID, RuntimeDiagnosticRetention,
    RuntimeDiagnostics, RuntimeDiagnosticsSettings, RuntimePressureRetention,
    RuntimePressureSettings, RuntimePressureSnapshots, prelude::plugin,
};

fn items(value: usize) -> ItemLimit {
    ItemLimit::new(value).unwrap()
}

fn bytes(value: usize) -> ByteLimit {
    ByteLimit::new(value).unwrap()
}

fn configured_settings() -> (RuntimeDiagnosticsSettings, RuntimePressureSettings) {
    let runtime = RuntimeDiagnosticsSettings::new(items(8), bytes(4096))
        .unwrap()
        .with_field_limit(items(4))
        .unwrap()
        .with_summary_byte_limit(bytes(64))
        .unwrap()
        .with_field_text_byte_limit(bytes(128))
        .unwrap()
        .with_retention(RuntimeDiagnosticRetention::FrameWindow(
            NonZeroU64::new(7).unwrap(),
        ));
    let pressure = RuntimePressureSettings::new(items(4), items(8))
        .unwrap()
        .with_retention_frame_window(NonZeroU64::new(11).unwrap());
    (runtime, pressure)
}

#[test]
fn configured_plugin_definitions_share_stable_identity_and_converge() {
    let (runtime, pressure) = configured_settings();
    let first = plugin(runtime, pressure);
    let second = plugin(runtime, pressure);
    let key = first.key().unwrap();
    assert_eq!(key, second.key().unwrap());
    assert_eq!(
        key.definition(),
        PluginDefinitionId::new("nara.diagnostic.configured", 1)
    );

    let single = PluginPlan::resolve(first.clone()).unwrap();
    let repeated = PluginPlan::resolve((first, second)).unwrap();
    assert_eq!(single.entries().len(), 1);
    assert_eq!(single.entries(), repeated.entries());
    assert_eq!(single.fingerprint(), repeated.fingerprint());
    assert_eq!(single.entries()[0].plugin_id(), DIAGNOSTICS_PLUGIN_ID);
    assert_eq!(
        single.entries()[0].declaration(),
        &DIAGNOSTICS_PLUGIN_DECLARATION
    );
    assert_eq!(single.entries()[0].definition_key(), Some(key));
}

#[test]
fn configured_plugin_keys_cover_all_runtime_settings() {
    let runtime = RuntimeDiagnosticsSettings::default();
    let pressure = RuntimePressureSettings::default();
    let mut keys = BTreeSet::from([plugin(runtime, pressure).key().unwrap()]);
    for (field, changed) in [
        (
            "entry_limit",
            RuntimeDiagnosticsSettings::new(
                items(runtime.entry_limit().get() + 1),
                runtime.byte_limit(),
            )
            .unwrap(),
        ),
        (
            "byte_limit",
            RuntimeDiagnosticsSettings::new(
                runtime.entry_limit(),
                bytes(runtime.byte_limit().get() + 1),
            )
            .unwrap(),
        ),
        (
            "field_limit",
            runtime
                .with_field_limit(items(runtime.field_limit().get() + 1))
                .unwrap(),
        ),
        (
            "summary_byte_limit",
            runtime
                .with_summary_byte_limit(bytes(runtime.summary_byte_limit().get() + 1))
                .unwrap(),
        ),
        (
            "field_text_byte_limit",
            runtime
                .with_field_text_byte_limit(bytes(runtime.field_text_byte_limit().get() + 1))
                .unwrap(),
        ),
        (
            "retention_frame_window_1",
            runtime.with_retention(RuntimeDiagnosticRetention::FrameWindow(
                NonZeroU64::new(1).unwrap(),
            )),
        ),
        (
            "retention_frame_window_2",
            runtime.with_retention(RuntimeDiagnosticRetention::FrameWindow(
                NonZeroU64::new(2).unwrap(),
            )),
        ),
    ] {
        assert!(
            keys.insert(plugin(changed, pressure).key().unwrap()),
            "runtime setting {field} must have a distinct configuration key"
        );
    }
}

#[test]
fn configured_plugin_keys_cover_all_pressure_settings() {
    let runtime = RuntimeDiagnosticsSettings::default();
    let pressure = RuntimePressureSettings::default();
    let mut keys = BTreeSet::from([plugin(runtime, pressure).key().unwrap()]);
    for (field, changed) in [
        (
            "source_limit",
            RuntimePressureSettings::new(
                items(pressure.source_limit().get() + 1),
                pressure.measurements_per_source(),
            )
            .unwrap(),
        ),
        (
            "measurements_per_source",
            RuntimePressureSettings::new(
                pressure.source_limit(),
                items(pressure.measurements_per_source().get() + 1),
            )
            .unwrap(),
        ),
        (
            "retention_frame_window_1",
            pressure.with_retention(RuntimePressureRetention::FrameWindow(
                NonZeroU64::new(1).unwrap(),
            )),
        ),
        (
            "retention_frame_window_2",
            pressure.with_retention(RuntimePressureRetention::FrameWindow(
                NonZeroU64::new(2).unwrap(),
            )),
        ),
    ] {
        assert!(
            keys.insert(plugin(runtime, changed).key().unwrap()),
            "pressure setting {field} must have a distinct configuration key"
        );
    }
}

#[test]
fn configured_plugin_plan_installs_selected_world_settings() {
    let (runtime, pressure) = configured_settings();
    let plan = PluginPlan::resolve(plugin(runtime, pressure)).unwrap();
    for _ in 0..2 {
        let app = plan.instantiate_retained().unwrap();
        assert_eq!(app.plugin_plan_fingerprint(), plan.fingerprint());
        assert_eq!(
            app.world().resource::<RuntimeDiagnostics>().settings(),
            runtime
        );
        assert_eq!(
            app.world()
                .resource::<RuntimePressureSnapshots>()
                .settings(),
            pressure
        );
    }
}
