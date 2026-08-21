mod log;
mod process;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProcessOutputStream {
    Stdout,
    Stderr,
}

#[cfg(test)]
mod tests;
