//! Opt-in trusted fixture helper. Fault modes exist only for the finite process experiment.

use std::{
    fs,
    io::{self, Read, Write},
    process, thread,
    time::Duration,
};

#[path = "../../src/snap_local_position/numeric.rs"]
#[allow(dead_code)]
mod numeric;
#[allow(dead_code)]
mod snap_child_wire;
use snap_child_wire as wire;

fn gate() -> io::Result<()> {
    fs::write("ready", b"ready")?;
    while !fs::exists("release")? {
        thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}

fn run() -> io::Result<i32> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "normal".into());
    let mut input = [0; wire::REQUEST_BYTES];
    let mut stdin = io::stdin().lock();
    stdin.read_exact(&mut input)?;
    if stdin.read(&mut [0])? != 0 {
        return Ok(2);
    }
    let (sequence, grid, position) =
        wire::read_request(&input).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    if mode == "hold" {
        gate()?;
    }
    let result = numeric::snap_local_position(position, grid).map_err(|error| match error {
        numeric::SnapRejection::InvalidGrid => wire::Refusal::InvalidGrid,
        numeric::SnapRejection::NonFinitePosition => wire::Refusal::NonFinitePosition,
        numeric::SnapRejection::PositionOutOfRange => wire::Refusal::PositionOutOfRange,
        _ => wire::Refusal::ResultOutOfRange,
    });
    let mut output = wire::response(sequence, result);
    match mode.as_str() {
        "wrong-request" => output[8..16].copy_from_slice(&sequence.wrapping_add(1).to_le_bytes()),
        "invalid-number" => output[16..24].copy_from_slice(&f64::NAN.to_le_bytes()),
        "reserved" => output[7] = 1,
        "version" => output[4] = 2,
        "stderr-limit" => {
            io::stderr()
                .lock()
                .write_all(&[b'x'; wire::STDERR_BYTES + 1])?;
        }
        _ => {}
    }
    let mut stdout = io::stdout().lock();
    match mode.as_str() {
        "short" => stdout.write_all(&output[..wire::RESPONSE_BYTES - 1])?,
        "stdout-limit" => stdout.write_all(&[b'x'; 4096])?,
        "extra" => {
            stdout.write_all(&output)?;
            stdout.write_all(b"!")?;
        }
        _ => stdout.write_all(&output)?,
    }
    stdout.flush()?;
    if mode == "hold-after-response" {
        gate()?;
    }
    Ok(if mode == "nonzero" { 7 } else { 0 })
}

fn main() {
    process::exit(run().unwrap_or(2));
}
