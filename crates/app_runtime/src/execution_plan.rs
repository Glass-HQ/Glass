use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionPlan {
    pub label: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub command_label: String,
}
