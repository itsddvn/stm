use url::Url;

use crate::{
    domain::source::{SourceAnalysisRecord, SourceAnalysisStatus, SourceKind, SourceTrust},
    error::CoreError,
};

pub fn analyze_source(
    kind: SourceKind,
    submitted_url: &str,
) -> Result<SourceAnalysisRecord, CoreError> {
    let parsed = Url::parse(submitted_url)
        .map_err(|_| CoreError::MalformedInput("enter a complete HTTPS URL".to_string()))?;
    let sanitized = sanitize_source_url(&parsed)?;
    let blocked = parsed.scheme() != "https"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some();
    let source_host = parsed.host_str().unwrap_or("unknown").to_string();

    if blocked {
        let note = if parsed.scheme() != "https" {
            "Use a complete HTTPS URL."
        } else {
            "Remove embedded credentials and query parameters before review."
        };
        return Ok(blocked_analysis(kind, sanitized, source_host, note));
    }

    let normalized = Url::parse(&sanitized).map_err(CoreError::Url)?;
    let defaults = defaults_for(&kind);

    Ok(SourceAnalysisRecord {
        kind: kind.clone(),
        submitted_url: normalized.to_string(),
        normalized_url: Some(normalized.to_string()),
        status: SourceAnalysisStatus::ReviewReady,
        detected_name: detect_name(&kind, &normalized, defaults.detected_name),
        source_host: normalized.host_str().unwrap_or("unknown").to_string(),
        source_type: defaults.source_type.to_string(),
        publisher: defaults.publisher.to_string(),
        target: defaults.target.to_string(),
        trust: SourceTrust::ReviewRequired,
        risk_flags: defaults
            .risk_flags
            .iter()
            .map(|item| item.to_string())
            .collect(),
        notes: defaults.notes.iter().map(|item| item.to_string()).collect(),
    })
}

struct SourceDefaults {
    detected_name: &'static str,
    source_type: &'static str,
    publisher: &'static str,
    target: &'static str,
    risk_flags: &'static [&'static str],
    notes: &'static [&'static str],
}

fn defaults_for(kind: &SourceKind) -> SourceDefaults {
    match kind {
        SourceKind::Tool => SourceDefaults {
            detected_name: "Developer tool source",
            source_type: "Package or release source",
            publisher: "Publisher review required",
            target: "Canonical tool and owner mapping",
            risk_flags: &["Network metadata", "Ownership must be verified"],
            notes: &[
                "No executable or install arguments are accepted from this URL.",
                "Managed execution remains unavailable until a reviewed mapping matches.",
            ],
        },
        SourceKind::Skill => SourceDefaults {
            detected_name: "Agent Skill source",
            source_type: "Git repository and optional skill path",
            publisher: "Catalog match required",
            target: "Approved global client roots",
            risk_flags: &["Active instruction content", "Scripts and symlinks require review"],
            notes: &[
                "Project-local targets are never offered.",
                "Materialization remains blocked until immutable provenance matches a trusted catalog entry.",
            ],
        },
        SourceKind::Mcp => SourceDefaults {
            detected_name: "Remote MCP server",
            source_type: "Streamable HTTP endpoint",
            publisher: "Server owner review required",
            target: "Selected global MCP client configurations",
            risk_flags: &["Remote tool capabilities", "Authentication reference may be required"],
            notes: &[
                "Credential values are never stored in the fixture.",
                "Client configuration is not changed by this review.",
            ],
        },
    }
}

fn sanitize_source_url(url: &Url) -> Result<String, CoreError> {
    let mut sanitized = url.clone();
    sanitized.set_username("").map_err(|_| {
        CoreError::MalformedInput("source URL username could not be redacted".to_string())
    })?;
    sanitized.set_password(None).map_err(|_| {
        CoreError::MalformedInput("source URL password could not be redacted".to_string())
    })?;
    sanitized.set_query(None);
    sanitized.set_fragment(None);
    Ok(sanitized.to_string())
}

fn detect_name(kind: &SourceKind, url: &Url, fallback: &str) -> String {
    let path = url.path().to_ascii_lowercase();
    if matches!(kind, SourceKind::Tool) && path.contains("codex") {
        return "Codex CLI".to_string();
    }
    if matches!(kind, SourceKind::Skill) && path.contains("frontend-design") {
        return "Frontend Design skill".to_string();
    }
    if matches!(kind, SourceKind::Mcp)
        && url
            .host_str()
            .map(|host| host.contains("sentry"))
            .unwrap_or(false)
    {
        return "Sentry MCP".to_string();
    }
    fallback.to_string()
}

fn blocked_analysis(
    kind: SourceKind,
    submitted_url: String,
    source_host: String,
    note: &str,
) -> SourceAnalysisRecord {
    let defaults = defaults_for(&kind);
    SourceAnalysisRecord {
        kind,
        submitted_url,
        normalized_url: None,
        status: SourceAnalysisStatus::Blocked,
        detected_name: defaults.detected_name.to_string(),
        source_host,
        source_type: defaults.source_type.to_string(),
        publisher: defaults.publisher.to_string(),
        target: defaults.target.to_string(),
        trust: SourceTrust::Blocked,
        risk_flags: vec!["Source validation failed".to_string()],
        notes: vec![
            note.to_string(),
            "No analysis or installation preview was created.".to_string(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_analysis_rejects_credentials_and_redacts_query_data() {
        let blocked = analyze_source(
            SourceKind::Mcp,
            "https://user:secret@example.com/mcp?token=abc#frag",
        )
        .expect("analysis");
        assert_eq!(blocked.status, SourceAnalysisStatus::Blocked);
        assert_eq!(blocked.submitted_url, "https://example.com/mcp");
        assert!(!blocked.notes.join(" ").contains("secret"));
    }

    #[test]
    fn source_analysis_matches_locked_skill_fixture_behavior() {
        let accepted = analyze_source(
            SourceKind::Skill,
            "https://github.com/agentkit/skills/tree/main/frontend-design#fragment",
        )
        .expect("analysis");
        assert_eq!(accepted.status, SourceAnalysisStatus::ReviewReady);
        assert_eq!(accepted.detected_name, "Frontend Design skill");
        assert_eq!(accepted.trust, SourceTrust::ReviewRequired);
    }
}
