use std::{
    fs,
    path::{Path, PathBuf},
};

use stm_core::domain::provider::{InstallProviderPreference, PreferenceSnapshot, PreferencesStore};

const PREFERENCES_FILE: &str = "stm-preferences.json";

pub fn default_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("STM_DATA_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/stm")
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or(home)
            .join("stm")
    } else {
        home.join(".local/share/stm")
    }
}

pub struct JsonPreferencesStore {
    path: PathBuf,
}

impl JsonPreferencesStore {
    pub fn new(runtime_data_dir: impl Into<PathBuf>) -> Self {
        Self {
            path: runtime_data_dir.into().join(PREFERENCES_FILE),
        }
    }

    fn read(&self) -> PreferenceSnapshot {
        fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn write(&self, snapshot: &PreferenceSnapshot) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(snapshot).map_err(|error| error.to_string())?;
        atomic_write(&self.path, &bytes)
    }
}

impl PreferencesStore for JsonPreferencesStore {
    fn load(&self) -> PreferenceSnapshot {
        self.read()
    }

    fn set_provider_preference(
        &self,
        preference: InstallProviderPreference,
    ) -> Result<PreferenceSnapshot, String> {
        let mut snapshot = self.read();
        snapshot.provider_preference = preference;
        self.write(&snapshot)?;
        Ok(snapshot)
    }

    fn dismiss_quick_setup(&self) -> Result<PreferenceSnapshot, String> {
        let mut snapshot = self.read();
        snapshot.quick_setup_dismissed = true;
        self.write(&snapshot)?;
        Ok(snapshot)
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, bytes).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    #[cfg(target_os = "windows")]
    if path.exists() {
        fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    fs::rename(&temp, path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn persists_preference_and_dismissal_across_reload() {
        let temp = TempDir::new().expect("tempdir");
        let store = JsonPreferencesStore::new(temp.path());
        store
            .set_provider_preference(InstallProviderPreference::PreferHomebrew)
            .expect("preference");
        store.dismiss_quick_setup().expect("dismiss");
        let reloaded = JsonPreferencesStore::new(temp.path()).load();
        assert_eq!(
            reloaded.provider_preference,
            InstallProviderPreference::PreferHomebrew
        );
        assert!(reloaded.quick_setup_dismissed);
    }

    #[test]
    fn default_data_dir_honors_override() {
        std::env::set_var("STM_DATA_DIR", "/tmp/stm-test-data");
        assert_eq!(default_data_dir(), PathBuf::from("/tmp/stm-test-data"));
        std::env::remove_var("STM_DATA_DIR");
    }
}
