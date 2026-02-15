use axum::extract::DefaultBodyLimit;
use axum::routing::get;
use axum::Router;
use fraud_common::config::ApiConfig;
use fraud_common::types::{Alert, FraudScore};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tower_http::cors::{CorsLayer, AllowOrigin};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use tracing::info;
use http::header::HeaderValue;

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
        let mut buf = self.recent_scores.lock().unwrap_or_else(|e| e.into_inner());
        if buf.len() == MAX_RECENT {
            buf.pop_front();
        }
        buf.push_back(score);
        self.stats.lock().unwrap_or_else(|e| e.into_inner()).transactions_scored += 1;
    }

    /// Push a new alert into the ring buffer.
    pub fn push_alert(&self, alert: Alert) {
        let mut buf = self.recent_alerts.lock().unwrap_or_else(|e| e.into_inner());
        if buf.len() == MAX_RECENT {
            buf.pop_front();
        }
        buf.push_back(alert);
        self.stats.lock().unwrap_or_else(|e| e.into_inner()).alerts_raised += 1;
    }

    /// Increment blocks-processed counter.
    pub fn inc_blocks(&self) {
        self.stats.lock().unwrap_or_else(|e| e.into_inner()).blocks_processed += 1;
    }
}

/// Build the Axum router with all API routes.
pub fn build_router(state: Arc<AppState>) -> Router {
    build_router_with_dashboard(state, "dashboard")
}

/// Build the Axum router with a custom dashboard directory.
pub fn build_router_with_dashboard(state: Arc<AppState>, dashboard_dir: &str) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/api/scores", get(handlers::get_scores))
        .route("/api/alerts", get(handlers::get_alerts))
        .route("/api/stats", get(handlers::get_stats))
        .route("/ws", get(ws::ws_handler))
        .nest_service("/dashboard", ServeDir::new(dashboard_dir))
        // ── Security layers ──────────────────────────────────────────
        // CORS: allow same-origin + localhost only (override via env)
        .layer(CorsLayer::new()
            .allow_origin(AllowOrigin::predicate(|origin: &HeaderValue, _| {
                let o = origin.to_str().unwrap_or("");
                o.starts_with("http://localhost") || o.starts_with("http://127.0.0.1")
            }))
            .allow_methods([http::Method::GET]))
        // Security headers
        .layer(SetResponseHeaderLayer::overriding(
            http::header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            http::header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            http::header::HeaderName::from_static("x-xss-protection"),
            HeaderValue::from_static("1; mode=block"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            http::header::HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        // Request size limit (1 MiB)
        .layer(DefaultBodyLimit::max(1_048_576))
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chrono::Utc;
    use fraud_common::types::{Alert, FraudScore, ModelScore, RiskLevel};
    use tower::ServiceExt;
    use uuid::Uuid;

    fn make_state() -> Arc<AppState> {
        let (tx, _rx) = broadcast::channel(16);
        Arc::new(AppState::new(tx))
    }

    fn make_score(tx_hash: &str) -> FraudScore {
        FraudScore {
            id: Uuid::new_v4(),
            tx_hash: tx_hash.into(),
            block_number: 1,
            score: 0.5,
            risk_level: RiskLevel::Medium,
            model_scores: vec![ModelScore { model_name: "test".into(), score: 0.5, weight: 1.0 }],
            triggered_rules: vec![],
            scored_at: Utc::now(),
        }
    }

    fn make_alert(tx_hash: &str) -> Alert {
        Alert {
            id: Uuid::new_v4(),
            fraud_score: make_score(tx_hash),
            from: "0xfrom".into(),
            to: Some("0xto".into()),
            value: 1000,
            created_at: Utc::now(),
            acknowledged: false,
        }
    }

    #[test]
    fn push_score_increments_stats() {
        let state = make_state();
        state.push_score(make_score("0x1"));
        state.push_score(make_score("0x2"));

        let stats = state.stats.lock().unwrap();
        assert_eq!(stats.transactions_scored, 2);
        assert_eq!(state.recent_scores.lock().unwrap().len(), 2);
    }

    #[test]
    fn push_alert_increments_stats() {
        let state = make_state();
        state.push_alert(make_alert("0x1"));

        let stats = state.stats.lock().unwrap();
        assert_eq!(stats.alerts_raised, 1);
        assert_eq!(state.recent_alerts.lock().unwrap().len(), 1);
    }

    #[test]
    fn inc_blocks() {
        let state = make_state();
        state.inc_blocks();
        state.inc_blocks();
        state.inc_blocks();

        let stats = state.stats.lock().unwrap();
        assert_eq!(stats.blocks_processed, 3);
    }

    #[test]
    fn score_ring_buffer_eviction() {
        let state = make_state();
        for i in 0..(MAX_RECENT + 10) {
            state.push_score(make_score(&format!("0x{i}")));
        }
        let buf = state.recent_scores.lock().unwrap();
        assert_eq!(buf.len(), MAX_RECENT);
        // Oldest should have been evicted; first in buffer should be "0x10"
        assert_eq!(buf.front().unwrap().tx_hash, "0x10");
    }

    #[test]
    fn alert_ring_buffer_eviction() {
        let state = make_state();
        for i in 0..(MAX_RECENT + 5) {
            state.push_alert(make_alert(&format!("0x{i}")));
        }
        let buf = state.recent_alerts.lock().unwrap();
        assert_eq!(buf.len(), MAX_RECENT);
    }

    #[tokio::test]
    async fn health_endpoint() {
        let state = make_state();
        let app = build_router(state);

        let resp = app
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
    }

    #[tokio::test]
    async fn scores_endpoint() {
        let state = make_state();
        state.push_score(make_score("0xabc"));
        let app = build_router(state);

        let resp = app
            .oneshot(Request::builder().uri("/api/scores").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"], 1);
    }

    #[tokio::test]
    async fn alerts_endpoint() {
        let state = make_state();
        state.push_alert(make_alert("0xdef"));
        let app = build_router(state);

        let resp = app
            .oneshot(Request::builder().uri("/api/alerts").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"], 1);
    }

    #[tokio::test]
    async fn stats_endpoint() {
        let state = make_state();
        state.inc_blocks();
        state.push_score(make_score("0x1"));
        state.push_alert(make_alert("0x2"));
        let app = build_router(state);

        let resp = app
            .oneshot(Request::builder().uri("/api/stats").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["blocks_processed"], 1);
        assert_eq!(json["transactions_scored"], 1);
        assert_eq!(json["alerts_raised"], 1);
    }

    #[tokio::test]
    async fn not_found_returns_404() {
        let state = make_state();
        let app = build_router(state);

        let resp = app
            .oneshot(Request::builder().uri("/nonexistent").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
