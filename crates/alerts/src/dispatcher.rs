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
