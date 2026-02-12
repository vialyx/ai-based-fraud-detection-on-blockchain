use axum::extract::State;
use axum::Json;
use fraud_common::types::{Alert, FraudScore};
use serde::Serialize;
use std::sync::Arc;

use crate::routes::AppState;

/// Health-check response.
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}

/// GET /health
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Response for the recent scores endpoint.
#[derive(Serialize)]
pub struct ScoresResponse {
    pub count: usize,
    pub scores: Vec<FraudScore>,
}

/// GET /api/scores – returns the most recent fraud scores kept in memory.
pub async fn get_scores(State(state): State<Arc<AppState>>) -> Json<ScoresResponse> {
    let scores = state.recent_scores.lock().unwrap().clone();
    let scores: Vec<_> = scores.into();
    Json(ScoresResponse {
        count: scores.len(),
        scores,
    })
}

/// Response for the recent alerts endpoint.
#[derive(Serialize)]
pub struct AlertsResponse {
    pub count: usize,
    pub alerts: Vec<Alert>,
}

/// GET /api/alerts – returns the most recent alerts kept in memory.
pub async fn get_alerts(State(state): State<Arc<AppState>>) -> Json<AlertsResponse> {
    let alerts = state.recent_alerts.lock().unwrap().clone();
    let alerts: Vec<_> = alerts.into();
    Json(AlertsResponse {
        count: alerts.len(),
        alerts,
    })
}

/// Pipeline statistics.
#[derive(Serialize)]
pub struct StatsResponse {
    pub blocks_processed: u64,
    pub transactions_scored: u64,
    pub alerts_raised: u64,
}

/// GET /api/stats
pub async fn get_stats(State(state): State<Arc<AppState>>) -> Json<StatsResponse> {
    let stats = state.stats.lock().unwrap();
    Json(StatsResponse {
        blocks_processed: stats.blocks_processed,
        transactions_scored: stats.transactions_scored,
        alerts_raised: stats.alerts_raised,
    })
}
