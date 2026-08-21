use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ElevationStrategy {
    pub os: HostOs,
    pub supported: bool,
    pub mechanism: String,
    pub fallback: String,
    pub captures_password: bool,
    pub persistent_helper: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HostOs {
    Windows,
    Macos,
    Linux,
}

pub fn strategy_for_current_host() -> ElevationStrategy {
    if cfg!(target_os = "windows") {
        ElevationStrategy {
      os: HostOs::Windows,
      supported: true,
      mechanism: "relaunch approved child process with ShellExecute runas or manager-native UAC prompt".to_string(),
      fallback: "detect-only when the manager lacks a non-interactive elevated flow".to_string(),
      captures_password: false,
      persistent_helper: false,
    }
    } else if cfg!(target_os = "macos") {
        ElevationStrategy {
      os: HostOs::Macos,
      supported: true,
      mechanism: "delegate elevation to system authorization prompt driven by approved installer or manager tooling".to_string(),
      fallback: "vendor handoff or detect-only when authorization services are unavailable".to_string(),
      captures_password: false,
      persistent_helper: false,
    }
    } else {
        ElevationStrategy {
            os: HostOs::Linux,
            supported: true,
            mechanism: "delegate to approved polkit or desktop privilege broker when present"
                .to_string(),
            fallback: "detect-only when no desktop broker is available".to_string(),
            captures_password: false,
            persistent_helper: false,
        }
    }
}
