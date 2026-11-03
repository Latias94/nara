//! Independent process recovery using only stored project data and the public CLI feature.

use std::{ffi::OsString, io::Write, path::PathBuf, process::ExitCode};

use nara_engine::{
    core::ItemLimit,
    project_host::{
        AuthoringSourceOperationLimits, AuthoredSchemaUpgradeRecovery,
        AuthoredSchemaUpgradeRecoveryMemberState, AuthoredSchemaUpgradeRecoveryPhase,
        AuthoredSchemaUpgradeRecoveryView,
    },
    project_tooling::host_fs,
};

const USAGE: &str = concat!(
    "usage: nara-authored-upgrade-recovery <inspect|resume> <project> ",
    "--profile <base|name> --host-work <nonzero> --open-handles <nonzero> ",
    "--max-steps <nonzero> ",
    "[--all | --stop-after-sources <count|all> | --stop-after-ready]",
);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Inspect,
    Resume,
}

#[derive(Clone, Copy)]
enum Stop {
    Complete,
    Sources(usize),
    SourcesAndBundle,
    ReadyBeforeReceipt,
}

struct Options {
    action: Action,
    root: PathBuf,
    profile: Option<String>,
    limits: AuthoringSourceOperationLimits,
    max_steps: ItemLimit,
    stop: Option<Stop>,
}

fn text(value: Option<OsString>, label: &str) -> Result<String, String> {
    value
        .ok_or_else(|| format!("missing {label}; {USAGE}"))?
        .into_string()
        .map_err(|_| format!("{label} must be UTF-8"))
}

fn finite(value: String, label: &str) -> Result<ItemLimit, String> {
    let count = value
        .parse::<usize>()
        .map_err(|_| format!("{label} must be a nonzero finite integer"))?;
    ItemLimit::new(count).ok_or_else(|| format!("{label} must be nonzero"))
}

fn options() -> Result<Options, String> {
    let mut args = std::env::args_os().skip(1);
    let action = match text(args.next(), "action")?.as_str() {
        "inspect" => Action::Inspect,
        "resume" => Action::Resume,
        _ => return Err(USAGE.into()),
    };
    let root = PathBuf::from(args.next().ok_or(USAGE)?);
    let mut profile = None;
    let mut work = None;
    let mut handles = None;
    let mut steps = None;
    let mut stop = None;
    while let Some(argument) = args.next() {
        let argument = text(Some(argument), "option")?;
        match argument.as_str() {
            "--profile" if profile.is_none() => {
                profile = Some(text(args.next(), "profile")?);
            }
            "--host-work" if work.is_none() => {
                work = Some(finite(text(args.next(), "host work")?, "host work")?);
            }
            "--open-handles" if handles.is_none() => {
                handles = Some(finite(text(args.next(), "open handles")?, "open handles")?);
            }
            "--max-steps" if steps.is_none() => {
                steps = Some(finite(
                    text(args.next(), "maximum steps")?,
                    "maximum steps",
                )?);
            }
            "--all" if stop.is_none() => stop = Some(Stop::Complete),
            "--stop-after-ready" if stop.is_none() => stop = Some(Stop::ReadyBeforeReceipt),
            "--stop-after-sources" if stop.is_none() => {
                let count = text(args.next(), "source count")?;
                stop = Some(if count == "all" {
                    Stop::SourcesAndBundle
                } else {
                    Stop::Sources(count.parse().map_err(|_| "invalid source count")?)
                });
            }
            _ => return Err(format!("unknown or repeated option: {argument}; {USAGE}")),
        }
    }
    if (action == Action::Resume) != stop.is_some() {
        return Err(format!(
            "resume requires exactly one stop selection; {USAGE}"
        ));
    }
    let profile = profile.ok_or("an explicit --profile is required (base selects no overlay)")?;
    Ok(Options {
        action,
        root,
        profile: (profile != "base").then_some(profile),
        limits: AuthoringSourceOperationLimits::default()
            .with_host_work(work.ok_or("--host-work is required")?)
            .with_open_handles(handles.ok_or("--open-handles is required")?),
        max_steps: steps.ok_or("--max-steps is required")?,
        stop,
    })
}

fn member_name(state: AuthoredSchemaUpgradeRecoveryMemberState) -> &'static str {
    match state {
        AuthoredSchemaUpgradeRecoveryMemberState::Original => "original",
        AuthoredSchemaUpgradeRecoveryMemberState::Successor => "successor",
        AuthoredSchemaUpgradeRecoveryMemberState::Unchanged => "unchanged",
    }
}

fn print_view(view: AuthoredSchemaUpgradeRecoveryView) {
    let phase = match view.phase {
        AuthoredSchemaUpgradeRecoveryPhase::Review => "review",
        AuthoredSchemaUpgradeRecoveryPhase::PublishingSources => "publishing-sources",
        AuthoredSchemaUpgradeRecoveryPhase::PreparingReady => "preparing-ready",
        AuthoredSchemaUpgradeRecoveryPhase::PublishingReady => "publishing-ready",
        AuthoredSchemaUpgradeRecoveryPhase::Complete => "complete",
    };
    println!(
        "view phase={phase} sources={} completed={} bundle={}",
        view.source_count, view.completed_sources, view.bundle_complete,
    );
}

fn reached(stop: Stop, view: AuthoredSchemaUpgradeRecoveryView) -> bool {
    match stop {
        Stop::Sources(count) => view.completed_sources >= count,
        Stop::SourcesAndBundle => view.bundle_complete,
        Stop::Complete | Stop::ReadyBeforeReceipt => {
            view.phase == AuthoredSchemaUpgradeRecoveryPhase::Complete
        }
    }
}

fn run() -> Result<(), String> {
    let options = options()?;
    let root = host_fs::writable_directory(&options.root).map_err(|error| error.to_string())?;
    let mut recovery =
        AuthoredSchemaUpgradeRecovery::inspect(root, options.profile.as_deref(), options.limits)
            .map_err(|error| format!("inspect refused kind={:?}: {error}", error.kind()))?;
    let review = recovery
        .review()
        .ok_or("inspection did not retain Review")?;
    let mut original = 0;
    let mut successor = 0;
    let mut unchanged = 0;
    let mut dirty = 0;
    for source in review.sources() {
        match source.observed {
            AuthoredSchemaUpgradeRecoveryMemberState::Original => original += 1,
            AuthoredSchemaUpgradeRecoveryMemberState::Successor => successor += 1,
            AuthoredSchemaUpgradeRecoveryMemberState::Unchanged => unchanged += 1,
        }
        dirty += usize::from(source.original_disk != source.reviewed_working);
    }
    println!(
        "review sources={} original={original} successor={successor} unchanged={unchanged} dirty={dirty} bundle={}",
        review.sources().len(),
        member_name(review.bundle_state()),
    );
    println!(
        "schema previous_owners={} successor_owners={}",
        review.schema_bundle().owners().len(),
        review.successor_schema_bundle().owners().len(),
    );
    if options.action == Action::Inspect {
        print_view(recovery.view());
        return Ok(());
    }
    let stop = options.stop.ok_or("missing resume stop selection")?;
    if matches!(stop, Stop::Sources(count) if count > recovery.view().source_count) {
        return Err("stop source count exceeds the complete reviewed membership".into());
    }
    let mut view = recovery
        .confirm()
        .map_err(|error| format!("confirm refused kind={:?}: {error}", error.kind()))?;
    for _ in 0..options.max_steps.get() {
        if reached(stop, view) {
            break;
        }
        view = recovery
            .advance()
            .map_err(|error| format!("advance refused kind={:?}: {error}", error.kind()))?;
    }
    print_view(view);
    if !reached(stop, view) {
        return Err("finite step limit reached before the selected stop".into());
    }
    if matches!(stop, Stop::ReadyBeforeReceipt) {
        // Simulate process loss after the final Ready write, before consuming its receipt.
        // No old Editor Session exists in this executable, and no destructor acknowledges it.
        println!("ready receipt=unconsumed acknowledged=false");
        std::io::stdout()
            .flush()
            .map_err(|error| error.to_string())?;
        std::process::exit(0);
    }
    if view.phase == AuthoredSchemaUpgradeRecoveryPhase::Complete {
        let receipt = recovery
            .into_receipt()
            .map_err(|_| "Complete did not supply the actual Ready receipt")?;
        let ready = receipt.ready_publication();
        if ready.verified_content_digest().is_none()
            || ready.published_identity() != Some(ready.candidate_identity())
            || ready.previous_identity().is_none()
        {
            return Err("the Ready receipt lacks confirmed replacement evidence".into());
        }
        println!(
            "ready confirmed=true sources={} content_verified=true",
            receipt.source_count(),
        );
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("recovery rejected: {error}");
            ExitCode::from(2)
        }
    }
}
