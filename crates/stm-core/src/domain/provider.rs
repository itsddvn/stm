use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallProviderPreference {
    Automatic,
    PreferHomebrew,
    PreferBun,
}

impl Default for InstallProviderPreference {
    fn default() -> Self {
        Self::Automatic
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Homebrew,
    Bun,
    Node,
    Npm,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderTrust {
    ApprovedRoot,
    UntrustedPath,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DetectedProvider {
    pub kind: ProviderKind,
    pub path: String,
    pub version: Option<String>,
    pub trust: ProviderTrust,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInventory {
    pub generation: String,
    pub homebrew: Option<DetectedProvider>,
    pub bun: Option<DetectedProvider>,
    pub node: Option<DetectedProvider>,
    pub npm: Option<DetectedProvider>,
}

impl ProviderInventory {
    pub fn trusted(&self, kind: ProviderKind) -> Option<&DetectedProvider> {
        let candidate = match kind {
            ProviderKind::Homebrew => self.homebrew.as_ref(),
            ProviderKind::Bun => self.bun.as_ref(),
            ProviderKind::Node => self.node.as_ref(),
            ProviderKind::Npm => self.npm.as_ref(),
        }?;
        (candidate.trust == ProviderTrust::ApprovedRoot).then_some(candidate)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceSnapshot {
    pub provider_preference: InstallProviderPreference,
    pub quick_setup_dismissed: bool,
}

pub trait PreferencesStore: Send + Sync {
    fn load(&self) -> PreferenceSnapshot;
    fn set_provider_preference(
        &self,
        preference: InstallProviderPreference,
    ) -> Result<PreferenceSnapshot, String>;
    fn dismiss_quick_setup(&self) -> Result<PreferenceSnapshot, String>;
}

pub struct MemoryPreferencesStore {
    snapshot: std::sync::Mutex<PreferenceSnapshot>,
}

impl MemoryPreferencesStore {
    pub fn new() -> Self {
        Self {
            snapshot: std::sync::Mutex::new(PreferenceSnapshot::default()),
        }
    }
}

impl Default for MemoryPreferencesStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PreferencesStore for MemoryPreferencesStore {
    fn load(&self) -> PreferenceSnapshot {
        self.snapshot.lock().expect("preferences").clone()
    }

    fn set_provider_preference(
        &self,
        preference: InstallProviderPreference,
    ) -> Result<PreferenceSnapshot, String> {
        let mut snapshot = self.snapshot.lock().expect("preferences");
        snapshot.provider_preference = preference;
        Ok(snapshot.clone())
    }

    fn dismiss_quick_setup(&self) -> Result<PreferenceSnapshot, String> {
        let mut snapshot = self.snapshot.lock().expect("preferences");
        snapshot.quick_setup_dismissed = true;
        Ok(snapshot.clone())
    }
}
