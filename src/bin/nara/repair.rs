use super::args::RepairArguments;
use nara::project_tooling::{
    ProjectToolInvocationError, ProjectToolRequest, ProjectToolSummary, invoke_project_tool,
};

pub(crate) fn execute(
    arguments: RepairArguments,
) -> Result<ProjectToolSummary, ProjectToolInvocationError> {
    invoke_project_tool(ProjectToolRequest::RepairStartupScene {
        project: arguments.project,
        profile: arguments.profile,
        replacement: arguments.replacement,
    })
}
