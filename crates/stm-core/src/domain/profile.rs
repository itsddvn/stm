use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformProfileDocument {
    pub version: String,
    pub profiles: Vec<PlatformProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformProfile {
    pub target: String,
    pub defaults: Vec<String>,
    pub optional: Vec<String>,
}

impl PlatformProfileDocument {
    pub fn for_target(&self, target: &str) -> Option<&PlatformProfile> {
        self.profiles
            .iter()
            .find(|profile| profile.target == target)
            .or_else(|| {
                let family = target.split('_').next().unwrap_or(target);
                self.profiles
                    .iter()
                    .find(|profile| profile.target.starts_with(family))
            })
    }
}
