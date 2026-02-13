use fraud_common::config::AlertsConfig;
use fraud_common::error::Error;
use fraud_common::types::PipelineEvent;
use tokio::sync::{broadcast, mpsc};
use tracing::{info, warn};

use crate::channels::{AlertChannel, LogChannel, WebhookChannel};

/// Receives scored / alert events and fans them out to all configured
/// notification channels.  Also re-broadcasts alerts over a `broadcast`
/// channel so the API layer can push them to WebSocket clients.
pub struct AlertDispatcher {
    channels: Vec<Box<dyn AlertChannel>>,
    /// Broadcast sender for the API WebSocket feed.
    ws_tx: broadcast::Sender<String>,
}

impl AlertDispatcher {
    pub fn new(config: &AlertsConfig) -> (Self, broadcast::Receiver<String>) {
        let (ws_tx, ws_rx) = broadcast::channel(256);

        let mut channels: Vec<Box<dyn AlertChannel>> = Vec::new();

        if config.log_enabled {
            channels.push(Box::new(LogChannel));
        }
        if let Some(ref url) = config.webhook_url {
            channels.push(Box::new(WebhookChannel::new(url.clone())));
        }

        (Self { channels, ws_tx }, ws_rx)
    }

    /// Broadcast sender handle – clone this for additional subscribers
    /// (e.g., WebSocket connections).
    pub fn broadcast_sender(&self) -> broadcast::Sender<String> {
        self.ws_tx.clone()
    }

    /// Pipeline stage: reads `AlertRaised` events and dispatches them.
    pub async fn run(self, mut rx: mpsc::Receiver<PipelineEvent>) -> Result<(), Error> {
        info!(
            channels = self.channels.len(),
            "alert dispatcher started"
        );

        while let Some(event) = rx.recv().await {
            if let PipelineEvent::AlertRaised(alert) = event {
                // Deliver to each notification channel
                for ch in &self.channels {
                    ch.send(&alert).await;
                }

                // Broadcast JSON for WebSocket subscribers
                if let Ok(json) = serde_json::to_string(&alert) {
                    // Ignore send errors (no active subscribers is fine)
                    let _ = self.ws_tx.send(json);
                }
            }
        }

        warn!("alert dispatcher stopped (upstream closed)");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fraud_common::config::AlertsConfig;
    use fraud_common::types::{Alert, FraudScore, RiskLevel};
    use chrono::Utc;
    use uuid::Uuid;

    fn make_alert() -> Alert {
        Alert {
            id: Uuid::new_v4(),
            fraud_score: FraudScore {
                id: Uuid::new_v4(),
                tx_hash: "0xalert".into(),
                block_number: 1,
                score: 0.9,
                risk_level: RiskLevel::Critical,
                model_scores: vec![],
                triggered_rules: vec!["test".into()],
                scored_at: Utc::now(),
            },
            from: "0xfrom".into(),
            to: Some("0xto".into()),
            value: 1000,
            created_at: Utc::now(),
            acknowledged: false,
        }
    }

    #[test]
    fn creates_log_channel_when_enabled() {
        let config = AlertsConfig {
            log_enabled: true,
            webhook_url: None,
        };
        let (dispatcher, _rx) = AlertDispatcher::new(&config);
        assert_eq!(dispatcher.channels.len(), 1);
        assert_eq!(dispatcher.channels[0].name(), "log");
    }

    #[test]
    fn creates_both_channels() {
        let config = AlertsConfig {
            log_enabled: true,
            webhook_url: Some("http://example.com/hook".into()),
        };
        let (dispatcher, _rx) = AlertDispatcher::new(&config);
        assert_eq!(dispatcher.channels.len(), 2);
    }

    #[test]
    fn creates_no_channels_when_disabled() {
        let config = AlertsConfig {
            log_enabled: false,
            webhook_url: None,
        };
        let (dispatcher, _rx) = AlertDispatcher::new(&config);
        assert_eq!(dispatcher.channels.len(), 0);
    }

    #[test]
    fn broadcast_sender_clone_works() {
        let config = AlertsConfig { log_enabled: true, webhook_url: None };
        let (dispatcher, _rx) = AlertDispatcher::new(&config);
        let _sender = dispatcher.broadcast_sender();
        // Just verify it doesn't panic
    }

    #[tokio::test]
    async fn dispatches_alert_and_broadcasts() {
        let config = AlertsConfig { log_enabled: true, webhook_url: None };
        let (dispatcher, _rx) = AlertDispatcher::new(&config);

        let mut ws_rx = dispatcher.broadcast_sender().subscribe();

        let (tx, rx) = mpsc::channel(16);

        tokio::spawn(async move {
            dispatcher.run(rx).await.ok();
        });

        let alert = make_alert();
        tx.send(PipelineEvent::AlertRaised(alert)).await.unwrap();
        drop(tx); // close upstream

        // Should receive the broadcast JSON
        let json = ws_rx.recv().await.unwrap();
        assert!(json.contains("0xalert"));
    }

    #[tokio::test]
    async fn ignores_non_alert_events() {
        let config = AlertsConfig { log_enabled: false, webhook_url: None };
        let (dispatcher, _rx) = AlertDispatcher::new(&config);

        let (tx, rx) = mpsc::channel(16);

        tokio::spawn(async move {
            dispatcher.run(rx).await.ok();
        });

        // Send a Scored event (not AlertRaised) – should be ignored
        let score = FraudScore {
            id: Uuid::new_v4(),
            tx_hash: "0xignore".into(),
            block_number: 1,
            score: 0.3,
            risk_level: RiskLevel::Low,
            model_scores: vec![],
            triggered_rules: vec![],
            scored_at: Utc::now(),
        };
        tx.send(PipelineEvent::Scored(score)).await.unwrap();
        drop(tx);

        // Give dispatcher time to process
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        // No assertion needed – we just verify it doesn't panic
    }
}
