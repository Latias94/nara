use std::{
    fmt,
    io::{BufRead, Read},
    marker::PhantomData,
    sync::mpsc::SyncSender,
};
#[cfg(nara_cooked_player)]
use std::{
    io,
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
};

use nara::core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits, preflight_serde_shape};
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor, value::MapAccessDeserializer},
};
use two_level_game::command::SemanticCommand;

const MAX_LINE_BYTES: usize = 4096;
const REQUEST_SHAPE_LIMITS: SerdeShapeLimits = SerdeShapeLimits::new(
    DepthLimit::new(4).expect("nonzero control depth limit"),
    ItemLimit::new(32).expect("nonzero control node limit"),
    ItemLimit::new(8).expect("nonzero control container limit"),
    ByteLimit::new(64).expect("nonzero control string limit"),
    ByteLimit::new(256).expect("nonzero control total string limit"),
);

#[derive(Debug)]
pub(super) enum Event {
    Request(Request),
    Rejected(String),
    Eof,
    Failed(String),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "request", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Request {
    Observe {},
    Shutdown {},
    Command {
        #[serde(deserialize_with = "deserialize_object")]
        command: SemanticCommand,
    },
    SideRoom {},
    Lookout {},
    Save {},
    Restore {},
}

#[cfg(nara_cooked_player)]
pub(super) fn spawn() -> io::Result<(Receiver<Event>, JoinHandle<()>)> {
    let (sender, receiver) = mpsc::sync_channel(16);
    let worker = thread::Builder::new()
        .name("two-level-server-input".into())
        .spawn(move || read_events(io::stdin().lock(), sender))?;
    Ok((receiver, worker))
}

fn read_events(mut reader: impl BufRead, sender: SyncSender<Event>) {
    let mut line = Vec::with_capacity(MAX_LINE_BYTES);
    loop {
        line.clear();
        let event = match reader
            .by_ref()
            .take(MAX_LINE_BYTES as u64)
            .read_until(b'\n', &mut line)
        {
            Ok(0) => Event::Eof,
            Ok(_) if line.ends_with(b"\n") => match parse_request(&line) {
                Ok(request) => Event::Request(request),
                Err(error) => Event::Rejected(error),
            },
            Ok(MAX_LINE_BYTES) => Event::Failed(format!(
                "control request line has no newline within {MAX_LINE_BYTES} bytes"
            )),
            Ok(_) => Event::Failed("control input ended before the request newline".into()),
            Err(error) => Event::Failed(format!("read control input: {error}")),
        };
        let terminal = matches!(
            event,
            Event::Eof | Event::Failed(_) | Event::Request(Request::Shutdown {})
        );
        if sender.send(event).is_err() || terminal {
            return;
        }
    }
}

fn parse_request(line: &[u8]) -> Result<Request, String> {
    let mut deserializer = serde_json::Deserializer::from_slice(line);
    preflight_serde_shape(&mut deserializer, REQUEST_SHAPE_LIMITS)
        .map_err(|error| format!("control request JSON refused: {error}"))?;
    deserializer
        .end()
        .map_err(|error| format!("trailing control request data: {error}"))?;
    deserialize_object(&mut serde_json::Deserializer::from_slice(line))
        .map_err(|error| format!("invalid control request: {error}"))
}

fn deserialize_object<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    struct ObjectVisitor<T>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a JSON object")
        }

        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_map(ObjectVisitor(PhantomData))
}

#[cfg(test)]
#[path = "control/tests.rs"]
mod tests;
