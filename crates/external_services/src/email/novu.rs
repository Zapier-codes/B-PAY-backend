//! Novu-backed [`EmailClient`](crate::email::EmailClient) implementation.
//!
//! Novu is a notification-routing service, not a raw SMTP/SES transport: mail
//! is sent by triggering a *workflow* that the operator has defined in Novu
//! (with the actual provider — Resend/Postmark/SES/etc. — configured behind
//! it). This client therefore does not talk to an SMTP relay at all; it calls
//! Novu's trigger endpoint and hands it the recipient, subject and rendered
//! HTML body as trigger payload.
//!
//! Because the intermediate representation this crate produces is HTML
//! ([`IntermediateString`]), the body is passed through unchanged, same as the
//! SMTP client does. Novu exposes it to the workflow as `payload.body`; the
//! workflow's email step is expected to reference `{{payload.body}}` (and
//! `{{payload.subject}}` for the subject) rather than hard-coding content, so
//! that every transactional template this codebase already emits renders
//! correctly without per-template Novu configuration.

use std::time::Duration;

use common_utils::{errors::CustomResult, pii};
use error_stack::ResultExt;
use hyperswitch_masking::{PeekInterface, Secret};
use serde::Serialize;

use crate::email::{EmailClient, EmailError, EmailResult, EmailSettings, IntermediateString};

/// Client for sending email through a Novu workflow trigger.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct NovuClient {
    /// Sender email id, surfaced to the Novu workflow as the trigger `from`.
    pub sender: pii::Email,
    /// Novu-specific configuration.
    pub novu_config: NovuConfig,
}

impl Default for NovuClient {
    fn default() -> Self {
        Self {
            sender: pii::Email::default(),
            novu_config: NovuConfig::default(),
        }
    }
}

/// Configuration for the Novu email client.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct NovuConfig {
    /// Novu API key (`Authorization: ApiKey <key>`). Found under
    /// Settings → API Keys in the Novu dashboard.
    pub api_key: Secret<String>,
    /// Base URL of the Novu API. Defaults to Novu's hosted API when unset so
    /// the common case needs no configuration; point this at a self-hosted
    /// Novu deployment to avoid the hosted service entirely.
    #[serde(default = "NovuConfig::default_base_url")]
    pub base_url: String,
    /// Identifier of the Novu workflow to trigger for every outbound email.
    pub workflow_id: String,
    /// Request timeout in seconds.
    #[serde(default = "NovuConfig::default_timeout")]
    pub timeout: u64,
}

impl Default for NovuConfig {
    fn default() -> Self {
        Self {
            api_key: Secret::new(String::new()),
            base_url: Self::default_base_url(),
            workflow_id: String::new(),
            timeout: Self::default_timeout(),
        }
    }
}

impl NovuConfig {
    fn default_base_url() -> String {
        "https://api.novu.co".to_string()
    }

    fn default_timeout() -> u64 {
        30
    }

    /// Validation for the Novu client specific configs.
    pub fn validate(&self) -> Result<(), &'static str> {
        use common_utils::{ext_traits::ConfigExt, fp_utils::when};

        when(self.api_key.peek().is_default_or_empty(), || {
            Err("email.novu.api_key must not be empty")
        })?;
        when(self.workflow_id.is_default_or_empty(), || {
            Err("email.novu.workflow_id must not be empty")
        })?;
        when(self.base_url.is_default_or_empty(), || {
            Err("email.novu.base_url must not be empty")
        })
    }
}

/// The trigger payload Novu's `/v1/events/trigger` endpoint accepts.
#[derive(Debug, Serialize)]
struct NovuTriggerRequest {
    name: String,
    to: NovuSubscriber,
    payload: NovuPayload,
}

#[derive(Debug, Serialize)]
struct NovuSubscriber {
    subscriber_id: String,
    email: String,
}

#[derive(Debug, Serialize)]
struct NovuPayload {
    subject: String,
    body: String,
    sender: String,
}

/// Errors that could occur during Novu operations.
#[derive(Debug, thiserror::Error)]
pub enum NovuError {
    /// Failed to build the HTTP client used to reach Novu.
    #[error("Failed to build Novu HTTP client {0:?}")]
    ClientBuildingFailure(reqwest::Error),
    /// The request to Novu failed at the transport level.
    #[error("Failed to send request to Novu {0:?}")]
    RequestFailure(reqwest::Error),
    /// Novu accepted the connection but rejected the trigger.
    #[error("Novu rejected the email trigger with status {status}: {body}")]
    TriggerRejected {
        /// HTTP status returned by Novu.
        status: u16,
        /// Response body returned by Novu, for diagnosis.
        body: String,
    },
}

impl NovuClient {
    /// Constructs a new Novu client.
    pub async fn create(conf: &EmailSettings, novu_config: NovuConfig) -> Self {
        Self {
            sender: conf.sender_email.clone(),
            novu_config,
        }
    }

    fn build_client(&self) -> EmailResult<reqwest::Client> {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(self.novu_config.timeout))
            .build()
            .map_err(NovuError::ClientBuildingFailure)
            .change_context(EmailError::ClientBuildingFailure)
    }
}

#[async_trait::async_trait]
impl EmailClient for NovuClient {
    type RichText = String;

    fn convert_to_rich_text(
        &self,
        intermediate_string: IntermediateString,
    ) -> CustomResult<Self::RichText, EmailError> {
        Ok(intermediate_string.into_inner())
    }

    async fn send_email(
        &self,
        recipient: pii::Email,
        subject: String,
        body: Self::RichText,
        _proxy_url: Option<&String>,
    ) -> EmailResult<()> {
        let email = recipient.peek().to_string();
        let request_body = NovuTriggerRequest {
            name: self.novu_config.workflow_id.clone(),
            to: NovuSubscriber {
                subscriber_id: email.clone(),
                email,
            },
            payload: NovuPayload {
                subject,
                body,
                sender: self.sender.peek().to_string(),
            },
        };

        let url = format!(
            "{}/v1/events/trigger",
            self.novu_config.base_url.trim_end_matches('/')
        );

        let response = self
            .build_client()?
            .post(url)
            .header(
                "Authorization",
                format!("ApiKey {}", self.novu_config.api_key.peek()),
            )
            .json(&request_body)
            .send()
            .await
            .map_err(NovuError::RequestFailure)
            .change_context(EmailError::EmailSendingFailure)?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable response body>".to_string());
            return Err(NovuError::TriggerRejected {
                status: status.as_u16(),
                body,
            })
            .change_context(EmailError::EmailSendingFailure);
        }

        Ok(())
    }
}
