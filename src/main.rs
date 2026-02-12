use anyhow::Context;
use fraud_common::config::AppConfig;
use fraud_common::types::PipelineEvent;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Logging ──────────────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .init();

    info!("🔍 AI-based Fraud Detection on Blockchain – starting up");

    // ── Configuration ────────────────────────────────────────────────────
    let config = AppConfig::load_default().context("failed to load configuration")?;
    let buf = config.pipeline.channel_buffer;

    // ── Channels between pipeline stages ─────────────────────────────────
    //
    //  Indexer ──(blocks)──▶ FeatureExtractor ──(features)──▶ Scorer ──(scored+alerts)──▶ AlertDispatcher
    //                                                                                         │
    //                                                                               broadcast ──▶ API/WS
    let (indexer_tx, indexer_rx) = mpsc::channel::<PipelineEvent>(buf);
    let (features_tx, features_rx) = mpsc::channel::<PipelineEvent>(buf);
    let (scorer_tx, scorer_rx) = mpsc::channel::<PipelineEvent>(buf);

    // ── Alert dispatcher (also provides broadcast for WS) ────────────────
    let (dispatcher, _ws_rx) =
        fraud_alerts::AlertDispatcher::new(&config.alerts);
    let broadcast_tx = dispatcher.broadcast_sender();

    // ── API shared state ─────────────────────────────────────────────────
    let app_state = Arc::new(fraud_api::routes::AppState::new(broadcast_tx.clone()));
    let app_state_ingest = Arc::clone(&app_state);

    // ── SQLite storage (optional) ────────────────────────────────────────
    let storage: Option<Arc<fraud_storage::Storage>> = if config.storage.enabled {
        match fraud_storage::Storage::new(&config.storage) {
            Ok(s) => {
                info!("SQLite storage enabled at {}", config.storage.db_path);
                Some(Arc::new(s))
            }
            Err(e) => {
                warn!(error = %e, "failed to open storage – running without persistence");
                None
            }
        }
    } else {
        info!("SQLite storage disabled by configuration");
        None
    };
    let storage_ingest = storage.clone();

    // ── Spawn pipeline stages ────────────────────────────────────────────

    // 1. Block indexer
    let indexer_cfg = config.indexer.clone();
    tokio::spawn(async move {
        match fraud_indexer::BlockStream::new(&indexer_cfg).await {
            Ok(stream) => {
                if let Err(e) = stream.run(indexer_tx).await {
                    error!(error = %e, "indexer stopped");
                }
            }
            Err(e) => error!(error = %e, "failed to create block stream"),
        }
    });

    // 2. Feature extractor
    let extractor = fraud_features::FeatureExtractor::new();
    tokio::spawn(async move {
        if let Err(e) = extractor.run(indexer_rx, features_tx).await {
            error!(error = %e, "feature extractor stopped");
        }
    });

    // 3. Ensemble scorer
    let scorer = fraud_scorer::EnsembleScorer::new(&config.scorer);
    tokio::spawn(async move {
        if let Err(e) = scorer.run(features_rx, scorer_tx).await {
            error!(error = %e, "scorer stopped");
        }
    });

    // 4. Ingestion loop – feeds scored events into both the alert dispatcher
    //    and the API state, and optionally persists to SQLite.
    let (alert_tx, alert_rx) = mpsc::channel::<PipelineEvent>(buf);
    tokio::spawn(async move {
        let mut rx = scorer_rx;
        while let Some(event) = rx.recv().await {
            match &event {
                PipelineEvent::Scored(score) => {
                    app_state_ingest.push_score(score.clone());
                    if let Some(ref db) = storage_ingest {
                        if let Err(e) = db.insert_score(score) {
                            warn!(error = %e, "failed to persist score");
                        }
                    }
                }
                PipelineEvent::AlertRaised(alert) => {
                    app_state_ingest.push_alert(alert.clone());
                    if let Some(ref db) = storage_ingest {
                        if let Err(e) = db.insert_alert(alert) {
                            warn!(error = %e, "failed to persist alert");
                        }
                    }
                    let _ = alert_tx.send(event.clone()).await;
                }
                PipelineEvent::NewBlock { .. } => {
                    app_state_ingest.inc_blocks();
                }
                _ => {}
            }
        }
    });

    // 5. Alert dispatcher
    tokio::spawn(async move {
        if let Err(e) = dispatcher.run(alert_rx).await {
            error!(error = %e, "alert dispatcher stopped");
        }
    });

    // ── HTTP API server (blocks forever) ─────────────────────────────────
    info!("all pipeline stages launched");
    fraud_api::routes::serve(&config.api, app_state).await?;

    Ok(())
}
