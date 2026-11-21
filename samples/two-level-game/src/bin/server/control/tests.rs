use std::{
    io::{self, BufRead, BufReader, Read},
    sync::mpsc,
    thread,
};

use super::{CloseRequest, Event, Request, SemanticCommand, read_events};

fn collect_events(reader: impl BufRead + Send) -> Vec<Event> {
    thread::scope(|scope| {
        let (sender, receiver) = mpsc::sync_channel(1);
        scope.spawn(move || read_events(reader, sender));
        receiver.into_iter().take(16).collect()
    })
}

#[test]
fn rejected_complete_lines_preserve_order_and_leave_semantics_to_session() {
    let events = collect_events(
        br#"{"request":"observe"}
{"request":
{"request":"observe","\u0072equest":"shutdown"}
{"request":"shutdown","path":"foreign"}
{"request":"shutdown"}{}
{"request":"command","command":{"operation":"move","direction":4,"pressed":true}}
"#
        .as_slice(),
    );
    let [
        Event::Request(Request::Observe {}),
        Event::Rejected(malformed),
        Event::Rejected(duplicate),
        Event::Rejected(unknown),
        Event::Rejected(trailing),
        Event::Request(Request::Command {
            command:
                SemanticCommand::Move {
                    direction: 4,
                    pressed: true,
                },
            after_ticks,
        }),
        Event::Eof,
    ] = events.as_slice()
    else {
        panic!("unexpected control events: {events:?}");
    };
    assert!(malformed.starts_with("control request JSON refused:"));
    assert!(duplicate.contains("duplicate map key"), "{duplicate}");
    assert!(unknown.contains("unknown field `path`"), "{unknown}");
    assert!(trailing.starts_with("trailing control request data:"));
    assert_eq!(after_ticks.get(), 1);
}

#[test]
fn requests_and_commands_require_objects_with_known_fields() {
    let events = collect_events(
        br#"["shutdown"]
{"request":"command","command":["move",3,true]}
{"request":"command","command":{"operation":"marker","tick":5}}
"#
        .as_slice(),
    );
    let [
        Event::Rejected(request),
        Event::Rejected(command),
        Event::Rejected(unknown),
        Event::Eof,
    ] = events.as_slice()
    else {
        panic!("unexpected control events: {events:?}");
    };
    assert!(request.contains("expected a JSON object"), "{request}");
    assert!(command.contains("expected a JSON object"), "{command}");
    assert!(unknown.contains("unknown field `tick`"), "{unknown}");
}

#[test]
fn paused_controls_and_delayed_commands_keep_exact_elapsed_and_offsets() {
    let events = collect_events(
        br#"{"request":"pause"}
{"request":"resume"}
{"request":"advance","elapsed_nanos":23000000}
{"request":"capture"}
{"request":"command","after_ticks":5,"command":{"operation":"marker"}}
{"request":"command","after_ticks":0,"command":{"operation":"marker"}}
{"request":"advance","elapsed_nanos":-1}
"#
        .as_slice(),
    );
    let [
        Event::Request(Request::Pause {}),
        Event::Request(Request::Resume {}),
        Event::Request(Request::Advance {
            elapsed_nanos: 23_000_000,
        }),
        Event::Request(Request::Capture {}),
        Event::Request(Request::Command {
            command: SemanticCommand::Marker,
            after_ticks,
        }),
        Event::Rejected(_),
        Event::Rejected(_),
        Event::Eof,
    ] = events.as_slice()
    else {
        panic!("unexpected control events: {events:?}");
    };
    assert_eq!(after_ticks.get(), 5);
}

#[test]
fn line_limit_counts_the_delimiter_and_stops_before_the_next_byte() {
    let request = br#"{"request":"lookout"}"#;
    let mut input = request.to_vec();
    input.resize(4094, b' ');
    input.extend_from_slice(b"\r\n");
    input.extend_from_slice(request);
    input.resize(8192, b' ');
    let trailing = b"\n{\"request\":\"observe\"}\n";
    input.extend_from_slice(trailing);

    let mut reader = input.as_slice();
    let events = collect_events(&mut reader);
    let [Event::Request(Request::Lookout {}), Event::Failed(error)] = events.as_slice() else {
        panic!("unexpected control events: {events:?}");
    };
    assert_eq!(
        error,
        "control request line has no newline within 4096 bytes"
    );
    assert_eq!(reader, trailing);
}

#[test]
fn eof_after_unterminated_json_fails_the_stream() {
    let events = collect_events(br#"{"request":"restore"}"#.as_slice());
    let [Event::Failed(error)] = events.as_slice() else {
        panic!("unexpected control events: {events:?}");
    };
    assert_eq!(error, "control input ended before the request newline");
}

#[test]
fn read_failure_follows_complete_requests_and_stops_the_stream() {
    struct ReadFailure;

    impl Read for ReadFailure {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("input failure"))
        }
    }

    let input = b"{\"request\":\"side_room\"}\n{\"request\":\"restore\"}";
    let events = collect_events(BufReader::new(input.as_slice().chain(ReadFailure)));
    let [Event::Request(Request::SideRoom {}), Event::Failed(error)] = events.as_slice() else {
        panic!("unexpected control events: {events:?}");
    };
    assert_eq!(error, "read control input: input failure");
}

#[test]
fn rejected_close_resumes_reading_and_accepted_close_leaves_later_requests_unread() {
    let mut reader = br#"{"request":"save"}
{"request":"retire"}
{"request":"shutdown"}
{"request":"restore"}
"#
    .as_slice();
    thread::scope(|scope| {
        let (sender, receiver) = mpsc::sync_channel(1);
        let reader = &mut reader;
        scope.spawn(move || read_events(reader, sender));
        assert!(matches!(
            receiver.recv().unwrap(),
            Event::Request(Request::Save {})
        ));
        let Event::Close {
            request: CloseRequest::Retire,
            keep_reading,
        } = receiver.recv().unwrap()
        else {
            panic!("expected a Retire decision");
        };
        keep_reading.send(()).unwrap();
        let Event::Close {
            request: CloseRequest::Shutdown,
            keep_reading,
        } = receiver.recv().unwrap()
        else {
            panic!("expected a Shutdown decision");
        };
        drop(keep_reading);
        assert!(receiver.recv().is_err());
    });
    assert_eq!(reader, b"{\"request\":\"restore\"}\n");
}
