use std::{
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use super::*;

const DIRECT_CLEANUP_CHILD: &str = "cargo::native::tests::direct_cleanup_child";

struct Root(PathBuf);

impl Root {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("nara-cargo-owner-{}-{stamp}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let root = self.0.canonicalize().unwrap();
        assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            root.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("nara-cargo-owner-")
        );
        fs::remove_dir_all(root).unwrap();
    }
}

fn generated_request(root: &Root, limits: CargoBuildLimits) -> CargoBuildRequest {
    fs::create_dir_all(root.0.join("src")).unwrap();
    for (path, bytes) in [
        ("Cargo.toml", "manifest"),
        ("Cargo.lock", "lock"),
        ("src/main.rs", "fn main() {}\n"),
    ] {
        fs::write(root.0.join(path), bytes).unwrap();
    }
    CargoBuildRequest::new(
        &root.0,
        RelativePath::new("Cargo.toml").unwrap(),
        CargoBuildSelection::new(
            "fixture",
            "player",
            "dev",
            "x86_64-pc-windows-msvc",
            Vec::<String>::new(),
            false,
        )
        .unwrap(),
        CargoBuildTools::discover().unwrap(),
        ["Cargo.toml", "Cargo.lock", "src/main.rs"].map(|name| RelativePath::new(name).unwrap()),
        limits,
    )
    .unwrap()
}

#[test]
fn generated_rust_input_owns_exact_bytes_and_command_selection_without_global_environment_changes()
{
    let root = Root::new();
    let mut source = b"pub const VALUE: &str = \"A\\ntext\";\n".to_vec();
    let expected = source.clone();
    let request = generated_request(&root, CargoBuildLimits::default())
        .with_generated_rust_input("player-bootstrap.rs", &source)
        .unwrap();
    source.fill(b'B');
    let fact = request.generated_rust_input().unwrap();
    assert_eq!(fact.path(), "player-bootstrap.rs");
    assert_eq!(fact.digest(), ContentDigest::of_bytes(&expected));
    let selected_command = command(&request, request.tools.cargo_path());
    let selected = selected_command
        .get_envs()
        .find(|(key, _)| *key == GENERATED_RUST_SOURCE_ENV)
        .unwrap()
        .1
        .unwrap();
    assert_eq!(selected, std::str::from_utf8(&expected).unwrap());
    assert!(!root.0.join("player-bootstrap.rs").exists());

    let ordinary = generated_request(&root, CargoBuildLimits::default());
    assert!(ordinary.generated_rust_input().is_none());
    assert!(
        command(&ordinary, ordinary.tools.cargo_path())
            .get_envs()
            .any(|(key, value)| key == GENERATED_RUST_SOURCE_ENV && value.is_none())
    );
}

#[test]
fn generated_rust_input_rejects_ambiguous_names_invalid_transport_and_a_second_slot() {
    let root = Root::new();
    for name in [
        "",
        "/generated.rs",
        "./generated.rs",
        "src/../generated.rs",
        "a\\b.rs",
        "a//b.rs",
    ] {
        assert!(matches!(
            generated_request(&root, CargoBuildLimits::default())
                .with_generated_rust_input(name, b"const VALUE: u8 = 1;"),
            Err(CargoBuildFailure::InvalidGeneratedRustInput)
        ));
    }
    for source in [b"".as_slice(), b"\xff".as_slice(), b"a\0b".as_slice()] {
        assert!(matches!(
            generated_request(&root, CargoBuildLimits::default())
                .with_generated_rust_input("generated.rs", source),
            Err(CargoBuildFailure::InvalidGeneratedRustInput)
        ));
    }
    assert!(matches!(
        generated_request(&root, CargoBuildLimits::default())
            .with_generated_rust_input("src/main.rs", b"const VALUE: u8 = 1;"),
        Err(CargoBuildFailure::DuplicateDeclaredInput)
    ));
    let selected = generated_request(&root, CargoBuildLimits::default())
        .with_generated_rust_input("generated.rs", b"const VALUE: u8 = 1;")
        .unwrap();
    assert!(matches!(
        selected.with_generated_rust_input("another.rs", b"const VALUE: u8 = 2;"),
        Err(CargoBuildFailure::DuplicateDeclaredInput)
    ));
}

#[test]
fn generated_rust_input_limits_include_source_files_and_parent_transport_work() {
    let root = Root::new();
    let maximum = vec![b' '; MAX_GENERATED_RUST_SOURCE_BYTES];
    generated_request(&root, CargoBuildLimits::default())
        .with_generated_rust_input("generated.rs", &maximum)
        .unwrap();
    assert!(matches!(
        generated_request(&root, CargoBuildLimits::default()).with_generated_rust_input(
            "generated.rs",
            &vec![b' '; MAX_GENERATED_RUST_SOURCE_BYTES + 1]
        ),
        Err(CargoBuildFailure::Limit("generated Rust source bytes"))
    ));

    let source = b"const VALUE: u8 = 1;";
    let name = "generated.rs";
    let mut exact = CargoBuildLimits::default();
    exact.work_bytes =
        ByteLimit::new(generated_source_work(source.len(), name.len()).unwrap()).unwrap();
    generated_request(&root, exact)
        .with_generated_rust_input(name, source)
        .unwrap();
    exact.work_bytes = ByteLimit::new(exact.work_bytes.get() - 1).unwrap();
    assert!(matches!(
        generated_request(&root, exact).with_generated_rust_input(name, source),
        Err(CargoBuildFailure::Limit("generated Rust input work"))
    ));

    let request = generated_request(&root, CargoBuildLimits::default())
        .with_generated_rust_input(name, source)
        .unwrap();
    let length = request
        .inputs
        .iter()
        .map(|path| {
            fs::metadata(root.0.join(input_spelling(path).unwrap()))
                .unwrap()
                .len()
        })
        .sum::<u64>()
        + source.len() as u64;
    let mut limits = CargoBuildLimits::default();
    limits.input_count = ItemLimit::new(4).unwrap();
    limits.input_bytes = ByteLimit::new(length as usize).unwrap();
    let exact = generated_request(&root, limits)
        .with_generated_rust_input(name, source)
        .unwrap();
    capture_inputs(&host_fs::readable_directory(&root.0).unwrap(), &exact).unwrap();
    limits.input_bytes = ByteLimit::new(length as usize - 1).unwrap();
    let short = generated_request(&root, limits)
        .with_generated_rust_input(name, source)
        .unwrap();
    assert!(matches!(
        capture_inputs(&host_fs::readable_directory(&root.0).unwrap(), &short),
        Err(CargoBuildFailure::Limit("declared input bytes"))
    ));
    limits.input_count = ItemLimit::new(3).unwrap();
    assert!(matches!(
        generated_request(&root, limits).with_generated_rust_input(name, source),
        Err(CargoBuildFailure::Limit("declared input count"))
    ));
}

#[test]
fn generated_input_origin_name_and_bytes_participate_in_path_free_provenance() {
    let root = Root::new();
    let source = b"pub const REQUEST_VALUE: u8 = 17;";
    let request = generated_request(&root, CargoBuildLimits::default())
        .with_generated_rust_input("generated.rs", source)
        .unwrap();
    let digest = ContentDigest::of_bytes(b"tool");
    let mut provenance = CargoBuildProvenance {
        selection: request.selection.clone(),
        resolved_features: Box::default(),
        toolchain: CargoToolchainObservation {
            cargo: digest,
            rustc: digest,
            cargo_version: digest,
            rustc_version: digest,
            host_target: "x86_64-pc-windows-msvc".to_owned(),
        },
        inputs: Vec::new(),
        generated_rust_input: None,
        metadata: ContentDigest::of_bytes(b"metadata"),
        digest,
    };
    let executable = ContentDigest::of_bytes(b"executable");
    let absent = provenance_digest(&provenance, executable).unwrap();
    provenance.generated_rust_input = request.generated_rust_input().cloned();
    let generated = provenance_digest(&provenance, executable).unwrap();
    assert_ne!(generated, absent);
    let fact = provenance.generated_rust_input.take().unwrap();
    provenance.inputs.push(fact.clone());
    assert_ne!(
        provenance_digest(&provenance, executable).unwrap(),
        generated
    );
    provenance.inputs.clear();
    provenance.generated_rust_input = Some(fact);
    provenance.generated_rust_input.as_mut().unwrap().path = "another.rs".to_owned();
    let renamed = provenance_digest(&provenance, executable).unwrap();
    assert_ne!(renamed, generated);
    provenance.generated_rust_input.as_mut().unwrap().digest = ContentDigest::of_bytes(b"B");
    assert_ne!(provenance_digest(&provenance, executable).unwrap(), renamed);
    assert!(!format!("{provenance:?}").contains(std::str::from_utf8(source).unwrap()));
}

#[test]
fn generated_source_and_file_observations_share_preallocation_admission() {
    let root = Root::new();
    let mut request = generated_request(&root, CargoBuildLimits::default());
    for index in 0..597 {
        let name = format!("g{index:03}");
        fs::write(root.0.join(&name), b"").unwrap();
        request.inputs.push(RelativePath::new(name).unwrap());
    }
    let ordinary_work = request
        .inputs
        .iter()
        .map(|path| {
            std::mem::size_of::<ObservedInput>() + (input_spelling(path).unwrap().len() + 1) * 4
        })
        .sum::<usize>();
    let source = vec![b' '; MAX_GENERATED_RUST_SOURCE_BYTES];
    let name = "g.rs";
    let generated_work = generated_source_work(source.len(), name.len()).unwrap();
    request.limits.work_bytes = ByteLimit::new(ordinary_work + generated_work).unwrap();
    let mut request = request.with_generated_rust_input(name, &source).unwrap();
    let root = host_fs::readable_directory(&root.0).unwrap();
    assert_eq!(capture_inputs(&root, &request).unwrap().len(), 600);
    request.limits.work_bytes = ByteLimit::new(ordinary_work + generated_work - 1).unwrap();
    assert!(matches!(
        capture_inputs(&root, &request),
        Err(CargoBuildFailure::Limit("declared input work"))
    ));
    request.limits.work_bytes = ByteLimit::new(ordinary_work.max(generated_work)).unwrap();
    assert!(matches!(
        capture_inputs(&root, &request),
        Err(CargoBuildFailure::Limit("declared input work"))
    ));
}

#[test]
fn separate_directory_sessions_revalidate_the_same_object_and_exclude_reentry() {
    let root = Root::new();
    let lease = TargetBuildLease::acquire_at(root.0.clone()).unwrap();
    lease.verify().unwrap();
    assert!(matches!(
        TargetBuildLease::acquire_at(root.0.clone()),
        Err(CargoBuildFailure::TargetBusy)
    ));
    drop(lease);
    TargetBuildLease::acquire_at(root.0.clone())
        .unwrap()
        .verify()
        .unwrap();
}

#[test]
fn failed_reap_and_forwarded_cleanup_keep_every_original_output_lease() {
    let first = Root::new();
    let second = Root::new();
    let first_lease = TargetBuildLease::acquire_at(first.0.clone()).unwrap();
    let second_lease = TargetBuildLease::acquire_at(second.0.clone()).unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", DIRECT_CLEANUP_CHILD, "--ignored"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut waits = 0;
    let error = finish_child_with(child, |_| {
        waits += 1;
        Err(io::Error::other("injected wait failure"))
    })
    .unwrap_err();
    assert_eq!(waits, 2);
    let CargoBuildFailure::CleanupIncomplete(mut custody) = error else {
        panic!("an unconfirmed direct child must keep cleanup custody")
    };
    // This is the same accumulation performed when an outer build callback forwards an inner
    // incomplete cleanup. Neither output owner may be overwritten by the other one.
    custody.leases.push(first_lease);
    custody.leases.push(second_lease);
    assert!(custody.process_id().is_some());
    for root in [&first, &second] {
        assert!(matches!(
            TargetBuildLease::acquire_at(root.0.clone()),
            Err(CargoBuildFailure::TargetBusy)
        ));
    }
    custody.retry_cleanup().unwrap();
    assert_eq!(custody.process_id(), None);
    for root in [&first, &second] {
        TargetBuildLease::acquire_at(root.0.clone())
            .unwrap()
            .verify()
            .unwrap();
    }
}

#[test]
fn output_limit_and_bootstrap_error_preserve_unconfirmed_child_custody() {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", DIRECT_CLEANUP_CHILD, "--ignored", "--nocapture"])
        .stderr(Stdio::null());
    let error = bounded_status_output_with(&mut command, 1, |child| {
        finish_child_with(child, |_| Err(io::Error::other("injected wait failure")))
    })
    .unwrap_err();
    let error = ProjectToolBootstrapError::from(error);
    let ProjectToolBootstrapError::Process(CargoBuildFailure::CleanupIncomplete(mut custody)) =
        error
    else {
        panic!("output overflow and bootstrap forwarding must retain the unconfirmed child")
    };
    assert!(custody.process_id().is_some());
    custody.retry_cleanup().unwrap();
    assert_eq!(custody.process_id(), None);
}

#[test]
#[ignore = "direct child helper; the owning cleanup test starts and reaps it explicitly"]
fn direct_cleanup_child() {
    std::thread::sleep(Duration::from_secs(20));
}
