use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AscStatus {
    NotInstalled,
    InstalledNotAuthenticated,
    Authenticated,
}

pub fn get_status() -> AscStatus {
    if !check_asc_installed() {
        AscStatus::NotInstalled
    } else if !is_authenticated() {
        AscStatus::InstalledNotAuthenticated
    } else {
        AscStatus::Authenticated
    }
}

pub fn check_homebrew_installed() -> bool {
    Command::new("brew")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn install_asc_via_homebrew() -> Result<()> {
    let tap_output = Command::new("brew")
        .args(["tap", "rudrankriyam/tap"])
        .output()
        .context("Failed to run brew tap")?;

    if !tap_output.status.success() {
        let stderr = String::from_utf8_lossy(&tap_output.stderr);
        anyhow::bail!("brew tap failed: {}", stderr);
    }

    let install_output = Command::new("brew")
        .args(["install", "rudrankriyam/tap/asc"])
        .output()
        .context("Failed to run brew install")?;

    if !install_output.status.success() {
        let stderr = String::from_utf8_lossy(&install_output.stderr);
        anyhow::bail!("brew install failed: {}", stderr);
    }

    Ok(())
}

pub fn install_asc_via_script() -> Result<()> {
    let output = Command::new("sh")
        .args(["-c", "curl -fsSL https://raw.githubusercontent.com/rudrankriyam/App-Store-Connect-CLI/main/install.sh | bash"])
        .output()
        .context("Failed to run install script")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Install script failed: {}", stderr);
    }

    Ok(())
}

pub fn install_asc() -> Result<()> {
    if check_homebrew_installed() {
        install_asc_via_homebrew()
    } else {
        install_asc_via_script()
    }
}

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
    pub processing_state: String,
    pub uploaded_date: Option<String>,
    pub expired: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaGroup {
    pub id: String,
    pub name: String,
    pub is_internal: bool,
    pub public_link: Option<String>,
    pub public_link_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTester {
    pub id: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub invite_type: String,
    pub state: String,
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
        .arg("apps")
        .output()
        .context("Failed to run asc apps. Is asc installed?")?;

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
        .args(["builds", "list", "--app", app_id])
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
        #[serde(default)]
        expired: bool,
    }

    let response: AscBuildsResponse = serde_json::from_str(output)?;

    Ok(response
        .data
        .into_iter()
        .map(|build| Build {
            id: build.id,
            version: build.attributes.version,
            processing_state: build.attributes.processing_state,
            uploaded_date: build.attributes.uploaded_date,
            expired: build.attributes.expired,
        })
        .collect())
}

pub fn list_beta_groups(app_id: &str) -> Result<Vec<BetaGroup>> {
    let output = Command::new("asc")
        .args(["testflight", "beta-groups", "list", "--app", app_id])
        .output()
        .context("Failed to run asc testflight beta-groups list")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("asc testflight beta-groups list failed: {}", stderr);
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
        #[serde(rename = "isInternalGroup", default)]
        is_internal_group: bool,
        #[serde(rename = "publicLinkEnabled", default)]
        public_link_enabled: bool,
        #[serde(rename = "publicLink")]
        public_link: Option<String>,
    }

    let response: AscBetaGroupsResponse = serde_json::from_str(output)?;

    Ok(response
        .data
        .into_iter()
        .map(|group| BetaGroup {
            id: group.id,
            name: group.attributes.name,
            is_internal: group.attributes.is_internal_group,
            public_link_enabled: group.attributes.public_link_enabled,
            public_link: group.attributes.public_link,
        })
        .collect())
}

pub fn list_beta_testers(app_id: &str) -> Result<Vec<BetaTester>> {
    let output = Command::new("asc")
        .args(["testflight", "beta-testers", "list", "--app", app_id])
        .output()
        .context("Failed to run asc testflight beta-testers list")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("asc testflight beta-testers list failed: {}", stderr);
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
        #[serde(rename = "firstName")]
        first_name: Option<String>,
        #[serde(rename = "lastName")]
        last_name: Option<String>,
        email: Option<String>,
        #[serde(rename = "inviteType", default)]
        invite_type: String,
        #[serde(default)]
        state: String,
    }

    let response: AscBetaTestersResponse = serde_json::from_str(output)?;

    Ok(response
        .data
        .into_iter()
        .map(|tester| BetaTester {
            id: tester.id,
            first_name: tester.attributes.first_name,
            last_name: tester.attributes.last_name,
            email: tester.attributes.email,
            invite_type: tester.attributes.invite_type,
            state: tester.attributes.state,
        })
        .collect())
}

pub fn is_authenticated() -> bool {
    let output = Command::new("asc")
        .args(["auth", "status"])
        .output();

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            !stdout.contains("No credentials stored")
        }
        Err(_) => false,
    }
}

#[derive(Debug, Clone, Default)]
pub struct AuthStatus {
    pub is_authenticated: bool,
    pub profile_name: Option<String>,
    pub key_id: Option<String>,
    pub issuer_id: Option<String>,
}

pub fn get_auth_status() -> AuthStatus {
    let output = Command::new("asc")
        .args(["auth", "status", "--verbose"])
        .output();

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);

            if stdout.contains("No credentials stored") {
                return AuthStatus::default();
            }

            let mut status = AuthStatus {
                is_authenticated: true,
                ..Default::default()
            };

            for line in stdout.lines() {
                let line = line.trim();
                if line.starts_with("- ") || line.starts_with("• ") {
                    let profile_line = line.trim_start_matches("- ").trim_start_matches("• ");
                    if let Some(paren_pos) = profile_line.find(" (") {
                        let name = profile_line[..paren_pos].trim();
                        status.profile_name = Some(name.to_string());

                        if let Some(key_start) = profile_line.find("Key ID: ") {
                            let after_key = &profile_line[key_start + 8..];
                            if let Some(key_end) = after_key.find(')') {
                                status.key_id = Some(after_key[..key_end].to_string());
                            }
                        }
                    }
                }
            }

            status
        }
        Err(_) => AuthStatus::default(),
    }
}

pub fn logout() -> Result<()> {
    let output = Command::new("asc")
        .args(["auth", "logout"])
        .output()
        .context("Failed to run asc auth logout")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("asc auth logout failed: {}", stderr);
    }

    Ok(())
}

pub fn authenticate(name: &str, key_id: &str, issuer_id: &str, private_key_path: &str) -> Result<()> {
    let output = Command::new("asc")
        .args([
            "auth",
            "login",
            "--name",
            name,
            "--key-id",
            key_id,
            "--issuer-id",
            issuer_id,
            "--private-key",
            private_key_path,
        ])
        .output()
        .context("Failed to run asc auth login")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("asc auth login failed: {}", stderr);
    }

    Ok(())
}

pub fn open_api_keys_page() -> Result<()> {
    Command::new("open")
        .arg("https://appstoreconnect.apple.com/access/integrations/api")
        .spawn()
        .context("Failed to open App Store Connect API keys page")?;
    Ok(())
}
