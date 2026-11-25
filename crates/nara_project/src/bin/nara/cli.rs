use std::{
    env,
    error::Error,
    ffi::OsString,
    fs,
    io::{self, Write},
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
    delivery::DeliveryOperation,
    play::{
        BuildDiagnostic, BuildError, BuildEvent, BuildRequest, OutputLine, OutputStream,
        PlayProcess, build_interruptible,
    },
    workflow::{
        COOKER_BINARY, DeliveryError, DeliveryRequest, GAME_BINARY, ProjectCheckError,
        ProjectTrust, SCHEMA_BINARY, SCHEMA_CACHE_PATH, SchemaCacheError, check_project,
        create_project, deliver_project, read_schema_cache, refresh_schema_interruptible,
    },
};
use serde::Deserialize;

#[path = "args.rs"]
mod args;
use args::{BuildOptions, Command, DeliveryOptions};

const HELP: &str = "\
Nara project workflow

Usage:
  nara new <destination> --package <name> --nara-checkout <path>
  nara check <project> [--profile <name>]
  nara schema <project> [--trusted]
  nara build <project> --trusted [--bin <name>] [--features <list>] [--release]
  nara play <project> --trusted [--bin <name>] [--features <list>] [--release] [-- <game args>]
  nara cook <project> --trusted --recipe <name> --target <triple> --output-parent <dir> --name <name> [cooker options]
  nara export <project> --trusted --recipe <name> --target <triple> --output-parent <dir> --name <name> [cooker options]

Schema reads the last cache unless --trusted is supplied. A trusted command may
execute Cargo build scripts and project code. Build and Play use Cargo's
default-run target or game, plus the target's declared required features.
Cook and Export build the project's cook bin with its declared required features.
Cook publishes content.pack. Export publishes the executable, content.pack, and
package.json. The output parent must exist and the named destination must be new.
Cooker options are [--bin <name>] [--features <list>] [--release] [-- <cooker args>].
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
        Command::Check { project, profile } => {
            let checked = check_project(&project, profile.as_deref()).inspect_err(|error| {
                if let ProjectCheckError::Manifest { diagnostics } = error {
                    let mut stderr = io::stderr().lock();
                    for entry in diagnostics.iter() {
                        let _ = writeln!(
                            stderr,
                            "{}: {}",
                            entry.code().as_str(),
                            entry.summary().as_str()
                        );
                    }
                }
            })?;
            writeln!(
                io::stdout().lock(),
                "Checked nara.toml, {} scene(s), {} prefab(s), and {} asset path(s).",
                checked.scenes,
                checked.prefabs,
                checked.asset_paths,
            )?;
            if checked.deferred != 0 {
                let _ = writeln!(
                    io::stderr().lock(),
                    "warning: {} component or reference check(s) need Schema or runtime data",
                    checked.deferred,
                );
            }
        }
        Command::Schema { project, refresh } => {
            let catalog = if refresh {
                let manifest = CargoPackage::read(&project)?;
                let request = BuildRequest::new(&project, &manifest.package.name, SCHEMA_BINARY);
                let required = manifest.features(SCHEMA_BINARY, &[]);
                let interrupted = watch_interrupt()?;
                let console = WorkflowConsole::start()?;
                let result = refresh_schema_interruptible(
                    &BuildRequest {
                        features: &required,
                        ..request
                    },
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
        Command::Build {
            project,
            options,
            game_args,
        } => {
            return build_or_play(&project, &options, game_args.as_deref());
        }
        Command::Delivery {
            project,
            operation,
            options,
        } => return run_delivery(&project, operation, &options),
    }
    Ok(ExitCode::SUCCESS)
}

fn build_or_play(
    project: &Path,
    options: &BuildOptions,
    game_args: Option<&[OsString]>,
) -> Result<ExitCode, Box<dyn Error>> {
    let manifest = CargoPackage::read(project)?;
    let binary = options
        .binary
        .as_deref()
        .or(manifest.package.default_run.as_deref())
        .unwrap_or(GAME_BINARY);
    let features = manifest.features(binary, &options.features);
    let request = BuildRequest {
        features: &features,
        release: options.release,
        target: game_args.map(|_| "host-tuple"),
        ..BuildRequest::new(project, &manifest.package.name, binary)
    };
    let interrupted = watch_interrupt()?;
    let console = WorkflowConsole::start()?;
    let result = build_interruptible(
        &request,
        || interrupted.load(Ordering::Relaxed),
        |event| console.build(event),
    );
    let executable = match result {
        Ok(executable) => executable,
        Err(error) => {
            console.finish(&interrupted);
            return match error {
                BuildError::Interrupted => Ok(ExitCode::from(130)),
                error => Err(error.into()),
            };
        }
    };
    if interrupted.load(Ordering::Relaxed) {
        return Ok(ExitCode::from(130));
    }
    if let Some(arguments) = game_args {
        let result = PlayProcess::launch(project, &executable, arguments)
            .map_err(|error| -> Box<dyn Error> { error.into() })
            .and_then(|process| wait_for_play(process, &interrupted, &console));
        console.finish(&interrupted);
        return result;
    }
    console.finish(&interrupted);
    writeln!(io::stdout().lock(), "{}", executable.display())?;
    Ok(ExitCode::SUCCESS)
}

fn run_delivery(
    project: &Path,
    operation: DeliveryOperation,
    options: &DeliveryOptions,
) -> Result<ExitCode, Box<dyn Error>> {
    let manifest = CargoPackage::read(project)?;
    let binary = options.build.binary.as_deref().unwrap_or(COOKER_BINARY);
    let features = manifest.features(binary, &options.build.features);
    let request = DeliveryRequest {
        build: BuildRequest {
            features: &features,
            release: options.build.release,
            ..BuildRequest::new(project, &manifest.package.name, binary)
        },
        operation,
        recipe: &options.recipe,
        target: &options.target,
        output_parent: &options.output_parent,
        name: &options.name,
        cooker_args: &options.cooker_args,
    };
    let interrupted = watch_interrupt()?;
    let console = WorkflowConsole::start()?;
    let result = deliver_project(
        &request,
        ProjectTrust::Trusted,
        || interrupted.load(Ordering::Relaxed),
        |event| console.build(event),
    );
    console.finish(&interrupted);
    let receipt = match result {
        Err(DeliveryError::Interrupted) => return Ok(ExitCode::from(130)),
        Err(error @ DeliveryError::UnconfirmedPublication { .. })
            if interrupted.load(Ordering::Relaxed) =>
        {
            writeln!(io::stderr().lock(), "error: {error}")?;
            return Ok(ExitCode::from(130));
        }
        result => result?,
    };
    let path = match operation {
        DeliveryOperation::Cook => &receipt.content_pack,
        DeliveryOperation::Export => &receipt.directory,
    };
    writeln!(io::stdout().lock(), "{}", path.display())?;
    if let Some(error) = receipt.postpublication_error {
        let message = format!("published {} with an error: {error}", path.display());
        if interrupted.load(Ordering::Relaxed) {
            writeln!(io::stderr().lock(), "error: {message}")?;
            return Ok(ExitCode::from(130));
        }
        return Err(message.into());
    }
    Ok(ExitCode::SUCCESS)
}

fn watch_interrupt() -> Result<Arc<AtomicBool>, ctrlc::Error> {
    let interrupted = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&interrupted);
    ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))?;
    Ok(interrupted)
}

fn wait_for_play(
    mut process: PlayProcess,
    interrupted: &AtomicBool,
    console: &WorkflowConsole,
) -> Result<ExitCode, Box<dyn Error>> {
    loop {
        if interrupted.load(Ordering::Relaxed) {
            process.stop(|line| console.game(line))?;
            return Ok(ExitCode::from(130));
        }
        if let Some(status) = process.poll(|line| console.game(line))? {
            return Ok(status
                .code()
                .and_then(|code| u8::try_from(code).ok())
                .map_or(ExitCode::FAILURE, ExitCode::from));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

enum ConsoleEvent {
    Build(BuildEvent),
    Game(OutputLine),
}

// Keep a blocked console pipe out of the process-supervision loop. Flush on normal completion.
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

fn write_diagnostic(output: &mut impl Write, diagnostic: &BuildDiagnostic) -> io::Result<()> {
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

#[derive(Deserialize)]
struct CargoPackage {
    package: Package,
    #[serde(default, rename = "bin")]
    targets: Vec<Binary>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    #[serde(rename = "default-run")]
    default_run: Option<String>,
}

#[derive(Deserialize)]
struct Binary {
    name: String,
    #[serde(default, rename = "required-features")]
    required_features: Vec<String>,
}

impl CargoPackage {
    fn read(project: &Path) -> Result<Self, Box<dyn Error>> {
        let path = project.join("Cargo.toml");
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        toml::from_str(&text).map_err(|error| {
            format!("could not read [package] in {}: {error}", path.display()).into()
        })
    }

    fn features<'selection>(
        &'selection self,
        binary: &str,
        additional: &'selection [String],
    ) -> Vec<&'selection str> {
        let mut features: Vec<_> = self
            .targets
            .iter()
            .find(|target| target.name == binary)
            .into_iter()
            .flat_map(|target| &target.required_features)
            .chain(additional)
            .map(String::as_str)
            .collect();
        features.sort_unstable();
        features.dedup();
        features
    }
}
