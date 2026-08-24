use crate::{
    domain::source::{SourceAnalysisRecord, SourceAnalysisStatus, SourceTrust},
    error::CoreError,
    feasibility::source_analysis::analyze_source,
};

const MAX_RESPONSE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceProbeEvidence {
    pub final_url: String,
    pub status: u16,
    pub content_length: Option<u64>,
    pub sampled_bytes: u64,
}

pub trait SourceProbe: Send + Sync {
    fn probe(&self, url: &str) -> Result<SourceProbeEvidence, CoreError>;
}

pub fn analyze_source_with_probe(
    kind: crate::domain::source::SourceKind,
    submitted_url: &str,
    probe: &dyn SourceProbe,
) -> Result<SourceAnalysisRecord, CoreError> {
    let mut analysis = analyze_source(kind, submitted_url)?;
    if analysis.status != SourceAnalysisStatus::ReviewReady {
        return Ok(analysis);
    }
    let Some(normalized_url) = analysis.normalized_url.as_deref() else {
        return Ok(analysis);
    };
    match probe.probe(normalized_url).and_then(|evidence| {
        validate_probe_evidence(normalized_url, &evidence)?;
        Ok(evidence)
    }) {
        Ok(evidence) => {
            analysis.normalized_url = Some(evidence.final_url.clone());
            analysis.source_host = url::Url::parse(&evidence.final_url)?
                .host_str()
                .unwrap_or("unknown")
                .to_string();
            analysis.notes.push(format!(
                "HTTPS provenance probe returned status {} within the 64 KiB boundary.",
                evidence.status
            ));
        }
        Err(error) => {
            analysis.status = SourceAnalysisStatus::Blocked;
            analysis.trust = SourceTrust::ReviewRequired;
            analysis.risk_flags.push(
                "Live HTTPS provenance could not be established within the bounded review policy."
                    .to_string(),
            );
            analysis.notes.push(format!(
                "HTTPS provenance probe unavailable; managed execution remains blocked: {error}"
            ));
        }
    }
    Ok(analysis)
}

fn validate_probe_evidence(
    requested_url: &str,
    evidence: &SourceProbeEvidence,
) -> Result<(), CoreError> {
    if !(200..300).contains(&evidence.status) {
        return Err(CoreError::ProcessExecution(format!(
            "source probe returned HTTP {}",
            evidence.status
        )));
    }
    if evidence.sampled_bytes > MAX_RESPONSE_BYTES
        || evidence
            .content_length
            .is_some_and(|length| length > MAX_RESPONSE_BYTES)
    {
        return Err(CoreError::ProcessExecution(
            "source response exceeds 64 KiB review boundary".to_string(),
        ));
    }
    let requested = url::Url::parse(requested_url)?;
    let final_url = url::Url::parse(&evidence.final_url)?;
    if final_url.scheme() != "https"
        || final_url.username() != ""
        || final_url.password().is_some()
        || requested.host_str() != final_url.host_str()
        || requested.port_or_known_default() != final_url.port_or_known_default()
    {
        return Err(CoreError::CommandDenied(
            "source probe left the approved HTTPS origin".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::domain::source::{SourceKind, SourceTrust};

    use super::*;

    struct FixtureProbe;

    impl SourceProbe for FixtureProbe {
        fn probe(&self, _: &str) -> Result<SourceProbeEvidence, CoreError> {
            Ok(SourceProbeEvidence {
                final_url: "https://github.com/openai/codex".to_string(),
                status: 200,
                content_length: Some(1024),
                sampled_bytes: 1024,
            })
        }
    }

    struct FailingProbe;

    impl SourceProbe for FailingProbe {
        fn probe(&self, _: &str) -> Result<SourceProbeEvidence, CoreError> {
            Err(CoreError::ProcessExecution(
                "fixture network unavailable".to_string(),
            ))
        }
    }

    struct UnapprovedEvidenceProbe;

    impl SourceProbe for UnapprovedEvidenceProbe {
        fn probe(&self, _: &str) -> Result<SourceProbeEvidence, CoreError> {
            Ok(SourceProbeEvidence {
                final_url: "https://attacker.example/replacement".to_string(),
                status: 404,
                content_length: Some(1024),
                sampled_bytes: 1024,
            })
        }
    }

    #[test]
    fn network_evidence_does_not_promote_catalog_trust() {
        let analysis = analyze_source_with_probe(
            SourceKind::Tool,
            "https://github.com/openai/codex",
            &FixtureProbe,
        )
        .expect("analysis");
        assert_eq!(analysis.trust, SourceTrust::ReviewRequired);
        assert!(analysis
            .notes
            .iter()
            .any(|note| note.contains("status 200")));
    }

    #[test]
    fn failed_network_provenance_blocks_lifecycle_planning() {
        let analysis = analyze_source_with_probe(
            SourceKind::Tool,
            "https://github.com/openai/codex",
            &FailingProbe,
        )
        .expect("analysis");
        assert_eq!(analysis.status, SourceAnalysisStatus::Blocked);
        assert_eq!(analysis.trust, SourceTrust::ReviewRequired);
    }

    #[test]
    fn unsuccessful_or_cross_origin_probe_evidence_stays_blocked() {
        let analysis = analyze_source_with_probe(
            SourceKind::Tool,
            "https://github.com/openai/codex",
            &UnapprovedEvidenceProbe,
        )
        .expect("analysis");
        assert_eq!(analysis.status, SourceAnalysisStatus::Blocked);
        assert_eq!(analysis.trust, SourceTrust::ReviewRequired);
    }
}
