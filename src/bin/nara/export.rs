use super::args::ExportArguments;
use nara::project_tooling::{
    ProjectToolInvocationError, ProjectToolRequest, ProjectToolSummary, invoke_project_tool,
};

pub(crate) fn execute(
    arguments: ExportArguments,
) -> Result<ProjectToolSummary, ProjectToolInvocationError> {
    invoke_project_tool(ProjectToolRequest::Export {
        project: arguments.project,
        parent: arguments.parent,
        destination: arguments.destination,
    })
}
