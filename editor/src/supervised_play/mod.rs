mod build;
mod failure;
mod generation;
mod handshake;
mod output_projection;
mod owner;
mod preparation;
mod process;
mod store_root;
mod supervisor;

pub(super) use nara::project_tooling::process::ProcessOutputStream;
pub(crate) use nara::project_tooling::process::SupervisedProcessWake as SupervisedPlayWake;
pub(crate) use owner::SupervisedDesktopPlayOwner;

#[cfg(test)]
mod tests;
