use std::{
    ffi::{OsStr, OsString},
    io,
    os::windows::ffi::OsStrExt,
    path::Path,
};

const WINDOWS_COMMAND_LINE_LIMIT: usize = 32_767;

/// Encodes the executable and arguments, including the terminator required by `CreateProcessW`.
pub(crate) fn encode_command_line(
    executable: &Path,
    arguments: &[OsString],
) -> io::Result<Vec<u16>> {
    let mut encoded = Vec::new();
    append_windows_argument(&mut encoded, executable.as_os_str())?;
    for argument in arguments {
        encoded.push(b' ' as u16);
        append_windows_argument(&mut encoded, argument)?;
    }
    if encoded.len() >= WINDOWS_COMMAND_LINE_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the encoded Windows command line exceeds 32,767 UTF-16 code units",
        ));
    }
    encoded.push(0);
    Ok(encoded)
}

fn append_windows_argument(output: &mut Vec<u16>, argument: &OsStr) -> io::Result<()> {
    let units = argument.encode_wide().collect::<Vec<_>>();
    if units.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the Windows process argument contains an interior NUL",
        ));
    }
    let quoted = units.is_empty()
        || units.iter().any(|unit| {
            *unit == u16::from(b' ') || *unit == u16::from(b'\t') || *unit == u16::from(b'"')
        });
    if !quoted {
        output.extend(units);
        return Ok(());
    }

    output.push(b'"' as u16);
    let mut backslashes = 0_usize;
    for unit in units {
        if unit == b'\\' as u16 {
            backslashes += 1;
            continue;
        }
        if unit == b'"' as u16 {
            output.extend(std::iter::repeat_n(
                b'\\' as u16,
                backslashes.saturating_mul(2).saturating_add(1),
            ));
            output.push(unit);
        } else {
            output.extend(std::iter::repeat_n(b'\\' as u16, backslashes));
            output.push(unit);
        }
        backslashes = 0;
    }
    output.extend(std::iter::repeat_n(
        b'\\' as u16,
        backslashes.saturating_mul(2),
    ));
    output.push(b'"' as u16);
    Ok(())
}
