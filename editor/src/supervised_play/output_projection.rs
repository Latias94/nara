use std::{collections::VecDeque, path::Path};

use nara::{
    project_workflow::__private::{
        CargoCompilerDiagnostic, CargoDiagnosticLevel, CargoDiagnostics,
    },
    tooling::{
        EditorExternalPlayCompilerLevel, EditorExternalPlayDiagnostics, EditorExternalPlayMessage,
        EditorExternalPlayMessageKind, EditorExternalPlayOutput,
    },
};

use super::{
    ProcessOutputStream,
    failure::HostFailureFacts,
    log::{ProcessLogRing, ProcessLogStats},
};

const OUTPUT_MESSAGE_LIMIT: usize = 128;
const OUTPUT_BYTE_LIMIT: usize = 128 * 1024;
const OUTPUT_MESSAGE_BYTE_LIMIT: usize = 1024;
const OUTPUT_CODE_BYTE_LIMIT: usize = 128;

#[derive(Debug, Clone, Copy)]
struct ProjectionLimits {
    message_limit: usize,
    byte_limit: usize,
    message_byte_limit: usize,
}

impl Default for ProjectionLimits {
    fn default() -> Self {
        Self {
            message_limit: OUTPUT_MESSAGE_LIMIT,
            byte_limit: OUTPUT_BYTE_LIMIT,
            message_byte_limit: OUTPUT_MESSAGE_BYTE_LIMIT,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ProcessCursor {
    sequence: u64,
    dropped_records: u64,
    dropped_bytes: u64,
    truncated_records: u64,
}

/// Deep private module that converts untrusted Editor-owned output into one neutral snapshot.
pub(super) struct EditorExternalPlayOutputProjection {
    limits: ProjectionLimits,
    sequence: u64,
    compiler: EditorExternalPlayDiagnostics,
    compiler_entries_observed: usize,
    messages: VecDeque<EditorExternalPlayMessage>,
    retained_bytes: usize,
    dropped_messages: u64,
    dropped_bytes: u64,
    truncated_messages: u64,
    loss_observed: bool,
    product_cursor: ProcessCursor,
    loss_notice_published: bool,
    published: EditorExternalPlayOutput,
}

impl EditorExternalPlayOutputProjection {
    pub(super) fn new() -> Self {
        Self::with_limits(ProjectionLimits::default())
    }

    fn with_limits(limits: ProjectionLimits) -> Self {
        Self {
            limits,
            sequence: 0,
            compiler: EditorExternalPlayDiagnostics::default(),
            compiler_entries_observed: 0,
            messages: VecDeque::new(),
            retained_bytes: 0,
            dropped_messages: 0,
            dropped_bytes: 0,
            truncated_messages: 0,
            loss_observed: false,
            product_cursor: ProcessCursor::default(),
            loss_notice_published: false,
            published: EditorExternalPlayOutput::default(),
        }
    }

    pub(super) fn reset_attempt(&mut self) {
        self.sequence = 0;
        self.compiler = EditorExternalPlayDiagnostics::default();
        self.compiler_entries_observed = 0;
        self.messages.clear();
        self.messages.shrink_to_fit();
        self.retained_bytes = 0;
        self.dropped_messages = 0;
        self.dropped_bytes = 0;
        self.truncated_messages = 0;
        self.loss_observed = false;
        self.product_cursor = ProcessCursor::default();
        self.loss_notice_published = false;
        self.published = EditorExternalPlayOutput::default();
    }

    pub(super) fn clear(&mut self) {
        self.reset_attempt();
    }

    pub(super) fn snapshot(&self) -> EditorExternalPlayOutput {
        self.published.clone()
    }

    pub(super) fn observe_host_failure(
        &mut self,
        stage: &'static str,
        reason: &'static str,
        source: Option<&(dyn std::error::Error + 'static)>,
    ) {
        self.observe_host_failure_facts(HostFailureFacts::capture(stage, reason, source));
    }

    pub(super) fn observe_host_failure_facts(&mut self, facts: HostFailureFacts) {
        // Only fixed owner labels and numeric/category I/O facts cross the public output seam.
        let mut text = format!(
            "External Play failed: stage={}; reason={}",
            facts.stage, facts.reason
        );
        if let Some(operation) = facts.operation {
            text.push_str(&format!("; operation={operation}"));
        }
        if let Some(category) = facts.fs_failure {
            text.push_str(&format!("; fs_failure={category}"));
        }
        if let Some(kind) = facts.io_kind {
            text.push_str(&format!("; io_kind={kind:?}"));
        }
        if let Some(code) = facts.os_error {
            text.push_str(&format!("; os_error={code}"));
        }
        let (text, truncated) = sanitize_message(
            text.as_bytes(),
            Path::new(""),
            self.limits.message_byte_limit,
        );
        self.truncated_messages = self.truncated_messages.saturating_add(u64::from(truncated));
        self.push_message(
            EditorExternalPlayMessageKind::OutputNotice,
            Some(format!("external-play.{}", facts.stage)),
            text,
            truncated,
        );
        self.refresh_snapshot();
    }

    pub(super) fn observe_cargo_diagnostics(
        &mut self,
        diagnostics: Option<&CargoDiagnostics>,
        project_root: &Path,
    ) {
        let Some(diagnostics) = diagnostics else {
            return;
        };
        let previous_sequence = self.sequence;
        let summary = compiler_summary(diagnostics);
        let mut changed = self.compiler != summary;
        self.compiler = summary;

        if diagnostics.entries().len() < self.compiler_entries_observed {
            self.compiler_entries_observed = 0;
        }
        for diagnostic in diagnostics
            .entries()
            .iter()
            .skip(self.compiler_entries_observed)
        {
            self.observe_compiler_diagnostic(diagnostic, project_root);
            changed = true;
        }
        self.compiler_entries_observed = diagnostics.entries().len();
        if changed {
            if self.sequence == previous_sequence {
                self.touch();
            }
            self.refresh_snapshot();
        }
    }

    fn observe_compiler_diagnostic(
        &mut self,
        diagnostic: &CargoCompilerDiagnostic,
        project_root: &Path,
    ) {
        let (text, projection_truncated) = sanitize_message(
            diagnostic.message().as_bytes(),
            project_root,
            self.limits.message_byte_limit,
        );
        if projection_truncated {
            self.truncated_messages = self.truncated_messages.saturating_add(1);
        }
        self.push_message(
            EditorExternalPlayMessageKind::Compiler(compiler_level(diagnostic.level())),
            sanitize_code(diagnostic.code()),
            text,
            projection_truncated || diagnostic.truncated(),
        );
    }

    pub(super) fn observe_product_logs(
        &mut self,
        logs: &ProcessLogRing,
        output_loss_observed: bool,
        project_root: &Path,
    ) {
        let stats = logs.stats();
        let mut changed = self.observe_process_loss(stats, output_loss_observed);
        let cursor_sequence = self.product_cursor.sequence;
        for entry in logs
            .iter()
            .filter(|entry| entry.sequence().get() > cursor_sequence)
        {
            let (text, projection_truncated) =
                sanitize_message(entry.bytes(), project_root, self.limits.message_byte_limit);
            if projection_truncated {
                self.truncated_messages = self.truncated_messages.saturating_add(1);
            }
            self.push_message(
                match entry.stream() {
                    ProcessOutputStream::Stdout => EditorExternalPlayMessageKind::ProductStdout,
                    ProcessOutputStream::Stderr => EditorExternalPlayMessageKind::ProductStderr,
                },
                None,
                text,
                projection_truncated || entry.truncated_prefix_bytes() > 0,
            );
            self.product_cursor.sequence = entry.sequence().get();
            changed = true;
        }
        if changed {
            self.refresh_snapshot();
        }
    }

    fn observe_process_loss(&mut self, stats: ProcessLogStats, output_loss_observed: bool) -> bool {
        let previous = self.product_cursor;
        let dropped_records = monotonic_delta(stats.dropped_records(), previous.dropped_records);
        let dropped_bytes = monotonic_delta(stats.dropped_bytes(), previous.dropped_bytes);
        let truncated_records =
            monotonic_delta(stats.truncated_records(), previous.truncated_records);
        self.product_cursor.dropped_records = stats.dropped_records();
        self.product_cursor.dropped_bytes = stats.dropped_bytes();
        self.product_cursor.truncated_records = stats.truncated_records();

        let newly_lost = dropped_records > 0
            || dropped_bytes > 0
            || truncated_records > 0
            || (output_loss_observed && !self.loss_observed);
        if !newly_lost {
            return false;
        }
        self.dropped_messages = self.dropped_messages.saturating_add(dropped_records);
        self.dropped_bytes = self.dropped_bytes.saturating_add(dropped_bytes);
        self.truncated_messages = self.truncated_messages.saturating_add(truncated_records);
        self.loss_observed |= output_loss_observed || dropped_records > 0 || dropped_bytes > 0;
        if !self.loss_notice_published {
            self.push_message(
                EditorExternalPlayMessageKind::OutputNotice,
                None,
                "Some product output was discarded before it could be displayed.".to_owned(),
                false,
            );
            self.loss_notice_published = true;
        } else {
            self.touch();
        }
        true
    }

    fn push_message(
        &mut self,
        kind: EditorExternalPlayMessageKind,
        code: Option<String>,
        text: String,
        truncated: bool,
    ) {
        let Some(sequence) = self.next_sequence() else {
            self.dropped_messages = self.dropped_messages.saturating_add(1);
            self.dropped_bytes = self.dropped_bytes.saturating_add(usize_to_u64(text.len()));
            self.loss_observed = true;
            return;
        };
        let retained_bytes = text
            .len()
            .saturating_add(code.as_deref().map_or(0, str::len));
        if retained_bytes > self.limits.byte_limit {
            self.dropped_messages = self.dropped_messages.saturating_add(1);
            self.dropped_bytes = self
                .dropped_bytes
                .saturating_add(usize_to_u64(retained_bytes));
            return;
        }
        while self.messages.len() >= self.limits.message_limit
            || self.retained_bytes.saturating_add(retained_bytes) > self.limits.byte_limit
        {
            let Some(evicted) = self.messages.pop_front() else {
                break;
            };
            let evicted_bytes = message_bytes(&evicted);
            self.retained_bytes = self.retained_bytes.saturating_sub(evicted_bytes);
            self.dropped_messages = self.dropped_messages.saturating_add(1);
            self.dropped_bytes = self
                .dropped_bytes
                .saturating_add(usize_to_u64(evicted_bytes));
        }
        let Some(message) = EditorExternalPlayMessage::new(sequence, kind, code, text, truncated)
        else {
            self.dropped_messages = self.dropped_messages.saturating_add(1);
            self.dropped_bytes = self
                .dropped_bytes
                .saturating_add(usize_to_u64(retained_bytes));
            self.loss_observed = true;
            return;
        };
        self.retained_bytes = self.retained_bytes.saturating_add(retained_bytes);
        self.messages.push_back(message);
    }

    fn next_sequence(&mut self) -> Option<u64> {
        let next = self.sequence.checked_add(1)?;
        self.sequence = next;
        Some(next)
    }

    fn touch(&mut self) {
        if self.next_sequence().is_none() {
            self.loss_observed = true;
        }
    }

    fn refresh_snapshot(&mut self) {
        self.published = EditorExternalPlayOutput::new(
            self.sequence,
            self.compiler,
            self.messages.iter().cloned().collect(),
            self.dropped_messages,
            self.dropped_bytes,
            self.truncated_messages,
            self.loss_observed,
        )
        .expect("the private projection maintains the public output bounds");
    }
}

fn compiler_summary(diagnostics: &CargoDiagnostics) -> EditorExternalPlayDiagnostics {
    let mut error_count = 0_u64;
    let mut warning_count = 0_u64;
    let mut note_count = 0_u64;
    let mut help_count = 0_u64;
    let mut failure_note_count = 0_u64;
    let mut other_count = 0_u64;
    for diagnostic in diagnostics.entries() {
        match diagnostic.level() {
            CargoDiagnosticLevel::Error => error_count = error_count.saturating_add(1),
            CargoDiagnosticLevel::Warning => warning_count = warning_count.saturating_add(1),
            CargoDiagnosticLevel::Note => note_count = note_count.saturating_add(1),
            CargoDiagnosticLevel::Help => help_count = help_count.saturating_add(1),
            CargoDiagnosticLevel::FailureNote => {
                failure_note_count = failure_note_count.saturating_add(1);
            }
            CargoDiagnosticLevel::Other => other_count = other_count.saturating_add(1),
        }
    }
    EditorExternalPlayDiagnostics::new(
        error_count,
        warning_count,
        note_count,
        help_count,
        failure_note_count,
        other_count,
        diagnostics.dropped_count(),
        diagnostics.truncated_count(),
    )
}

const fn compiler_level(level: CargoDiagnosticLevel) -> EditorExternalPlayCompilerLevel {
    match level {
        CargoDiagnosticLevel::Error => EditorExternalPlayCompilerLevel::Error,
        CargoDiagnosticLevel::Warning => EditorExternalPlayCompilerLevel::Warning,
        CargoDiagnosticLevel::Note => EditorExternalPlayCompilerLevel::Note,
        CargoDiagnosticLevel::Help => EditorExternalPlayCompilerLevel::Help,
        CargoDiagnosticLevel::FailureNote => EditorExternalPlayCompilerLevel::FailureNote,
        CargoDiagnosticLevel::Other => EditorExternalPlayCompilerLevel::Other,
    }
}

fn sanitize_code(code: Option<&str>) -> Option<String> {
    code.filter(|code| {
        !code.is_empty()
            && code.len() <= OUTPUT_CODE_BYTE_LIMIT
            && code
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    })
    .map(str::to_owned)
}

fn monotonic_delta(current: u64, previous: u64) -> u64 {
    current.checked_sub(previous).unwrap_or(current)
}

fn message_bytes(message: &EditorExternalPlayMessage) -> usize {
    message
        .text()
        .len()
        .saturating_add(message.code().map_or(0, str::len))
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn sanitize_message(bytes: &[u8], project_root: &Path, byte_limit: usize) -> (String, bool) {
    let decoded = String::from_utf8_lossy(bytes);
    let terminal_safe = strip_terminal_controls(&decoded);
    let root_safe = redact_project_root(terminal_safe, project_root);
    let locator_safe = root_safe
        .split_whitespace()
        .map(redact_locator_token)
        .collect::<Vec<_>>()
        .join(" ");
    let message = if locator_safe.is_empty() {
        "[filtered output]".to_owned()
    } else {
        locator_safe
    };
    truncate_utf8(message, byte_limit)
}

#[derive(Clone, Copy)]
enum TerminalEscape {
    Text,
    Escape,
    Csi,
    Osc,
    OscEscape,
}

fn strip_terminal_controls(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut state = TerminalEscape::Text;
    for character in input.chars() {
        state = match state {
            TerminalEscape::Text => match character {
                '\u{001b}' => TerminalEscape::Escape,
                '\u{009b}' => TerminalEscape::Csi,
                character if character.is_whitespace() => {
                    output.push(' ');
                    TerminalEscape::Text
                }
                character if forbidden_display_char(character) => TerminalEscape::Text,
                character => {
                    output.push(character);
                    TerminalEscape::Text
                }
            },
            TerminalEscape::Escape => match character {
                '[' => TerminalEscape::Csi,
                ']' => TerminalEscape::Osc,
                _ => TerminalEscape::Text,
            },
            TerminalEscape::Csi => {
                if ('@'..='~').contains(&character) {
                    TerminalEscape::Text
                } else {
                    TerminalEscape::Csi
                }
            }
            TerminalEscape::Osc => match character {
                '\u{0007}' => TerminalEscape::Text,
                '\u{001b}' => TerminalEscape::OscEscape,
                _ => TerminalEscape::Osc,
            },
            TerminalEscape::OscEscape => {
                if character == '\\' {
                    TerminalEscape::Text
                } else {
                    TerminalEscape::Osc
                }
            }
        };
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn forbidden_display_char(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{061c}'
                | '\u{200b}'..='\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2060}'..='\u{206f}'
                | '\u{feff}'
        )
}

fn redact_project_root(mut text: String, project_root: &Path) -> String {
    let root = project_root.to_string_lossy();
    if root.is_empty() {
        return text;
    }
    let mut variants = vec![root.into_owned()];
    let slash = variants[0].replace('\\', "/");
    let backslash = variants[0].replace('/', "\\");
    if !variants.contains(&slash) {
        variants.push(slash);
    }
    if !variants.contains(&backslash) {
        variants.push(backslash);
    }
    variants.sort_unstable_by_key(|variant| std::cmp::Reverse(variant.len()));
    for variant in variants {
        text = replace_ascii_case_insensitive(text, &variant, "[project-root]");
    }
    text
}

fn replace_ascii_case_insensitive(input: String, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return input;
    }
    let lower_input = input.to_ascii_lowercase();
    let lower_needle = needle.to_ascii_lowercase();
    let mut cursor = 0_usize;
    let mut output = String::with_capacity(input.len());
    while let Some(relative) = lower_input[cursor..].find(&lower_needle) {
        let start = cursor + relative;
        let end = start + needle.len();
        if !input.is_char_boundary(start) || !input.is_char_boundary(end) {
            cursor = end.min(input.len());
            continue;
        }
        output.push_str(&input[cursor..start]);
        output.push_str(replacement);
        cursor = end;
    }
    output.push_str(&input[cursor..]);
    output
}

fn redact_locator_token(token: &str) -> &str {
    let candidate = token.trim_start_matches(|character: char| {
        matches!(character, '(' | '[' | '{' | '<' | '\'' | '"' | '`' | '=')
    });
    let lower = candidate.to_ascii_lowercase();
    if lower.contains("http://")
        || lower.contains("https://")
        || lower.contains("file://")
        || candidate.contains("\\\\")
        || candidate.starts_with("//")
        || candidate.starts_with('/')
        || contains_windows_absolute_path(candidate)
        || contains_embedded_unix_absolute_path(candidate)
    {
        "[redacted-locator]"
    } else {
        token
    }
}

fn contains_windows_absolute_path(value: &str) -> bool {
    value
        .as_bytes()
        .windows(3)
        .enumerate()
        .any(|(index, bytes)| {
            bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && matches!(bytes[2], b'/' | b'\\')
                && (index == 0
                    || value.as_bytes()[index - 1].is_ascii_whitespace()
                    || matches!(
                        value.as_bytes()[index - 1],
                        b'(' | b'[' | b'{' | b'=' | b'"'
                    ))
        })
}

fn contains_embedded_unix_absolute_path(value: &str) -> bool {
    value.as_bytes().iter().enumerate().any(|(index, byte)| {
        *byte == b'/'
            && index > 0
            && matches!(
                value.as_bytes()[index - 1],
                b'(' | b'[' | b'{' | b'=' | b'"'
            )
    })
}

fn truncate_utf8(mut text: String, byte_limit: usize) -> (String, bool) {
    if text.len() <= byte_limit {
        return (text, false);
    }
    const ELLIPSIS: &str = "…";
    let mut end = byte_limit.saturating_sub(ELLIPSIS.len()).min(text.len());
    while !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    text.truncate(end);
    text.push_str(ELLIPSIS);
    (text, true)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use nara::project_workflow::__private::CargoJsonMessage;
    use serde_json::json;

    use super::*;
    use crate::supervised_play::log::ProcessLogLimits;

    #[test]
    fn host_failure_projects_only_bounded_labels_and_io_facts() {
        let mut projection = EditorExternalPlayOutputProjection::with_limits(ProjectionLimits {
            message_limit: 2,
            byte_limit: 512,
            message_byte_limit: 128,
        });
        let sensitive = std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "private-token /private/project/product https://secret.invalid",
        );
        projection.observe_host_failure("spawn", "os-spawn", Some(&sensitive));
        let numeric = std::io::Error::from_raw_os_error(13);
        projection.observe_host_failure("spawn", "os-spawn", Some(&numeric));
        let output = projection.snapshot();
        assert_eq!(output.messages().len(), 2);
        assert_eq!(output.compiler(), EditorExternalPlayDiagnostics::default());
        assert_eq!(
            output.messages()[0].kind(),
            EditorExternalPlayMessageKind::OutputNotice
        );
        assert_eq!(
            output.messages()[0].text(),
            "External Play failed: stage=spawn; reason=os-spawn; io_kind=PermissionDenied"
        );
        assert!(output.messages()[1].text().ends_with("; os_error=13"));
        assert!(
            output
                .messages()
                .iter()
                .all(|message| !message.text().contains("private-token"))
        );

        projection.observe_host_failure("handshake", "ready-deadline", None);
        let output = projection.snapshot();
        assert_eq!(output.messages().len(), 2);
        assert_eq!(output.dropped_messages(), 1);
        assert_eq!(output.messages()[1].code(), Some("external-play.handshake"));
    }

    #[test]
    fn projection_removes_terminal_controls_sensitive_locators_and_multiline_text() {
        let project_root = Path::new("C:\\private\\sample game");
        let mut ring = ProcessLogRing::new(log_limits(8, 4096));
        let bytes = b"\x1b[31mfailed\x1b[0m at C:\\private\\sample game\\src\\main.rs\nhttps://example.invalid/private \xe2\x80\xae bad \xf0\x9f\x92";
        ring.append(ProcessOutputStream::Stderr, bytes)
            .expect("the test output sequence is available");
        let mut projection = EditorExternalPlayOutputProjection::new();

        projection.observe_product_logs(&ring, false, project_root);

        let output = projection.snapshot();
        let message = output.messages().first().expect("one message is projected");
        assert_eq!(message.kind(), EditorExternalPlayMessageKind::ProductStderr);
        assert!(!message.text().contains('\u{001b}'));
        assert!(!message.text().contains('\u{202e}'));
        assert!(!message.text().contains('\n'));
        assert!(!message.text().contains("private"));
        assert!(!message.text().contains("example.invalid"));
        assert!(message.text().contains("[project-root]"));
        assert!(message.text().contains("[redacted-locator]"));
        assert!(message.text().contains('\u{fffd}'));
    }

    #[test]
    fn literal_project_root_prefix_does_not_bypass_locator_redaction() {
        let mut ring = ProcessLogRing::new(log_limits(8, 4096));
        ring.append(
            ProcessOutputStream::Stdout,
            b"[project-root]https://example.invalid [project-root]file://private",
        )
        .unwrap();
        let mut projection = EditorExternalPlayOutputProjection::new();
        projection.observe_product_logs(&ring, false, Path::new("C:/game"));
        let output = projection.snapshot();
        assert_eq!(output.messages().len(), 1);
        assert!(!output.messages()[0].text().contains("://"));
        assert!(output.messages()[0].text().contains("[redacted-locator]"));
    }

    #[test]
    fn projection_retains_compiler_code_and_sanitizes_only_structured_message_facts() {
        let parsed = CargoJsonMessage::parse(json!({
            "reason": "compiler-message",
            "message": {
                "level": "error",
                "code": { "code": "E0425" },
                "message": "cannot find value at C:\\private\\game\\src\\main.rs"
            }
        }))
        .expect("the compiler message is valid Cargo JSON");
        let CargoJsonMessage::CompilerDiagnostic(diagnostic) = parsed else {
            panic!("the fixture is a compiler diagnostic");
        };
        let mut projection = EditorExternalPlayOutputProjection::new();

        projection.observe_compiler_diagnostic(&diagnostic, Path::new("C:\\private\\game"));
        projection.refresh_snapshot();

        let message = projection.snapshot().messages()[0].clone();
        assert_eq!(
            message.kind(),
            EditorExternalPlayMessageKind::Compiler(EditorExternalPlayCompilerLevel::Error)
        );
        assert_eq!(message.code(), Some("E0425"));
        assert_eq!(
            message.text(),
            "cannot find value at [project-root]\\src\\main.rs"
        );
    }

    #[test]
    fn projection_reports_raw_and_projection_loss_under_fixed_limits() {
        let mut ring = ProcessLogRing::new(log_limits(2, 24));
        for message in ["first-output", "second-output", "third-output-that-is-long"] {
            ring.append(ProcessOutputStream::Stdout, message.as_bytes())
                .expect("the test output sequence is available");
        }
        let mut projection = EditorExternalPlayOutputProjection::with_limits(ProjectionLimits {
            message_limit: 2,
            byte_limit: 48,
            message_byte_limit: 24,
        });

        projection.observe_product_logs(&ring, true, Path::new("C:\\project"));

        let output = projection.snapshot();
        assert!(output.messages().len() <= 2);
        assert!(output.dropped_messages() > 0);
        assert!(output.dropped_bytes() > 0);
        assert!(output.truncated_messages() > 0);
        assert!(output.loss_observed());
        assert!(output.is_truncated());
    }

    #[test]
    fn reset_and_clear_drop_every_projected_message_and_fact() {
        let mut ring = ProcessLogRing::new(log_limits(2, 64));
        ring.append(ProcessOutputStream::Stdout, b"play output")
            .expect("the test output sequence is available");
        let mut projection = EditorExternalPlayOutputProjection::new();
        projection.observe_product_logs(&ring, false, Path::new("C:\\project"));
        assert!(!projection.snapshot().messages().is_empty());

        projection.reset_attempt();
        assert_eq!(projection.snapshot(), EditorExternalPlayOutput::default());

        projection.observe_product_logs(&ring, false, Path::new("C:\\project"));
        projection.clear();
        assert_eq!(projection.snapshot(), EditorExternalPlayOutput::default());
    }

    fn log_limits(records: usize, bytes: usize) -> ProcessLogLimits {
        ProcessLogLimits::new(
            NonZeroUsize::new(records).expect("test record limit is non-zero"),
            NonZeroUsize::new(bytes).expect("test byte limit is non-zero"),
        )
    }
}
