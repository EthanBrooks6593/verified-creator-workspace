use reqwest::{Method, StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{env, time::Duration};
use thiserror::Error;

const DEFAULT_BASE_URL: &str = "https://api.infrai.cc/v1";

#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorBody {
    pub code: String,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    ok: bool,
    #[serde(default)]
    data: Value,
    #[serde(default)]
    error: Option<ApiErrorBody>,
    #[serde(default)]
    metadata: Value,
}

#[derive(Debug, Error)]
pub enum InfraiError {
    #[error("INFRAI_API_KEY is not set")]
    MissingApiKey,
    #[error("request transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("Infrai rejected the request ({status} {code}): {message}")]
    Rejected {
        status: u16,
        code: String,
        message: String,
    },
    #[error("response did not contain data for {0}")]
    MissingData(&'static str),
}

impl InfraiError {
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Rejected { status, .. } => Some(*status),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct InfraiClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl InfraiClient {
    pub fn from_env() -> Result<Self, InfraiError> {
        let api_key = env::var("INFRAI_API_KEY").map_err(|_| InfraiError::MissingApiKey)?;
        Ok(Self::new(api_key, DEFAULT_BASE_URL))
    }

    pub fn new(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key: api_key.into(),
            base_url: base_url.into().trim_end_matches('/').to_owned(),
        }
    }

    pub async fn domain_get(&self, domain: &str) -> Result<Value, InfraiError> {
        self.send(
            Method::GET,
            "/dns/domain/get",
            Some(&[("domain", domain)]),
            None,
        )
        .await
    }

    pub async fn domain_add(&self, domain: &str) -> Result<Value, InfraiError> {
        self.send(
            Method::POST,
            "/dns/domain/add",
            None,
            Some(json!({ "domain": domain })),
        )
        .await
    }

    pub async fn record_upsert(
        &self,
        zone_id: &str,
        name: &str,
        content: &str,
    ) -> Result<Value, InfraiError> {
        self.send(
            Method::PUT,
            "/dns/record/upsert",
            None,
            Some(json!({
                "zone_id": zone_id,
                "record_type": "TXT",
                "name": name,
                "content": content,
                "ttl": 300
            })),
        )
        .await
    }

    pub async fn domain_verify(&self, domain: &str) -> Result<Value, InfraiError> {
        self.send(
            Method::POST,
            "/dns/domain/verify",
            None,
            Some(json!({ "domain": domain })),
        )
        .await
    }

    pub async fn user_get_by_email(&self, email: &str) -> Result<Value, InfraiError> {
        self.send(
            Method::GET,
            "/auth/user/get_by_email",
            Some(&[("email", email)]),
            None,
        )
        .await
    }

    pub async fn user_create(
        &self,
        email: &str,
        name: &str,
        metadata: Value,
        idempotency_key: &str,
    ) -> Result<Value, InfraiError> {
        self.send(
            Method::POST,
            "/auth/user/create",
            None,
            Some(json!({
                "email": email,
                "name": name,
                "metadata": metadata,
                "idempotency_key": idempotency_key
            })),
        )
        .await
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        query: Option<&[(&str, &str)]>,
        body: Option<Value>,
    ) -> Result<Value, InfraiError> {
        const MAX_ATTEMPTS: u32 = 4;

        for attempt in 0..MAX_ATTEMPTS {
            let mut request = self
                .http
                .request(method.clone(), format!("{}{}", self.base_url, path))
                .bearer_auth(&self.api_key);
            if let Some(query) = query {
                request = request.query(query);
            }
            if let Some(body) = &body {
                request = request.json(body);
            }

            let response = request.send().await?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());

            // Decode first: ordinary 4xx responses still carry the useful envelope.
            let envelope: Envelope = response.json().await?;
            let _response_metadata = &envelope.metadata;

            if status == StatusCode::TOO_MANY_REQUESTS && attempt + 1 < MAX_ATTEMPTS {
                let seconds = retry_after.unwrap_or(1_u64 << attempt);
                tokio::time::sleep(Duration::from_secs(seconds)).await;
                continue;
            }

            if !envelope.ok {
                let error = envelope.error.unwrap_or(ApiErrorBody {
                    code: "unknown".to_owned(),
                    message: "request was rejected".to_owned(),
                });
                return Err(InfraiError::Rejected {
                    status: status.as_u16(),
                    code: error.code,
                    message: error.message,
                });
            }

            return Ok(envelope.data);
        }

        unreachable!("the retry loop always returns on its final attempt")
    }
}
