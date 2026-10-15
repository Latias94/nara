use super::args::CheckArguments;
use nara::project_tooling::{
    ProjectToolInvocationError, ProjectToolRequest, ProjectToolSummary, invoke_project_tool,
};

pub(crate) fn execute(
    arguments: CheckArguments,
) -> Result<ProjectToolSummary, ProjectToolInvocationError> {
    invoke_project_tool(ProjectToolRequest::Check {
        project: arguments.project,
        profile: arguments.profile,
    })
}
