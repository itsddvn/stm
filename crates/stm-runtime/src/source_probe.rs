use std::{io::Read, time::Duration};

use reqwest::{
    blocking::Client,
    header::{CONTENT_LENGTH, RANGE},
    redirect::Policy,
};
use stm_core::{
    lifecycle::{SourceProbe, SourceProbeEvidence},
    CoreError,
};

const MAX_REDIRECTS: usize = 3;
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

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
