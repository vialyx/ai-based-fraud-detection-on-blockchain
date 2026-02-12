use axum::routing::get;
use axum::Router;
use fraud_common::config::ApiConfig;
use fraud_common::types::{Alert, FraudScore};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::handlers;
use crate::ws;

/// Maximum number of recent scores / alerts kept in memory for the REST API.
const MAX_RECENT: usize = 500;

/// Shared application state accessible from all handlers.
pub struct AppState {
    pub recent_scores: Mutex<VecDeque<FraudScore>>,
    pub recent_alerts: Mutex<VecDeque<Alert>>,
    pub stats: Mutex<PipelineStats>,
    pub alert_broadcast: broadcast::Sender<String>,
}

#[derive(Default)]
pub struct PipelineStats {
    pub blocks_processed: u64,
    pub transactions_scored: u64,
    pub alerts_raised: u64,
}

impl AppState {
    pub fn new(alert_broadcast: broadcast::Sender<String>) -> Self {
        Self {
            recent_scores: Mutex::new(VecDeque::with_capacity(MAX_RECENT)),
            recent_alerts: Mutex::new(VecDeque::with_capacity(MAX_RECENT)),
            stats: Mutex::new(PipelineStats::default()),
            alert_broadcast,
        }
    }

    /// Push a new score into the ring buffer.
    pub fn push_score(&self, score: FraudScore) {
        let mut buf = self.recent_scores.lock().unwrap();
        if buf.len() == MAX_RECENT {
            buf.pop_front();
        }
        buf.push_back(score);
        self.stats.lock().unwrap().transactions_scored += 1;
    }

    /// Push a new alert into the ring buffer.
    pub fn push_alert(&self, alert: Alert) {
        let mut buf = self.recent_alerts.lock().unwrap();
        if buf.len() == MAX_RECENT {
            buf.pop_front();
        }
        buf.push_back(alert);
        self.stats.lock().unwrap().alerts_raised += 1;
    }

    /// Increment blocks-processed counter.
    pub fn inc_blocks(&self) {
        self.stats.lock().unwrap().blocks_processed += 1;
    }
}

/// Build the Axum router with all API routes.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/api/scores", get(handlers::get_scores))
        .route("/api/alerts", get(handlers::get_alerts))
        .route("/api/stats", get(handlers::get_stats))
        .route("/ws", get(ws::ws_handler))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Start the HTTP server (runs forever).
pub async fn serve(config: &ApiConfig, state: Arc<AppState>) -> Result<(), fraud_common::error::Error> {
    let app = build_router(state);
    let addr = format!("{}:{}", config.bind_address, config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| fraud_common::error::Error::Api(format!("bind failed: {e}")))?;

    info!(address = %addr, "API server listening");

    axum::serve(listener, app)
        .await
        .map_err(|e| fraud_common::error::Error::Api(format!("server error: {e}")))?;

    Ok(())
}
