use thiserror::Error;

use crate::RuntimeAction;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RuntimeError {
    #[error("runtime project `{0}` was not found")]
    ProjectNotFound(String),
    #[error("runtime target `{0}` was not found")]
    TargetNotFound(String),
    #[error("{0} is unavailable: {1}")]
    ActionUnavailable(RuntimeActionLabel, String),
    #[error("run requires a device selection")]
    DeviceRequired,
    #[error("runtime device `{0}` was not found")]
    DeviceNotFound(String),
    #[error("the selected device kind is not supported for this runtime target")]
    UnsupportedDeviceKind,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("{0}")]
pub struct RuntimeActionLabel(&'static str);

impl From<RuntimeAction> for RuntimeActionLabel {
    fn from(value: RuntimeAction) -> Self {
        Self(value.label())
    }
}
