//! Finite native-platform children; no language SDK or separately built executable is needed.

use serde_json::{Value, json};

use super::*;

pub(super) enum Output {
    Result(Value),
    Wire(Value),
    Malformed,
    StdoutLimit,
    StderrLimit,
    ValueBudget,
    Nonzero,
    Timeout,
}

pub(super) fn command(
    directory: &Path,
    describe: Output,
    invoke: Output,
    timeout: Duration,
) -> EditorSourceToolCommand {
    #[cfg(windows)]
    let (executable, arguments) = {
        let executable = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let script = format!(
            "$ErrorActionPreference = 'Stop'; \
             $message = [Console]::In.ReadToEnd() | ConvertFrom-Json; \
             if ($message.operation -eq 'describe') {{ {} }} else {{ {} }}",
            describe.windows(),
            invoke.windows(),
        );
        (
            executable,
            vec![
                OsString::from("-NoLogo"),
                OsString::from("-NoProfile"),
                OsString::from("-NonInteractive"),
                OsString::from("-Command"),
                OsString::from(script),
            ],
        )
    };
    #[cfg(unix)]
    let (executable, arguments) = {
        // WireRequest serializes request before operation. Parse only our own bounded input.
        let script = format!(
            "input=$(cat); sequence=${{input#*\\\"request\\\":}}; \
             sequence=${{sequence%%,*}}; \
             case \"$input\" in *'\"operation\":\"describe\"'*) {} ;; *) {} ;; esac",
            describe.unix(),
            invoke.unix(),
        );
        (
            PathBuf::from("/bin/sh"),
            vec![OsString::from("-c"), OsString::from(script)],
        )
    };
    EditorSourceToolCommand::new(executable, arguments, directory.to_owned(), timeout).unwrap()
}

impl Output {
    fn encoded(&self) -> String {
        let wire = match self {
            Self::Result(result) => json!({
                "protocol_version": 1,
                "request": "__REQUEST__",
                "result": result,
            }),
            Self::Wire(wire) => wire.clone(),
            _ => unreachable!("only wire responses are encoded"),
        };
        serde_json::to_string(&wire)
            .unwrap()
            .replace("\"__REQUEST__\"", "__REQUEST__")
    }

    #[cfg(windows)]
    fn windows(&self) -> String {
        match self {
            Self::Malformed => "[Console]::Out.Write('{')".into(),
            Self::StdoutLimit => "[Console]::Out.Write(('x' * 65537))".into(),
            Self::StderrLimit => "[Console]::Error.Write(('x' * 16385))".into(),
            Self::ValueBudget => String::from(
                "$writes = @(1..5 | ForEach-Object { \
                 @{ property = @{ root = 'amount'; steps = @() }; \
                    value = @{ type = 'string'; value = ('x' * 7000) } } }); \
                 @{ protocol_version = 1; request = $message.request; \
                    result = @{ kind = 'proposal'; title = 'Large values'; writes = $writes } } \
                 | ConvertTo-Json -Depth 8 -Compress",
            ),
            Self::Nonzero => "exit 23".into(),
            Self::Timeout => "Start-Sleep -Seconds 2".into(),
            Self::Result(_) | Self::Wire(_) => {
                let encoded = self.encoded();
                let expression = encoded
                    .split("__REQUEST__")
                    .map(|part| format!("'{}'", part.replace('\'', "''")))
                    .collect::<Vec<_>>()
                    .join(" + $message.request + ");
                format!("[Console]::Out.Write({expression})")
            }
        }
    }

    #[cfg(unix)]
    fn unix(&self) -> String {
        match self {
            Self::Malformed => "printf '%s' '{'".into(),
            Self::StdoutLimit => "printf '%65537s' ''".into(),
            Self::StderrLimit => "printf '%16385s' '' >&2".into(),
            Self::ValueBudget => String::from(
                "printf '%s%s%s' '{\"protocol_version\":1,\"request\":' \"$sequence\" \
                 ',\"result\":{\"kind\":\"proposal\",\"title\":\"Large values\",\"writes\":['; \
                 separator=''; for item in 1 2 3 4 5; do \
                 printf '%s%s' \"$separator\" \
                 '{\"property\":{\"root\":\"amount\",\"steps\":[]},\"value\":{\"type\":\"string\",\"value\":\"'; \
                 printf '%7000s' ''; printf '%s' '\"}}'; separator=','; done; printf '%s' ']}}'",
            ),
            Self::Nonzero => "exit 23".into(),
            // Replacing the shell keeps this timeout fixture a single finite direct child.
            Self::Timeout => "exec sleep 2".into(),
            Self::Result(_) | Self::Wire(_) => {
                let encoded = self.encoded();
                let mut parts = encoded.split("__REQUEST__");
                let prefix = parts.next().unwrap().replace('\'', "'\\''");
                match parts.next() {
                    Some(suffix) => format!(
                        "printf '%s%s%s' '{prefix}' \"$sequence\" '{}'",
                        suffix.replace('\'', "'\\''"),
                    ),
                    None => format!("printf '%s' '{prefix}'"),
                }
            }
        }
    }
}
