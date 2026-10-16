use super::args::SchemaArguments;
use nara::project_tooling::{
    ProjectToolInvocationError, ProjectToolRequest, ProjectToolSummary, invoke_project_tool,
};

pub(crate) fn execute(
    arguments: SchemaArguments,
) -> Result<ProjectToolSummary, ProjectToolInvocationError> {
    invoke_project_tool(ProjectToolRequest::Schema {
        project: arguments.project,
        profile: arguments.profile,
        write: arguments.write,
    })
}
