use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServiceCapabilitySet {
    pub capabilities: BTreeSet<ServiceCapability>,
}

impl ServiceCapabilitySet {
    pub fn supports(&self, capability: ServiceCapability) -> bool {
        self.capabilities.contains(&capability)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ServiceCapability {
    Authenticate,
    ListResources,
    UploadArtifact,
    ManageMetadata,
    PublishRelease,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceProviderDescriptor {
    pub id: String,
    pub label: String,
    pub auth_kind: ServiceAuthKind,
    pub capabilities: ServiceCapabilitySet,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServiceAuthKind {
    None,
    ApiKey,
    OAuth,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceResourceRef {
    pub provider_id: String,
    pub kind: String,
    pub external_id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceOperationRequest {
    pub provider_id: String,
    pub operation: String,
    pub resource: Option<ServiceResourceRef>,
    pub input: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceOperationUpdate {
    pub state: ServiceOperationState,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServiceOperationState {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::{
        ServiceAuthKind, ServiceCapability, ServiceCapabilitySet, ServiceProviderDescriptor,
    };

    #[test]
    fn advertises_capabilities_without_binding_to_a_transport() {
        let descriptor = ServiceProviderDescriptor {
            id: "app-store-connect".to_string(),
            label: "App Store Connect".to_string(),
            auth_kind: ServiceAuthKind::ApiKey,
            capabilities: ServiceCapabilitySet {
                capabilities: [
                    ServiceCapability::Authenticate,
                    ServiceCapability::UploadArtifact,
                    ServiceCapability::ManageMetadata,
                ]
                .into_iter()
                .collect(),
            },
        };

        assert!(descriptor
            .capabilities
            .supports(ServiceCapability::UploadArtifact));
        assert!(!descriptor
            .capabilities
            .supports(ServiceCapability::PublishRelease));
    }
}
