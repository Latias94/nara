use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use nara_app::{
    App, CoreStage, PluginError, PluginHook, PluginId, RuntimeFault, RuntimeFaultKind,
    RuntimeFaultReporter, RuntimeInstance, RuntimeStartError, StartupStage,
};

const GUARDS: [PluginId; 3] = [
    PluginId::new("nara.test.authority.third"),
    PluginId::new("nara.test.authority.first"),
    PluginId::new("nara.test.authority.second"),
];

fn install_guards(app: &mut App, calls: &Arc<Mutex<Vec<PluginId>>>, invalid: &Arc<AtomicUsize>) {
    for (index, plugin) in GUARDS.into_iter().enumerate() {
        let calls = Arc::clone(calls);
        let invalid = Arc::clone(invalid);
        app.__install_runtime_authority_validator(
            plugin,
            Arc::new(move |_, _| {
                calls.lock().unwrap().push(plugin);
                if invalid.load(Ordering::Acquire) == index {
                    Err(RuntimeFault::engine(
                        RuntimeFaultKind::RuntimeAuthority,
                        plugin.as_str(),
                    ))
                } else {
                    Ok(())
                }
            }),
        )
        .unwrap();
    }
}

#[test]
fn seal_checks_guards_in_install_order_and_attributes_the_first_fault() {
    for index in 0..GUARDS.len() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let invalid = Arc::new(AtomicUsize::new(index));
        let mut app = App::new();
        install_guards(&mut app, &calls, &invalid);

        let error = app.seal().unwrap_err();

        assert!(matches!(
            error.error(),
            PluginError::SetupFailed { plugin, message }
                if *plugin == GUARDS[index] && message.contains(GUARDS[index].as_str())
        ));
        let failure = error.plugin_failure().unwrap();
        assert_eq!(failure.plugin(), GUARDS[index]);
        assert_eq!(failure.hook(), PluginHook::Finish);
        assert_eq!(calls.lock().unwrap().as_slice(), &GUARDS[..=index]);
    }
}

#[test]
fn duplicate_domain_guard_ids_are_rejected_without_replacing_the_first_guard() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let invalid = Arc::new(AtomicUsize::new(usize::MAX));
    let mut app = App::new();
    install_guards(&mut app, &calls, &invalid);

    let error = app
        .__install_runtime_authority_validator(
            GUARDS[0],
            Arc::new(|_, _| {
                Err(RuntimeFault::engine(
                    RuntimeFaultKind::RuntimeAuthority,
                    "nara.test.authority.duplicate",
                ))
            }),
        )
        .err()
        .expect("a repeated plugin must not install another guard");

    assert!(matches!(
        error,
        PluginError::SetupFailed { plugin, message }
            if plugin == GUARDS[0] && message.contains("already installed for this plugin")
    ));
    app.seal().unwrap();
    assert_eq!(calls.lock().unwrap().as_slice(), &GUARDS);
}

#[test]
fn healthy_domain_guards_run_before_and_after_driver_operations() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let invalid = Arc::new(AtomicUsize::new(usize::MAX));
    let mut app = App::new();
    install_guards(&mut app, &calls, &invalid);
    let mut runtime = RuntimeInstance::start(app).unwrap();
    calls.lock().unwrap().clear();

    let outcome = runtime
        .with_driver_scope(|_| {
            assert_eq!(calls.lock().unwrap().as_slice(), &GUARDS);
        })
        .unwrap();

    assert!(outcome.post_operation_fault.is_none());
    let expected = [
        GUARDS[0], GUARDS[1], GUARDS[2], GUARDS[0], GUARDS[1], GUARDS[2],
    ];
    assert_eq!(calls.lock().unwrap().as_slice(), &expected);
    runtime.finish().unwrap();
}

#[test]
fn a_later_domain_guard_faults_startup_before_the_next_stage() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let invalid = Arc::new(AtomicUsize::new(usize::MAX));
    let later_runs = Arc::new(AtomicUsize::new(0));
    let mut app = App::new();
    install_guards(&mut app, &calls, &invalid);
    let break_authority = Arc::clone(&invalid);
    app.add_systems(StartupStage::Core, move || {
        break_authority.store(1, Ordering::Release);
    })
    .unwrap();
    let later = Arc::clone(&later_runs);
    app.add_systems(StartupStage::Platform, move || {
        later.fetch_add(1, Ordering::Relaxed);
    })
    .unwrap();

    let failure = RuntimeInstance::start(app).unwrap_err();
    let fault = match failure.error() {
        RuntimeStartError::Startup(fault) => fault,
        other => panic!("unexpected startup error: {other:?}"),
    };
    assert_eq!(fault.source(), GUARDS[1].as_str());
    assert_eq!(
        failure.world().resource::<RuntimeFaultReporter>().fault(),
        Some(fault.clone())
    );
    assert_eq!(later_runs.load(Ordering::Relaxed), 0);
    assert!(calls.lock().unwrap().ends_with(&GUARDS[..=1]));
}

#[test]
fn a_later_guard_fault_stops_the_next_stage_and_remains_canonical() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let invalid = Arc::new(AtomicUsize::new(usize::MAX));
    let later_runs = Arc::new(AtomicUsize::new(0));
    let mut app = App::new();
    install_guards(&mut app, &calls, &invalid);
    let break_authority = Arc::clone(&invalid);
    app.add_systems(CoreStage::Update, move || {
        break_authority.store(1, Ordering::Release);
    })
    .unwrap();
    let later = Arc::clone(&later_runs);
    app.add_systems(CoreStage::PostUpdate, move || {
        later.fetch_add(1, Ordering::Relaxed);
    })
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    calls.lock().unwrap().clear();

    let first = runtime.drive(Duration::ZERO).unwrap_err();
    assert_eq!(first.fault().source(), GUARDS[1].as_str());
    assert_eq!(later_runs.load(Ordering::Relaxed), 0);
    let calls_at_fault = calls.lock().unwrap().clone();
    assert!(calls_at_fault.ends_with(&GUARDS[..=1]));
    invalid.store(usize::MAX, Ordering::Release);
    let repeated = runtime.drive(Duration::ZERO).unwrap_err();

    assert_eq!(first, repeated);
    assert_eq!(
        runtime.fault_reporter().fault().as_ref(),
        Some(first.fault())
    );
    assert_eq!(*calls.lock().unwrap(), calls_at_fault);
    runtime.finish().unwrap();
}

#[test]
fn an_existing_canonical_fault_prevents_all_domain_guard_calls() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let invalid = Arc::new(AtomicUsize::new(usize::MAX));
    let mut app = App::new();
    install_guards(&mut app, &calls, &invalid);
    let mut runtime = RuntimeInstance::start(app).unwrap();
    calls.lock().unwrap().clear();
    let canonical = RuntimeFault::engine(
        RuntimeFaultKind::RequiredTask,
        "nara.test.authority.existing-fault",
    );
    runtime.fault_reporter().report(canonical.clone());
    invalid.store(0, Ordering::Release);

    let error = runtime.drive(Duration::ZERO).unwrap_err();

    assert_eq!(error.fault(), &canonical);
    assert!(calls.lock().unwrap().is_empty());
    runtime.finish().unwrap();
}
