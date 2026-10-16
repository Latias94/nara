mod build;
mod generation;
mod handshake;
mod log;
mod output_projection;
mod owner;
mod preparation;
mod process;
mod store_root;
mod supervisor;
mod wake;

pub(crate) use owner::SupervisedDesktopPlayOwner;
pub(crate) use wake::SupervisedPlayWake;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProcessOutputStream {
    Stdout,
    Stderr,
}

#[cfg(test)]
mod tests;
