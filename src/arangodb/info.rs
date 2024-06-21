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

#[derive(Debug, Serialize, Deserialize)]
pub struct DeploymentInfo {
    #[serde(alias = "type")]
    pub deployment_type: DeploymentType,
}

impl PartialEq<DeploymentType> for DeploymentInfo {
    fn eq(&self, other: &DeploymentType) -> bool {
        match self.deployment_type {
            DeploymentType::Cluster => match other {
                DeploymentType::Cluster => true,
                _ => false,
            },
            DeploymentType::Single => match other {
                DeploymentType::Single => true,
                _ => false,
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SupportInfo {
    pub deployment: DeploymentInfo,
}
