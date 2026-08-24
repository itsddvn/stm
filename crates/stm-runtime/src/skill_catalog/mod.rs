mod persistence;
mod remote;

use std::path::{Path, PathBuf};

use stm_core::skill_catalog::{
    AcceptedCatalogIdentity, CatalogRemote, CatalogSnapshotStore, SkillCatalogError,
    SkillCatalogService, SnapshotParts, SystemCatalogClock, VerifiedSkillCatalog,
};

pub use remote::FixedHttpsCatalogRemote;

pub struct FileCatalogSnapshotStore {
    path: PathBuf,
}

impl FileCatalogSnapshotStore {
    pub fn for_workspace_database(workspace_db_path: &Path) -> Self {
        Self {
            path: workspace_db_path.with_file_name("skill-catalog-last-known-good.json"),
        }
    }
}

impl CatalogSnapshotStore for FileCatalogSnapshotStore {
    fn read_state(&self) -> Result<SnapshotParts, SkillCatalogError> {
        persistence::read_state(&self.path)
    }

    fn persist_state(&self, snapshot: &VerifiedSkillCatalog) -> Result<(), SkillCatalogError> {
        persistence::persist_state(&self.path, snapshot)
    }
}

#[derive(Debug, Clone, Copy)]
struct OfflineCatalogRemote;

impl CatalogRemote for OfflineCatalogRemote {
    fn fetch(
        &self,
        _url: &'static str,
        _maximum_bytes: usize,
    ) -> Result<Vec<u8>, SkillCatalogError> {
        Err(SkillCatalogError::Transport(
            "remote catalog refresh was not requested".to_string(),
        ))
    }
}

pub fn load_current_authenticated_catalog(
    workspace_db_path: &Path,
) -> Result<VerifiedSkillCatalog, SkillCatalogError> {
    let store = FileCatalogSnapshotStore::for_workspace_database(workspace_db_path);
    SkillCatalogService::new(OfflineCatalogRemote, SystemCatalogClock)
        .load_last_good_or_bundled(&store, None)
}

pub fn refresh_authenticated_catalog(
    workspace_db_path: &Path,
    accepted: Option<&AcceptedCatalogIdentity>,
) -> Result<VerifiedSkillCatalog, SkillCatalogError> {
    let store = FileCatalogSnapshotStore::for_workspace_database(workspace_db_path);
    let remote = FixedHttpsCatalogRemote::new()?;
    SkillCatalogService::new(remote, SystemCatalogClock).load(&store, accepted)
}
