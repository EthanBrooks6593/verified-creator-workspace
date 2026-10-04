use crate::infrai_client::{InfraiClient, InfraiError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

const WORKSPACE_CAPABILITIES: [&str; 3] = [
    "digital_asset_delivery",
    "subscriber_updates",
    "content_processing",
];

#[derive(Debug, Clone)]
pub struct JoinRequest {
    pub company_domain: String,
    pub employee_email: String,
    pub employee_name: String,
    pub txt_name: String,
    pub txt_content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinDecision {
    pub employee_email: String,
    pub company_domain: String,
    pub workspace: String,
    pub access: Vec<String>,
}

#[derive(Debug, Error)]
pub enum JoinError {
    #[error("employee email does not belong to {expected_domain}")]
    DomainMismatch { expected_domain: String },
    #[error("Infrai request failed: {0}")]
    Infrai(#[from] InfraiError),
    #[error("domain response has no zone_id")]
    MissingZoneId,
}

pub fn decide_join(company_domain: &str, employee_email: &str) -> Result<JoinDecision, JoinError> {
    let actual_domain = employee_email
        .rsplit_once('@')
        .map(|(_, domain)| domain)
        .unwrap_or_default();

    if !actual_domain.eq_ignore_ascii_case(company_domain) {
        return Err(JoinError::DomainMismatch {
            expected_domain: company_domain.to_owned(),
        });
    }

    Ok(JoinDecision {
        employee_email: employee_email.to_owned(),
        company_domain: company_domain.to_owned(),
        workspace: format!("creator-{company_domain}"),
        access: WORKSPACE_CAPABILITIES
            .iter()
            .map(|capability| (*capability).to_owned())
            .collect(),
    })
}

pub async fn verify_and_join(
    infrai: &InfraiClient,
    request: JoinRequest,
) -> Result<JoinDecision, JoinError> {
    let decision = decide_join(&request.company_domain, &request.employee_email)?;

    let domain = match infrai.domain_get(&request.company_domain).await {
        Ok(domain) => domain,
        Err(error) if error.status() == Some(404) => {
            infrai.domain_add(&request.company_domain).await?
        }
        Err(error) => return Err(error.into()),
    };
    let zone_id = domain
        .get("zone_id")
        .and_then(Value::as_str)
        .ok_or(JoinError::MissingZoneId)?;

    infrai
        .record_upsert(zone_id, &request.txt_name, &request.txt_content)
        .await?;
    infrai.domain_verify(&request.company_domain).await?;

    let directory_entry = infrai.user_get_by_email(&request.employee_email).await;
    if matches!(&directory_entry, Err(error) if error.status() == Some(404)) {
        let idempotency_key = format!(
            "workspace:{}:{}",
            decision.workspace, request.employee_email
        );
        infrai
            .user_create(
                &request.employee_email,
                &request.employee_name,
                workspace_metadata(&decision),
                &idempotency_key,
            )
            .await?;
    } else {
        directory_entry?;
    }

    Ok(decision)
}

fn workspace_metadata(decision: &JoinDecision) -> Value {
    json!({
        "company_domain": decision.company_domain,
        "workspace": decision.workspace,
        "access": decision.access
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn company_employee_gets_creator_workspace_access() {
        let decision = decide_join("studio.example", "maya@studio.example").unwrap();

        assert_eq!(decision.workspace, "creator-studio.example");
        assert_eq!(
            decision.access,
            vec![
                "digital_asset_delivery",
                "subscriber_updates",
                "content_processing"
            ]
        );
    }

    #[test]
    fn outside_email_is_not_joined() {
        let error = decide_join("studio.example", "maya@personal.example").unwrap_err();
        assert!(matches!(error, JoinError::DomainMismatch { .. }));
    }
}
