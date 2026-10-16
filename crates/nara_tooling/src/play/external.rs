use std::sync::Arc;

use nara_fs::ContentDigest;

use super::{EditorPlayOperation, EditorPlayOperationResult};

const MAX_EXTERNAL_PLAY_MESSAGE_BYTES: usize = 1024;
const MAX_EXTERNAL_PLAY_MESSAGE_CODE_BYTES: usize = 128;
const MAX_EXTERNAL_PLAY_MESSAGE_COUNT: usize = 128;
const MAX_EXTERNAL_PLAY_OUTPUT_BYTES: usize = 128 * 1024;

/// Neutral state projected from the private supervised desktop owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorExternalPlayState {
    Idle,
    Building,
    Starting,
    Running,
    Stopping,
    Faulted,
    CleanupIncomplete,
}

/// UI-neutral compiler severity retained by external Play output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorExternalPlayCompilerLevel {
    Error,
    Warning,
    Note,
    Help,
    FailureNote,
    Other,
}

/// UI-neutral source of one bounded external Play message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorExternalPlayMessageKind {
    Compiler(EditorExternalPlayCompilerLevel),
    ProductStdout,
    ProductStderr,
    OutputNotice,
}

/// One immutable, path-safe message exposed by external Play.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorExternalPlayMessage {
    sequence: u64,
    kind: EditorExternalPlayMessageKind,
    code: Option<Box<str>>,
    text: Box<str>,
    truncated: bool,
}

impl EditorExternalPlayMessage {
    /// Constructs one already-sanitized message.
    ///
    /// The Editor owns sanitization. This constructor enforces the tooling seam independently so
    /// callers cannot publish multiline/control-bearing text or obvious absolute locators.
    #[must_use]
    pub fn new(
        sequence: u64,
        kind: EditorExternalPlayMessageKind,
        code: Option<String>,
        text: String,
        truncated: bool,
    ) -> Option<Self> {
        if sequence == 0
            || text.is_empty()
            || text.len() > MAX_EXTERNAL_PLAY_MESSAGE_BYTES
            || !safe_message_text(&text)
        {
            return None;
        }
        let code = match code {
            Some(code)
                if !code.is_empty()
                    && code.len() <= MAX_EXTERNAL_PLAY_MESSAGE_CODE_BYTES
                    && code.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')
                    }) =>
            {
                Some(code.into_boxed_str())
            }
            Some(_) => return None,
            None => None,
        };
        Some(Self {
            sequence,
            kind,
            code,
            text: text.into_boxed_str(),
            truncated,
        })
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn kind(&self) -> EditorExternalPlayMessageKind {
        self.kind
    }

    #[must_use]
    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    fn retained_bytes(&self) -> usize {
        self.text
            .len()
            .saturating_add(self.code.as_deref().map_or(0, str::len))
    }
}

/// Bounded, UI-neutral compiler diagnostic summary exposed by external Play.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditorExternalPlayDiagnostics {
    error_count: u64,
    warning_count: u64,
    note_count: u64,
    help_count: u64,
    failure_note_count: u64,
    other_count: u64,
    dropped_count: u64,
    truncated_count: u64,
}

impl EditorExternalPlayDiagnostics {
    #[must_use]
    pub const fn new(
        error_count: u64,
        warning_count: u64,
        note_count: u64,
        help_count: u64,
        failure_note_count: u64,
        other_count: u64,
        dropped_count: u64,
        truncated_count: u64,
    ) -> Self {
        Self {
            error_count,
            warning_count,
            note_count,
            help_count,
            failure_note_count,
            other_count,
            dropped_count,
            truncated_count,
        }
    }

    #[must_use]
    pub const fn error_count(self) -> u64 {
        self.error_count
    }

    #[must_use]
    pub const fn warning_count(self) -> u64 {
        self.warning_count
    }

    #[must_use]
    pub const fn note_count(self) -> u64 {
        self.note_count
    }

    #[must_use]
    pub const fn help_count(self) -> u64 {
        self.help_count
    }

    #[must_use]
    pub const fn failure_note_count(self) -> u64 {
        self.failure_note_count
    }

    #[must_use]
    pub const fn other_count(self) -> u64 {
        self.other_count
    }

    #[must_use]
    pub const fn dropped_count(self) -> u64 {
        self.dropped_count
    }

    #[must_use]
    pub const fn truncated_count(self) -> u64 {
        self.truncated_count
    }

    #[must_use]
    pub const fn retained_count(self) -> u64 {
        self.error_count
            .saturating_add(self.warning_count)
            .saturating_add(self.note_count)
            .saturating_add(self.help_count)
            .saturating_add(self.failure_note_count)
            .saturating_add(self.other_count)
    }

    #[must_use]
    pub const fn has_errors(self) -> bool {
        self.error_count > 0
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.retained_count() == 0 && self.dropped_count == 0 && self.truncated_count == 0
    }
}

/// Immutable bounded output projected from the private Editor owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorExternalPlayOutput {
    sequence: u64,
    compiler: EditorExternalPlayDiagnostics,
    messages: Arc<[EditorExternalPlayMessage]>,
    dropped_messages: u64,
    dropped_bytes: u64,
    truncated_messages: u64,
    loss_observed: bool,
}

impl Default for EditorExternalPlayOutput {
    fn default() -> Self {
        Self {
            sequence: 0,
            compiler: EditorExternalPlayDiagnostics::default(),
            messages: Arc::from([]),
            dropped_messages: 0,
            dropped_bytes: 0,
            truncated_messages: 0,
            loss_observed: false,
        }
    }
}

impl EditorExternalPlayOutput {
    #[must_use]
    pub fn new(
        sequence: u64,
        compiler: EditorExternalPlayDiagnostics,
        messages: Vec<EditorExternalPlayMessage>,
        dropped_messages: u64,
        dropped_bytes: u64,
        truncated_messages: u64,
        loss_observed: bool,
    ) -> Option<Self> {
        if messages.len() > MAX_EXTERNAL_PLAY_MESSAGE_COUNT {
            return None;
        }
        let mut previous_sequence = 0;
        let mut retained_bytes = 0_usize;
        for message in &messages {
            if message.sequence <= previous_sequence || message.sequence > sequence {
                return None;
            }
            previous_sequence = message.sequence;
            retained_bytes = retained_bytes.checked_add(message.retained_bytes())?;
            if retained_bytes > MAX_EXTERNAL_PLAY_OUTPUT_BYTES {
                return None;
            }
        }
        if sequence == 0
            && (!messages.is_empty()
                || dropped_messages > 0
                || dropped_bytes > 0
                || truncated_messages > 0
                || loss_observed
                || !compiler.is_empty())
        {
            return None;
        }
        Some(Self {
            sequence,
            compiler,
            messages: Arc::from(messages),
            dropped_messages,
            dropped_bytes,
            truncated_messages,
            loss_observed,
        })
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn compiler(&self) -> EditorExternalPlayDiagnostics {
        self.compiler
    }

    #[must_use]
    pub fn messages(&self) -> &[EditorExternalPlayMessage] {
        &self.messages
    }

    #[must_use]
    pub const fn dropped_messages(&self) -> u64 {
        self.dropped_messages
    }

    #[must_use]
    pub const fn dropped_bytes(&self) -> u64 {
        self.dropped_bytes
    }

    #[must_use]
    pub const fn truncated_messages(&self) -> u64 {
        self.truncated_messages
    }

    #[must_use]
    pub const fn loss_observed(&self) -> bool {
        self.loss_observed
    }

    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        self.dropped_messages > 0
            || self.dropped_bytes > 0
            || self.truncated_messages > 0
            || self.loss_observed
            || self.compiler.dropped_count() > 0
            || self.compiler.truncated_count() > 0
    }
}

/// Bounded external Play facts exposed to adapters without process authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorExternalPlayView {
    state: EditorExternalPlayState,
    generation: Option<u64>,
    last_good_available: bool,
    output: EditorExternalPlayOutput,
}

impl EditorExternalPlayView {
    #[must_use]
    pub fn new(
        state: EditorExternalPlayState,
        generation: Option<u64>,
        last_good_available: bool,
        output: EditorExternalPlayOutput,
    ) -> Self {
        Self {
            state,
            generation,
            last_good_available,
            output,
        }
    }

    #[must_use]
    pub const fn state(&self) -> EditorExternalPlayState {
        self.state
    }

    #[must_use]
    pub const fn generation(&self) -> Option<u64> {
        self.generation
    }

    #[must_use]
    pub const fn last_good_available(&self) -> bool {
        self.last_good_available
    }

    #[must_use]
    pub const fn output(&self) -> &EditorExternalPlayOutput {
        &self.output
    }

    #[must_use]
    pub const fn output_sequence(&self) -> u64 {
        self.output.sequence()
    }

    #[must_use]
    pub const fn output_truncated(&self) -> bool {
        self.output.is_truncated()
    }

    #[must_use]
    pub const fn diagnostics(&self) -> EditorExternalPlayDiagnostics {
        self.output.compiler()
    }
}

/// Host-produced external Play effect request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorExternalPlayRequest {
    attempt: u64,
    operation: EditorPlayOperation,
    source_receipt: Option<Box<[u8]>>,
}

impl EditorExternalPlayRequest {
    #[must_use]
    pub fn new(
        attempt: u64,
        operation: EditorPlayOperation,
        source_receipt: Option<Vec<u8>>,
    ) -> Option<Self> {
        (attempt != 0).then(|| Self {
            attempt,
            operation,
            source_receipt: source_receipt.map(Vec::into_boxed_slice),
        })
    }

    #[must_use]
    pub const fn attempt(&self) -> u64 {
        self.attempt
    }

    #[must_use]
    pub const fn operation(&self) -> EditorPlayOperation {
        self.operation
    }

    #[must_use]
    pub fn source_receipt(&self) -> Option<&[u8]> {
        self.source_receipt.as_deref()
    }
}

/// Bounded, generation-stamped observation returned by the private external Play owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorExternalPlayObservation {
    attempt: u64,
    state: EditorExternalPlayState,
    generation: Option<u64>,
    last_good_available: bool,
    source_receipt_digest: Option<ContentDigest>,
    output: EditorExternalPlayOutput,
    result: Option<EditorPlayOperationResult>,
}

impl EditorExternalPlayObservation {
    #[must_use]
    pub fn new(
        attempt: u64,
        state: EditorExternalPlayState,
        generation: Option<u64>,
        last_good_available: bool,
        source_receipt_digest: Option<ContentDigest>,
        output: EditorExternalPlayOutput,
        result: Option<EditorPlayOperationResult>,
    ) -> Option<Self> {
        if attempt == 0 {
            None
        } else {
            Some(Self {
                attempt,
                state,
                generation,
                last_good_available,
                source_receipt_digest,
                output,
                result,
            })
        }
    }

    #[must_use]
    pub const fn attempt(&self) -> u64 {
        self.attempt
    }

    #[must_use]
    pub const fn state(&self) -> EditorExternalPlayState {
        self.state
    }

    #[must_use]
    pub const fn generation(&self) -> Option<u64> {
        self.generation
    }

    #[must_use]
    pub const fn last_good_available(&self) -> bool {
        self.last_good_available
    }

    #[must_use]
    pub const fn source_receipt_digest(&self) -> Option<ContentDigest> {
        self.source_receipt_digest
    }

    #[must_use]
    pub const fn output(&self) -> &EditorExternalPlayOutput {
        &self.output
    }

    #[must_use]
    pub const fn output_sequence(&self) -> u64 {
        self.output.sequence()
    }

    #[must_use]
    pub const fn output_truncated(&self) -> bool {
        self.output.is_truncated()
    }

    #[must_use]
    pub const fn diagnostics(&self) -> EditorExternalPlayDiagnostics {
        self.output.compiler()
    }

    #[must_use]
    pub const fn result(&self) -> Option<EditorPlayOperationResult> {
        self.result
    }
}

fn safe_message_text(text: &str) -> bool {
    !text.chars().any(forbidden_message_char) && !contains_obvious_locator(text)
}

fn forbidden_message_char(character: char) -> bool {
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

fn contains_obvious_locator(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains("http://") || lower.contains("https://") || lower.contains("file://") {
        return true;
    }
    text.split_whitespace().any(|token| {
        let token = token.trim_start_matches(|character: char| {
            matches!(character, '(' | '[' | '{' | '<' | '\'' | '"' | '`')
        });
        token.starts_with("\\\\")
            || token.starts_with("//")
            || token.starts_with('/')
            || (token.as_bytes().get(1) == Some(&b':')
                && token
                    .as_bytes()
                    .first()
                    .is_some_and(|byte| byte.is_ascii_alphabetic())
                && token
                    .as_bytes()
                    .get(2)
                    .is_some_and(|byte| matches!(byte, b'/' | b'\\')))
    })
}

#[cfg(test)]
mod tests {
    use super::super::EditorPlayFailure;
    use super::*;

    fn message(sequence: u64, text: &str) -> EditorExternalPlayMessage {
        EditorExternalPlayMessage::new(
            sequence,
            EditorExternalPlayMessageKind::Compiler(EditorExternalPlayCompilerLevel::Error),
            Some("E0001".to_owned()),
            text.to_owned(),
            false,
        )
        .expect("the test message is safe")
    }

    #[test]
    fn external_play_request_rejects_zero_attempts() {
        assert!(EditorExternalPlayRequest::new(0, EditorPlayOperation::Play, None).is_none());

        let request = EditorExternalPlayRequest::new(
            7,
            EditorPlayOperation::RunLastGood,
            Some(vec![1, 2, 3]),
        )
        .expect("non-zero external Play attempts are representable");
        assert_eq!(request.attempt(), 7);
        assert_eq!(request.operation(), EditorPlayOperation::RunLastGood);
        assert_eq!(request.source_receipt(), Some(&[1, 2, 3][..]));
    }

    #[test]
    fn external_play_view_and_observation_share_immutable_bounded_output() {
        let digest = ContentDigest::of_bytes(b"source-receipt");
        let result = Some(EditorPlayOperationResult::Applied {
            operation: EditorPlayOperation::Play,
            generation: Some(11),
        });
        let diagnostics = EditorExternalPlayDiagnostics::new(2, 3, 1, 1, 0, 0, 4, 2);
        let output = EditorExternalPlayOutput::new(
            9,
            diagnostics,
            vec![message(9, "borrowed value does not live long enough")],
            4,
            17,
            2,
            true,
        )
        .expect("the bounded output is valid");
        let observation = EditorExternalPlayObservation::new(
            3,
            EditorExternalPlayState::Running,
            Some(11),
            true,
            Some(digest),
            output.clone(),
            result,
        )
        .expect("non-zero observations are representable");
        assert_eq!(observation.attempt(), 3);
        assert_eq!(observation.state(), EditorExternalPlayState::Running);
        assert_eq!(observation.generation(), Some(11));
        assert!(observation.last_good_available());
        assert_eq!(observation.source_receipt_digest(), Some(digest));
        assert_eq!(observation.output_sequence(), 9);
        assert!(observation.output_truncated());
        assert_eq!(observation.diagnostics().error_count(), 2);
        assert_eq!(observation.output().messages()[0].code(), Some("E0001"));
        assert_eq!(observation.result(), result);

        let view = EditorExternalPlayView::new(
            EditorExternalPlayState::Running,
            Some(11),
            true,
            observation.output().clone(),
        );
        assert_eq!(view.state(), EditorExternalPlayState::Running);
        assert_eq!(view.generation(), Some(11));
        assert!(view.last_good_available());
        assert_eq!(view.output_sequence(), 9);
        assert!(view.output_truncated());
        assert_eq!(view.diagnostics(), observation.diagnostics());
        assert!(std::ptr::eq(
            view.output().messages().as_ptr(),
            observation.output().messages().as_ptr()
        ));
    }

    #[test]
    fn external_play_output_rejects_unsafe_or_unbounded_messages() {
        assert!(
            EditorExternalPlayMessage::new(
                1,
                EditorExternalPlayMessageKind::ProductStdout,
                None,
                "first\nsecond".to_owned(),
                false,
            )
            .is_none()
        );
        assert!(
            EditorExternalPlayMessage::new(
                1,
                EditorExternalPlayMessageKind::ProductStderr,
                None,
                "opened C:\\private\\game\\data".to_owned(),
                false,
            )
            .is_none()
        );
        assert!(
            EditorExternalPlayMessage::new(
                1,
                EditorExternalPlayMessageKind::ProductStderr,
                None,
                "https://example.invalid/private".to_owned(),
                false,
            )
            .is_none()
        );
        assert!(
            EditorExternalPlayMessage::new(
                1,
                EditorExternalPlayMessageKind::ProductStdout,
                None,
                "x".repeat(MAX_EXTERNAL_PLAY_MESSAGE_BYTES + 1),
                false,
            )
            .is_none()
        );

        let messages = (1..=MAX_EXTERNAL_PLAY_MESSAGE_COUNT + 1)
            .map(|sequence| message(sequence as u64, "bounded"))
            .collect::<Vec<_>>();
        assert!(
            EditorExternalPlayOutput::new(
                messages.len() as u64,
                EditorExternalPlayDiagnostics::default(),
                messages,
                0,
                0,
                0,
                false,
            )
            .is_none()
        );
    }

    #[test]
    fn external_play_diagnostics_remain_fixed_size_and_neutral() {
        let diagnostics = EditorExternalPlayDiagnostics::new(1, 2, 3, 4, 5, 6, 7, 8);
        assert!(diagnostics.has_errors());
        assert_eq!(diagnostics.retained_count(), 21);
        assert!(!diagnostics.is_empty());
        assert_eq!(EditorExternalPlayDiagnostics::default().retained_count(), 0);
    }

    #[test]
    fn superseded_build_is_an_explicit_external_play_failure() {
        let result = EditorPlayOperationResult::Failed {
            operation: EditorPlayOperation::Play,
            failure: EditorPlayFailure::Superseded,
        };
        let observation = EditorExternalPlayObservation::new(
            5,
            EditorExternalPlayState::Faulted,
            None,
            true,
            None,
            EditorExternalPlayOutput::default(),
            Some(result),
        )
        .expect("non-zero superseded observations are representable");

        assert_eq!(observation.result(), Some(result));
    }
}
