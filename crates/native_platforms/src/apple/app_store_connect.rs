use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct App {
    pub id: String,
    pub name: String,
    pub bundle_id: String,
    pub sku: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Build {
    pub id: String,
    pub version: String,
    pub build_number: String,
    pub processing_state: String,
    pub uploaded_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaGroup {
    pub id: String,
    pub name: String,
    pub is_internal: bool,
    pub public_link_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTester {
    pub id: String,
    pub email: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub invite_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AscConfig {
    pub api_key_id: String,
    pub issuer_id: String,
    pub key_path: String,
}

pub fn check_asc_installed() -> bool {
    Command::new("asc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn list_apps() -> Result<Vec<App>> {
    let output = Command::new("asc")
        .args(["apps", "--json"])
        .output()
        .context("Failed to run asc apps. Is asc installed? (brew tap rudrankriyam/tap && brew install asc)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("asc apps failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_apps_output(&stdout)
}

fn parse_apps_output(output: &str) -> Result<Vec<App>> {
    #[derive(Deserialize)]
    struct AscAppsResponse {
        data: Vec<AscApp>,
    }

    #[derive(Deserialize)]
    struct AscApp {
        id: String,
        attributes: AscAppAttributes,
    }

    #[derive(Deserialize)]
    struct AscAppAttributes {
        name: String,
        #[serde(rename = "bundleId")]
        bundle_id: String,
        sku: Option<String>,
    }

    let response: AscAppsResponse = serde_json::from_str(output)?;

    Ok(response
        .data
        .into_iter()
        .map(|app| App {
            id: app.id,
            name: app.attributes.name,
            bundle_id: app.attributes.bundle_id,
            sku: app.attributes.sku,
        })
        .collect())
}

pub fn list_builds(app_id: &str) -> Result<Vec<Build>> {
    let output = Command::new("asc")
        .args(["builds", "list", "--app", app_id, "--json"])
        .output()
        .context("Failed to run asc builds list")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("asc builds list failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_builds_output(&stdout)
}

fn parse_builds_output(output: &str) -> Result<Vec<Build>> {
    #[derive(Deserialize)]
    struct AscBuildsResponse {
        data: Vec<AscBuild>,
    }

    #[derive(Deserialize)]
    struct AscBuild {
        id: String,
        attributes: AscBuildAttributes,
    }

    #[derive(Deserialize)]
    struct AscBuildAttributes {
        version: String,
        #[serde(rename = "uploadedDate")]
        uploaded_date: Option<String>,
        #[serde(rename = "processingState")]
        processing_state: String,
        #[serde(rename = "buildAudienceType")]
        build_audience_type: Option<String>,
    }

    let response: AscBuildsResponse = serde_json::from_str(output)?;

    Ok(response
        .data
        .into_iter()
        .map(|build| Build {
            id: build.id.clone(),
            version: build.attributes.version,
            build_number: build.id,
            processing_state: build.attributes.processing_state,
            uploaded_date: build.attributes.uploaded_date,
        })
        .collect())
}

pub fn list_beta_groups(app_id: &str) -> Result<Vec<BetaGroup>> {
    let output = Command::new("asc")
        .args(["beta-groups", "list", "--app", app_id, "--json"])
        .output()
        .context("Failed to run asc beta-groups list")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("asc beta-groups list failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_beta_groups_output(&stdout)
}

fn parse_beta_groups_output(output: &str) -> Result<Vec<BetaGroup>> {
    #[derive(Deserialize)]
    struct AscBetaGroupsResponse {
        data: Vec<AscBetaGroup>,
    }

    #[derive(Deserialize)]
    struct AscBetaGroup {
        id: String,
        attributes: AscBetaGroupAttributes,
    }

    #[derive(Deserialize)]
    struct AscBetaGroupAttributes {
        name: String,
        #[serde(rename = "isInternalGroup")]
        is_internal_group: bool,
        #[serde(rename = "publicLinkEnabled")]
        public_link_enabled: Option<bool>,
    }

    let response: AscBetaGroupsResponse = serde_json::from_str(output)?;

    Ok(response
        .data
        .into_iter()
        .map(|group| BetaGroup {
            id: group.id,
            name: group.attributes.name,
            is_internal: group.attributes.is_internal_group,
            public_link_enabled: group.attributes.public_link_enabled.unwrap_or(false),
        })
        .collect())
}

pub fn list_beta_testers(app_id: &str) -> Result<Vec<BetaTester>> {
    let output = Command::new("asc")
        .args(["beta-testers", "list", "--app", app_id, "--json"])
        .output()
        .context("Failed to run asc beta-testers list")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("asc beta-testers list failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_beta_testers_output(&stdout)
}

fn parse_beta_testers_output(output: &str) -> Result<Vec<BetaTester>> {
    #[derive(Deserialize)]
    struct AscBetaTestersResponse {
        data: Vec<AscBetaTester>,
    }

    #[derive(Deserialize)]
    struct AscBetaTester {
        id: String,
        attributes: AscBetaTesterAttributes,
    }

    #[derive(Deserialize)]
    struct AscBetaTesterAttributes {
        email: String,
        #[serde(rename = "firstName")]
        first_name: Option<String>,
        #[serde(rename = "lastName")]
        last_name: Option<String>,
        #[serde(rename = "inviteType")]
        invite_type: Option<String>,
    }

    let response: AscBetaTestersResponse = serde_json::from_str(output)?;

    Ok(response
        .data
        .into_iter()
        .map(|tester| BetaTester {
            id: tester.id,
            email: tester.attributes.email,
            first_name: tester.attributes.first_name,
            last_name: tester.attributes.last_name,
            invite_type: tester.attributes.invite_type,
        })
        .collect())
}

pub fn is_authenticated() -> bool {
    let output = Command::new("asc")
        .args(["auth", "status"])
        .output();

    match output {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

pub fn authenticate(api_key_id: &str, issuer_id: &str, key_path: &str) -> Result<()> {
    let output = Command::new("asc")
        .args([
            "auth",
            "login",
            "--api-key-id",
            api_key_id,
            "--issuer-id",
            issuer_id,
            "--key-path",
            key_path,
        ])
        .output()
        .context("Failed to run asc auth login")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("asc auth login failed: {}", stderr);
    }

    Ok(())
}
