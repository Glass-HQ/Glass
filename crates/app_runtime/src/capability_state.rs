#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityState {
    Available,
    RequiresSetup { reason: String },
    Unavailable { reason: String },
}

impl CapabilityState {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }
}
