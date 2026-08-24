use std::{collections::BTreeMap, fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{application::adapters::compute_sha256, error::CoreError};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UiContractVerification {
    pub version: String,
    pub locked: bool,
    pub artifact_count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UiContractManifest {
    contract_version: String,
    status: String,
    artifacts: Vec<String>,
    lock_file: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UiContractLock {
    contract_version: String,
    artifacts: BTreeMap<String, String>,
}

pub fn verify_locked_ui_contract(project_root: &Path) -> Result<UiContractVerification, CoreError> {
    let manifest_path = project_root.join("contracts/ui/ui-contract.manifest.json");
    let manifest: UiContractManifest = serde_json::from_str(&fs::read_to_string(&manifest_path)?)?;
    if manifest.status != "locked" {
        return Ok(UiContractVerification {
            version: manifest.contract_version,
            locked: false,
            artifact_count: manifest.artifacts.len(),
        });
    }
    let lock_path = project_root.join("contracts/ui").join(&manifest.lock_file);
    let lock: UiContractLock = serde_json::from_str(&fs::read_to_string(lock_path)?)?;
    if lock.contract_version != manifest.contract_version {
        return Err(CoreError::MalformedInput(
            "ui contract lock version mismatch".to_string(),
        ));
    }
    for artifact in &manifest.artifacts {
        let Some(expected_digest) = lock.artifacts.get(artifact) else {
            return Err(CoreError::MalformedInput(format!(
                "ui contract lock missing artifact digest for {artifact}"
            )));
        };
        let artifact_path = project_root.join(artifact);
        let actual_digest = compute_sha256([fs::read(&artifact_path)?]);
        let actual_digest = actual_digest
            .strip_prefix("sha256:")
            .unwrap_or(&actual_digest);
        if actual_digest != expected_digest {
            return Err(CoreError::MalformedInput(format!(
                "ui contract artifact digest mismatch for {artifact}"
            )));
        }
    }
    Ok(UiContractVerification {
        version: manifest.contract_version,
        locked: true,
        artifact_count: lock.artifacts.len(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn ui_contract_verification_reads_contract_version_and_lock_artifact_count() {
        let temp = TempDir::new().expect("tempdir");
        let contract_root = temp.path().join("contracts/ui");
        let app_path = temp.path().join("src/app/app.tsx");
        let test_path = temp.path().join("src/test/ui-contract.test.ts");
        fs::create_dir_all(&contract_root).expect("contract dir");
        fs::create_dir_all(app_path.parent().expect("app parent")).expect("app dir");
        fs::create_dir_all(test_path.parent().expect("test parent")).expect("test dir");
        fs::write(&app_path, "app contract").expect("app artifact");
        fs::write(&test_path, "test contract").expect("test artifact");

        fs::write(
            contract_root.join("ui-contract.manifest.json"),
            r#"{
  "contractVersion": "1.0.0",
  "status": "locked",
  "approval": {
    "approvedBy": "Project lead",
    "approvedAt": "2026-08-20T18:32:28Z"
  },
  "lockFile": "ui-contract.lock.json",
  "artifacts": [
    "src/app/app.tsx",
    "src/test/ui-contract.test.ts"
  ]
}
"#,
        )
        .expect("manifest");
        let app_digest = compute_sha256([b"app contract".to_vec()]);
        let test_digest = compute_sha256([b"test contract".to_vec()]);
        fs::write(
            contract_root.join("ui-contract.lock.json"),
            format!(
                r#"{{
  "contractVersion": "1.0.0",
  "artifacts": {{
    "src/app/app.tsx": "{}",
    "src/test/ui-contract.test.ts": "{}"
  }}
}}
"#,
                app_digest.trim_start_matches("sha256:"),
                test_digest.trim_start_matches("sha256:")
            ),
        )
        .expect("lock");

        let verification = verify_locked_ui_contract(temp.path()).expect("verification");
        assert_eq!(verification.version, "1.0.0");
        assert!(verification.locked);
        assert_eq!(verification.artifact_count, 2);
    }

    #[test]
    fn ui_contract_verification_rejects_missing_lock_entries() {
        let temp = TempDir::new().expect("tempdir");
        let contract_root = temp.path().join("contracts/ui");
        fs::create_dir_all(&contract_root).expect("contract dir");

        fs::write(
            contract_root.join("ui-contract.manifest.json"),
            r#"{
  "contractVersion": "1.0.0",
  "status": "locked",
  "approval": {
    "approvedBy": "Project lead",
    "approvedAt": "2026-08-20T18:32:28Z"
  },
  "lockFile": "ui-contract.lock.json",
  "artifacts": [
    "src/app/app.tsx"
  ]
}
"#,
        )
        .expect("manifest");
        fs::write(
            contract_root.join("ui-contract.lock.json"),
            r#"{
  "contractVersion": "1.0.0",
  "artifacts": {}
}
"#,
        )
        .expect("lock");

        let error = verify_locked_ui_contract(temp.path()).expect_err("verification should fail");
        assert!(matches!(error, CoreError::MalformedInput(_)));
    }
}
