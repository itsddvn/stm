use std::time::SystemTime;

use crate::domain::source::SourceAnalysisRecord;

#[derive(Debug, Clone)]
pub struct SourceAnalysisBinding {
    pub record: SourceAnalysisRecord,
    pub resource_id: Option<String>,
    pub expires_at: SystemTime,
}
