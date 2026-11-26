use std::{
    env,
    error::Error,
    ffi::OsString,
    io::{self, Write as _},
    path::Path,
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{SyncSender, TrySendError, sync_channel},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use nara_project::{
    play::{
        BuildDiagnostic, BuildError, BuildEvent, BuildRequest, OutputLine, OutputStream,
        PlayProcess, ProjectTrust, build_interruptible,
    },
    workflow::{
        CargoPackage, SCHEMA_BINARY, SCHEMA_CACHE_PATH, SchemaCacheError, check_project,
        create_project, read_schema_cache, refresh_schema_interruptible,
    },
};

#[path = "args.rs"]
mod args;
use args::{Command, PlayOptions};

const HELP: &str = "\
Nara project workflow

Usage:
  nara new <destination> --package <name> --nara-checkout <path>
  nara check <project>
  nara schema <project> [--trusted]
  nara play <project> --trusted [--bin <name>] [--features <list>] [--release] [-- <game args>]

Check validates nara.toml and the local Cargo package declaration. Schema reads
the last cache by default. With --trusted it builds and runs the exporter.
Play builds and runs Cargo's default-run target or game, with the target's
declared required features. Trusted commands can execute project code.
";

pub(super) fn run() -> ExitCode {
    match args::parse(env::args_os().skip(1)) {
        Ok(command) => match execute(command) {
            Ok(code) => code,
            Err(error) => {
                let _ = writeln!(io::stderr().lock(), "error: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "error: {error}\n\n{HELP}");
            ExitCode::from(2)
        }
    }
}

fn execute(command: Command) -> Result<ExitCode, Box<dyn Error>> {
    match command {
        Command::Help => write!(io::stdout().lock(), "{HELP}")?,
        Command::Version => writeln!(io::stdout().lock(), "nara {}", env!("CARGO_PKG_VERSION"))?,
        Command::New {
            destination,
            package,
            checkout,
        } => {
            create_project(&destination, &package, &checkout)?;
            writeln!(io::stdout().lock(), "Created {}.", destination.display())?;
        }
        Command::Check(project) => {
            check_project(&project)?;
            writeln!(
                io::stdout().lock(),
                "Checked nara.toml and the Cargo package declaration."
            )?;
        }
        Command::Schema { project, refresh } => {
            let catalog = if refresh {
                let package = CargoPackage::read(&project)?;
                let features: Vec<_> = package
                    .required_features(SCHEMA_BINARY)
                    .iter()
                    .map(String::as_str)
                    .collect();
                let request = BuildRequest {
                    features: &features,
                    ..BuildRequest::new(&project, package.name(), SCHEMA_BINARY)
                };
                let interrupted = watch_interrupt()?;
                let console = WorkflowConsole::start()?;
                let result = refresh_schema_interruptible(
                    &request,
                    ProjectTrust::Trusted,
                    || interrupted.load(Ordering::Relaxed),
                    |event| console.build(event),
                );
                console.finish(&interrupted);
                match result {
                    Err(SchemaCacheError::Interrupted) => return Ok(ExitCode::from(130)),
                    result => result?,
                }
            } else {
                read_schema_cache(&project)?.ok_or_else(|| {
                    format!("no Schema cache at {SCHEMA_CACHE_PATH}. Create it with `nara schema <project> --trusted`")
                })?
            };
            writeln!(io::stdout().lock(), "{}", catalog.to_json_string()?)?;
        }
        Command::Play {
            project,
            options,
            game_args,
        } => return play(&project, &options, &game_args),
    }
    Ok(ExitCode::SUCCESS)
}

fn play(
    project: &Path,
    options: &PlayOptions,
    game_args: &[OsString],
) -> Result<ExitCode, Box<dyn Error>> {
    let package = CargoPackage::read(project)?;
    let binary = options
        .binary
        .as_deref()
        .unwrap_or_else(|| package.default_binary());
    let mut features: Vec<_> = package
        .required_features(binary)
        .iter()
        .chain(&options.features)
        .map(String::as_str)
        .collect();
    features.sort_unstable();
    features.dedup();
    let request = BuildRequest {
        features: &features,
        release: options.release,
        ..BuildRequest::new(project, package.name(), binary)
    };
    let interrupted = watch_interrupt()?;
    let stop_requested = || interrupted.load(Ordering::Relaxed);
    let console = WorkflowConsole::start()?;
    let trust = ProjectTrust::Trusted;
    let result = (|| {
        let executable = match build_interruptible(&request, trust, stop_requested, |event| {
            console.build(event)
        }) {
            Err(BuildError::Interrupted) => return Ok(ExitCode::from(130)),
            result => result?,
        };
        if stop_requested() {
            return Ok(ExitCode::from(130));
        }
        let status = PlayProcess::launch(project, &executable, game_args, trust)?
            .wait_interruptible(stop_requested, |line| console.game(line))?;
        if stop_requested() {
            return Ok(ExitCode::from(130));
        }
        Ok(status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .map_or(ExitCode::FAILURE, ExitCode::from))
    })();
    console.finish(&interrupted);
    result
}

fn watch_interrupt() -> Result<Arc<AtomicBool>, ctrlc::Error> {
    let interrupted = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&interrupted);
    ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))?;
    Ok(interrupted)
}

enum ConsoleEvent {
    Build(BuildEvent),
    Game(OutputLine),
}

struct WorkflowConsole {
    sender: SyncSender<ConsoleEvent>,
    omitted: Arc<AtomicUsize>,
    writer: JoinHandle<()>,
}

impl WorkflowConsole {
    fn start() -> io::Result<Self> {
        let (sender, receiver) = sync_channel(256);
        let omitted = Arc::new(AtomicUsize::new(0));
        let skipped = Arc::clone(&omitted);
        let writer = thread::Builder::new()
            .name("nara-console".into())
            .spawn(move || {
                for event in receiver {
                    match event {
                        ConsoleEvent::Build(event) => build_event(event),
                        ConsoleEvent::Game(line) => game_output(line),
                    }
                }
                let skipped = skipped.load(Ordering::Relaxed);
                if skipped != 0 {
                    let _ = writeln!(
                        io::stderr().lock(),
                        "omitted {skipped} buffered log entries"
                    );
                }
            })?;
        Ok(Self {
            sender,
            omitted,
            writer,
        })
    }

    fn build(&self, event: BuildEvent) {
        self.send(ConsoleEvent::Build(event));
    }

    fn game(&self, line: OutputLine) {
        self.send(ConsoleEvent::Game(line));
    }

    fn send(&self, event: ConsoleEvent) {
        if matches!(self.sender.try_send(event), Err(TrySendError::Full(_))) {
            self.omitted.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn finish(self, interrupted: &AtomicBool) {
        drop(self.sender);
        while !self.writer.is_finished() && !interrupted.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(10));
        }
        if self.writer.is_finished() {
            let _ = self.writer.join();
        }
    }
}

fn build_event(event: BuildEvent) {
    let mut stderr = io::stderr().lock();
    match event {
        BuildEvent::Output(line) => {
            let _ = writeln!(stderr, "{}", line.text);
        }
        BuildEvent::Diagnostic { diagnostic, .. } => {
            let _ = write_diagnostic(&mut stderr, &diagnostic);
        }
    }
}

fn write_diagnostic(output: &mut impl io::Write, diagnostic: &BuildDiagnostic) -> io::Result<()> {
    if let Some(rendered) = &diagnostic.rendered {
        return writeln!(output, "{}", rendered.trim_end_matches(['\r', '\n']));
    }
    let structured = diagnostic.diagnostic();
    if let Some(location) = structured.location {
        write!(output, "{}:", location.path.display())?;
        if let Some((line, column)) = location.line_column {
            write!(output, "{line}:{column}:")?;
        }
        write!(output, " ")?;
    }
    write!(output, "{}", diagnostic.level)?;
    if let Some(code) = structured.code {
        write!(output, "[{code}]")?;
    }
    writeln!(output, ": {}", structured.message)?;
    for child in &diagnostic.children {
        write_diagnostic(output, child)?;
    }
    Ok(())
}

fn game_output(line: OutputLine) {
    match line.stream {
        OutputStream::Stdout => {
            let _ = writeln!(io::stdout().lock(), "{}", line.text);
        }
        OutputStream::Stderr => {
            let _ = writeln!(io::stderr().lock(), "{}", line.text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_preserves_cargo_levels_positions_and_rendered_children() {
        let diagnostic = serde_json::from_value(serde_json::json!({
            "level": "failure-note",
            "code": { "code": "E0001" },
            "message": "the build failed",
            "rendered": null,
            "spans": [{
                "file_name": "game.rs",
                "line_start": 2,
                "line_end": 2,
                "column_start": 3,
                "column_end": 3,
                "is_primary": true
            }],
            "children": [{
                "level": "help",
                "message": "original help",
                "rendered": "rendered help\r\n",
                "spans": [],
                "children": []
            }]
        }))
        .unwrap();
        let mut output = Vec::new();
        write_diagnostic(&mut output, &diagnostic).unwrap();
        assert_eq!(
            output,
            b"game.rs:2:3: failure-note[E0001]: the build failed\nrendered help\n"
        );
    }
}
