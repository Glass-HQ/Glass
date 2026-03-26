use crate::CapabilityState;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeCapabilitySet {
    pub run: CapabilityState,
    pub build: CapabilityState,
}
