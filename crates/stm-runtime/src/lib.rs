//! Infrastructure implementations for STM desktop composition.

pub mod bun_bootstrap;
pub mod homebrew_bootstrap;
pub mod host;
pub mod lifecycle_executor;
mod linux;
pub mod manager_evidence;
#[cfg(test)]
mod native_quick_setup_tests;
#[cfg(test)]
mod platform_contract_tests;
pub mod preferences;
pub mod process_liveness;
pub mod providers;
pub mod source_probe;
pub mod storage;

pub use bun_bootstrap::{prepare_bun_binary, BunBootstrapError, VerifiedBunBinary};
pub use homebrew_bootstrap::{
    download_and_verify_homebrew_pkg, verify_homebrew_pkg_identity, HomebrewBootstrapError,
    VerifiedHomebrewPkg,
};
pub use host::RealHostExecutableResolver;
pub use lifecycle_executor::RealLifecycleExecutor;
pub use manager_evidence::RealManagerEvidence;
pub use preferences::{default_data_dir, JsonPreferencesStore};
pub use process_liveness::NativeProcessLiveness;
pub use providers::detect_provider_inventory;
pub use source_probe::BoundedHttpsSourceProbe;
pub use storage::SqliteSnapshotStore;
