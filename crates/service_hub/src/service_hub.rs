use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use thiserror::Error;

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
pub struct ServiceArtifactRef {
    pub kind: ServiceArtifactKind,
    pub path: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceArtifactKind {
    Ipa,
    Pkg,
    AppBundle,
    Binary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceOperationRequest {
    pub provider_id: String,
    pub operation: String,
    pub resource: Option<ServiceResourceRef>,
    pub artifact: Option<ServiceArtifactRef>,
    pub input: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceCommandPlan {
    pub label: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<String, String>,
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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ServiceError {
    #[error("service provider `{0}` is not supported")]
    UnknownProvider(String),
    #[error("service operation `{0}` is not supported")]
    UnsupportedOperation(String),
    #[error("service operation requires resource kind `{expected}`, got `{actual}`")]
    UnexpectedResourceKind { expected: String, actual: String },
    #[error("service operation requires an artifact")]
    ArtifactRequired,
    #[error("service operation requires artifact kind `{expected}`, got `{actual}`")]
    UnexpectedArtifactKind { expected: String, actual: String },
    #[error("service input `{0}` is required")]
    MissingInput(&'static str),
}

pub trait ServiceProvider {
    fn descriptor(&self) -> ServiceProviderDescriptor;
    fn build_operation(
        &self,
        request: &ServiceOperationRequest,
    ) -> Result<ServiceCommandPlan, ServiceError>;
}

pub struct ServiceHub {
    providers: Vec<Box<dyn ServiceProvider + Send + Sync>>,
}

impl Default for ServiceHub {
    fn default() -> Self {
        Self {
            providers: vec![Box::new(AscServiceProvider)],
        }
    }
}

impl ServiceHub {
    pub fn providers(&self) -> Vec<ServiceProviderDescriptor> {
        self.providers
            .iter()
            .map(|provider| provider.descriptor())
            .collect()
    }

    pub fn build_operation(
        &self,
        request: &ServiceOperationRequest,
    ) -> Result<ServiceCommandPlan, ServiceError> {
        let provider = self
            .providers
            .iter()
            .find(|provider| provider.descriptor().id == request.provider_id)
            .ok_or_else(|| ServiceError::UnknownProvider(request.provider_id.clone()))?;
        provider.build_operation(request)
    }
}

pub struct AscServiceProvider;

impl ServiceProvider for AscServiceProvider {
    fn descriptor(&self) -> ServiceProviderDescriptor {
        ServiceProviderDescriptor {
            id: "app-store-connect".to_string(),
            label: "App Store Connect".to_string(),
            auth_kind: ServiceAuthKind::ApiKey,
            capabilities: ServiceCapabilitySet {
                capabilities: [
                    ServiceCapability::Authenticate,
                    ServiceCapability::ListResources,
                    ServiceCapability::UploadArtifact,
                    ServiceCapability::ManageMetadata,
                    ServiceCapability::PublishRelease,
                ]
                .into_iter()
                .collect(),
            },
        }
    }

    fn build_operation(
        &self,
        request: &ServiceOperationRequest,
    ) -> Result<ServiceCommandPlan, ServiceError> {
        match request.operation.as_str() {
            "auth_status" => Ok(build_asc_auth_status()),
            "list_apps" => Ok(build_asc_list_apps(request)),
            "list_builds" => build_asc_list_builds(request),
            "build_pre_release_version" => build_asc_pre_release_version(request),
            "upload_build" => build_asc_upload_build(request),
            "release_run" => build_asc_release_run(request),
            other => Err(ServiceError::UnsupportedOperation(other.to_string())),
        }
    }
}

fn build_asc_auth_status() -> ServiceCommandPlan {
    ServiceCommandPlan {
        label: "Validate App Store Connect authentication".to_string(),
        command: "asc".to_string(),
        args: vec![
            "auth".to_string(),
            "status".to_string(),
            "--validate".to_string(),
            "--output".to_string(),
            "json".to_string(),
            "--pretty".to_string(),
        ],
        cwd: None,
        env: BTreeMap::new(),
    }
}

fn build_asc_list_apps(request: &ServiceOperationRequest) -> ServiceCommandPlan {
    let mut args = vec![
        "apps".to_string(),
        "list".to_string(),
        "--output".to_string(),
        "json".to_string(),
        "--pretty".to_string(),
    ];

    if let Some(limit) = request.input.get("limit") {
        args.push("--limit".to_string());
        args.push(limit.clone());
    }
    if let Some(name) = request.input.get("name") {
        args.push("--name".to_string());
        args.push(name.clone());
    }
    if let Some(bundle_id) = request.input.get("bundle_id") {
        args.push("--bundle-id".to_string());
        args.push(bundle_id.clone());
    }
    if request
        .input
        .get("paginate")
        .is_some_and(|value| value == "true")
    {
        args.push("--paginate".to_string());
    }

    ServiceCommandPlan {
        label: "List App Store Connect apps".to_string(),
        command: "asc".to_string(),
        args,
        cwd: None,
        env: BTreeMap::new(),
    }
}

fn build_asc_list_builds(
    request: &ServiceOperationRequest,
) -> Result<ServiceCommandPlan, ServiceError> {
    let app = request
        .resource
        .as_ref()
        .ok_or(ServiceError::MissingInput("app"))?;
    ensure_resource_kind(app, "app")?;

    let mut args = vec![
        "builds".to_string(),
        "list".to_string(),
        "--app".to_string(),
        app.external_id.clone(),
        "--output".to_string(),
        "json".to_string(),
        "--pretty".to_string(),
    ];

    if let Some(version) = request.input.get("version") {
        args.push("--version".to_string());
        args.push(version.clone());
    }
    if let Some(build_number) = request.input.get("build_number") {
        args.push("--build-number".to_string());
        args.push(build_number.clone());
    }
    if let Some(limit) = request.input.get("limit") {
        args.push("--limit".to_string());
        args.push(limit.clone());
    }
    if let Some(sort) = request.input.get("sort") {
        args.push("--sort".to_string());
        args.push(sort.clone());
    }
    if let Some(processing_state) = request.input.get("processing_state") {
        args.push("--processing-state".to_string());
        args.push(processing_state.clone());
    }
    if request
        .input
        .get("paginate")
        .is_some_and(|value| value == "true")
    {
        args.push("--paginate".to_string());
    }

    Ok(ServiceCommandPlan {
        label: format!("List ASC builds for {}", app.label),
        command: "asc".to_string(),
        args,
        cwd: None,
        env: BTreeMap::new(),
    })
}

fn build_asc_pre_release_version(
    request: &ServiceOperationRequest,
) -> Result<ServiceCommandPlan, ServiceError> {
    let build = request
        .resource
        .as_ref()
        .ok_or(ServiceError::MissingInput("build"))?;
    ensure_resource_kind(build, "build")?;

    Ok(ServiceCommandPlan {
        label: format!("Fetch ASC pre-release version for build {}", build.label),
        command: "asc".to_string(),
        args: vec![
            "builds".to_string(),
            "pre-release-version".to_string(),
            "get".to_string(),
            "--build".to_string(),
            build.external_id.clone(),
            "--output".to_string(),
            "json".to_string(),
            "--pretty".to_string(),
        ],
        cwd: None,
        env: BTreeMap::new(),
    })
}

fn build_asc_upload_build(
    request: &ServiceOperationRequest,
) -> Result<ServiceCommandPlan, ServiceError> {
    let app = request
        .resource
        .as_ref()
        .ok_or(ServiceError::MissingInput("app"))?;
    ensure_resource_kind(app, "app")?;

    let artifact = request
        .artifact
        .as_ref()
        .ok_or(ServiceError::ArtifactRequired)?;
    let (flag, platform) = match artifact.kind {
        ServiceArtifactKind::Ipa => ("--ipa", None),
        ServiceArtifactKind::Pkg => ("--pkg", Some("MAC_OS")),
        ServiceArtifactKind::AppBundle => {
            return Err(ServiceError::UnexpectedArtifactKind {
                expected: "ipa or pkg".to_string(),
                actual: "app_bundle".to_string(),
            });
        }
        ServiceArtifactKind::Binary => {
            return Err(ServiceError::UnexpectedArtifactKind {
                expected: "ipa or pkg".to_string(),
                actual: "binary".to_string(),
            });
        }
    };

    let mut args = vec![
        "builds".to_string(),
        "upload".to_string(),
        "--app".to_string(),
        app.external_id.clone(),
        flag.to_string(),
        artifact.path.to_string_lossy().into_owned(),
        "--output".to_string(),
        "json".to_string(),
        "--pretty".to_string(),
    ];

    if let Some(platform) = platform {
        args.push("--platform".to_string());
        args.push(platform.to_string());
    }
    if let Some(version) = request.input.get("version") {
        args.push("--version".to_string());
        args.push(version.clone());
    }
    if let Some(build_number) = request.input.get("build_number") {
        args.push("--build-number".to_string());
        args.push(build_number.clone());
    }
    if request
        .input
        .get("wait")
        .is_some_and(|value| value == "true")
    {
        args.push("--wait".to_string());
    }

    Ok(ServiceCommandPlan {
        label: format!("Upload build to App Store Connect for {}", app.label),
        command: "asc".to_string(),
        args,
        cwd: None,
        env: BTreeMap::new(),
    })
}

fn build_asc_release_run(
    request: &ServiceOperationRequest,
) -> Result<ServiceCommandPlan, ServiceError> {
    let app = request
        .resource
        .as_ref()
        .ok_or(ServiceError::MissingInput("app"))?;
    ensure_resource_kind(app, "app")?;

    let version = request
        .input
        .get("version")
        .ok_or(ServiceError::MissingInput("version"))?;
    let build = request
        .input
        .get("build")
        .ok_or(ServiceError::MissingInput("build"))?;
    let metadata_dir = request
        .input
        .get("metadata_dir")
        .ok_or(ServiceError::MissingInput("metadata_dir"))?;

    let mut args = vec![
        "release".to_string(),
        "run".to_string(),
        "--app".to_string(),
        app.external_id.clone(),
        "--version".to_string(),
        version.clone(),
        "--build".to_string(),
        build.clone(),
        "--metadata-dir".to_string(),
        metadata_dir.clone(),
        "--output".to_string(),
        "json".to_string(),
        "--pretty".to_string(),
    ];

    if request
        .input
        .get("dry_run")
        .is_some_and(|value| value == "true")
    {
        args.push("--dry-run".to_string());
    } else {
        args.push("--confirm".to_string());
    }

    Ok(ServiceCommandPlan {
        label: format!("Run ASC release for {}", app.label),
        command: "asc".to_string(),
        args,
        cwd: None,
        env: BTreeMap::new(),
    })
}

fn ensure_resource_kind(resource: &ServiceResourceRef, expected: &str) -> Result<(), ServiceError> {
    if resource.kind == expected {
        Ok(())
    } else {
        Err(ServiceError::UnexpectedResourceKind {
            expected: expected.to_string(),
            actual: resource.kind.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AscServiceProvider, ServiceArtifactKind, ServiceArtifactRef, ServiceAuthKind,
        ServiceCapability, ServiceHub, ServiceOperationRequest, ServiceProvider,
        ServiceResourceRef,
    };
    use std::{collections::BTreeMap, path::PathBuf};

    #[test]
    fn advertises_capabilities_without_binding_to_a_transport() {
        let descriptor = AscServiceProvider.descriptor();

        assert!(
            descriptor
                .capabilities
                .supports(ServiceCapability::UploadArtifact)
        );
        assert!(
            descriptor
                .capabilities
                .supports(ServiceCapability::PublishRelease)
        );
        assert_eq!(descriptor.auth_kind, ServiceAuthKind::ApiKey);
    }

    #[test]
    fn builds_real_asc_auth_status_command() {
        let provider = AscServiceProvider;
        let plan = provider
            .build_operation(&ServiceOperationRequest {
                provider_id: "app-store-connect".to_string(),
                operation: "auth_status".to_string(),
                resource: None,
                artifact: None,
                input: BTreeMap::new(),
            })
            .unwrap();

        assert_eq!(plan.command, "asc");
        assert_eq!(plan.args[0], "auth");
        assert!(plan.args.contains(&"--validate".to_string()));
    }

    #[test]
    fn builds_real_asc_list_apps_command() {
        let provider = AscServiceProvider;
        let plan = provider
            .build_operation(&ServiceOperationRequest {
                provider_id: "app-store-connect".to_string(),
                operation: "list_apps".to_string(),
                resource: None,
                artifact: None,
                input: BTreeMap::from([
                    ("limit".to_string(), "25".to_string()),
                    ("paginate".to_string(), "true".to_string()),
                ]),
            })
            .unwrap();

        assert_eq!(plan.command, "asc");
        assert_eq!(plan.args[0], "apps");
        assert!(plan.args.contains(&"--limit".to_string()));
        assert!(plan.args.contains(&"--paginate".to_string()));
    }

    #[test]
    fn builds_real_asc_upload_command_from_artifact_handoff() {
        let hub = ServiceHub::default();
        let plan = hub
            .build_operation(&ServiceOperationRequest {
                provider_id: "app-store-connect".to_string(),
                operation: "upload_build".to_string(),
                resource: Some(ServiceResourceRef {
                    provider_id: "app-store-connect".to_string(),
                    kind: "app".to_string(),
                    external_id: "123456789".to_string(),
                    label: "Glass".to_string(),
                }),
                artifact: Some(ServiceArtifactRef {
                    kind: ServiceArtifactKind::Ipa,
                    path: PathBuf::from("/tmp/Glass.ipa"),
                }),
                input: BTreeMap::from([
                    ("version".to_string(), "1.2.3".to_string()),
                    ("build_number".to_string(), "42".to_string()),
                    ("wait".to_string(), "true".to_string()),
                ]),
            })
            .unwrap();

        assert_eq!(plan.command, "asc");
        assert_eq!(plan.args[0], "builds");
        assert!(plan.args.contains(&"--ipa".to_string()));
        assert!(plan.args.contains(&"/tmp/Glass.ipa".to_string()));
        assert!(plan.args.contains(&"--wait".to_string()));
    }

    #[test]
    fn rejects_app_bundle_for_asc_upload() {
        let provider = AscServiceProvider;
        let result = provider.build_operation(&ServiceOperationRequest {
            provider_id: "app-store-connect".to_string(),
            operation: "upload_build".to_string(),
            resource: Some(ServiceResourceRef {
                provider_id: "app-store-connect".to_string(),
                kind: "app".to_string(),
                external_id: "123456789".to_string(),
                label: "Glass".to_string(),
            }),
            artifact: Some(ServiceArtifactRef {
                kind: ServiceArtifactKind::AppBundle,
                path: PathBuf::from("/tmp/Glass.app"),
            }),
            input: BTreeMap::new(),
        });

        assert!(result.is_err());
    }

    #[test]
    fn builds_real_asc_pre_release_version_command() {
        let provider = AscServiceProvider;
        let plan = provider
            .build_operation(&ServiceOperationRequest {
                provider_id: "app-store-connect".to_string(),
                operation: "build_pre_release_version".to_string(),
                resource: Some(ServiceResourceRef {
                    provider_id: "app-store-connect".to_string(),
                    kind: "build".to_string(),
                    external_id: "BUILD-ID".to_string(),
                    label: "42".to_string(),
                }),
                artifact: None,
                input: BTreeMap::new(),
            })
            .unwrap();

        assert_eq!(plan.args[0], "builds");
        assert_eq!(plan.args[1], "pre-release-version");
        assert!(plan.args.contains(&"--build".to_string()));
        assert!(plan.args.contains(&"BUILD-ID".to_string()));
    }

    #[test]
    fn builds_real_asc_release_command() {
        let provider = AscServiceProvider;
        let plan = provider
            .build_operation(&ServiceOperationRequest {
                provider_id: "app-store-connect".to_string(),
                operation: "release_run".to_string(),
                resource: Some(ServiceResourceRef {
                    provider_id: "app-store-connect".to_string(),
                    kind: "app".to_string(),
                    external_id: "123456789".to_string(),
                    label: "Glass".to_string(),
                }),
                artifact: None,
                input: BTreeMap::from([
                    ("version".to_string(), "1.2.3".to_string()),
                    ("build".to_string(), "BUILD-ID".to_string()),
                    (
                        "metadata_dir".to_string(),
                        "./metadata/version/1.2.3".to_string(),
                    ),
                    ("dry_run".to_string(), "true".to_string()),
                ]),
            })
            .unwrap();

        assert_eq!(plan.args[0], "release");
        assert_eq!(plan.args[1], "run");
        assert!(plan.args.contains(&"--metadata-dir".to_string()));
        assert!(plan.args.contains(&"--dry-run".to_string()));
    }
}
