use std::time::{Duration, SystemTime};

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::error::CoreError;

pub const PLAN_TTL: Duration = Duration::from_secs(10 * 60);

pub fn now() -> SystemTime {
    SystemTime::now()
}

pub fn format_timestamp(value: SystemTime) -> Result<String, CoreError> {
    let value = OffsetDateTime::from(value);
    value
        .format(&Rfc3339)
        .map_err(|error| CoreError::MalformedInput(format!("timestamp format failed: {error}")))
}

pub fn parse_timestamp(value: &str) -> Result<SystemTime, CoreError> {
    let parsed = OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|_| CoreError::LifecycleConsentDenied("invalid consent timestamp".to_string()))?;
    Ok(SystemTime::from(parsed))
}
