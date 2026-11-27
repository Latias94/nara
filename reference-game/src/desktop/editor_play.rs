use std::{
    error::Error,
    io::{self, BufRead, IsTerminal, Write},
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    thread,
    time::Duration,
};

use nara::{
    app::{
        App, RuntimeControl, RuntimeControlRequestResult, RuntimeControlStatus,
        RuntimeControlTicket, RuntimeInstance,
    },
    render_wgpu::desktop::{DesktopPresentation, DesktopPresentationDriver},
    winit::{WinitControlFlow, WinitRunProgress, WinitRunner},
};

use crate::Round;

use super::is_game_frame;

const POLL_WAIT: Duration = Duration::from_millis(16);

#[derive(Clone, Copy)]
struct EditorCommand {
    control: RuntimeControl,
    acknowledgement: &'static str,
}

pub(super) fn run(app: App) -> Result<(), Box<dyn Error>> {
    let commands = start_input_reader()?;
    let runtime = RuntimeInstance::start(app)?;
    let mut session = WinitRunner::default().start_with_presentation_on(
        runtime,
        Box::new(DesktopPresentationDriver::default()),
        Duration::from_secs(5),
        None,
    );
    let mut ready = false;
    let mut pending = None::<(EditorCommand, RuntimeControlTicket)>;
    loop {
        let progress = session.poll(POLL_WAIT);
        if let Some((command, ticket)) = pending {
            match session.control_status(ticket) {
                Some(RuntimeControlStatus::Pending) => {}
                Some(RuntimeControlStatus::Applied) => {
                    pending = None;
                    report(command.acknowledgement)?;
                    if progress == WinitRunProgress::Running {
                        match command.control {
                            RuntimeControl::Pause => {
                                session.set_control_flow(WinitControlFlow::Wait)?
                            }
                            RuntimeControl::Resume => {
                                session.set_control_flow(WinitControlFlow::Poll)?
                            }
                            _ => {}
                        }
                    }
                }
                status => {
                    pending = None;
                    eprintln!(
                        "editor control {:?} did not complete: {status:?}",
                        command.control
                    );
                }
            }
        }

        match progress {
            WinitRunProgress::Running => {}
            WinitRunProgress::Retiring => continue,
            WinitRunProgress::Retired | WinitRunProgress::Incomplete => break,
        }
        #[cfg(feature = "hot-reload")]
        {
            use nara::app::RuntimeState;

            let world = session.runtime().world();
            if let Some(reload) = world.get_resource::<super::reload::Reload>() {
                reload.observe(world.resource::<DesktopPresentation>());
                if session.runtime().state() == RuntimeState::Paused && reload.notified() {
                    session.request_paused_maintenance()?;
                }
            }
        }
        if !ready {
            if !session
                .runtime()
                .world()
                .resource::<DesktopPresentation>()
                .latest()
                .as_ref()
                .is_some_and(is_game_frame)
            {
                continue;
            }
            let shields = session.runtime().world().resource::<Round>().shields();
            println!("Night Relay started with {shields} shields");
            report("ready")?;
            ready = true;
        }
        if let (None, Some(commands)) = (pending, commands.as_ref()) {
            match commands.try_recv() {
                Ok(command) => match session.request_control(command.control) {
                    RuntimeControlRequestResult::Accepted(ticket) => {
                        pending = Some((command, ticket))
                    }
                    RuntimeControlRequestResult::Rejected(reason) => {
                        eprintln!(
                            "editor control {:?} was rejected: {reason:?}",
                            command.control
                        );
                    }
                },
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    session.request_close();
                    session.execute();
                    break;
                }
            }
        }
    }
    session
        .into_retired()
        .map_err(|_| "could not finish retiring the desktop window")?
        .result?;
    Ok(())
}

fn report(event: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "NARA_EDITOR {event}")?;
    stdout.flush()
}

fn start_input_reader() -> io::Result<Option<Receiver<EditorCommand>>> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return Ok(None);
    }
    let (sender, receiver) = mpsc::sync_channel(16);
    thread::Builder::new()
        .name("editor-control-stdin".into())
        .spawn(move || {
            if let Err(error) = read_commands(stdin.lock(), sender) {
                eprintln!("could not read editor controls: {error}");
            }
        })?;
    Ok(Some(receiver))
}

fn read_commands(mut input: impl BufRead, sender: SyncSender<EditorCommand>) -> io::Result<()> {
    let mut line = [0; b"resume\r".len()];
    let mut length = 0;
    loop {
        let bytes = input.fill_buf()?;
        if bytes.is_empty() {
            return Ok(());
        }
        let available = bytes.len();
        for &byte in bytes {
            if byte != b'\n' {
                if let Some(slot) = line.get_mut(length) {
                    *slot = byte;
                }
                length = (length + 1).min(line.len() + 1);
                continue;
            }
            let command = line.get(..length).and_then(parse_command);
            length = 0;
            if let Some(command) = command
                && sender.send(command).is_err()
            {
                return Ok(());
            }
        }
        input.consume(available);
    }
}

fn parse_command(line: &[u8]) -> Option<EditorCommand> {
    let (control, acknowledgement) = match line.strip_suffix(b"\r").unwrap_or(line) {
        b"pause" => (RuntimeControl::Pause, "paused"),
        b"resume" => (RuntimeControl::Resume, "running"),
        b"step" => (RuntimeControl::StepFixedTick, "stepped"),
        _ => return None,
    };
    Some(EditorCommand {
        control,
        acknowledgement,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_input_admits_only_complete_protocol_lines() -> io::Result<()> {
        let (sender, receiver) = mpsc::sync_channel(4);
        read_commands(
            &b"pause\r\n\xff\nstep\nstep extra\nresume\npause"[..],
            sender,
        )?;
        let commands: Vec<_> = receiver
            .into_iter()
            .map(|command| command.control)
            .collect();
        assert_eq!(
            commands,
            [
                RuntimeControl::Pause,
                RuntimeControl::StepFixedTick,
                RuntimeControl::Resume
            ]
        );
        Ok(())
    }
}
