use sha2::{Digest, Sha256};
use stm_core::domain::lifecycle::{
    LifecycleConsentAuthorization, LifecycleExecutionResult, LifecycleExecutionStatus,
    LifecycleItemResult, LifecycleItemStatus, LifecyclePlan, LifecyclePrivilege,
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub(super) fn validate_authorization(
    plan: &LifecyclePlan,
    authorization: &LifecycleConsentAuthorization,
) -> Result<(), String> {
    if authorization.plan_digest != plan.digest
        || authorization.plan_expires_at != plan.expires_at
        || plan.privilege != LifecyclePrivilege::UserConfirmation
    {
        return Err("Product update consent does not match the reviewed plan".into());
    }
    let expires_at = OffsetDateTime::parse(&plan.expires_at, &Rfc3339)
        .map_err(|_| "Product update plan expiry is invalid".to_string())?;
    if expires_at <= OffsetDateTime::now_utc() {
        return Err("Product update plan expired; review a fresh plan".into());
    }
    Ok(())
}

pub(super) fn terminal_result(
    plan: &LifecyclePlan,
    operation_id: &str,
    status: LifecycleExecutionStatus,
    item_status: LifecycleItemStatus,
    detail: &str,
) -> LifecycleExecutionResult {
    let succeeded = status == LifecycleExecutionStatus::Success;
    let receipt = succeeded.then(|| format!("product-update:{}", plan.target_version));
    LifecycleExecutionResult {
        operation_id: operation_id.into(),
        plan_digest: plan.digest.clone(),
        status,
        completed_steps: usize::from(succeeded),
        total_steps: 1,
        can_cancel: false,
        receipt: receipt.clone(),
        redacted_detail: detail.into(),
        items: vec![LifecycleItemResult {
            id: "stm-product".into(),
            label: format!("STM {}", plan.target_version),
            status: item_status,
            receipt,
            redacted_detail: detail.into(),
        }],
        retry_actions: Vec::new(),
        recovery_actions: Vec::new(),
    }
}

pub(super) fn plan_digest(plan: &LifecyclePlan) -> Result<String, String> {
    let mut value = plan.clone();
    value.digest.clear();
    serde_json::to_vec(&value)
        .map(|bytes| format!("sha256:{:x}", Sha256::digest(bytes)))
        .map_err(|_| "Product update plan serialization failed".to_string())
}

pub(super) fn opaque_id(prefix: &str, sequence: u64, value: &str, now: OffsetDateTime) -> String {
    let digest = Sha256::digest(format!(
        "{prefix}:{sequence}:{value}:{}",
        now.unix_timestamp_nanos()
    ));
    format!("{prefix}-{digest:x}")[..prefix.len() + 1 + 20].to_string()
}

pub(super) fn format_timestamp(value: OffsetDateTime) -> Result<String, String> {
    value
        .format(&Rfc3339)
        .map_err(|_| "Product update timestamp formatting failed".to_string())
}

#[cfg(test)]
mod tests {
    use stm_core::domain::lifecycle::{
        LifecycleExecution, LifecyclePlanRequest, LifecycleResourceKind, LifecycleRevalidation,
        LifecycleRevalidationState,
    };

    use super::*;

    fn sample_plan(expires_at: OffsetDateTime) -> LifecyclePlan {
        let mut plan = LifecyclePlan {
            request: LifecyclePlanRequest {
                resource_kind: LifecycleResourceKind::Product,
                action: "product-update".into(),
                resource_id: "stm".into(),
                source_analysis_handle: None,
                item_ids: None,
                children: Vec::new(),
                mapping_id: None,
            },
            plan_id: "product-plan-fixture".into(),
            canonical_id: "stm-product".into(),
            mapping_id: "signed-product-updater:stable".into(),
            resource_id: "stm".into(),
            owner: "Signed STM product channel".into(),
            source: "https://github.com".into(),
            current_version: "0.1.0".into(),
            target_version: "0.2.0".into(),
            privilege: LifecyclePrivilege::UserConfirmation,
            affected_paths: vec!["application:com.itsddvn.stm".into()],
            affected_records: vec!["product-release:stable".into()],
            confidence: "fixture".into(),
            limitations: Vec::new(),
            digest: String::new(),
            expires_at: format_timestamp(expires_at).expect("expiry"),
            revalidation: LifecycleRevalidation {
                state: LifecycleRevalidationState::Fresh,
                checked_at: format_timestamp(OffsetDateTime::now_utc()).expect("checked"),
                checks: vec!["signature".into()],
            },
            execution: LifecycleExecution::SignedProductUpdate {
                executable: "tauri-signed-updater".into(),
                argv: vec!["0.2.0".into()],
            },
        };
        plan.digest = plan_digest(&plan).expect("digest");
        plan
    }

    #[test]
    fn consent_is_exact_and_expiry_bound() {
        let plan = sample_plan(OffsetDateTime::now_utc() + time::Duration::minutes(5));
        let authorization = LifecycleConsentAuthorization {
            plan_digest: plan.digest.clone(),
            plan_expires_at: plan.expires_at.clone(),
            granted_at: plan.revalidation.checked_at.clone(),
        };
        assert!(validate_authorization(&plan, &authorization).is_ok());
        let mut changed = authorization;
        changed.plan_digest.push('0');
        assert!(validate_authorization(&plan, &changed).is_err());
        let expired = sample_plan(OffsetDateTime::now_utc() - time::Duration::minutes(1));
        assert!(validate_authorization(
            &expired,
            &LifecycleConsentAuthorization {
                plan_digest: expired.digest.clone(),
                plan_expires_at: expired.expires_at.clone(),
                granted_at: expired.revalidation.checked_at.clone(),
            },
        )
        .is_err());
    }

    #[test]
    fn successful_product_result_uses_a_separate_receipt() {
        let plan = sample_plan(OffsetDateTime::now_utc() + time::Duration::minutes(5));
        let result = terminal_result(
            &plan,
            "product-operation-fixture",
            LifecycleExecutionStatus::Success,
            LifecycleItemStatus::Success,
            "installed",
        );
        assert_eq!(result.receipt.as_deref(), Some("product-update:0.2.0"));
        assert_eq!(result.completed_steps, 1);
        assert!(result.retry_actions.is_empty());
        assert!(result.recovery_actions.is_empty());
    }
}
