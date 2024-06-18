use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct VersionInformation {
    server: String,
    license: String,
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DeploymentType {
    Cluster,
    Single,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeploymentInfo {
    #[serde(alias = "type")]
    pub deployment_type: DeploymentType,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SupportInfo {
    pub deployment: DeploymentInfo,
}
