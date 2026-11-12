use super::*;
use crate::{EditorDocumentSourceBinding, EditorWorkspace};

enum CounterCommand {
    Allocate,
    Noop,
    Reject,
}

struct CounterOutcome {
    allocated: Option<u64>,
}

struct CounterDocument<const OWNER: u8> {
    next: u64,
    clock: DocumentRevision,
}

impl<const OWNER: u8> CounterDocument<OWNER> {
    fn new() -> Self {
        Self {
            next: 1,
            clock: DocumentRevision::initial(),
        }
    }
}

fn refused() -> EditorExtensionDocumentError {
    EditorExtensionDocumentError::new(
        DiagnosticCode::new("test.extension-refused").unwrap(),
        SafeSummary::new("The counter command was refused").unwrap(),
    )
}

impl<const OWNER: u8> EditorExtensionDocument for CounterDocument<OWNER> {
    type Command = CounterCommand;
    type Outcome = CounterOutcome;
    type View = u64;

    const TYPE_ID: &'static str = if OWNER == 1 {
        "test.counter-first"
    } else {
        "test.counter-second"
    };

    fn title(&self) -> &str {
        Self::TYPE_ID
    }

    fn content_state(&self) -> DocumentContentState {
        self.clock.content_state()
    }

    fn history_status(&self) -> DocumentHistoryStatus {
        DocumentHistoryStatus::default()
    }

    fn view(&self) -> Self::View {
        self.next
    }

    fn apply(
        &mut self,
        command: &Self::Command,
    ) -> Result<Self::Outcome, EditorExtensionDocumentError> {
        match command {
            CounterCommand::Allocate => {
                let allocated = self.next;
                self.next = self.next.checked_add(1).ok_or_else(refused)?;
                self.clock = self.clock.next();
                Ok(CounterOutcome {
                    allocated: Some(allocated),
                })
            }
            CounterCommand::Noop => Ok(CounterOutcome { allocated: None }),
            CounterCommand::Reject => Err(refused()),
        }
    }

    fn undo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        Err(refused())
    }

    fn redo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        Err(refused())
    }

    fn encode(&self) -> Result<Vec<u8>, EditorExtensionDocumentError> {
        Ok(self.next.to_le_bytes().to_vec())
    }

    fn decode(bytes: &[u8]) -> Result<Self, EditorExtensionDocumentError> {
        Ok(Self {
            next: u64::from_le_bytes(bytes.try_into().map_err(|_| refused())?),
            clock: DocumentRevision::initial(),
        })
    }
}

fn open(workspace: &mut EditorWorkspace) -> super::super::EditorDocumentId {
    workspace
        .open_extension_document(
            EditorDocumentSourceBinding::new("counter.bin"),
            CounterDocument::<1>::new(),
        )
        .unwrap()
}

#[test]
fn accepted_output_retains_exact_command_owner_revision_and_shared_observation() {
    let mut workspace = EditorWorkspace::new();
    let document = open(&mut workspace);
    let before = workspace.document_revision(document).unwrap();
    let command = EditorExtensionCommand::new::<CounterDocument<1>>(
        document,
        before,
        CounterCommand::Allocate,
    );
    let report = workspace.apply_extension_action(command.clone());
    assert!(report.applied);
    assert_eq!(report.document, Some(document));
    assert_eq!(report.revision, Some(before.next()));
    let outcome = report.extension_outcome.as_ref().unwrap();
    assert_eq!(outcome.command(), &command);
    assert_eq!(outcome.revision(), before.next());
    assert_eq!(
        outcome.get::<CounterDocument<1>>().unwrap().allocated,
        Some(1)
    );
    assert!(outcome.get::<CounterDocument<2>>().is_none());
    assert_eq!(report, report.clone());
    assert_eq!(outcome, &outcome.clone());
    assert!(format!("{report:?}").contains("EditorExtensionCommandOutcome"));
    let different = EditorExtensionCommand::new::<CounterDocument<1>>(
        document,
        before,
        CounterCommand::Allocate,
    );
    assert_ne!(outcome.command(), &different);
}

#[test]
fn successful_noop_returns_its_output_without_changing_content_or_consuming_an_id() {
    let mut workspace = EditorWorkspace::new();
    let document = open(&mut workspace);
    let before = workspace.document_revision(document).unwrap();
    assert_eq!(workspace.document_is_dirty(document), Some(false));
    let command =
        EditorExtensionCommand::new::<CounterDocument<1>>(document, before, CounterCommand::Noop);
    let report = workspace.apply_extension_action(command.clone());
    assert!(report.applied);
    assert_eq!(report.revision, Some(before.next()));
    assert_eq!(report.dirty, Some(false));
    let outcome = report.extension_outcome.unwrap();
    assert_eq!(outcome.command(), &command);
    assert_eq!(outcome.revision(), before.next());
    assert_eq!(outcome.get::<CounterDocument<1>>().unwrap().allocated, None);
    let next = workspace.apply_extension_command::<CounterDocument<1>>(
        document,
        before.next(),
        CounterCommand::Allocate,
    );
    assert!(next.applied);
    assert_eq!(
        next.extension_outcome
            .unwrap()
            .get::<CounterDocument<1>>()
            .unwrap()
            .allocated,
        Some(1)
    );
    assert_eq!(outcome.revision(), before.next());
}

#[test]
fn domain_owner_and_stale_refusals_publish_no_output_or_allocator_change() {
    let mut workspace = EditorWorkspace::new();
    let document = open(&mut workspace);
    let before = workspace.document_revision(document).unwrap();
    let rejected = workspace.apply_extension_command::<CounterDocument<1>>(
        document,
        before,
        CounterCommand::Reject,
    );
    assert!(!rejected.applied);
    assert!(rejected.extension_outcome.is_none());
    let wrong_owner = workspace.apply_extension_command::<CounterDocument<2>>(
        document,
        before,
        CounterCommand::Allocate,
    );
    assert!(!wrong_owner.applied);
    assert!(wrong_owner.extension_outcome.is_none());
    assert_eq!(workspace.document_revision(document), Some(before));
    assert_eq!(workspace.document_is_dirty(document), Some(false));
    let accepted = workspace.apply_extension_command::<CounterDocument<1>>(
        document,
        before,
        CounterCommand::Allocate,
    );
    assert!(accepted.applied);
    let outcome = accepted.extension_outcome.unwrap();
    assert_eq!(
        outcome.get::<CounterDocument<1>>().unwrap().allocated,
        Some(1)
    );
    let stale = workspace.apply_extension_action(outcome.command().clone());
    assert!(!stale.applied);
    assert!(stale.extension_outcome.is_none());
    assert_eq!(workspace.document_revision(document), Some(before.next()));
    let next = workspace.apply_extension_command::<CounterDocument<1>>(
        document,
        before.next(),
        CounterCommand::Allocate,
    );
    assert!(next.applied);
    assert_eq!(
        next.extension_outcome
            .unwrap()
            .get::<CounterDocument<1>>()
            .unwrap()
            .allocated,
        Some(2)
    );
}
