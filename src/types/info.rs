use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct VersionInformation {
    server: String,
    license: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DeploymentType {
    Cluster,
    Single,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeploymentInfo {
    #[serde(alias = "type")]
    pub deployment_type: DeploymentType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportInfo {
    pub deployment: DeploymentInfo,
}

#[derive(Eq, PartialEq, Copy, Clone)]
pub enum LoadStrategy {
    Dump,
    Aql,
}
