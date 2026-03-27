#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeDevice {
    pub id: String,
    pub name: String,
    pub kind: RuntimeDeviceKind,
    pub state: RuntimeDeviceState,
    pub os_version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuntimeDeviceKind {
    Simulator,
    Desktop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeDeviceState {
    Booted,
    Shutdown,
    Unknown,
}
