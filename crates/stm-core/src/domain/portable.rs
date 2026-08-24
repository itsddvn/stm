use serde::{Deserialize, Serialize};

pub const PORTABLE_SCHEMA_VERSION: u32 = 1;
pub const MAX_PORTABLE_BYTES: usize = 64 * 1024;
pub const MAX_PORTABLE_RESOURCES: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortableSetupDocument {
    pub schema_version: u32,
    pub target: String,
    pub resources: Vec<PortableResource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortableResource {
    pub kind: String,
    pub id: String,
    pub desired_action: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credential_reference_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PortableImportResult {
    pub document: PortableSetupDocument,
    pub warnings: Vec<String>,
    pub review_required_ids: Vec<String>,
}

impl PortableSetupDocument {
    pub fn validate_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_PORTABLE_BYTES {
            return Err("portable setup exceeds 64 KiB".to_string());
        }
        let document: Self = serde_json::from_slice(bytes)
            .map_err(|error| format!("invalid portable setup: {error}"))?;
        if document.schema_version != PORTABLE_SCHEMA_VERSION {
            return Err("unsupported portable schema version".to_string());
        }
        if document.target.trim().is_empty() {
            return Err("portable setup requires a target".to_string());
        }
        if document.resources.len() > MAX_PORTABLE_RESOURCES {
            return Err(format!(
                "portable setup exceeds {MAX_PORTABLE_RESOURCES} resources"
            ));
        }
        for resource in &document.resources {
            if resource.id.trim().is_empty() {
                return Err("portable resource id is required".to_string());
            }
            if looks_like_machine_path(&resource.id) {
                return Err("portable resource IDs may not contain machine paths".to_string());
            }
            if !matches!(resource.kind.as_str(), "tool" | "skill" | "mcp") {
                return Err(format!(
                    "unsupported portable resource kind: {}",
                    resource.kind
                ));
            }
            if !matches!(
                resource.desired_action.as_str(),
                "keep" | "install" | "update" | "enable" | "add" | "review"
            ) {
                return Err(format!(
                    "unsupported portable desired action: {}",
                    resource.desired_action
                ));
            }
            for reference in &resource.credential_reference_ids {
                if !is_valid_credential_reference(reference) {
                    return Err("credential reference must be a bounded identifier".to_string());
                }
            }
        }
        Ok(document)
    }

    pub fn to_safe_json_bytes(&self) -> Result<Vec<u8>, String> {
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("portable setup serialization failed: {error}"))?;
        if bytes.len() > MAX_PORTABLE_BYTES {
            return Err("portable setup exceeds 64 KiB".to_string());
        }
        let text = String::from_utf8_lossy(&bytes);
        let secret_patterns = [
            r"(?i)bearer\s+[A-Za-z0-9\-._~+/]{20,}",
            r"eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+",
            r"AKIA[0-9A-Z]{16}",
            r"-----BEGIN (?:RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----",
            r"ghp_[A-Za-z0-9]{36}",
            r"sk_(?:live|test)_[A-Za-z0-9]{24,}",
        ];
        for pattern in secret_patterns {
            if regex::Regex::new(pattern)
                .map_err(|error| error.to_string())?
                .is_match(&text)
            {
                return Err("portable export contains a secret-shaped value".to_string());
            }
        }
        Ok(bytes)
    }
}
pub fn is_valid_credential_reference(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

pub fn looks_like_machine_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    value.starts_with('/')
        || value.starts_with('\\')
        || value.starts_with("~/")
        || value.starts_with("~\\")
        || value.starts_with("$HOME/")
        || value.starts_with("$HOME\\")
        || value.starts_with("%USERPROFILE%")
        || value.to_ascii_lowercase().starts_with("file:")
        || value.to_ascii_lowercase().starts_with("$env:userprofile")
        || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
}

pub fn validate_portable_document(
    document: &PortableSetupDocument,
    current_target: &str,
) -> Result<Vec<String>, crate::error::CoreError> {
    if document.schema_version != PORTABLE_SCHEMA_VERSION {
        return Err(crate::error::CoreError::MalformedInput(
            "unsupported portable schema version".to_string(),
        ));
    }
    if document.target.trim().is_empty() {
        return Err(crate::error::CoreError::MalformedInput(
            "portable setup requires a target".to_string(),
        ));
    }
    if document.target != current_target {
        return Err(crate::error::CoreError::MalformedInput(format!(
            "this file is for {} and cannot be imported on {}",
            document.target, current_target
        )));
    }
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_mismatched_targets() {
        let foreign = PortableSetupDocument {
            schema_version: 1,
            target: "linux_x64".to_string(),
            resources: vec![],
        };
        assert!(validate_portable_document(&foreign, "macos_arm64").is_err());
        let same_family = PortableSetupDocument {
            schema_version: 1,
            target: "macos_x64".to_string(),
            resources: vec![],
        };
        assert!(validate_portable_document(&same_family, "macos_arm64").is_err());
    }

    #[test]
    fn rejects_command_fields_and_oversize() {
        let error = PortableSetupDocument::validate_bytes(
            br#"{"schemaVersion":1,"target":"macos_arm64","resources":[{"kind":"tool","id":"git","desiredAction":"install","command":"rm"}]}"#,
        )
        .expect_err("command");
        assert!(error.contains("invalid portable setup"));
        let huge = format!(
            r#"{{"schemaVersion":1,"target":"macos_arm64","resources":[{{"kind":"tool","id":"{}","desiredAction":"install"}}]}}"#,
            "x".repeat(70_000)
        );
        assert!(PortableSetupDocument::validate_bytes(huge.as_bytes())
            .unwrap_err()
            .contains("64 KiB"));
    }
}
