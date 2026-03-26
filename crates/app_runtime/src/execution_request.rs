use crate::RuntimeAction;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionRequest {
    pub project_id: String,
    pub target_id: String,
    pub device_id: Option<String>,
    pub action: RuntimeAction,
}
