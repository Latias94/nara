use std::{
    io::{self, Write},
    process::ExitCode,
};

use serde_json::{Value, json};
use two_level_game::Scenario;

use super::output;

pub(super) const MAX_RECORD_BYTES: usize = 256 * 1024;
pub(super) const MAX_STREAM_BYTES: usize = 4 * 1024 * 1024;
pub(super) const MAX_RECORDS: usize = 1024;

#[derive(Clone, Copy)]
pub(super) enum Record {
    Observation,
    Ready,
    Result,
}

impl Record {
    fn prefix(self) -> &'static str {
        match self {
            Self::Observation => "TWO_LEVEL_DESKTOP_OBSERVATION ",
            Self::Ready => "TWO_LEVEL_DESKTOP_READY ",
            Self::Result => "TWO_LEVEL_DESKTOP_RESULT ",
        }
    }
}

#[derive(Default)]
pub(super) struct Emitter {
    records: usize,
    bytes: usize,
    ready: bool,
    observations_truncated: bool,
}

impl Emitter {
    pub(super) fn emit(&mut self, kind: Record, record: Value) -> Result<(), String> {
        self.emit_to(&mut io::stdout().lock(), kind, record)
            .map_err(|error| format!("desktop Player output failed: {error}"))
    }

    pub(super) fn emit_to(
        &mut self,
        writer: &mut impl Write,
        kind: Record,
        mut record: Value,
    ) -> io::Result<()> {
        record["sequence"] = json!(self.records + 1);
        record["observations_truncated"] = json!(self.observations_truncated);
        let prefix = kind.prefix();
        let encoded = serde_json::to_vec(&record).map_err(io::Error::other)?;
        let bytes = prefix.len() + encoded.len() + 1;
        let reserved_records = match kind {
            Record::Observation => 1 + usize::from(!self.ready),
            Record::Ready => 1,
            Record::Result => 0,
        };
        if bytes > MAX_RECORD_BYTES {
            return Err(io::Error::other(
                "desktop Player record exceeded its bounded allowance",
            ));
        }
        let records = self.records + 1 + reserved_records;
        if self.bytes + bytes + reserved_records * MAX_RECORD_BYTES > MAX_STREAM_BYTES
            || records > MAX_RECORDS
        {
            if matches!(kind, Record::Observation) {
                self.observations_truncated = true;
                return Ok(());
            }
            return Err(io::Error::other(
                "desktop Player observation exceeded its bounded record or stream allowance",
            ));
        }
        writer.write_all(prefix.as_bytes())?;
        writer.write_all(&encoded)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        self.records += 1;
        self.bytes += bytes;
        self.ready |= matches!(kind, Record::Ready);
        Ok(())
    }
}

pub(super) fn refuse(scenario: Option<Scenario>, message: &str) -> ExitCode {
    let mut result = output::failure(scenario, message);
    result["first_present_verified"] = json!(false);
    result["original_runtime_retired"] = json!(false);
    result["original_pump_returned"] = json!(false);
    result["presented_frames"] = json!(0);
    result["launch_accepted"] = json!(false);
    result["launch_admitted"] = json!(false);
    result["resume_applied"] = json!(false);
    result["ready"] = json!(false);
    result["presentation"] = Value::Null;
    result["adapter"] = Value::Null;
    let _ = output::diagnostic_to(&mut io::stderr().lock(), message);
    let _ = Emitter::default().emit(Record::Result, result);
    ExitCode::FAILURE
}
