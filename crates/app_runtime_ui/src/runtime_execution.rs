use app_runtime::{ExecutionRequest, RuntimeCatalog, RuntimeError};
use gpui::Window;
use workspace::Workspace;

use crate::runtime_execution_page::RuntimeExecutionPage;

pub fn execute_runtime_request(
    workspace: &mut Workspace,
    catalog: &RuntimeCatalog,
    request: &ExecutionRequest,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) -> Result<(), RuntimeError> {
    let plan = catalog.build_execution_plan(request)?;
    let session_label = describe_session(catalog, request)?;
    RuntimeExecutionPage::open_or_reuse(
        workspace,
        runtime_task_id(request, cx),
        session_label,
        request.clone(),
        plan,
        window,
        cx,
    );
    Ok(())
}

fn describe_session(
    catalog: &RuntimeCatalog,
    request: &ExecutionRequest,
) -> Result<String, RuntimeError> {
    let project = catalog
        .projects
        .iter()
        .find(|project| project.id == request.project_id)
        .ok_or_else(|| RuntimeError::ProjectNotFound(request.project_id.clone()))?;
    let target = project
        .targets
        .iter()
        .find(|target| target.id == request.target_id)
        .ok_or_else(|| RuntimeError::TargetNotFound(request.target_id.clone()))?;

    Ok(match &request.device_id {
        Some(device_id) => {
            let device = project
                .devices
                .iter()
                .find(|device| &device.id == device_id)
                .ok_or_else(|| RuntimeError::DeviceNotFound(device_id.clone()))?;
            format!(
                "{} {} on {}",
                request.action.label(),
                target.label,
                device.name
            )
        }
        None => format!("{} {}", request.action.label(), target.label),
    })
}

fn runtime_task_id(request: &ExecutionRequest, cx: &gpui::Context<Workspace>) -> String {
    format!(
        "app-runtime-{}-{}-{}-{}-{}",
        sanitize_for_id(&format!("{:?}", cx.entity().entity_id())),
        sanitize_for_id(request.action.label()),
        sanitize_for_id(&request.project_id),
        sanitize_for_id(&request.target_id),
        sanitize_for_id(request.device_id.as_deref().unwrap_or("none")),
    )
}

fn sanitize_for_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect()
}
