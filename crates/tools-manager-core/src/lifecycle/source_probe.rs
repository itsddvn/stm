use std::{io::Read, time::Duration};

use reqwest::{
    blocking::Client,
    header::{CONTENT_LENGTH, RANGE},
    redirect::Policy,
};

use crate::{
    domain::source::{SourceAnalysisRecord, SourceAnalysisStatus, SourceTrust},
    error::CoreError,
    feasibility::source_analysis::analyze_source,
};

const MAX_REDIRECTS: usize = 3;
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

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

#[derive(Debug, Default)]
pub struct BoundedHttpsSourceProbe;

impl SourceProbe for BoundedHttpsSourceProbe {
    fn probe(&self, url: &str) -> Result<SourceProbeEvidence, CoreError> {
        let origin = url::Url::parse(url)?;
        let policy = Policy::custom(move |attempt| {
            let target = attempt.url();
            if target.scheme() != "https"
                || target.host_str() != origin.host_str()
                || target.port_or_known_default() != origin.port_or_known_default()
                || !target.username().is_empty()
                || target.password().is_some()
            {
                attempt.stop()
            } else if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.error("source redirect limit exceeded")
            } else {
                attempt.follow()
            }
        });
        let client = Client::builder()
            .connect_timeout(REQUEST_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .redirect(policy)
            .user_agent("stm-source-review/0.1")
            .build()
            .map_err(|error| {
                CoreError::ProcessExecution(format!("source probe setup failed: {error}"))
            })?;
        let mut response = client
            .get(url)
            .header(RANGE, format!("bytes=0-{}", MAX_RESPONSE_BYTES - 1))
            .send()
            .map_err(|error| {
                CoreError::ProcessExecution(format!("source probe failed: {error}"))
            })?;
        if response.url().scheme() != "https" {
            return Err(CoreError::CommandDenied(
                "source redirect left HTTPS".to_string(),
            ));
        }
        if !response.status().is_success() {
            return Err(CoreError::ProcessExecution(format!(
                "source probe returned HTTP {}",
                response.status().as_u16()
            )));
        }
        let content_length = response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        if content_length.is_some_and(|length| length > MAX_RESPONSE_BYTES) {
            return Err(CoreError::ProcessExecution(
                "source response exceeds 64 KiB review boundary".to_string(),
            ));
        }
        let final_url = sanitized_url(response.url())?;
        let status = response.status().as_u16();
        let sampled_bytes = std::io::copy(
            &mut response.by_ref().take(MAX_RESPONSE_BYTES + 1),
            &mut std::io::sink(),
        )?;
        if sampled_bytes > MAX_RESPONSE_BYTES {
            return Err(CoreError::ProcessExecution(
                "source response exceeds 64 KiB review boundary".to_string(),
            ));
        }
        Ok(SourceProbeEvidence {
            final_url,
            status,
            content_length,
            sampled_bytes,
        })
    }
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

fn sanitized_url(url: &url::Url) -> Result<String, CoreError> {
    let mut sanitized = url.clone();
    sanitized
        .set_username("")
        .map_err(|_| CoreError::CommandDenied("redirect credentials rejected".to_string()))?;
    sanitized
        .set_password(None)
        .map_err(|_| CoreError::CommandDenied("redirect credentials rejected".to_string()))?;
    sanitized.set_query(None);
    sanitized.set_fragment(None);
    Ok(sanitized.to_string())
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
