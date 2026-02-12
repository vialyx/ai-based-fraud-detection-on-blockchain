use fraud_common::types::Alert;
use tracing::{error, info};

/// A notification channel that can deliver alerts.
pub trait AlertChannel: Send + Sync {
    /// Deliver the alert. Errors are logged but do not stop the pipeline.
    fn send<'a>(&'a self, alert: &'a Alert) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>>;
    fn name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// Log channel – prints alerts to tracing
// ---------------------------------------------------------------------------

pub struct LogChannel;

impl AlertChannel for LogChannel {
    fn send<'a>(&'a self, alert: &'a Alert) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            info!(
                id = %alert.id,
                tx = %alert.fraud_score.tx_hash,
                score = alert.fraud_score.score,
                risk = %alert.fraud_score.risk_level,
                rules = ?alert.fraud_score.triggered_rules,
                "🚨 ALERT"
            );
        })
    }

    fn name(&self) -> &str {
        "log"
    }
}

// ---------------------------------------------------------------------------
// Webhook channel – POSTs JSON to an external URL
// ---------------------------------------------------------------------------

pub struct WebhookChannel {
    url: String,
    client: reqwest::Client,
}

impl WebhookChannel {
    pub fn new(url: String) -> Self {
        Self {
            url,
            client: reqwest::Client::new(),
        }
    }
}

impl AlertChannel for WebhookChannel {
    fn send<'a>(&'a self, alert: &'a Alert) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            match serde_json::to_string(alert) {
                Ok(body) => {
                    let res = self
                        .client
                        .post(&self.url)
                        .header("content-type", "application/json")
                        .body(body)
                        .send()
                        .await;

                    match res {
                        Ok(resp) => {
                            info!(
                                channel = "webhook",
                                status = %resp.status(),
                                "alert delivered"
                            );
                        }
                        Err(e) => {
                            error!(channel = "webhook", error = %e, "failed to deliver alert");
                        }
                    }
                }
                Err(e) => {
                    error!(channel = "webhook", error = %e, "failed to serialize alert");
                }
            }
        })
    }

    fn name(&self) -> &str {
        "webhook"
    }
}
